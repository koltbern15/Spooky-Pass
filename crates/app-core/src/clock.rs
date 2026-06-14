//! Time abstraction for the session/auto-lock machinery.
//!
//! Two notions of time are needed:
//!
//! * a **monotonic** instant for measuring idle duration (immune to wall-clock
//!   jumps, NTP steps, and DST changes), and
//! * a **wall-clock** RFC 3339 string for stamping entry `created` / `updated`.
//!
//! Both are funnelled through the [`Clock`] trait so tests can drive time
//! deterministically (see [`crate::testing::FakeClock`]) without sleeping or
//! reading the real system clock.

use std::time::Instant;

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// A source of monotonic instants and wall-clock timestamps.
///
/// Implementors must be `Send + Sync` so the clock can live inside the
/// `Mutex`-guarded session shared across Tauri command threads.
pub trait Clock: Send + Sync {
    /// A monotonic instant, used only to measure elapsed idle time. The absolute
    /// value is meaningless; only differences between two `now()` calls matter.
    fn now(&self) -> Instant;

    /// The current wall-clock time as an RFC 3339 string (UTC), used to stamp
    /// entry `created` / `updated` fields.
    fn now_rfc3339(&self) -> String;
}

/// The production [`Clock`]: monotonic `Instant::now()` and the real UTC wall
/// clock formatted as RFC 3339.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_rfc3339(&self) -> String {
        // `Rfc3339` formatting of a UTC `OffsetDateTime` cannot fail, but we
        // avoid `unwrap` in production code and fall back to the Unix epoch in
        // the impossible error case rather than panicking the tray process.
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}
