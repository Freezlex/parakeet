use domain::{ConversationId, Message, MessageId, OutboxEntry, ShortTag};

pub trait MessageStore: Send + Sync {
    fn upsert(&self, message: Message);
    fn get(&self, id: &MessageId) -> Option<Message>;
    fn find_by_tag(&self, conversation: &ConversationId, tag: ShortTag) -> Option<Message>;
    fn thread(&self, conversation: &ConversationId) -> Vec<Message>;
    fn conversations(&self) -> Vec<ConversationId>;
    fn enqueue(&self, entry: OutboxEntry);
    fn outbox(&self) -> Vec<OutboxEntry>;
    fn update_outbox(&self, entry: OutboxEntry);
    fn retire(&self, id: &MessageId);
}
