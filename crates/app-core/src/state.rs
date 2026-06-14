//! Application session state and the shared, lockable handle around it.
//!
//! [`AppState`] is what the Tauri layer `manage`s: a single value, shared across
//! all command-handler threads, wrapping the mutable [`Session`] behind a
//! `std::sync::Mutex`. The mutex is *synchronous* and is **never** held across
//! an `.await`; command handlers do their (non-async) work inside the guard and
//! drop it before returning.
//!
//! The [`SessionState`] enum is a small typestate: either `Locked` (no key
//! material in memory) or `Unlocked` (holding the live [`UnlockedVault`] and the
//! monotonic instant of the last activity, for idle auto-lock).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vault_core::UnlockedVault;

use crate::clock::Clock;

/// Whether a vault is open, and if so the live session data.
pub enum SessionState {
    /// No vault is open; no key material is in memory.
    Locked,
    /// A vault is open. Holds the unlocked vault (and thus the derived key) plus
    /// the monotonic instant of the most recent activity, used to decide idle
    /// auto-lock.
    Unlocked {
        /// The live unlocked vault. Dropping it (or calling `lock()`) zeroizes
        /// the derived key.
        vault: UnlockedVault,
        /// Monotonic instant of the last user activity (read or mutation).
        last_activity: Instant,
    },
}

/// The mutable session: current [`SessionState`] plus the immutable
/// configuration (vault path, idle timeout, clock) it operates against.
pub struct Session {
    /// Locked or unlocked, with session data when unlocked.
    pub(crate) state: SessionState,
    /// Absolute path to the vault file on disk.
    pub(crate) vault_path: PathBuf,
    /// Idle window after which the vault auto-locks.
    pub(crate) idle_timeout: Duration,
    /// Injected time source (real in production, fake in tests).
    pub(crate) clock: Box<dyn Clock>,
}

impl Session {
    /// Lock the session in place, zeroizing the key.
    ///
    /// Moves the current state out via [`std::mem::replace`] so the
    /// [`UnlockedVault`] can be consumed by `lock()` (which zeroizes the derived
    /// key). Returns `true` if a vault was actually open (i.e. a transition
    /// occurred), `false` if it was already locked.
    pub(crate) fn do_lock(&mut self) -> bool {
        match std::mem::replace(&mut self.state, SessionState::Locked) {
            SessionState::Unlocked { vault, .. } => {
                // Consume the unlocked vault: this zeroizes the derived key. We
                // intentionally drop the relockable `LockedVault` it returns —
                // the on-disk file is the source of truth and is re-read on the
                // next unlock.
                let _ = vault.lock();
                true
            }
            SessionState::Locked => false,
        }
    }
}

/// The shared, mutex-guarded session handle managed by Tauri.
///
/// All service operations go through `&self`, taking the lock internally. The
/// mutex is never held across an await.
pub struct AppState {
    pub(crate) inner: Mutex<Session>,
}

impl AppState {
    /// Build a fresh, **locked** application state.
    ///
    /// * `vault_path` — absolute path to the vault file (production: from
    ///   [`crate::paths::resolve_vault_path`]; tests: a tempdir path).
    /// * `idle_timeout` — idle window before auto-lock.
    /// * `clock` — time source ([`crate::clock::SystemClock`] in production).
    pub fn new(vault_path: PathBuf, idle_timeout: Duration, clock: Box<dyn Clock>) -> Self {
        AppState {
            inner: Mutex::new(Session {
                state: SessionState::Locked,
                vault_path,
                idle_timeout,
                clock,
            }),
        }
    }
}
