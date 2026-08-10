use domain::{ConversationId, Direction, Message, MessageId, MessageState, PeerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportBadge {
    Pending,
    Matrix,
    Sms,
    Reconciled,
    Failed,
}

impl TransportBadge {
    pub fn label(self) -> &'static str {
        match self {
            TransportBadge::Pending => "pending",
            TransportBadge::Matrix => "matrix",
            TransportBadge::Sms => "sms",
            TransportBadge::Reconciled => "reconciled",
            TransportBadge::Failed => "failed",
        }
    }

    pub fn css_class(self) -> &'static str {
        self.label()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageView {
    pub id: MessageId,
    pub body: String,
    pub origin_ts: u64,
    pub direction: Direction,
    pub badge: TransportBadge,

    pub tag: String,

    pub failure: Option<String>,
}

impl From<&Message> for MessageView {
    fn from(message: &Message) -> Self {
        MessageView {
            id: message.id,
            body: message.body.clone(),
            origin_ts: message.origin_ts,
            direction: message.direction,
            badge: badge_for(message),
            tag: message.tag.to_base32(),
            failure: match &message.state {
                MessageState::Failed(reason) => Some(reason.clone()),
                _ => None,
            },
        }
    }
}

fn badge_for(message: &Message) -> TransportBadge {
    if let MessageState::Failed(_) = message.state {
        return TransportBadge::Failed;
    }
    match (&message.delivery.matrix, &message.delivery.sms) {
        (Some(_), Some(_)) => TransportBadge::Reconciled,
        (Some(_), None) => TransportBadge::Matrix,
        (None, Some(_)) => TransportBadge::Sms,
        (None, None) => TransportBadge::Pending,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub peer: PeerId,
    pub display_name: String,
    pub last_body: String,
    pub last_ts: u64,
    pub pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        DeliverySet, MatrixDelivery, MessageId, ShortTag, SmsDelivery, SmsLocalCopy, SmsRowId,
    };

    fn message(delivery: DeliverySet, state: MessageState) -> Message {
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
            state,
        }
    }

    fn matrix() -> MatrixDelivery {
        MatrixDelivery {
            event_id: "$e".into(),
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
    fn badges_follow_the_delivery_set() {
        let cases = [
            (DeliverySet::default(), TransportBadge::Pending),
            (
                DeliverySet::matrix_only(matrix()),
                TransportBadge::Matrix,
            ),
            (
                DeliverySet::sms_only(sms(SmsLocalCopy::Present)),
                TransportBadge::Sms,
            ),
            (
                DeliverySet {
                    matrix: Some(matrix()),
                    sms: Some(sms(SmsLocalCopy::Removed)),
                },
                TransportBadge::Reconciled,
            ),
        ];
        for (delivery, expected) in cases {
            let view = MessageView::from(&message(delivery, MessageState::Delivered));
            assert_eq!(view.badge, expected);
        }
    }

    #[test]
    fn a_failed_message_shows_its_reason_whatever_the_delivery_set_says() {
        let view = MessageView::from(&message(
            DeliverySet::default(),
            MessageState::Failed("no cell service".into()),
        ));
        assert_eq!(view.badge, TransportBadge::Failed);
        assert_eq!(view.failure.as_deref(), Some("no cell service"));
    }

    #[test]
    fn the_view_carries_the_tag_in_its_sms_form() {
        let m = message(DeliverySet::default(), MessageState::Pending);
        let view = MessageView::from(&m);
        assert_eq!(view.tag, m.tag.to_base32());
        assert_eq!(view.tag.len(), ShortTag::from_u64(0).to_base32().len());
    }
}
