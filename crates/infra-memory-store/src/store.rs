use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::{ConversationId, Message, MessageId, OutboxEntry, ShortTag};
use ports::MessageStore;

#[derive(Debug, Default)]
struct Inner {
    messages: HashMap<MessageId, Message>,

    by_tag: HashMap<(ConversationId, ShortTag), MessageId>,

    outbox: Vec<OutboxEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    inner: Arc<Mutex<Inner>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("store lock").messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MessageStore for MemoryStore {
    fn upsert(&self, message: Message) {
        let mut guard = self.inner.lock().expect("store lock");

        if let Some(previous) = guard.messages.get(&message.id)
            && (previous.tag != message.tag || previous.conversation != message.conversation)
        {
            let stale = (previous.conversation.clone(), previous.tag);
            guard.by_tag.remove(&stale);
        }

        guard.by_tag.insert(
            (message.conversation.clone(), message.tag),
            message.id,
        );
        guard.messages.insert(message.id, message);
    }

    fn get(&self, id: &MessageId) -> Option<Message> {
        self.inner
            .lock()
            .expect("store lock")
            .messages
            .get(id)
            .cloned()
    }

    fn find_by_tag(&self, conversation: &ConversationId, tag: ShortTag) -> Option<Message> {
        let guard = self.inner.lock().expect("store lock");
        let id = guard.by_tag.get(&(conversation.clone(), tag))?;
        guard.messages.get(id).cloned()
    }

    fn thread(&self, conversation: &ConversationId) -> Vec<Message> {
        let guard = self.inner.lock().expect("store lock");
        let mut messages: Vec<_> = guard
            .messages
            .values()
            .filter(|m| &m.conversation == conversation)
            .cloned()
            .collect();
        messages.sort_by(|a, b| a.origin_ts.cmp(&b.origin_ts).then(a.id.cmp(&b.id)));
        messages
    }

    fn conversations(&self) -> Vec<ConversationId> {
        let guard = self.inner.lock().expect("store lock");
        let mut latest: HashMap<ConversationId, u64> = HashMap::new();
        for message in guard.messages.values() {
            let entry = latest.entry(message.conversation.clone()).or_default();
            *entry = (*entry).max(message.origin_ts);
        }
        let mut conversations: Vec<_> = latest.into_iter().collect();
        conversations.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        conversations.into_iter().map(|(id, _)| id).collect()
    }

    fn enqueue(&self, entry: OutboxEntry) {
        let mut guard = self.inner.lock().expect("store lock");
        if let Some(slot) = guard.outbox.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        } else {
            guard.outbox.push(entry);
        }
    }

    fn outbox(&self) -> Vec<OutboxEntry> {
        self.inner.lock().expect("store lock").outbox.clone()
    }

    fn update_outbox(&self, entry: OutboxEntry) {
        let mut guard = self.inner.lock().expect("store lock");
        if let Some(slot) = guard.outbox.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        }
    }

    fn retire(&self, id: &MessageId) {
        self.inner
            .lock()
            .expect("store lock")
            .outbox
            .retain(|e| &e.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        DeliverySet, Direction, MatrixDelivery, MessageState, PeerId, SmsDelivery, SmsLocalCopy,
        SmsRowId,
    };

    fn message(id: u128, conversation: &str, origin_ts: u64) -> Message {
        let id = MessageId::from_parts(origin_ts, id);
        Message {
            id,
            tag: id.short_tag(),
            conversation: ConversationId::new(conversation),
            author: PeerId::new(conversation),
            direction: Direction::Incoming,
            body: format!("message {origin_ts}"),
            origin_ts,
            delivery: DeliverySet::default(),
            state: MessageState::Delivered,
        }
    }

    #[test]
    fn a_message_is_findable_by_id_and_by_tag() {
        let store = MemoryStore::new();
        let m = message(1, "bob", 1_000);
        store.upsert(m.clone());

        assert_eq!(store.get(&m.id), Some(m.clone()));
        assert_eq!(store.find_by_tag(&m.conversation, m.tag), Some(m));
    }

    #[test]
    fn a_tag_from_another_conversation_does_not_match() {
        let store = MemoryStore::new();
        let m = message(1, "bob", 1_000);
        store.upsert(m.clone());
        assert_eq!(store.find_by_tag(&ConversationId::new("carol"), m.tag), None);
    }

    #[test]
    fn threads_are_ordered_by_compose_time_not_arrival() {
        let store = MemoryStore::new();
        let late_arrival = {
            let mut m = message(1, "bob", 1_000);
            m.delivery = DeliverySet::matrix_only(MatrixDelivery {
                event_id: "$late".into(),
                server_ts: 9_999_999,
            });
            m
        };
        store.upsert(message(2, "bob", 3_000));
        store.upsert(late_arrival);
        store.upsert(message(3, "bob", 2_000));

        let bodies: Vec<_> = store
            .thread(&ConversationId::new("bob"))
            .into_iter()
            .map(|m| m.origin_ts)
            .collect();
        assert_eq!(bodies, vec![1_000, 2_000, 3_000]);
    }

    #[test]
    fn upserting_replaces_rather_than_duplicates() {
        let store = MemoryStore::new();
        let mut m = message(1, "bob", 1_000);
        store.upsert(m.clone());

        m.delivery = DeliverySet::sms_only(SmsDelivery {
            row: SmsRowId::new("row-1"),
            ts: 1_000,
            local_copy: SmsLocalCopy::Present,
        });
        store.upsert(m.clone());

        assert_eq!(store.len(), 1);
        assert_eq!(store.thread(&m.conversation).len(), 1);
        assert!(store.get(&m.id).unwrap().has_local_sms_copy());
    }

    #[test]
    fn conversations_are_listed_most_recent_first() {
        let store = MemoryStore::new();
        store.upsert(message(1, "bob", 1_000));
        store.upsert(message(2, "carol", 5_000));
        store.upsert(message(3, "dave", 3_000));

        assert_eq!(
            store.conversations(),
            vec![
                ConversationId::new("carol"),
                ConversationId::new("dave"),
                ConversationId::new("bob"),
            ]
        );
    }

    #[test]
    fn the_outbox_keeps_insertion_order_and_deduplicates() {
        let store = MemoryStore::new();
        let entries: Vec<_> = (0..3)
            .map(|i| {
                OutboxEntry::new(
                    MessageId::from_parts(1_000 + i, 7),
                    ConversationId::new("bob"),
                    PeerId::new("bob"),
                    format!("m{i}"),
                    1_000 + i,
                )
            })
            .collect();
        for e in &entries {
            store.enqueue(e.clone());
        }
        store.enqueue(entries[0].clone());

        let queued: Vec<_> = store.outbox().into_iter().map(|e| e.body).collect();
        assert_eq!(queued, vec!["m0", "m1", "m2"]);
    }

    #[test]
    fn updating_an_entry_preserves_its_position() {
        let store = MemoryStore::new();
        let mut first = OutboxEntry::new(
            MessageId::from_parts(1_000, 7),
            ConversationId::new("bob"),
            PeerId::new("bob"),
            "first".into(),
            1_000,
        );
        let second = OutboxEntry::new(
            MessageId::from_parts(2_000, 7),
            ConversationId::new("bob"),
            PeerId::new("bob"),
            "second".into(),
            2_000,
        );
        store.enqueue(first.clone());
        store.enqueue(second);

        first.begin_matrix();
        first.matrix_failed(1_000);
        store.update_outbox(first.clone());

        let queue = store.outbox();
        assert_eq!(queue[0].body, "first");
        assert_eq!(queue[0].matrix, domain::TransportState::Failed);
    }

    #[test]
    fn retiring_removes_only_the_named_entry() {
        let store = MemoryStore::new();
        let a = OutboxEntry::new(
            MessageId::from_parts(1_000, 7),
            ConversationId::new("bob"),
            PeerId::new("bob"),
            "a".into(),
            1_000,
        );
        let b = OutboxEntry::new(
            MessageId::from_parts(2_000, 7),
            ConversationId::new("bob"),
            PeerId::new("bob"),
            "b".into(),
            2_000,
        );
        store.enqueue(a.clone());
        store.enqueue(b.clone());

        store.retire(&a.id);

        assert_eq!(store.outbox().len(), 1);
        assert_eq!(store.outbox()[0].id, b.id);
    }

    #[test]
    fn changing_a_records_tag_drops_the_stale_index_entry() {
        let store = MemoryStore::new();
        let mut m = message(1, "bob", 1_000);
        let old_tag = m.tag;
        store.upsert(m.clone());

        m.tag = ShortTag::from_u64(0xFFFF_FFFF_FFFF_FFFF);
        store.upsert(m.clone());

        assert_eq!(store.find_by_tag(&m.conversation, old_tag), None);
        assert_eq!(store.find_by_tag(&m.conversation, m.tag), Some(m));
    }
}
