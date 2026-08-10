use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ports::Clock;

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Default)]
pub struct ManualClock {
    now_ms: AtomicU64,
}

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        ManualClock {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::Relaxed);
    }

    pub fn advance(&self, by_ms: u64) -> u64 {
        self.now_ms.fetch_add(by_ms, Ordering::Relaxed) + by_ms
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_only_moves_when_told_to() {
        let clock = ManualClock::new(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        assert_eq!(clock.now_ms(), 1_000);
        assert_eq!(clock.advance(500), 1_500);
        assert_eq!(clock.now_ms(), 1_500);
    }

    #[test]
    fn the_system_clock_is_after_the_epoch() {
        assert!(SystemClock.now_ms() > 1_700_000_000_000);
    }
}
