use std::sync::{Arc, Mutex};

use domain::{MessageId, ShortTag, SmsRowId};
use ports::Clock;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    pub ts: u64,
    pub activity: Activity,
}

impl ActivityEntry {
    pub fn summary(&self) -> String {
        self.activity.summary()
    }
}

#[derive(Clone)]
pub struct ActivityLog {
    clock: Arc<dyn Clock>,
    entries: Arc<Mutex<Vec<ActivityEntry>>>,
}

impl std::fmt::Debug for ActivityLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityLog")
            .field("entries", &self.entries)
            .finish_non_exhaustive()
    }
}

const CAPACITY: usize = 200;

impl ActivityLog {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        ActivityLog {
            clock,
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record(&self, activity: Activity) {
        let mut guard = self.entries.lock().expect("activity log lock");
        guard.push(ActivityEntry {
            ts: self.clock.now_ms(),
            activity,
        });
        let overflow = guard.len().saturating_sub(CAPACITY);
        if overflow > 0 {
            guard.drain(..overflow);
        }
    }

    pub fn entries(&self) -> Vec<ActivityEntry> {
        self.entries.lock().expect("activity log lock").clone()
    }

    /// Newest first, by recording order (ties in `ts` keep insertion order).
    pub fn recent(&self, limit: usize) -> Vec<ActivityEntry> {
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
    use infra_memory_store::ManualClock;

    #[test]
    fn the_log_keeps_the_newest_entries() {
        let log = ActivityLog::new(Arc::new(ManualClock::new(0)));
        for i in 0..CAPACITY + 10 {
            log.record(Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(i as u64),
            });
        }
        let entries = log.entries();
        assert_eq!(entries.len(), CAPACITY);
        assert_eq!(
            entries[0].activity,
            Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(10)
            }
        );
    }

    #[test]
    fn recent_returns_newest_first() {
        let log = ActivityLog::new(Arc::new(ManualClock::new(0)));
        for i in 0..3 {
            log.record(Activity::ReceivedMatrix {
                tag: ShortTag::from_u64(i),
            });
        }
        assert_eq!(
            log.recent(2)
                .into_iter()
                .map(|entry| entry.activity)
                .collect::<Vec<_>>(),
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
