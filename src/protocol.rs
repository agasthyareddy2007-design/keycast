use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const DISCOVERY_PORT: u16 = 45832;
pub const INPUT_PORT: u16 = 45833;
pub const QUERY: &[u8] = b"KEYCAST_DISCOVER_V1";
const MAX_FRAME: usize = 2048;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Key {
    pub vk: u16,
    pub scan: u16,
    pub extended: bool,
    pub up: bool,
}
impl Key {
    pub fn identity(self) -> (u16, u16, bool) {
        (self.vk, self.scan, self.extended)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum Message {
    Hello,
    Ready,
    Key(Key),
    Release,
    Ping,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    session: [u8; 32],
    sequence: u64,
    message: Message,
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("Windows secure random unavailable");
    bytes
}
pub fn new_code() -> String {
    random::<16>().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn normalize_code(code: &str) -> Option<String> {
    let code: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    (code.len() == 32 && code.bytes().all(|b| b.is_ascii_hexdigit())).then(|| code.to_lowercase())
}
pub fn cipher(code: &str) -> LessSafeKey {
    LessSafeKey::new(
        UnboundKey::new(&aead::CHACHA20_POLY1305, &Sha256::digest(code.as_bytes())).unwrap(),
    )
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Pairing or protocol verification failed",
    )
}

// Each direction has a separate sequence. Every frame is bound to a fresh
// receiver challenge, preventing recordings being replayed in another session.
pub struct Channel {
    cipher: LessSafeKey,
    session: [u8; 32],
    tx: u64,
    rx: u64,
}
impl Channel {
    pub fn new(code: &str, session: [u8; 32]) -> Self {
        Self {
            cipher: cipher(code),
            session,
            tx: 0,
            rx: 0,
        }
    }
    pub fn send(&mut self, stream: &mut impl Write, message: Message) -> io::Result<()> {
        let raw = serde_json::to_vec(&Envelope {
            session: self.session,
            sequence: self.tx,
            message,
        })
        .map_err(|_| invalid())?;
        let nonce = random::<12>();
        let mut encrypted = raw;
        self.cipher
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::empty(),
                &mut encrypted,
            )
            .map_err(|_| invalid())?;
        let len = (nonce.len() + encrypted.len()) as u32;
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(&nonce)?;
        stream.write_all(&encrypted)?;
        self.tx = self.tx.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
    pub fn receive(&mut self, stream: &mut impl Read) -> io::Result<Message> {
        let mut size = [0; 4];
        stream.read_exact(&mut size)?;
        let size = u32::from_be_bytes(size) as usize;
        if !(28..=MAX_FRAME).contains(&size) {
            return Err(invalid());
        }
        let mut frame = vec![0; size];
        stream.read_exact(&mut frame)?;
        let nonce = Nonce::assume_unique_for_key(frame[..12].try_into().unwrap());
        let raw = self
            .cipher
            .open_in_place(nonce, Aad::empty(), &mut frame[12..])
            .map_err(|_| invalid())?;
        let envelope: Envelope = serde_json::from_slice(raw).map_err(|_| invalid())?;
        if envelope.session != self.session || envelope.sequence != self.rx {
            return Err(invalid());
        }
        self.rx = self.rx.checked_add(1).ok_or_else(invalid)?;
        Ok(envelope.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    const CODE: &str = "00112233445566778899aabbccddeeff";
    fn frame() -> Vec<u8> {
        let mut output = Vec::new();
        Channel::new(CODE, [1; 32])
            .send(&mut output, Message::Ping)
            .unwrap();
        output
    }
    #[test]
    fn authenticated_roundtrip() {
        assert_eq!(
            Channel::new(CODE, [1; 32])
                .receive(&mut Cursor::new(frame()))
                .unwrap(),
            Message::Ping
        );
    }
    #[test]
    fn wrong_code_rejected() {
        assert!(Channel::new("different", [1; 32])
            .receive(&mut Cursor::new(frame()))
            .is_err());
    }
    #[test]
    fn tampering_rejected() {
        let mut bytes = frame();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(Channel::new(CODE, [1; 32])
            .receive(&mut Cursor::new(bytes))
            .is_err());
    }
    #[test]
    fn replay_and_other_session_rejected() {
        let bytes = frame();
        let mut channel = Channel::new(CODE, [1; 32]);
        channel.receive(&mut Cursor::new(&bytes)).unwrap();
        assert!(channel.receive(&mut Cursor::new(&bytes)).is_err());
        assert!(Channel::new(CODE, [2; 32])
            .receive(&mut Cursor::new(bytes))
            .is_err());
    }
    #[test]
    fn oversized_frame_rejected() {
        assert!(Channel::new(CODE, [1; 32])
            .receive(&mut Cursor::new(u32::MAX.to_be_bytes()))
            .is_err());
    }
    #[test]
    fn code_validation() {
        assert_eq!(
            normalize_code("00112233-44556677-8899AABB-CCDDEEFF"),
            Some(CODE.into())
        );
        assert!(normalize_code("1234").is_none());
    }
}
