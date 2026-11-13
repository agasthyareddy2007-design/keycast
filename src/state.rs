use crate::protocol::Key;
use std::{
    collections::BTreeMap,
    net::Ipv4Addr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::SyncSender,
        Mutex,
    },
    time::Instant,
};

#[derive(Clone, Copy)]
pub struct Captured {
    pub epoch: u64,
    pub key: Key,
}
pub struct Peer {
    pub revision: u64,
    pub name: String,
    pub seen: Instant,
    pub selected: bool,
    pub status: String,
    pub tx: Option<SyncSender<Captured>>,
}
pub struct Shared {
    // Odd = enabled; every transition changes the epoch, invalidating queued keys.
    pub epoch: AtomicU64,
    pub receiver: AtomicBool,
    pub hook_ready: AtomicBool,
    pub peers: Mutex<BTreeMap<Ipv4Addr, Peer>>,
    pub status: Mutex<String>,
    pub input_tx: SyncSender<Captured>,
    pub sent: AtomicU64,
}
impl Shared {
    pub fn active(&self) -> bool {
        self.epoch.load(Ordering::SeqCst) & 1 == 1
    }
    pub fn pause(&self) {
        self.transition(false);
    }
    pub fn resume(&self) {
        self.transition(true);
    }
    fn transition(&self, active: bool) {
        let mut old = self.epoch.load(Ordering::SeqCst);
        loop {
            let next = (old.wrapping_add(2) & !1) | u64::from(active);
            match self
                .epoch
                .compare_exchange_weak(old, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(current) => old = current,
            }
        }
    }
    pub fn status(&self, text: impl Into<String>) {
        *self.status.lock().unwrap() = text.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_invalidates_pending_events() {
        let (tx, _) = std::sync::mpsc::sync_channel(1);
        let shared = Shared {
            epoch: AtomicU64::new(0),
            receiver: AtomicBool::new(false),
            hook_ready: AtomicBool::new(false),
            peers: Mutex::new(BTreeMap::new()),
            status: Mutex::new(String::new()),
            input_tx: tx,
            sent: AtomicU64::new(0),
        };
        shared.resume();
        let before = shared.epoch.load(Ordering::SeqCst);
        shared.pause();
        assert!(!shared.active());
        shared.resume();
        assert!(shared.active());
        assert_ne!(before, shared.epoch.load(Ordering::SeqCst));
    }
}
