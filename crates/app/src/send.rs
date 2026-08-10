use domain::{
    next_action, sms_frame, ConversationId, DeliverySet, Direction, MatrixDelivery, Message,
    MessageId, MessageState, NetStatus, OutboxEntry, PeerId, SendDecision, SmsDelivery,
    SmsLocalCopy,
};
use ports::{MatrixSendRequest, TransportError};
use schema_matrix::txn_id_for;

use crate::activity::Activity;
use crate::client::Client;

const MAX_STEPS_PER_ENTRY: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PumpReport {
    pub sent_via_matrix: usize,
    pub fell_back_to_sms: usize,
    pub backfilled: usize,
    pub failed: usize,
    pub still_queued: usize,
}

impl Client {
    pub async fn send(&self, peer: &PeerId, body: &str) -> MessageId {
        let id = self.ids.mint();
        let now = self.clock.now_ms();
        let conversation = ConversationId::with_peer(peer);

        self.store.upsert(Message {
            id,
            tag: id.short_tag(),
            conversation: conversation.clone(),
            author: self.me.clone(),
            direction: Direction::Outgoing,
            body: body.to_owned(),
            origin_ts: now,
            delivery: DeliverySet::default(),
            state: MessageState::Pending,
        });
        self.store.enqueue(OutboxEntry::new(
            id,
            conversation,
            peer.clone(),
            body.to_owned(),
            now,
        ));
        self.log.record(Activity::Composed {
            id,
            body: body.to_owned(),
        });

        self.pump().await;
        id
    }

    pub async fn pump(&self) -> PumpReport {
        let mut report = PumpReport::default();
        for entry in self.store.outbox() {
            self.pump_entry(entry, &mut report).await;
        }
        report.still_queued = self.store.outbox().len();
        report
    }

    async fn pump_entry(&self, mut entry: OutboxEntry, report: &mut PumpReport) {
        for _ in 0..MAX_STEPS_PER_ENTRY {
            let decision = next_action(&entry, self.net.status(), self.clock.now_ms());
            match decision {
                SendDecision::Done => {
                    self.store.retire(&entry.id);
                    return;
                }

                SendDecision::Wait { .. } => {
                    self.store.update_outbox(entry);
                    return;
                }

                SendDecision::Abandon => {
                    let reason = "no transport could deliver this message".to_owned();
                    self.mark_failed(&entry.id, &reason);
                    self.store.retire(&entry.id);
                    self.log.record(Activity::SendFailed {
                        id: entry.id,
                        reason,
                    });
                    report.failed += 1;
                    return;
                }

                SendDecision::TryMatrix | SendDecision::BackfillMatrix => {
                    let is_backfill = decision == SendDecision::BackfillMatrix;
                    entry.begin_matrix();
                    self.store.update_outbox(entry.clone());

                    match self.deliver_over_matrix(&entry, is_backfill).await {
                        Ok(delivery) => {
                            entry.matrix_succeeded();
                            self.attach_matrix_delivery(&entry.id, delivery);
                            self.net.observe(NetStatus::Online);
                            if is_backfill {
                                self.log.record(Activity::Backfilled { id: entry.id });
                                report.backfilled += 1;
                            } else {
                                self.log.record(Activity::SentViaMatrix { id: entry.id });
                                report.sent_via_matrix += 1;
                            }
                        }
                        Err(err) => {
                            entry.matrix_failed(self.clock.now_ms());
                            if err.is_recoverable() {
                                self.net.observe(NetStatus::Offline);
                            }
                        }
                    }
                    self.store.update_outbox(entry.clone());
                }

                SendDecision::FallbackToSms => {
                    entry.begin_sms();
                    self.store.update_outbox(entry.clone());

                    match self.deliver_over_sms(&entry).await {
                        Ok(delivery) => {
                            entry.sms_succeeded();
                            self.attach_sms_delivery(&entry.id, delivery);
                            self.log.record(Activity::FellBackToSms {
                                id: entry.id,
                                reason: "matrix unavailable".to_owned(),
                            });
                            report.fell_back_to_sms += 1;
                        }
                        Err(err) => {
                            entry.sms_failed();
                            self.log.record(Activity::SendFailed {
                                id: entry.id,
                                reason: err.to_string(),
                            });
                        }
                    }
                    self.store.update_outbox(entry.clone());
                }
            }
        }
        self.store.update_outbox(entry);
    }

    async fn deliver_over_matrix(
        &self,
        entry: &OutboxEntry,
        is_backfill: bool,
    ) -> Result<MatrixDelivery, TransportError> {
        let to = self
            .directory
            .matrix_id(&entry.peer)
            .ok_or_else(|| TransportError::rejected("no matrix id for this contact"))?;

        self.matrix
            .send(MatrixSendRequest {
                conversation: entry.conversation.clone(),
                to,
                body: entry.body.clone(),
                message_id: entry.id,
                origin_ts: entry.origin_ts,
                via_sms_fallback: is_backfill,
                txn_id: txn_id_for(entry.id),
            })
            .await
    }

    async fn deliver_over_sms(&self, entry: &OutboxEntry) -> Result<SmsDelivery, TransportError> {
        let to = self
            .directory
            .phone(&entry.peer)
            .ok_or_else(|| TransportError::rejected("no phone number for this contact"))?;

        let text = sms_frame::encode(&entry.body, entry.id.short_tag());
        let row = self.sms.send(&to, &text).await?;

        Ok(SmsDelivery {
            row,
            ts: self.clock.now_ms(),
            local_copy: SmsLocalCopy::Present,
        })
    }

    fn attach_matrix_delivery(&self, id: &MessageId, delivery: MatrixDelivery) {
        let Some(mut message) = self.store.get(id) else {
            return;
        };
        message.delivery.matrix = Some(delivery);
        message.state = MessageState::Sent;
        self.store.upsert(message);
    }

    fn attach_sms_delivery(&self, id: &MessageId, delivery: SmsDelivery) {
        let Some(mut message) = self.store.get(id) else {
            return;
        };
        message.delivery.sms = Some(delivery);
        message.state = MessageState::Sent;
        self.store.upsert(message);
    }

    fn mark_failed(&self, id: &MessageId, reason: &str) {
        let Some(mut message) = self.store.get(id) else {
            return;
        };
        message.state = MessageState::Failed(reason.to_owned());
        self.store.upsert(message);
    }
}
