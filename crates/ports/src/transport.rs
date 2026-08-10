use async_trait::async_trait;
use domain::{
    ConversationId, MatrixDelivery, MatrixUserId, MessageId, PhoneNumber, SmsRowId,
};

use crate::error::TransportError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixSendRequest {
    pub conversation: ConversationId,
    pub to: MatrixUserId,
    pub body: String,
    pub message_id: MessageId,
    pub origin_ts: u64,
    pub via_sms_fallback: bool,
    pub txn_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMatrixEvent {
    pub conversation: ConversationId,
    pub sender: MatrixUserId,
    pub content_json: String,
    pub event_id: String,
    pub server_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundSms {
    pub from: PhoneNumber,
    pub text: String,
    pub received_ts: u64,
    pub row: SmsRowId,
}

#[async_trait]
pub trait MatrixTransport: Send + Sync {
    async fn send(&self, request: MatrixSendRequest) -> Result<MatrixDelivery, TransportError>;
    fn drain_inbound(&self) -> Vec<InboundMatrixEvent>;
    fn is_reachable(&self) -> bool;
}

#[async_trait]
pub trait SmsTransport: Send + Sync {
    async fn send(&self, to: &PhoneNumber, text: &str) -> Result<SmsRowId, TransportError>;
    fn drain_inbound(&self) -> Vec<InboundSms>;
    async fn delete_local(&self, row: &SmsRowId) -> Result<(), TransportError>;
}
