//! Test support: a deterministic [`Clock`] implementation.
//!
//! [`FakeClock`] lets tests advance monotonic time and set the wall clock by
//! hand, so the session / auto-lock logic can be exercised without sleeping or
//! touching the real system clock. It lives in a public (non-`cfg(test)`)
//! module so the crate's *integration* tests — which compile as a separate
//! crate — can use it too.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::clock::Clock;

/// A controllable [`Clock`] for tests.
///
/// * The monotonic side starts at the process's `Instant::now()` and only moves
///   forward when [`FakeClock::advance`] is called — never on its own.
/// * The wall-clock side returns a fixed RFC 3339 string that the test sets via
///   [`FakeClock::set_wall`] (default: a stable sentinel timestamp).
///
/// Cloning shares the same underlying state, so a clock handed to an
/// [`crate::AppState`] and a copy kept by the test stay in sync.
#[derive(Clone)]
pub struct FakeClock {
    inner: std::sync::Arc<Inner>,
}

struct Inner {
    /// Monotonic position, expressed as an offset from `base`.
    elapsed: Mutex<Duration>,
    /// Fixed origin so `now()` returns real `Instant`s (which cannot be
    /// constructed from raw values on stable Rust).
    base: Instant,
    /// The wall-clock string returned by `now_rfc3339`.
    wall: Mutex<String>,
}

/// The default wall-clock timestamp used until a test overrides it.
pub const DEFAULT_WALL: &str = "2026-06-14T00:00:00Z";

impl FakeClock {
    /// Create a fake clock at elapsed `0` with the [`DEFAULT_WALL`] timestamp.
    pub fn new() -> Self {
        FakeClock {
            inner: std::sync::Arc::new(Inner {
                elapsed: Mutex::new(Duration::ZERO),
                base: Instant::now(),
                wall: Mutex::new(DEFAULT_WALL.to_string()),
            }),
        }
    }

    /// Advance the monotonic clock by `delta`. Does not affect the wall clock.
    pub fn advance(&self, delta: Duration) {
        let mut elapsed = self.inner.elapsed.lock().expect("FakeClock mutex");
        *elapsed += delta;
    }

    /// Set the wall-clock string returned by [`Clock::now_rfc3339`].
    pub fn set_wall(&self, rfc3339: impl Into<String>) {
        let mut wall = self.inner.wall.lock().expect("FakeClock mutex");
        *wall = rfc3339.into();
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let elapsed = *self.inner.elapsed.lock().expect("FakeClock mutex");
        self.inner.base + elapsed
    }

    fn now_rfc3339(&self) -> String {
        self.inner.wall.lock().expect("FakeClock mutex").clone()
    }
}
