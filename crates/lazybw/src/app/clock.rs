//! The application clock. Tests use a fixed clock so relative times, TOTP
//! codes and auto-lock are deterministic.

use chrono::{DateTime, Utc};
use std::time::Instant;

/// A source of wall-clock time and monotonic instants.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// The current UTC time.
    fn now(&self) -> DateTime<Utc>;
    /// The current monotonic instant.
    fn instant(&self) -> Instant;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn instant(&self) -> Instant {
        Instant::now()
    }
}

/// A clock frozen at a fixed time.
#[derive(Debug)]
pub struct FixedClock {
    now: DateTime<Utc>,
    instant: Instant,
}

impl FixedClock {
    /// Freezes at `now`. The fixed clock is the shared test time (`util::time::test_now`).
    #[must_use]
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now, instant: Instant::now() }
    }

    /// The test clock: 2026-10-30T14:00:00Z.
    #[must_use]
    pub fn test() -> Self {
        Self::new(crate::util::time::test_now())
    }

    /// Moves the clock forward.
    pub fn advance(&mut self, duration: std::time::Duration) {
        if let Some(next) =
            chrono::Duration::from_std(duration).ok().and_then(|d| self.now.checked_add_signed(d))
        {
            self.now = next;
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn instant(&self) -> Instant {
        self.instant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_stable() {
        let clock = FixedClock::test();
        assert_eq!(clock.now(), crate::util::time::test_now());
        assert_eq!(clock.instant(), clock.instant());
    }
}
