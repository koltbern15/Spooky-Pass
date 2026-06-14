//! The background driver that turns the (pure) auto-lock decision into a real,
//! periodic check.
//!
//! app-core's [`AppState::tick_autolock`] is intentionally timer-free — it just
//! consults the injected clock and decides whether to lock *right now*. Something
//! has to call it on a cadence; that is this thread. It wakes every
//! [`POLL_INTERVAL`], ticks, and — when a tick actually performs an idle lock —
//! emits the `vault-locked` event so the webview drops back to the unlock screen.
//!
//! The poll interval only bounds how late the lock can fire (at most one interval
//! past the true idle deadline); it is unrelated to the configured idle timeout.

use std::thread;
use std::time::Duration;

use app_core::AppState;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::tray::VAULT_LOCKED_EVENT;

/// How often the driver re-evaluates the idle deadline.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Spawn the auto-lock driver thread for `app`.
///
/// The thread runs for the lifetime of the process; it holds only an
/// [`AppHandle`] (cheap to clone) and never the session lock across its sleep.
pub fn spawn<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    thread::Builder::new()
        .name("spooky-autolock".into())
        .spawn(move || run(handle))
        .expect("spawn auto-lock driver thread");
}

fn run<R: Runtime>(app: AppHandle<R>) {
    loop {
        thread::sleep(POLL_INTERVAL);

        // Scope the state borrow so it is released before we (potentially) emit.
        let locked = {
            let state = app.state::<AppState>();
            state.tick_autolock()
        };

        if locked {
            // A real idle lock just happened; tell the webview to re-lock its UI.
            let _ = app.emit(VAULT_LOCKED_EVENT, ());
        }
    }
}
