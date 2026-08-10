use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{ConversationId, MatrixDelivery, MatrixUserId};
use ports::{Clock, InboundMatrixEvent, MatrixSendRequest, MatrixTransport, TransportError};
use schema_matrix::{MessageContent, Via};

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

/* 
 * Need to follow matrix default event and don't implement weird events.*
 * Was just too long for a fake matrix instance... So sort of mock :)
 */
// TODO(freezlex): Use proper matrix event types instead of this custom one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    pub event_id: String,
    pub room: String,
    pub sender: MatrixUserId,
    pub content_json: String,
    pub server_ts: u64,
}

#[derive(Debug, Default)]
struct Server {
    timeline: Vec<StoredEvent>,
    transactions: HashMap<String, String>,
    pending: HashMap<MatrixUserId, Vec<InboundMatrixEvent>>,
}

#[derive(Clone)]
pub struct FakeHomeserver {
    inner: Arc<Mutex<Server>>,
    next_event: Arc<AtomicU64>,
    reachable: Arc<AtomicBool>,
    clock: Arc<dyn Clock>,
}

impl Default for FakeHomeserver {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FakeHomeserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeHomeserver")
            .field("reachable", &self.is_reachable())
            .field("events", &self.inner.lock().map(|g| g.timeline.len()).ok())
            .finish_non_exhaustive()
    }
}

impl FakeHomeserver {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        FakeHomeserver {
            inner: Arc::new(Mutex::new(Server::default())),
            next_event: Arc::new(AtomicU64::new(0)),
            reachable: Arc::new(AtomicBool::new(true)),
            clock,
        }
    }

    pub fn client(&self, user: impl Into<String>) -> FakeMatrixTransport {
        let user = MatrixUserId::new(user);
        self.inner
            .lock()
            .expect("homeserver lock")
            .pending
            .entry(user.clone())
            .or_default();
        FakeMatrixTransport {
            server: self.clone(),
            user,
            connected: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_reachable(&self, reachable: bool) {
        self.reachable.store(reachable, Ordering::Relaxed);
    }

    pub fn is_reachable(&self) -> bool {
        self.reachable.load(Ordering::Relaxed)
    }

    pub fn timeline(&self) -> Vec<StoredEvent> {
        self.inner.lock().expect("homeserver lock").timeline.clone()
    }

    pub fn event_count(&self) -> usize {
        self.inner.lock().expect("homeserver lock").timeline.len()
    }

    fn room_for(a: &MatrixUserId, b: &MatrixUserId) -> String {
        let (lo, hi) = if a.as_str() <= b.as_str() {
            (a.as_str(), b.as_str())
        } else {
            (b.as_str(), a.as_str())
        };
        format!("!{lo}~{hi}")
    }
}

#[derive(Debug, Clone)]
pub struct FakeMatrixTransport {
    server: FakeHomeserver,
    user: MatrixUserId,
    connected: Arc<AtomicBool>,
}

impl FakeMatrixTransport {
    pub fn user(&self) -> &MatrixUserId {
        &self.user
    }

    pub fn homeserver(&self) -> &FakeHomeserver {
        &self.server
    }

    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl MatrixTransport for FakeMatrixTransport {
    async fn send(&self, request: MatrixSendRequest) -> Result<MatrixDelivery, TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::unreachable("no data connection"));
        }
        if !self.server.is_reachable() {
            return Err(TransportError::unreachable("homeserver not reachable"));
        }

        let mut guard = self.server.inner.lock().expect("homeserver lock");

        if let Some(event_id) = guard.transactions.get(&request.txn_id) {
            let server_ts = guard
                .timeline
                .iter()
                .find(|e| &e.event_id == event_id)
                .map(|e| e.server_ts)
                .unwrap_or_default();
            return Ok(MatrixDelivery {
                event_id: event_id.clone(),
                server_ts,
            });
        }

        let content = MessageContent::new(
            request.body.clone(),
            request.message_id,
            request.origin_ts,
            if request.via_sms_fallback {
                Via::SmsFallback
            } else {
                Via::Direct
            },
        );
        let event_id = format!(
            "${:012}",
            self.server.next_event.fetch_add(1, Ordering::Relaxed)
        );
        let server_ts = self.server.clock.now_ms();
        let room = FakeHomeserver::room_for(&self.user, &request.to);

        guard.timeline.push(StoredEvent {
            event_id: event_id.clone(),
            room: room.clone(),
            sender: self.user.clone(),
            content_json: content.to_json(),
            server_ts,
        });
        guard
            .transactions
            .insert(request.txn_id.clone(), event_id.clone());

        if let Some(queue) = guard.pending.get_mut(&request.to) {
            queue.push(InboundMatrixEvent {
                conversation: ConversationId::new(self.user.as_str()),
                sender: self.user.clone(),
                content_json: content.to_json(),
                event_id: event_id.clone(),
                server_ts,
            });
        }

        Ok(MatrixDelivery {
            event_id,
            server_ts,
        })
    }

    fn drain_inbound(&self) -> Vec<InboundMatrixEvent> {
        if !self.connected.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let mut guard = self.server.inner.lock().expect("homeserver lock");
        guard
            .pending
            .get_mut(&self.user)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    fn is_reachable(&self) -> bool {
        self.connected.load(Ordering::Relaxed) && self.server.is_reachable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::MessageId;
    use kit_arch::block_on;
    use schema_matrix::txn_id_for;

    fn request(id: MessageId, to: &MatrixUserId, via_sms_fallback: bool) -> MatrixSendRequest {
        MatrixSendRequest {
            conversation: ConversationId::new(to.as_str()),
            to: to.clone(),
            body: "hi".into(),
            message_id: id,
            origin_ts: 1_000,
            via_sms_fallback,
            txn_id: txn_id_for(id),
        }
    }

    fn pair() -> (FakeHomeserver, FakeMatrixTransport, FakeMatrixTransport) {
        let hs = FakeHomeserver::new();
        let alice = hs.client("@alice:matrix.org");
        let bob = hs.client("@bob:matrix.org");
        (hs, alice, bob)
    }

    #[test]
    fn a_sent_event_reaches_the_other_client() {
        let (_hs, alice, bob) = pair();
        let id = MessageId::from_parts(1_000, 7);
        block_on(alice.send(request(id, bob.user(), false))).unwrap();

        let inbound = bob.drain_inbound();
        assert_eq!(inbound.len(), 1);
        let content = MessageContent::from_json(&inbound[0].content_json).unwrap();
        assert_eq!(content.body, "hi");
        assert_eq!(content.message_id(), Some(id));
        assert_eq!(inbound[0].sender, *alice.user());
    }

    #[test]
    fn the_sender_does_not_receive_its_own_echo() {
        let (_hs, alice, bob) = pair();
        block_on(alice.send(request(MessageId::from_parts(1_000, 7), bob.user(), false))).unwrap();
        assert!(alice.drain_inbound().is_empty());
    }

    #[test]
    fn an_unreachable_homeserver_fails_recoverably() {
        let (hs, alice, bob) = pair();
        hs.set_reachable(false);
        let err =
            block_on(alice.send(request(MessageId::from_parts(1_000, 7), bob.user(), false)))
                .unwrap_err();
        assert!(err.is_recoverable(), "a fallback needs a recoverable error");
        assert_eq!(hs.event_count(), 0);
    }

    #[test]
    fn resending_the_same_transaction_id_does_not_duplicate() {
        let (hs, alice, bob) = pair();
        let id = MessageId::from_parts(1_000, 7);

        let first = block_on(alice.send(request(id, bob.user(), false))).unwrap();
        let second = block_on(alice.send(request(id, bob.user(), true))).unwrap();

        assert_eq!(first.event_id, second.event_id);
        assert_eq!(hs.event_count(), 1);
        assert_eq!(bob.drain_inbound().len(), 1);
    }

    #[test]
    fn recovering_from_an_outage_lets_the_same_message_through() {
        let (hs, alice, bob) = pair();
        let id = MessageId::from_parts(1_000, 7);

        hs.set_reachable(false);
        assert!(block_on(alice.send(request(id, bob.user(), false))).is_err());

        hs.set_reachable(true);
        block_on(alice.send(request(id, bob.user(), true))).unwrap();

        let inbound = bob.drain_inbound();
        assert_eq!(inbound.len(), 1);
        let content = MessageContent::from_json(&inbound[0].content_json).unwrap();
        assert_eq!(content.via(), Via::SmsFallback);
        assert_eq!(
            content.parakeet_origin_ts,
            Some(1_000),
            "a backfill keeps its compose time"
        );
    }

    #[test]
    fn both_participants_agree_on_the_room_name() {
        let a = MatrixUserId::new("@alice:hs");
        let b = MatrixUserId::new("@bob:hs");
        assert_eq!(
            FakeHomeserver::room_for(&a, &b),
            FakeHomeserver::room_for(&b, &a)
        );
    }
}
