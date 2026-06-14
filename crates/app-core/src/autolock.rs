//! Idle auto-lock decision logic.
//!
//! This module contains **no timers and no sleeps**. It is pure, clock-driven
//! decision logic: the production binary runs a background thread that calls
//! [`AppState::tick_autolock`] on a coarse interval, and that method consults the
//! injected [`crate::clock::Clock`] to decide whether the idle window has
//! elapsed. Tests drive the very same code path by advancing a
//! [`crate::testing::FakeClock`] — never by waiting on real time.

use crate::state::{AppState, SessionState};

impl AppState {
    /// Check whether the vault has been idle longer than the configured timeout
    /// and, if so, lock it.
    ///
    /// Returns `true` if this call performed an auto-lock transition (the vault
    /// was unlocked and the idle window had elapsed), and `false` otherwise —
    /// including when the vault is already locked, so calling this on a locked
    /// session is a harmless no-op.
    ///
    /// The decision uses the monotonic clock: `now - last_activity >=
    /// idle_timeout`. The wall clock plays no part, so NTP steps and DST changes
    /// cannot prematurely (or belatedly) trip the lock.
    pub fn tick_autolock(&self) -> bool {
        let mut session = self.inner.lock().expect("session mutex poisoned");

        let elapsed = match &session.state {
            SessionState::Unlocked { last_activity, .. } => session
                .clock
                .now()
                .saturating_duration_since(*last_activity),
            SessionState::Locked => return false,
        };

        if elapsed >= session.idle_timeout {
            session.do_lock()
        } else {
            false
        }
    }

    /// Record activity on the session, resetting the idle countdown.
    ///
    /// A no-op when the vault is locked. The service methods bump activity
    /// *inline* while already holding the session lock (a `std::sync::Mutex` is
    /// not re-entrant); this public form is for callers that hold no lock — e.g.
    /// a future IPC/native-messaging layer that wants to mark the session active
    /// without performing a vault operation.
    pub fn touch(&self) {
        let mut session = self.inner.lock().expect("session mutex poisoned");
        let now = session.clock.now();
        if let SessionState::Unlocked { last_activity, .. } = &mut session.state {
            *last_activity = now;
        }
    }
}
