use core::fmt;
use core::ops::{Add, Sub};
use core::sync::atomic::{AtomicU64, Ordering};

/// A monotonic tick count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Ticks(pub u64);

impl Ticks {
    pub const fn new(ticks: u64) -> Self {
        Self(ticks)
    }
}

impl Add<u64> for Ticks {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<Ticks> for Ticks {
    type Output = u64;

    fn sub(self, rhs: Ticks) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Display for Ticks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An absolute deadline based on monotonic ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Deadline(pub Ticks);

impl Deadline {
    pub const fn new(ticks: Ticks) -> Self {
        Self(ticks)
    }

    pub fn is_expired(&self, now: Ticks) -> bool {
        self.0 <= now
    }
}

/// A monotonic clock tracking system ticks.
pub struct MonotonicClock {
    ticks: AtomicU64,
}

impl MonotonicClock {
    pub const fn new() -> Self {
        Self {
            ticks: AtomicU64::new(0),
        }
    }

    /// Advances the clock by one tick and returns the new time.
    pub fn advance(&self) -> Ticks {
        Ticks(self.ticks.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// Returns the current time.
    pub fn now(&self) -> Ticks {
        Ticks(self.ticks.load(Ordering::Relaxed))
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn ticks_arithmetic() {
        let t1 = Ticks(100);
        let t2 = t1 + 50;
        assert_eq!(t2, Ticks(150));

        let diff = t2 - t1;
        assert_eq!(diff, 50);
    }

    #[test]
    fn deadline_ordering_and_expiry() {
        let now = Ticks(100);

        let past_deadline = Deadline(Ticks(50));
        let exact_deadline = Deadline(Ticks(100));
        let future_deadline = Deadline(Ticks(150));

        assert!(past_deadline < exact_deadline);
        assert!(exact_deadline < future_deadline);

        assert!(past_deadline.is_expired(now));
        assert!(exact_deadline.is_expired(now));
        assert!(!future_deadline.is_expired(now));
    }

    #[test]
    fn monotonic_clock_advance() {
        let clock = MonotonicClock::new();
        assert_eq!(clock.now(), Ticks(0));

        let t1 = clock.advance();
        assert_eq!(t1, Ticks(1));
        assert_eq!(clock.now(), Ticks(1));

        let t2 = clock.advance();
        assert_eq!(t2, Ticks(2));
        assert_eq!(clock.now(), Ticks(2));
    }

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", Ticks(42)), "42");
    }
}
