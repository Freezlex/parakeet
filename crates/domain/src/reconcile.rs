use crate::id::{MessageId, ShortTag};
use crate::message::{
    DeliverySet, Direction, MatrixDelivery, Message, MessageState, SmsDelivery, SmsLocalCopy,
};
use crate::peer::{ConversationId, PeerId, SmsRowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMatrix {
    pub conversation: ConversationId,
    pub author: PeerId,
    pub body: String,

    pub origin_ts: u64,

    pub tag: ShortTag,
    pub delivery: MatrixDelivery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSms {
    pub conversation: ConversationId,
    pub author: PeerId,

    pub body: String,

    pub origin_ts: u64,

    pub tag: ShortTag,
    pub delivery: SmsDelivery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Matrix(IncomingMatrix),
    Sms(IncomingSms),
}

impl Incoming {
    pub fn conversation(&self) -> &ConversationId {
        match self {
            Incoming::Matrix(m) => &m.conversation,
            Incoming::Sms(s) => &s.conversation,
        }
    }

    pub fn tag(&self) -> ShortTag {
        match self {
            Incoming::Matrix(m) => m.tag,
            Incoming::Sms(s) => s.tag,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    Insert(Message),

    Supersede {
        keep: MessageId,
        matrix: MatrixDelivery,
        drop_sms: SmsRowId,
    },

    AttachMatrix {
        keep: MessageId,
        matrix: MatrixDelivery,
    },

    AttachSms { keep: MessageId, sms: SmsDelivery },

    Ignore,
}

pub fn reconcile(
    incoming: &Incoming,
    existing: Option<&Message>,
    local_id: MessageId,
    direction: Direction,
) -> Reconciliation {
    let existing = existing.filter(|m| &m.conversation == incoming.conversation());

    match (incoming, existing) {
        (_, None) => Reconciliation::Insert(new_record(incoming, local_id, direction)),

        (Incoming::Matrix(m), Some(known)) => match (&known.delivery.matrix, &known.delivery.sms) {
            (Some(_), _) => Reconciliation::Ignore,
            (None, Some(sms)) => Reconciliation::Supersede {
                keep: known.id,
                matrix: m.delivery.clone(),
                drop_sms: sms.row.clone(),
            },
            (None, None) => Reconciliation::AttachMatrix {
                keep: known.id,
                matrix: m.delivery.clone(),
            },
        },

        (Incoming::Sms(s), Some(known)) => {
            if known.delivery.sms.is_some() {
                return Reconciliation::Ignore;
            }
            let local_copy = if known.delivery.matrix.is_some() {
                SmsLocalCopy::Removed
            } else {
                SmsLocalCopy::Present
            };
            Reconciliation::AttachSms {
                keep: known.id,
                sms: SmsDelivery {
                    local_copy,
                    ..s.delivery.clone()
                },
            }
        }
    }
}

fn new_record(incoming: &Incoming, local_id: MessageId, direction: Direction) -> Message {
    match incoming {
        Incoming::Matrix(m) => Message {
            id: local_id,
            tag: m.tag,
            conversation: m.conversation.clone(),
            author: m.author.clone(),
            direction,
            body: m.body.clone(),
            origin_ts: m.origin_ts,
            delivery: DeliverySet::matrix_only(m.delivery.clone()),
            state: MessageState::Delivered,
        },
        Incoming::Sms(s) => Message {
            id: local_id,
            tag: s.tag,
            conversation: s.conversation.clone(),
            author: s.author.clone(),
            direction,
            body: s.body.clone(),
            origin_ts: s.origin_ts,
            delivery: DeliverySet::sms_only(s.delivery.clone()),
            state: MessageState::Delivered,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation() -> ConversationId {
        ConversationId::new("bob")
    }

    fn tag() -> ShortTag {
        ShortTag::from_u64(0xDEAD_BEEF_CAFE_F00D)
    }

    fn local_id() -> MessageId {
        MessageId::from_parts(9_000, 1)
    }

    fn matrix_delivery() -> MatrixDelivery {
        MatrixDelivery {
            event_id: "$evt".into(),
            server_ts: 9_000,
        }
    }

    fn sms_delivery() -> SmsDelivery {
        SmsDelivery {
            row: SmsRowId::new("row-1"),
            ts: 1_000,
            local_copy: SmsLocalCopy::Present,
        }
    }

    fn incoming_matrix() -> Incoming {
        Incoming::Matrix(IncomingMatrix {
            conversation: conversation(),
            author: PeerId::new("bob"),
            body: "hi".into(),
            origin_ts: 1_000,
            tag: tag(),
            delivery: matrix_delivery(),
        })
    }

    fn incoming_sms() -> Incoming {
        Incoming::Sms(IncomingSms {
            conversation: conversation(),
            author: PeerId::new("bob"),
            body: "hi".into(),
            origin_ts: 1_000,
            tag: tag(),
            delivery: sms_delivery(),
        })
    }

    fn stored(delivery: DeliverySet) -> Message {
        Message {
            id: MessageId::from_parts(1_000, 42),
            tag: tag(),
            conversation: conversation(),
            author: PeerId::new("bob"),
            direction: Direction::Incoming,
            body: "hi".into(),
            origin_ts: 1_000,
            delivery,
            state: MessageState::Delivered,
        }
    }

    #[test]
    fn an_unknown_tag_inserts_a_new_message() {
        let r = reconcile(&incoming_sms(), None, local_id(), Direction::Incoming);
        let Reconciliation::Insert(m) = r else {
            panic!("expected Insert, got {r:?}");
        };
        assert_eq!(m.id, local_id());
        assert_eq!(m.tag, tag());
        assert_eq!(m.body, "hi");
        assert_eq!(m.delivery.sms.map(|s| s.row), Some(SmsRowId::new("row-1")));
    }

    #[test]
    fn matrix_after_sms_supersedes_and_drops_the_row() {
        let known = stored(DeliverySet::sms_only(sms_delivery()));
        let r = reconcile(
            &incoming_matrix(),
            Some(&known),
            local_id(),
            Direction::Incoming,
        );
        assert_eq!(
            r,
            Reconciliation::Supersede {
                keep: known.id,
                matrix: matrix_delivery(),
                drop_sms: SmsRowId::new("row-1"),
            }
        );
    }

    #[test]
    fn sms_after_matrix_attaches_and_asks_for_the_row_to_go() {
        let known = stored(DeliverySet::matrix_only(matrix_delivery()));
        let r = reconcile(&incoming_sms(), Some(&known), local_id(), Direction::Incoming);
        let Reconciliation::AttachSms { keep, sms } = r else {
            panic!("expected AttachSms, got {r:?}");
        };
        assert_eq!(keep, known.id);
        assert_eq!(sms.local_copy, SmsLocalCopy::Removed);
        assert_eq!(sms.row, SmsRowId::new("row-1"));
    }

    #[test]
    fn a_repeated_matrix_event_is_ignored() {
        let known = stored(DeliverySet::matrix_only(matrix_delivery()));
        let r = reconcile(
            &incoming_matrix(),
            Some(&known),
            local_id(),
            Direction::Incoming,
        );
        assert_eq!(r, Reconciliation::Ignore);
    }

    #[test]
    fn a_repeated_sms_is_ignored() {
        let known = stored(DeliverySet::sms_only(sms_delivery()));
        let r = reconcile(&incoming_sms(), Some(&known), local_id(), Direction::Incoming);
        assert_eq!(r, Reconciliation::Ignore);
    }

    #[test]
    fn a_reconciled_message_ignores_further_arrivals_on_both_transports() {
        let known = stored(DeliverySet {
            matrix: Some(matrix_delivery()),
            sms: Some(SmsDelivery {
                local_copy: SmsLocalCopy::Removed,
                ..sms_delivery()
            }),
        });
        for incoming in [incoming_matrix(), incoming_sms()] {
            let r = reconcile(&incoming, Some(&known), local_id(), Direction::Incoming);
            assert_eq!(r, Reconciliation::Ignore);
        }
    }

    #[test]
    fn a_sync_echo_attaches_to_our_own_pending_message() {
        let mut pending = stored(DeliverySet::default());
        pending.direction = Direction::Outgoing;
        pending.state = MessageState::Pending;
        let r = reconcile(
            &incoming_matrix(),
            Some(&pending),
            local_id(),
            Direction::Outgoing,
        );
        assert_eq!(
            r,
            Reconciliation::AttachMatrix {
                keep: pending.id,
                matrix: matrix_delivery(),
            }
        );
    }

    #[test]
    fn the_same_tag_in_another_thread_is_a_different_message() {
        let mut elsewhere = stored(DeliverySet::sms_only(sms_delivery()));
        elsewhere.conversation = ConversationId::new("carol");
        let r = reconcile(
            &incoming_matrix(),
            Some(&elsewhere),
            local_id(),
            Direction::Incoming,
        );
        assert!(matches!(r, Reconciliation::Insert(_)));
    }

    #[test]
    fn a_plain_sms_with_a_minted_tag_just_inserts() {
        let Incoming::Sms(mut s) = incoming_sms() else {
            unreachable!()
        };
        s.tag = ShortTag::from_u64(0x1234_5678_9ABC_DEF0);
        let r = reconcile(&Incoming::Sms(s), None, local_id(), Direction::Incoming);
        assert!(matches!(r, Reconciliation::Insert(_)));
    }
}
