use std::sync::Arc;

use domain::{ConversationId, MessageId, NetStatus, PeerId};
use ports::{
    Clock, ConnectivityMonitor, Directory, IdGen, MatrixTransport, MessageStore, SmsTransport,
};

use crate::activity::ActivityLog;
use crate::view::{ConversationSummary, MessageView};

#[derive(Clone)]
pub struct Client {
    pub me: PeerId,
    pub(crate) matrix: Arc<dyn MatrixTransport>,
    pub(crate) sms: Arc<dyn SmsTransport>,
    pub(crate) store: Arc<dyn MessageStore>,
    pub(crate) net: Arc<dyn ConnectivityMonitor>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: Arc<dyn IdGen>,
    pub(crate) directory: Arc<dyn Directory>,
    pub(crate) log: ActivityLog,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("me", &self.me)
            .field("net", &self.net.status())
            .finish_non_exhaustive()
    }
}

pub struct ClientParts {
    pub me: PeerId,
    pub matrix: Arc<dyn MatrixTransport>,
    pub sms: Arc<dyn SmsTransport>,
    pub store: Arc<dyn MessageStore>,
    pub net: Arc<dyn ConnectivityMonitor>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub directory: Arc<dyn Directory>,
}

impl Client {
    pub fn new(parts: ClientParts) -> Self {
        Client {
            me: parts.me,
            matrix: parts.matrix,
            sms: parts.sms,
            store: parts.store,
            net: parts.net,
            clock: parts.clock,
            ids: parts.ids,
            directory: parts.directory,
            log: ActivityLog::new(),
        }
    }

    pub fn activity(&self) -> &ActivityLog {
        &self.log
    }

    pub fn connectivity(&self) -> NetStatus {
        self.net.status()
    }

    pub fn set_connectivity(&self, status: NetStatus) {
        let was_offline = self.net.status() == NetStatus::Offline;
        self.net.observe(status);
        if was_offline && status != NetStatus::Offline {
            for mut entry in self.store.outbox() {
                entry.next_matrix_attempt_ms = 0;
                self.store.update_outbox(entry);
            }
        }
    }

    pub fn thread(&self, conversation: &ConversationId) -> Vec<MessageView> {
        self.store
            .thread(conversation)
            .iter()
            .map(MessageView::from)
            .collect()
    }

    pub fn conversations(&self) -> Vec<ConversationSummary> {
        self.store
            .conversations()
            .into_iter()
            .filter_map(|id| {
                let messages = self.store.thread(&id);
                let last = messages.last()?;
                let peer = PeerId::new(id.as_str());
                Some(ConversationSummary {
                    display_name: self.directory.display_name(&peer),
                    peer,
                    last_body: last.body.clone(),
                    last_ts: last.origin_ts,
                    pending: messages
                        .iter()
                        .filter(|m| m.delivery.is_empty())
                        .count(),
                    id,
                })
            })
            .collect()
    }

    pub fn outbox_len(&self) -> usize {
        self.store.outbox().len()
    }

    pub fn awaiting_backfill(&self) -> Vec<MessageId> {
        self.store
            .outbox()
            .into_iter()
            .filter(|e| e.sms == domain::TransportState::Done)
            .map(|e| e.id)
            .collect()
    }

    pub fn conversation_with(&self, peer: &PeerId) -> ConversationId {
        ConversationId::with_peer(peer)
    }
}
