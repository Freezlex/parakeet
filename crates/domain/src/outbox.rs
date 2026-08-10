use crate::id::MessageId;
use crate::peer::{ConversationId, PeerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetStatus {
    Online,
    Offline,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    #[default]
    Pending,
    InFlight,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    pub id: MessageId,
    pub conversation: ConversationId,
    pub peer: PeerId,
    pub body: String,
    pub origin_ts: u64,
    pub matrix: TransportState,
    pub sms: TransportState,
    pub matrix_attempts: u32,
    pub sms_attempts: u32,
    pub next_matrix_attempt_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendDecision {
    TryMatrix,
    FallbackToSms,
    BackfillMatrix,
    Wait { retry_after_ms: u64 },
    Done,
    Abandon,
}

pub const MAX_SMS_ATTEMPTS: u32 = 3;
pub const OFFLINE_POLL_MS: u64 = 5_000;
pub fn backoff_ms(attempts: u32) -> u64 {
    const BASE_MS: u64 = 1_000;
    const CAP_MS: u64 = 60_000;
    BASE_MS.saturating_mul(1u64 << attempts.min(6)).min(CAP_MS)
}

impl OutboxEntry {
    pub fn new(
        id: MessageId,
        conversation: ConversationId,
        peer: PeerId,
        body: String,
        origin_ts: u64,
    ) -> Self {
        OutboxEntry {
            id,
            conversation,
            peer,
            body,
            origin_ts,
            matrix: TransportState::Pending,
            sms: TransportState::Pending,
            matrix_attempts: 0,
            sms_attempts: 0,
            next_matrix_attempt_ms: 0,
        }
    }

    pub fn begin_matrix(&mut self) {
        self.matrix = TransportState::InFlight;
        self.matrix_attempts += 1;
    }

    pub fn matrix_succeeded(&mut self) {
        self.matrix = TransportState::Done;
    }

    pub fn matrix_failed(&mut self, now_ms: u64) {
        self.matrix = TransportState::Failed;
        self.next_matrix_attempt_ms = now_ms.saturating_add(backoff_ms(self.matrix_attempts));
    }

    pub fn begin_sms(&mut self) {
        self.sms = TransportState::InFlight;
        self.sms_attempts += 1;
    }

    pub fn sms_succeeded(&mut self) {
        self.sms = TransportState::Done;
    }

    pub fn sms_failed(&mut self) {
        self.sms = TransportState::Failed;
    }

    pub fn reached_recipient(&self) -> bool {
        self.matrix == TransportState::Done || self.sms == TransportState::Done
    }
}

pub fn next_action(entry: &OutboxEntry, net: NetStatus, now_ms: u64) -> SendDecision {
    use TransportState::{Done, Failed, InFlight, Pending};

    if entry.matrix == Done {
        return SendDecision::Done;
    }

    if entry.matrix == InFlight || entry.sms == InFlight {
        return SendDecision::Wait { retry_after_ms: 0 };
    }

    if entry.sms == Done {
        return match net {
            NetStatus::Offline => SendDecision::Wait {
                retry_after_ms: OFFLINE_POLL_MS,
            },
            _ if now_ms < entry.next_matrix_attempt_ms => SendDecision::Wait {
                retry_after_ms: entry.next_matrix_attempt_ms - now_ms,
            },
            _ => SendDecision::BackfillMatrix,
        };
    }

    if entry.matrix == Pending && net != NetStatus::Offline {
        return SendDecision::TryMatrix;
    }

    if entry.sms_attempts >= MAX_SMS_ATTEMPTS {
        debug_assert_eq!(entry.sms, Failed, "attempts only accrue on failure");
        return SendDecision::Abandon;
    }
    SendDecision::FallbackToSms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> OutboxEntry {
        OutboxEntry::new(
            MessageId::from_parts(1_000, 7),
            ConversationId::new("bob"),
            PeerId::new("bob"),
            "hi".into(),
            1_000,
        )
    }

    #[test]
    fn online_sends_over_matrix() {
        assert_eq!(
            next_action(&entry(), NetStatus::Online, 1_000),
            SendDecision::TryMatrix
        );
    }

    #[test]
    fn offline_falls_back_without_even_trying_matrix() {
        assert_eq!(
            next_action(&entry(), NetStatus::Offline, 1_000),
            SendDecision::FallbackToSms
        );
    }

    #[test]
    fn a_matrix_failure_falls_back_rather_than_retrying() {
        let mut e = entry();
        e.begin_matrix();
        e.matrix_failed(1_000);
        assert_eq!(
            next_action(&e, NetStatus::Online, 1_000),
            SendDecision::FallbackToSms
        );
    }

    #[test]
    fn an_in_flight_attempt_is_left_alone() {
        let mut e = entry();
        e.begin_matrix();
        assert_eq!(
            next_action(&e, NetStatus::Online, 1_000),
            SendDecision::Wait { retry_after_ms: 0 }
        );
    }

    #[test]
    fn after_falling_back_the_entry_survives_and_waits_for_connectivity() {
        let mut e = entry();
        e.begin_matrix();
        e.matrix_failed(1_000);
        e.begin_sms();
        e.sms_succeeded();

        assert_eq!(
            next_action(&e, NetStatus::Offline, 2_000),
            SendDecision::Wait {
                retry_after_ms: OFFLINE_POLL_MS
            }
        );
    }

    #[test]
    fn coming_back_online_backfills_matrix() {
        let mut e = entry();
        e.begin_matrix();
        e.matrix_failed(1_000);
        e.begin_sms();
        e.sms_succeeded();

        let backoff = backoff_ms(1);
        assert_eq!(
            next_action(&e, NetStatus::Online, 1_000),
            SendDecision::Wait {
                retry_after_ms: backoff
            }
        );

        assert_eq!(
            next_action(&e, NetStatus::Online, 1_000 + backoff),
            SendDecision::BackfillMatrix
        );
    }

    #[test]
    fn the_backfill_id_is_the_id_that_went_out_over_sms() {
        let e = entry();
        let sent_id = e.id;
        let mut e = e;
        e.begin_matrix();
        e.matrix_failed(0);
        e.begin_sms();
        e.sms_succeeded();
        assert_eq!(e.id, sent_id, "the id must survive the fallback untouched");
    }

    #[test]
    fn matrix_success_retires_the_entry() {
        let mut e = entry();
        e.begin_matrix();
        e.matrix_succeeded();
        assert_eq!(next_action(&e, NetStatus::Online, 1_000), SendDecision::Done);
    }

    #[test]
    fn a_backfilled_entry_retires_even_though_sms_also_carried_it() {
        let mut e = entry();
        e.begin_matrix();
        e.matrix_failed(0);
        e.begin_sms();
        e.sms_succeeded();
        e.begin_matrix();
        e.matrix_succeeded();
        assert_eq!(next_action(&e, NetStatus::Online, 9_999), SendDecision::Done);
        assert!(e.reached_recipient());
    }

    #[test]
    fn a_degraded_link_still_tries_matrix_first() {
        assert_eq!(
            next_action(&entry(), NetStatus::Degraded, 1_000),
            SendDecision::TryMatrix
        );
    }

    #[test]
    fn repeated_sms_failure_is_abandoned() {
        let mut e = entry();
        for _ in 0..MAX_SMS_ATTEMPTS {
            assert_eq!(
                next_action(&e, NetStatus::Offline, 1_000),
                SendDecision::FallbackToSms
            );
            e.begin_sms();
            e.sms_failed();
        }
        assert_eq!(
            next_action(&e, NetStatus::Offline, 1_000),
            SendDecision::Abandon
        );
    }

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_ms(0), 1_000);
        assert_eq!(backoff_ms(1), 2_000);
        assert_eq!(backoff_ms(2), 4_000);
        assert_eq!(backoff_ms(50), 60_000);
    }
}
