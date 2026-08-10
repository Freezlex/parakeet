use domain::{
    reconcile, ConversationId, Direction, Incoming, IncomingMatrix, IncomingSms, PeerId,
    Reconciliation, SmsDelivery, SmsLocalCopy,
};
use ports::{InboundMatrixEvent, InboundSms};
use schema_matrix::MessageContent;

use crate::activity::Activity;
use crate::client::Client;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestReport {
    pub inserted: usize,
    pub superseded: usize,
    pub ignored: usize,
}

impl Client {
    pub async fn ingest(&self) -> IngestReport {
        let mut report = IngestReport::default();

        for event in self.matrix.drain_inbound() {
            self.ingest_matrix(event, &mut report).await;
        }
        for sms in self.sms.drain_inbound() {
            self.ingest_sms(sms, &mut report).await;
        }

        report
    }

    pub async fn sync(&self) -> (IngestReport, crate::send::PumpReport) {
        let ingested = self.ingest().await;
        let pumped = self.pump().await;
        (ingested, pumped)
    }

    async fn ingest_matrix(&self, event: InboundMatrixEvent, report: &mut IngestReport) {
        let Ok(content) = MessageContent::from_json(&event.content_json) else {
            return;
        };

        let peer = self
            .directory
            .peer_by_matrix_id(&event.sender)
            .unwrap_or_else(|| PeerId::new(event.sender.as_str()));
        let conversation = ConversationId::with_peer(&peer);

        let tag = content
            .message_id()
            .map(|id| id.short_tag())
            .unwrap_or_else(|| self.ids.mint_tag());

        let incoming = Incoming::Matrix(IncomingMatrix {
            conversation: conversation.clone(),
            author: peer.clone(),
            body: content.body.clone(),
            origin_ts: content.origin_ts_or(event.server_ts),
            tag,
            delivery: domain::MatrixDelivery {
                event_id: event.event_id,
                server_ts: event.server_ts,
            },
        });

        let direction = if peer == self.me {
            Direction::Outgoing
        } else {
            Direction::Incoming
        };
        self.apply(incoming, direction, report).await;
    }

    async fn ingest_sms(&self, sms: InboundSms, report: &mut IngestReport) {
        let framed = domain::sms_frame::decode(&sms.text);

        let peer = self
            .directory
            .peer_by_phone(&sms.from)
            .unwrap_or_else(|| PeerId::new(sms.from.as_str()));
        let conversation = ConversationId::with_peer(&peer);

        let from_app = framed.tag.is_some();
        let tag = framed.tag.unwrap_or_else(|| self.ids.mint_tag());
        self.log.record(Activity::ReceivedSms { tag, from_app });

        let incoming = Incoming::Sms(IncomingSms {
            conversation,
            author: peer,
            body: framed.body,
            origin_ts: sms.received_ts,
            tag,
            delivery: SmsDelivery {
                row: sms.row,
                ts: sms.received_ts,
                local_copy: SmsLocalCopy::Present,
            },
        });

        self.apply(incoming, Direction::Incoming, report).await;
    }

    async fn apply(&self, incoming: Incoming, direction: Direction, report: &mut IngestReport) {
        let conversation = incoming.conversation().clone();
        let tag = incoming.tag();
        let existing = self.store.find_by_tag(&conversation, tag);

        match reconcile(&incoming, existing.as_ref(), self.ids.mint(), direction) {
            Reconciliation::Insert(message) => {
                if matches!(incoming, Incoming::Matrix(_)) {
                    self.log.record(Activity::ReceivedMatrix { tag });
                }
                self.store.upsert(message);
                report.inserted += 1;
            }

            Reconciliation::AttachMatrix { keep, matrix } => {
                self.log.record(Activity::ReceivedMatrix { tag });
                if let Some(mut message) = self.store.get(&keep) {
                    message.delivery.matrix = Some(matrix);
                    self.store.upsert(message);
                }
            }

            Reconciliation::Supersede {
                keep,
                matrix,
                drop_sms,
            } => {
                self.log.record(Activity::ReceivedMatrix { tag });
                let Some(mut message) = self.store.get(&keep) else {
                    return;
                };
                message.delivery.matrix = Some(matrix);

                match self.sms.delete_local(&drop_sms).await {
                    Ok(()) => {
                        if let Some(sms) = message.delivery.sms.as_mut() {
                            sms.local_copy = SmsLocalCopy::Removed;
                        }
                        self.log.record(Activity::Superseded {
                            id: keep,
                            dropped_row: drop_sms,
                        });
                        report.superseded += 1;
                    }
                    Err(err) => {
                        self.log.record(Activity::SupersedeIncomplete {
                            id: keep,
                            reason: err.to_string(),
                        });
                    }
                }
                self.store.upsert(message);
            }

            Reconciliation::AttachSms { keep, mut sms } => {
                let Some(mut message) = self.store.get(&keep) else {
                    return;
                };
                if sms.local_copy == SmsLocalCopy::Removed {
                    match self.sms.delete_local(&sms.row).await {
                        Ok(()) => {
                            self.log.record(Activity::Superseded {
                                id: keep,
                                dropped_row: sms.row.clone(),
                            });
                            report.superseded += 1;
                        }
                        Err(err) => {
                            sms.local_copy = SmsLocalCopy::Present;
                            self.log.record(Activity::SupersedeIncomplete {
                                id: keep,
                                reason: err.to_string(),
                            });
                        }
                    }
                }
                message.delivery.sms = Some(sms);
                self.store.upsert(message);
            }

            Reconciliation::Ignore => {
                self.log.record(Activity::Ignored { tag });
                report.ignored += 1;
            }
        }
    }
}
