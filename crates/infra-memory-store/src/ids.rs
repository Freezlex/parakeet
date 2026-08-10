use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use domain::MessageId;
use ports::{Clock, IdGen};

fn split_mix64(state: &AtomicU64) -> u64 {
    let mut z = state.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn random_80(state: &AtomicU64) -> u128 {
    let hi = u128::from(split_mix64(state));
    let lo = u128::from(split_mix64(state));
    ((hi << 64) | lo) & ((1u128 << 80) - 1)
}

pub struct EntropyIdGen {
    clock: Arc<dyn Clock>,
    state: AtomicU64,
}

impl EntropyIdGen {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(std::process::id() as usize);
        hasher.write_u64(clock.now_ms());
        EntropyIdGen {
            clock,
            state: AtomicU64::new(hasher.finish()),
        }
    }
}

impl std::fmt::Debug for EntropyIdGen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntropyIdGen").finish_non_exhaustive()
    }
}

impl IdGen for EntropyIdGen {
    fn mint(&self) -> MessageId {
        MessageId::from_parts(self.clock.now_ms(), random_80(&self.state))
    }
}

pub struct SequentialIdGen {
    clock: Arc<dyn Clock>,
    state: AtomicU64,
}

impl SequentialIdGen {
    pub fn new(clock: Arc<dyn Clock>, seed: u64) -> Self {
        SequentialIdGen {
            clock,
            state: AtomicU64::new(seed),
        }
    }
}

impl std::fmt::Debug for SequentialIdGen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SequentialIdGen").finish_non_exhaustive()
    }
}

impl IdGen for SequentialIdGen {
    fn mint(&self) -> MessageId {
        MessageId::from_parts(self.clock.now_ms(), random_80(&self.state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use std::collections::HashSet;

    #[test]
    fn minted_ids_carry_the_current_time() {
        let clock = Arc::new(ManualClock::new(1_754_800_000_000));
        let ids = EntropyIdGen::new(clock.clone());
        assert_eq!(ids.mint().timestamp_ms(), 1_754_800_000_000);
        clock.set(1_754_800_005_000);
        assert_eq!(ids.mint().timestamp_ms(), 1_754_800_005_000);
    }

    #[test]
    fn ids_minted_in_the_same_millisecond_are_still_distinct() {
        let ids = EntropyIdGen::new(Arc::new(ManualClock::new(1_000)));
        let minted: HashSet<_> = (0..1_000).map(|_| ids.mint()).collect();
        assert_eq!(minted.len(), 1_000);
    }

    #[test]
    fn short_tags_are_distinct_too() {
        let ids = EntropyIdGen::new(Arc::new(ManualClock::new(1_000)));
        let tags: HashSet<_> = (0..1_000).map(|_| ids.mint().short_tag()).collect();
        assert_eq!(tags.len(), 1_000);
    }

    #[test]
    fn a_seeded_generator_repeats_itself() {
        let clock = Arc::new(ManualClock::new(1_000));
        let first: Vec<_> = {
            let ids = SequentialIdGen::new(clock.clone(), 42);
            (0..5).map(|_| ids.mint()).collect()
        };
        let second: Vec<_> = {
            let ids = SequentialIdGen::new(clock.clone(), 42);
            (0..5).map(|_| ids.mint()).collect()
        };
        assert_eq!(first, second);
    }
}
