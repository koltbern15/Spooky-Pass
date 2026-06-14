//! Spooky-Pass desktop application entry point (the Tauri shell).
//!
//! Responsibilities, all thin wiring over `app-core`:
//!
//! * build and `manage` the single [`AppState`] (the mutex-guarded session) with
//!   the real [`SystemClock`] and the idle timeout loaded from the saved config;
//! * register the single-instance guard + autostart plugin;
//! * expose the `#[tauri::command]` surface (see [`commands`]);
//! * stand up the tray icon (see [`tray`]), spawn the idle auto-lock driver
//!   (see [`autolock_driver`]), and start the autofill IPC server the browser
//!   native-host relays to (see [`ipc`]);
//! * keep the app resident: closing the main window *hides* it instead of
//!   exiting, so the Core stays available for browser autofill.
//!
//! All real secret handling lives in `app-core` / `vault-core`; this binary
//! never sees the derived key and only forwards the (zeroizing) master password.

// On Windows, suppress the extra console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autolock_driver;
mod commands;
mod ipc;
mod tray;

use std::time::Duration;

use app_core::{config, resolve_vault_path, AppState, SystemClock};
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

fn main() {
    // Resolve the on-disk vault path (creating the data dir). If this fails the
    // app genuinely cannot run, so surface the error and abort before building
    // the webview.
    let vault_path = match resolve_vault_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Spooky-Pass: cannot determine vault location: {e}");
            std::process::exit(1);
        }
    };

    // Load the persisted idle-timeout preference (defaults if absent).
    let config_path = config::config_path_for_vault(&vault_path);
    let idle_timeout = Duration::from_secs(config::load(&config_path).idle_timeout_secs);

    let state = AppState::new(vault_path, idle_timeout, Box::new(SystemClock));

    tauri::Builder::default()
        // Single-instance MUST be registered first: a second launch focuses the
        // already-running Core instead of starting a second one (which would
        // fight over the autofill socket).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            // Start on login (per the design doc: the Core auto-starts so
            // autofill is available). No extra launch args.
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state)
        .setup(|app| {
            let handle = app.handle();
            tray::build(handle)?;
            autolock_driver::spawn(handle);
            // Stand up the local-socket IPC server the browser native-host relays
            // to (shares the same managed AppState as the GUI commands).
            ipc::spawn(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps the Core resident in the tray rather
            // than exiting: hide the window and swallow the close.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::create_vault,
            commands::unlock,
            commands::lock,
            commands::list_entries,
            commands::get_entry,
            commands::add_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::generate_password,
            commands::set_idle_timeout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Spooky-Pass");
}
