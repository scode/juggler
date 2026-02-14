//! Shared time abstraction used across runtime code and tests.
//!
//! `Clock` provides an injectable source of current UTC time. `SystemClock`
//! serves production code, and `FixedClock` supports deterministic tests.
//!
//! Modules that depend on time accept shared clock trait objects instead of
//! calling `Utc::now()` directly.

#[cfg(test)]
use chrono::Duration;
use chrono::{DateTime, Utc};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

/// An abstraction over a source of the current time.
///
/// Implementations must be thread-safe.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Returns the current instant in UTC.
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock that returns the real, current time from the system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[inline]
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A test clock that always returns a controlled instant.
///
/// You can update the current instant via `set_now` or `advance` to make tests
/// deterministic and expressive.
#[derive(Debug)]
#[cfg(test)]
pub struct FixedClock {
    inner: Mutex<DateTime<Utc>>,
}

#[cfg(test)]
impl FixedClock {
    /// Create a new `FixedClock` pinned at the provided instant.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            inner: Mutex::new(now),
        }
    }

    /// Create a new `FixedClock` from an RFC3339 timestamp (e.g., "2025-01-07T09:00:00Z").
    ///
    /// Panics if the input is not a valid RFC3339 timestamp.
    pub fn from_rfc3339(s: &str) -> Self {
        let dt = DateTime::parse_from_rfc3339(s)
            .expect("invalid RFC3339 timestamp")
            .with_timezone(&Utc);
        Self::new(dt)
    }

    /// Set the current instant to `now`.
    pub fn set_now(&self, now: DateTime<Utc>) {
        *self.inner.lock().expect("poisoned FixedClock") = now;
    }

    /// Advance (or rewind if negative) the current instant by `delta`.
    pub fn advance(&self, delta: Duration) {
        let mut guard = self.inner.lock().expect("poisoned FixedClock");
        *guard += delta;
    }
}

#[cfg(test)]
impl Clone for FixedClock {
    fn clone(&self) -> Self {
        let now = *self.inner.lock().expect("poisoned FixedClock");
        Self::new(now)
    }
}

#[cfg(test)]
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.inner.lock().expect("poisoned FixedClock")
    }
}

/// A convenient alias for sharing a clock behind an `Arc`.
pub type SharedClock = Arc<dyn Clock>;

/// Create a shared production clock.
pub fn system_clock() -> SharedClock {
    Arc::new(SystemClock)
}

/// Create a shared fixed clock initialized at `now`.
#[cfg(test)]
pub fn fixed_clock(now: DateTime<Utc>) -> SharedClock {
    Arc::new(FixedClock::new(now))
}

/// Create a shared fixed clock at a standard test time (2025-01-01 00:00:00 UTC).
#[cfg(test)]
pub fn test_clock() -> SharedClock {
    use chrono::TimeZone;
    fixed_clock(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_fixed_time() {
        let t0 = Utc::now();
        let clock = FixedClock::new(t0);
        assert_eq!(clock.now(), t0);

        let t1 = t0 + Duration::days(1);
        clock.set_now(t1);
        assert_eq!(clock.now(), t1);
    }

    #[test]
    fn fixed_clock_advance_and_rewind() {
        let t0 = Utc::now();
        let clock = FixedClock::new(t0);

        clock.advance(Duration::hours(5));
        assert_eq!(clock.now(), t0 + Duration::hours(5));

        clock.advance(Duration::hours(-2));
        assert_eq!(clock.now(), t0 + Duration::hours(3));
    }

    #[test]
    fn fixed_clock_clone_copies_time_value() {
        let t0 = Utc::now();
        let clock1 = FixedClock::new(t0);
        let clock2 = clock1.clone();
        assert_eq!(clock1.now(), t0);
        assert_eq!(clock2.now(), t0);

        // Mutate one and ensure the other remains independent
        clock1.advance(Duration::minutes(10));
        assert_eq!(clock1.now(), t0 + Duration::minutes(10));
        assert_eq!(clock2.now(), t0);
    }

    #[test]
    fn system_clock_compiles_and_returns_utc() {
        let c = SystemClock;
        let now = c.now();
        assert_eq!(now.timezone(), Utc);
    }

    #[test]
    fn shared_alias_constructors() {
        let sysclock: SharedClock = system_clock();
        sysclock.now();
    }

    #[test]
    fn from_rfc3339_constructor() {
        let clock = FixedClock::from_rfc3339("2025-01-07T09:00:00Z");
        assert_eq!(
            clock.now(),
            DateTime::parse_from_rfc3339("2025-01-07T09:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }
}
