use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{PhoneNumber, SmsRowId};
use ports::{Clock, InboundSms, SmsTransport, TransportError};


/*
 * Same as fake matrix fake homeserver infra, need to be remove TODO(freezlex)
 */
 #[derive(Debug, Default)]
struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSms {
    pub row: SmsRowId,
    pub peer: PhoneNumber,
    pub text: String,
    pub ts: u64,
    pub outgoing: bool,
}

#[derive(Debug, Default)]
struct Device {
    pending: Vec<InboundSms>,

    storage: HashMap<SmsRowId, StoredSms>,
}

#[derive(Debug, Default)]
struct Network {
    devices: HashMap<PhoneNumber, Device>,
}

#[derive(Clone)]
pub struct FakeSmsNetwork {
    inner: Arc<Mutex<Network>>,
    next_row: Arc<AtomicU64>,
    clock: Arc<dyn Clock>,
}

impl Default for FakeSmsNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FakeSmsNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeSmsNetwork")
            .field("devices", &self.inner.lock().map(|g| g.devices.len()).ok())
            .finish_non_exhaustive()
    }
}

impl FakeSmsNetwork {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        FakeSmsNetwork {
            inner: Arc::new(Mutex::new(Network::default())),
            next_row: Arc::new(AtomicU64::new(0)),
            clock,
        }
    }

    pub fn handset(&self, number: impl AsRef<str>) -> FakeSmsTransport {
        let number = PhoneNumber::new(number);
        self.inner
            .lock()
            .expect("sms network lock")
            .devices
            .entry(number.clone())
            .or_default();
        FakeSmsTransport {
            network: self.clone(),
            number,
            reachable: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn stored_rows(&self, number: &PhoneNumber) -> Vec<StoredSms> {
        let guard = self.inner.lock().expect("sms network lock");
        let Some(device) = guard.devices.get(number) else {
            return Vec::new();
        };
        let mut rows: Vec<_> = device.storage.values().cloned().collect();
        rows.sort_by_key(|r| (r.ts, r.row.as_str().to_owned()));
        rows
    }

    fn next_row_id(&self) -> SmsRowId {
        SmsRowId::new(format!(
            "sms-{:012}",
            self.next_row.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn deliver(&self, from: &PhoneNumber, to: &PhoneNumber, text: &str, ts: u64) {
        let row = self.next_row_id();
        let mut guard = self.inner.lock().expect("sms network lock");
        let Some(device) = guard.devices.get_mut(to) else {
            return;
        };
        device.storage.insert(
            row.clone(),
            StoredSms {
                row: row.clone(),
                peer: from.clone(),
                text: text.to_owned(),
                ts,
                outgoing: false,
            },
        );
        device.pending.push(InboundSms {
            from: from.clone(),
            text: text.to_owned(),
            received_ts: ts,
            row,
        });
    }
}

#[derive(Debug, Clone)]
pub struct FakeSmsTransport {
    network: FakeSmsNetwork,
    number: PhoneNumber,
    reachable: Arc<AtomicBool>,
}

impl FakeSmsTransport {
    pub fn number(&self) -> &PhoneNumber {
        &self.number
    }

    pub fn set_reachable(&self, reachable: bool) {
        self.reachable.store(reachable, Ordering::Relaxed);
    }

    pub fn stored_rows(&self) -> Vec<StoredSms> {
        self.network.stored_rows(&self.number)
    }
}

#[async_trait]
impl SmsTransport for FakeSmsTransport {
    async fn send(&self, to: &PhoneNumber, text: &str) -> Result<SmsRowId, TransportError> {
        if !self.reachable.load(Ordering::Relaxed) {
            return Err(TransportError::unreachable("no cell service"));
        }

        let row = self.network.next_row_id();
        let ts = self.network.clock.now_ms();
        {
            let mut guard = self.network.inner.lock().expect("sms network lock");
            let device = guard
                .devices
                .get_mut(&self.number)
                .expect("handset is attached");
            device.storage.insert(
                row.clone(),
                StoredSms {
                    row: row.clone(),
                    peer: to.clone(),
                    text: text.to_owned(),
                    ts,
                    outgoing: true,
                },
            );
        }

        self.network.deliver(&self.number, to, text, ts);
        Ok(row)
    }

    fn drain_inbound(&self) -> Vec<InboundSms> {
        let mut guard = self.network.inner.lock().expect("sms network lock");
        guard
            .devices
            .get_mut(&self.number)
            .map(|d| std::mem::take(&mut d.pending))
            .unwrap_or_default()
    }

    async fn delete_local(&self, row: &SmsRowId) -> Result<(), TransportError> {
        let mut guard = self.network.inner.lock().expect("sms network lock");
        if let Some(device) = guard.devices.get_mut(&self.number) {
            device.storage.remove(row);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kit_arch::block_on;

    fn network() -> (FakeSmsNetwork, FakeSmsTransport, FakeSmsTransport) {
        let net = FakeSmsNetwork::new();
        let alice = net.handset("+33600000001");
        let bob = net.handset("+33600000002");
        (net, alice, bob)
    }

    #[test]
    fn a_sent_message_lands_in_the_peers_inbox_and_storage() {
        let (_net, alice, bob) = network();
        block_on(alice.send(bob.number(), "hello")).unwrap();

        let inbound = bob.drain_inbound();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].text, "hello");
        assert_eq!(inbound[0].from, *alice.number());
        assert_eq!(bob.stored_rows().len(), 1);
    }

    #[test]
    fn draining_is_destructive() {
        let (_net, alice, bob) = network();
        block_on(alice.send(bob.number(), "hello")).unwrap();
        assert_eq!(bob.drain_inbound().len(), 1);
        assert!(bob.drain_inbound().is_empty());
    }

    #[test]
    fn deleting_a_row_removes_it_from_storage_only_on_that_device() {
        let (_net, alice, bob) = network();
        block_on(alice.send(bob.number(), "hello")).unwrap();
        let row = bob.drain_inbound()[0].row.clone();

        block_on(bob.delete_local(&row)).unwrap();

        assert!(bob.stored_rows().is_empty(), "the recipient's row must go");
        assert_eq!(alice.stored_rows().len(), 1, "the sender keeps its own copy");
    }

    #[test]
    fn deleting_an_absent_row_succeeds() {
        let (_net, _alice, bob) = network();
        assert!(block_on(bob.delete_local(&SmsRowId::new("nope"))).is_ok());
    }

    #[test]
    fn no_cell_service_fails_the_send() {
        let (_net, alice, bob) = network();
        alice.set_reachable(false);
        let err = block_on(alice.send(bob.number(), "hello")).unwrap_err();
        assert!(err.is_recoverable());
        assert!(bob.drain_inbound().is_empty());
    }
}
