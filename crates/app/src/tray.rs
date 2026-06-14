//! The system-tray icon and its menu.
//!
//! Spooky-Pass lives in the tray and starts on login (see [`crate::main`]); the
//! main window is *hidden* rather than closed so the Core stays resident for
//! browser autofill. The tray menu provides the three escape hatches a resident
//! app needs: show the window, lock immediately, or quit for real.

use app_core::AppState;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};

/// Event emitted to the webview when the vault becomes locked (here, via "Lock
/// now"; the idle driver emits the same event). The frontend listens for this
/// to drop back to the unlock screen.
pub const VAULT_LOCKED_EVENT: &str = "vault-locked";

const MENU_OPEN: &str = "open";
const MENU_LOCK: &str = "lock";
const MENU_QUIT: &str = "quit";

/// Build the tray icon + menu and wire up its click handlers.
///
/// Called once during setup. Uses the app's default window icon as the tray
/// image so we don't ship a second asset.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, MENU_OPEN, "Open Spooky-Pass", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, MENU_LOCK, "Lock now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &lock, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundled default window icon");

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Spooky-Pass")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;

    Ok(())
}

/// Handle a tray-menu selection.
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id.as_ref() {
        MENU_OPEN => show_main_window(app),
        MENU_LOCK => lock_now(app),
        MENU_QUIT => app.exit(0),
        _ => {}
    }
}

/// Reveal and focus the main window (recreating nothing — it is only hidden).
fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Lock the vault immediately and notify the webview so it returns to the unlock
/// screen.
fn lock_now<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    // Locking is infallible-by-contract for the UI (idempotent); ignore the
    // returned status here since the event + a fresh `status()` drive the UI.
    let _ = state.lock();
    let _ = app.emit(VAULT_LOCKED_EVENT, ());
}
