use std::sync::atomic::{AtomicU8, Ordering};

use domain::NetStatus;
use ports::ConnectivityMonitor;

const ONLINE: u8 = 0;
const OFFLINE: u8 = 1;
const DEGRADED: u8 = 2;

#[derive(Debug)]
pub struct MemoryConnectivity {
    status: AtomicU8,
}

impl Default for MemoryConnectivity {
    fn default() -> Self {
        MemoryConnectivity::new(NetStatus::Online)
    }
}

impl MemoryConnectivity {
    pub fn new(status: NetStatus) -> Self {
        MemoryConnectivity {
            status: AtomicU8::new(encode(status)),
        }
    }

    pub fn set(&self, status: NetStatus) {
        self.status.store(encode(status), Ordering::Relaxed);
    }
}

impl ConnectivityMonitor for MemoryConnectivity {
    fn status(&self) -> NetStatus {
        decode(self.status.load(Ordering::Relaxed))
    }

    fn observe(&self, status: NetStatus) {
        self.set(status);
    }
}

fn encode(status: NetStatus) -> u8 {
    match status {
        NetStatus::Online => ONLINE,
        NetStatus::Offline => OFFLINE,
        NetStatus::Degraded => DEGRADED,
    }
}

fn decode(raw: u8) -> NetStatus {
    match raw {
        OFFLINE => NetStatus::Offline,
        DEGRADED => NetStatus::Degraded,
        _ => NetStatus::Online,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_round_trips() {
        for status in [NetStatus::Online, NetStatus::Offline, NetStatus::Degraded] {
            let net = MemoryConnectivity::new(status);
            assert_eq!(net.status(), status);
        }
    }

    #[test]
    fn a_transport_observation_updates_the_flag() {
        let net = MemoryConnectivity::default();
        net.observe(NetStatus::Offline);
        assert_eq!(net.status(), NetStatus::Offline);
    }
}
