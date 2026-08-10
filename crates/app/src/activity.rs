use std::sync::{Arc, Mutex};

use domain::{MessageId, ShortTag, SmsRowId};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Composed { id: MessageId, body: String },
    SentViaMatrix { id: MessageId },
    FellBackToSms { id: MessageId, reason: String },
    Backfilled { id: MessageId },
    SendFailed { id: MessageId, reason: String },
    ReceivedSms { tag: ShortTag, from_app: bool },
    ReceivedMatrix { tag: ShortTag },
    Superseded {
        id: MessageId,
        dropped_row: SmsRowId,
    },
    SupersedeIncomplete { id: MessageId, reason: String },
    Ignored { tag: ShortTag },
}

impl Activity {
    pub fn summary(&self) -> String {
        match self {
            Activity::Composed { id, body } => {
                format!("composed \"{body}\" as {}", id.short_tag())
            }
            Activity::SentViaMatrix { id } => format!("sent {} over matrix", id.short_tag()),
            Activity::FellBackToSms { id, reason } => {
                format!("fell back to sms for {} ({reason})", id.short_tag())
            }
            Activity::Backfilled { id } => {
                format!("backfilled {} to matrix under its original id", id.short_tag())
            }
            Activity::SendFailed { id, reason } => {
                format!("gave up on {}: {reason}", id.short_tag())
            }
            Activity::ReceivedSms { tag, from_app } => {
                if *from_app {
                    format!("received sms carrying id {tag}")
                } else {
                    "received a plain sms with no id".to_owned()
                }
            }
            Activity::ReceivedMatrix { tag } => format!("received matrix event for {tag}"),
            Activity::Superseded { id, dropped_row } => {
                format!(
                    "matrix event superseded the sms for {} — deleted row {dropped_row}",
                    id.short_tag()
                )
            }
            Activity::SupersedeIncomplete { id, reason } => {
                format!(
                    "kept the matrix event for {} but could not delete the sms: {reason}",
                    id.short_tag()
                )
            }
            Activity::Ignored { tag } => format!("ignored a duplicate of {tag}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActivityLog {
    entries: Arc<Mutex<Vec<Activity>>>,
}

const CAPACITY: usize = 200;

impl ActivityLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, activity: Activity) {
        let mut guard = self.entries.lock().expect("activity log lock");
        guard.push(activity);
        let overflow = guard.len().saturating_sub(CAPACITY);
        if overflow > 0 {
            guard.drain(..overflow);
        }
    }

    pub fn entries(&self) -> Vec<Activity> {
        self.entries.lock().expect("activity log lock").clone()
    }

    pub fn recent(&self, limit: usize) -> Vec<Activity> {
        let guard = self.entries.lock().expect("activity log lock");
        guard.iter().rev().take(limit).cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().expect("activity log lock").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_the_newest_entries() {
        let log = ActivityLog::new();
        for i in 0..CAPACITY + 10 {
            log.record(Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(i as u64),
            });
        }
        let entries = log.entries();
        assert_eq!(entries.len(), CAPACITY);
        assert_eq!(
            entries[0],
            Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(10)
            }
        );
    }

    #[test]
    fn recent_returns_newest_first() {
        let log = ActivityLog::new();
        for i in 0..3 {
            log.record(Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(i),
            });
        }
        assert_eq!(
            log.recent(2),
            vec![
                Activity::ReceivedMatrix {
                    tag: ShortTag::from_u64(2)
                },
                Activity::ReceivedMatrix {
                    tag: ShortTag::from_u64(1)
                },
            ]
        );
    }
}
