use crate::{
    input::{HeldKeys, WindowsInjector},
    protocol::{self, Channel, Message, DISCOVERY_PORT, INPUT_PORT, QUERY},
    state::{Captured, Peer, Shared},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const TICK: Duration = Duration::from_millis(5);
#[derive(Serialize, Deserialize)]
struct Announcement {
    protocol: String,
    name: String,
}

pub struct Runtime {
    pub stop: Arc<AtomicBool>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn dispatcher(shared: Arc<Shared>, events: Receiver<Captured>) {
    thread::spawn(move || {
        while let Ok(event) = events.recv() {
            if event.epoch != shared.epoch.load(Ordering::SeqCst) {
                continue;
            }
            for peer in shared.peers.lock().unwrap().values() {
                if peer.selected {
                    if let Some(tx) = &peer.tx {
                        if tx.try_send(event).is_err() {
                            shared.pause();
                            shared
                                .status("Paused: a receiver could not keep up. Resume when ready.");
                        }
                    }
                }
            }
        }
    });
}

pub fn start(shared: Arc<Shared>, receiver: bool, code: String) -> Runtime {
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    thread::spawn(move || {
        let result = if receiver {
            receiver_loop(shared.clone(), cancel, code)
        } else {
            discover(shared.clone(), cancel)
        };
        if let Err(e) = result {
            shared.pause();
            shared.status(format!("Network error: {e}. Restart the app to retry."));
        }
    });
    Runtime { stop }
}

fn discover(shared: Arc<Shared>, stop: Arc<AtomicBool>) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    let mut last = Instant::now() - Duration::from_secs(3);
    let mut buffer = [0; 1024];
    shared.status("Searching your local network for receivers...");
    while !stop.load(Ordering::SeqCst) {
        if last.elapsed() >= Duration::from_secs(2) {
            socket.send_to(QUERY, (Ipv4Addr::BROADCAST, DISCOVERY_PORT))?;
            last = Instant::now();
            shared
                .peers
                .lock()
                .unwrap()
                .retain(|_, p| p.selected || p.seen.elapsed() < Duration::from_secs(7));
        }
        match socket.recv_from(&mut buffer) {
            Ok((n, SocketAddr::V4(addr))) => {
                if let Ok(a) = serde_json::from_slice::<Announcement>(&buffer[..n]) {
                    if a.protocol == "KEYCAST_V1" && a.name.len() <= 128 {
                        let mut peers = shared.peers.lock().unwrap();
                        if peers.len() < 128 || peers.contains_key(addr.ip()) {
                            let p = peers.entry(*addr.ip()).or_insert_with(|| Peer {
                                revision: 0,
                                name: a.name.clone(),
                                seen: Instant::now(),
                                selected: false,
                                status: "Available".into(),
                                tx: None,
                            });
                            p.name = a.name;
                            p.seen = Instant::now();
                        }
                    }
                }
            }
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn configure(stream: &TcpStream) -> io::Result<()> {
    // Winsock accepted sockets inherit the listener's nonblocking mode.
    // Pairing uses read_exact, so restore blocking mode before its first read.
    // Otherwise a normal delay before Hello can close a valid connection.
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_millis(200)))
}

fn pairing_error(error: io::Error, stage: &str) -> io::Error {
    let advice = if stage == "receiver greeting" {
        "On the other PC, enable receiving and close duplicate Keycast windows. It may be paused or already connected."
    } else {
        "Check that both PCs use the same pairing code and updated app. Check the receiver's status message too."
    };
    io::Error::new(
        error.kind(),
        format!("Pairing stopped during {stage}: {error}. {advice}"),
    )
}

pub fn select(
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    ip: Ipv4Addr,
    code: String,
    selected: bool,
) {
    // Changing membership pauses the group so a new PC never starts mid-chord.
    shared.pause();
    let (tx, rx) = mpsc::sync_channel(256);
    let mut revision = 0;
    if let Some(peer) = shared.peers.lock().unwrap().get_mut(&ip) {
        peer.revision += 1;
        revision = peer.revision;
        peer.selected = selected;
        peer.tx = if selected { Some(tx) } else { None };
        peer.status = if selected {
            "Connecting..."
        } else {
            "Available"
        }
        .into();
    }
    if selected {
        thread::spawn(move || {
            let result = sender_session(&shared, &stop, ip, revision, &code, rx);
            if let Some(peer) = shared.peers.lock().unwrap().get_mut(&ip) {
                if peer.revision != revision {
                    return;
                }
                peer.selected = false;
                peer.tx = None;
                peer.status = match result {
                    Ok(()) => "Disconnected".into(),
                    Err(e) => {
                        shared.status(format!("{ip}: {e}"));
                        "Connection failed — see message below".into()
                    }
                };
            }
            shared.pause();
        });
    }
}

// Preserve partial frames across WouldBlock. Framing remains bounded even when
// the network splits the length prefix or encrypted payload into small packets.
#[derive(Default)]
struct Reader {
    buffer: Vec<u8>,
}
impl Reader {
    fn poll(
        &mut self,
        stream: &mut impl Read,
        channel: &mut Channel,
    ) -> io::Result<Option<Message>> {
        if self.buffer.len() >= 4 {
            let size = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
            if !(28..=2048).contains(&size) {
                return Err(io::Error::other("Invalid frame length"));
            }
            if self.buffer.len() >= size + 4 {
                let frame: Vec<_> = self.buffer.drain(..size + 4).collect();
                return channel.receive(&mut std::io::Cursor::new(frame)).map(Some);
            }
        }
        let mut bytes = [0; 1024];
        match stream.read(&mut bytes) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Peer disconnected",
            )),
            Ok(n) => {
                self.buffer.extend_from_slice(&bytes[..n]);
                if self.buffer.len() > 4096 {
                    return Err(io::Error::other("Input buffer exceeded"));
                }
                Ok(None)
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn sender_session(
    shared: &Shared,
    stop: &AtomicBool,
    ip: Ipv4Addr,
    revision: u64,
    code: &str,
    rx: Receiver<Captured>,
) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::V4(SocketAddrV4::new(ip, INPUT_PORT)),
        Duration::from_secs(2),
    )?;
    configure(&stream)?;
    let mut challenge = [0; 32];
    stream
        .read_exact(&mut challenge)
        .map_err(|e| pairing_error(e, "receiver greeting"))?;
    let mut channel = Channel::new(code, challenge);
    channel
        .send(&mut stream, Message::Hello)
        .map_err(|e| pairing_error(e, "authentication"))?;
    if channel
        .receive(&mut stream)
        .map_err(|e| pairing_error(e, "authentication"))?
        != Message::Ready
    {
        return Err(io::Error::other("Receiver not ready"));
    }
    stream.set_nonblocking(true)?;
    if let Some(p) = shared.peers.lock().unwrap().get_mut(&ip) {
        if p.revision == revision {
            p.status = "Paired / ready".into();
        }
    }
    let mut reader = Reader::default();
    let mut epoch = shared.epoch.load(Ordering::SeqCst);
    let mut down = HashSet::new();
    let mut heartbeat = Instant::now();
    let mut last_reply = Instant::now();
    loop {
        let selected = shared
            .peers
            .lock()
            .unwrap()
            .get(&ip)
            .is_some_and(|p| p.selected && p.revision == revision);
        if stop.load(Ordering::SeqCst) || !selected {
            let _ = channel.send(&mut stream, Message::Release);
            return Ok(());
        }
        let current = shared.epoch.load(Ordering::SeqCst);
        if current != epoch {
            channel.send(&mut stream, Message::Release)?;
            down.clear();
            epoch = current;
        }
        // Bound work per tick so pause/heartbeat can't be starved by a hot queue.
        for _ in 0..32 {
            let Ok(event) = rx.try_recv() else {
                break;
            };
            if epoch & 1 == 0
                || event.epoch != epoch
                || shared.epoch.load(Ordering::SeqCst) != epoch
            {
                continue;
            }
            let id = event.key.identity();
            if event.key.up {
                if !down.remove(&id) {
                    continue;
                }
            } else {
                down.insert(id);
            }
            channel.send(&mut stream, Message::Key(event.key))?;
            shared.sent.fetch_add(1, Ordering::Relaxed);
        }
        if heartbeat.elapsed() > Duration::from_millis(250) {
            channel.send(&mut stream, Message::Ping)?;
            heartbeat = Instant::now();
        }
        if let Some(message) = reader.poll(&mut stream, &mut channel)? {
            if message != Message::Ping {
                return Err(io::Error::other("Unexpected receiver message"));
            }
            last_reply = Instant::now();
        }
        if last_reply.elapsed() > Duration::from_secs(2) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Receiver stopped responding",
            ));
        }
        thread::sleep(TICK);
    }
}

fn receiver_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>, code: String) -> io::Result<()> {
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))?;
    let tcp = TcpListener::bind((Ipv4Addr::UNSPECIFIED, INPUT_PORT))?;
    udp.set_nonblocking(true)?;
    tcp.set_nonblocking(true)?;
    let name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".into());
    let announcement = serde_json::to_vec(&Announcement {
        protocol: "KEYCAST_V1".into(),
        name,
    })
    .unwrap();
    let busy = Arc::new(AtomicBool::new(false));
    let mut bytes = [0; 256];
    shared.status("Receiver available on the local network. Waiting for your main PC.");
    while !stop.load(Ordering::SeqCst) {
        if let Ok((n, addr)) = udp.recv_from(&mut bytes) {
            if &bytes[..n] == QUERY {
                let _ = udp.send_to(&announcement, addr);
            }
        }
        if let Ok((mut stream, addr)) = tcp.accept() {
            if shared.active() && !busy.swap(true, Ordering::SeqCst) {
                let (shared, stop, code, busy) =
                    (shared.clone(), stop.clone(), code.clone(), busy.clone());
                thread::spawn(move || {
                    let result = receive_session(&mut stream, &shared, &stop, &code, addr);
                    busy.store(false, Ordering::SeqCst);
                    match result {
                        Ok(()) => shared.status(
                            "Receiver paused. Click Enable receiving to allow a new connection.",
                        ),
                        Err(e) => shared
                            .status(format!("Connection ended: {e}. Waiting for your main PC.")),
                    }
                });
            }
        }
        thread::sleep(TICK);
    }
    Ok(())
}

fn receive_session(
    stream: &mut TcpStream,
    shared: &Shared,
    stop: &AtomicBool,
    code: &str,
    addr: SocketAddr,
) -> io::Result<()> {
    receive_with(stream, shared, stop, code, addr, WindowsInjector)
}

fn receive_with<I: crate::input::Injector>(
    stream: &mut TcpStream,
    shared: &Shared,
    stop: &AtomicBool,
    code: &str,
    addr: SocketAddr,
    injector: I,
) -> io::Result<()> {
    configure(stream)?;
    let epoch = shared.epoch.load(Ordering::SeqCst);
    let challenge = protocol::random::<32>();
    stream.write_all(&challenge)?;
    let mut channel = Channel::new(code, challenge);
    if channel
        .receive(stream)
        .map_err(|e| pairing_error(e, "authentication"))?
        != Message::Hello
    {
        return Err(io::Error::other("Pairing failed"));
    }
    if !shared.active() {
        return Ok(());
    }
    channel.send(stream, Message::Ready)?;
    stream.set_nonblocking(true)?;
    shared.status(format!(
        "Paired with {}. Input goes to the focused window.",
        addr.ip()
    ));
    let mut keys = HeldKeys::new(injector);
    let mut reader = Reader::default();
    let mut last = Instant::now();
    loop {
        if stop.load(Ordering::SeqCst) || shared.epoch.load(Ordering::SeqCst) != epoch {
            return Ok(());
        }
        if let Some(message) = reader.poll(stream, &mut channel)? {
            last = Instant::now();
            match message {
                Message::Key(key) => {
                    if shared.epoch.load(Ordering::SeqCst) != epoch {
                        return Ok(());
                    }
                    keys.apply(key)?;
                    shared.sent.fetch_add(1, Ordering::Relaxed);
                }
                Message::Release => keys.release(),
                Message::Ping => channel.send(stream, Message::Ping)?,
                _ => return Err(io::Error::other("Unexpected sender message")),
            }
        }
        if last.elapsed() > Duration::from_secs(1) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Sender heartbeat lost; held keys released",
            ));
        }
        thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input::Injector, protocol::Key};
    use std::{
        collections::BTreeMap,
        sync::{atomic::AtomicU64, Mutex},
    };

    struct Recording(Arc<Mutex<Vec<Key>>>);
    impl Injector for Recording {
        fn inject(&mut self, key: Key) -> io::Result<()> {
            self.0.lock().unwrap().push(key);
            Ok(())
        }
    }
    // Exercise the real receiver protocol over TCP with a recording injector,
    // so tests cannot type into the developer's desktop.
    fn cleanup_scenario(action: &str) {
        let (tx, _) = mpsc::sync_channel(1);
        let shared = Arc::new(Shared {
            epoch: AtomicU64::new(1),
            receiver: AtomicBool::new(true),
            hook_ready: AtomicBool::new(true),
            peers: Mutex::new(BTreeMap::new()),
            status: Mutex::new(String::new()),
            input_tx: tx,
            sent: AtomicU64::new(0),
        });
        let recorded = Arc::new(Mutex::new(vec![]));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        // Match production: accepted Windows sockets inherit nonblocking mode.
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server_shared = shared.clone();
        let log = recorded.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (mut socket, addr) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(TICK)
                    }
                    Err(e) => panic!("accept failed: {e}"),
                }
            };
            receive_with(
                &mut socket,
                &server_shared,
                &AtomicBool::new(false),
                "test-code",
                addr,
                Recording(log),
            )
        });
        let mut socket = TcpStream::connect(address).unwrap();
        configure(&socket).unwrap();
        let mut session = [0; 32];
        socket.read_exact(&mut session).unwrap();
        // A real Wi-Fi peer cannot return Hello in the same instant as greeting.
        thread::sleep(Duration::from_millis(100));
        let mut channel = Channel::new("test-code", session);
        channel.send(&mut socket, Message::Hello).unwrap();
        assert_eq!(channel.receive(&mut socket).unwrap(), Message::Ready);
        channel
            .send(
                &mut socket,
                Message::Key(Key {
                    vk: 65,
                    scan: 30,
                    extended: false,
                    up: false,
                }),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while recorded.lock().unwrap().is_empty() {
            assert!(Instant::now() < deadline, "Key was not processed");
            thread::sleep(TICK);
        }
        match action {
            "pause" => shared.pause(),
            "release" => {
                channel.send(&mut socket, Message::Release).unwrap();
                channel.send(&mut socket, Message::Ping).unwrap();
                assert_eq!(channel.receive(&mut socket).unwrap(), Message::Ping);
                shared.pause();
            }
            "disconnect" => {
                socket.shutdown(std::net::Shutdown::Both).unwrap();
            }
            "timeout" => (),
            _ => unreachable!(),
        }
        let result = server.join().unwrap();
        if action == "timeout" {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        }
        let events = recorded.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(!events[0].up);
        assert!(events[1].up);
    }
    #[test]
    fn tcp_pause_releases_keys() {
        cleanup_scenario("pause");
    }
    #[test]
    fn tcp_release_releases_keys() {
        cleanup_scenario("release");
    }
    #[test]
    fn tcp_disconnect_releases_keys() {
        cleanup_scenario("disconnect");
    }
    #[test]
    fn tcp_heartbeat_timeout_releases_keys() {
        cleanup_scenario("timeout");
    }
    struct Fragments {
        bytes: std::collections::VecDeque<u8>,
    }
    #[test]
    fn early_disconnect_explains_receiver_state() {
        let error = pairing_error(io::ErrorKind::UnexpectedEof.into(), "receiver greeting");
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(error.to_string().contains("enable receiving"));
    }
    #[test]
    fn authentication_disconnect_explains_pairing_code() {
        let error = pairing_error(io::ErrorKind::UnexpectedEof.into(), "authentication");
        assert!(error.to_string().contains("same pairing code"));
    }
    impl Read for Fragments {
        fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
            if let Some(byte) = self.bytes.pop_front() {
                dst[0] = byte;
                Ok(1)
            } else {
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
    }
    #[test]
    fn fragmented_encrypted_frame() {
        let mut output = vec![];
        Channel::new("code", [7; 32])
            .send(&mut output, Message::Release)
            .unwrap();
        let n = output.len();
        let mut source = Fragments {
            bytes: output.into(),
        };
        let mut reader = Reader::default();
        let mut channel = Channel::new("code", [7; 32]);
        for _ in 0..n {
            assert_eq!(reader.poll(&mut source, &mut channel).unwrap(), None);
        }
        assert_eq!(
            reader.poll(&mut source, &mut channel).unwrap(),
            Some(Message::Release)
        );
    }
}
