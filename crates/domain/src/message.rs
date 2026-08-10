use crate::id::{MessageId, ShortTag};
use crate::peer::{ConversationId, PeerId, SmsRowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    Matrix,
    Sms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixDelivery {
    pub event_id: String,
    pub server_ts: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsLocalCopy {
    Present,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmsDelivery {
    pub row: SmsRowId,
    pub ts: u64,
    pub local_copy: SmsLocalCopy,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeliverySet {
    pub matrix: Option<MatrixDelivery>,
    pub sms: Option<SmsDelivery>,
}

impl DeliverySet {
    pub fn matrix_only(delivery: MatrixDelivery) -> Self {
        DeliverySet {
            matrix: Some(delivery),
            sms: None,
        }
    }

    pub fn sms_only(delivery: SmsDelivery) -> Self {
        DeliverySet {
            matrix: None,
            sms: Some(delivery),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.matrix.is_none() && self.sms.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageState {
    Pending,
    Sent,
    Delivered,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub tag: ShortTag,
    pub conversation: ConversationId,
    pub author: PeerId,
    pub direction: Direction,
    pub body: String,
    pub origin_ts: u64,
    pub delivery: DeliverySet,
    pub state: MessageState,
}

impl Message {
    pub fn canonical_transport(&self) -> Option<Transport> {
        match (&self.delivery.matrix, &self.delivery.sms) {
            (Some(_), _) => Some(Transport::Matrix),
            (None, Some(_)) => Some(Transport::Sms),
            (None, None) => None,
        }
    }

    pub fn was_reconciled(&self) -> bool {
        self.delivery.matrix.is_some() && self.delivery.sms.is_some()
    }

    pub fn has_local_sms_copy(&self) -> bool {
        matches!(
            self.delivery.sms,
            Some(SmsDelivery {
                local_copy: SmsLocalCopy::Present,
                ..
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(delivery: DeliverySet) -> Message {
        let id = MessageId::from_parts(1_000, 7);
        Message {
            id,
            tag: id.short_tag(),
            conversation: ConversationId::new("bob"),
            author: PeerId::new("bob"),
            direction: Direction::Incoming,
            body: "hi".into(),
            origin_ts: 1_000,
            delivery,
            state: MessageState::Delivered,
        }
    }

    fn matrix() -> MatrixDelivery {
        MatrixDelivery {
            event_id: "$evt".into(),
            server_ts: 2_000,
        }
    }

    fn sms(local_copy: SmsLocalCopy) -> SmsDelivery {
        SmsDelivery {
            row: SmsRowId::new("row-1"),
            ts: 1_000,
            local_copy,
        }
    }

    #[test]
    fn matrix_outranks_sms_when_both_are_present() {
        let m = message(DeliverySet {
            matrix: Some(matrix()),
            sms: Some(sms(SmsLocalCopy::Removed)),
        });
        assert_eq!(m.canonical_transport(), Some(Transport::Matrix));
        assert!(m.was_reconciled());
    }

    #[test]
    fn sms_is_canonical_on_its_own() {
        let m = message(DeliverySet::sms_only(sms(SmsLocalCopy::Present)));
        assert_eq!(m.canonical_transport(), Some(Transport::Sms));
        assert!(!m.was_reconciled());
        assert!(m.has_local_sms_copy());
    }

    #[test]
    fn a_pending_message_has_no_canonical_transport() {
        let m = message(DeliverySet::default());
        assert_eq!(m.canonical_transport(), None);
        assert!(m.delivery.is_empty());
    }

    #[test]
    fn a_removed_row_no_longer_counts_as_a_local_copy() {
        let m = message(DeliverySet {
            matrix: Some(matrix()),
            sms: Some(sms(SmsLocalCopy::Removed)),
        });
        assert!(!m.has_local_sms_copy());
    }
}
