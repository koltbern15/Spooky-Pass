//! Vault-file location on disk.
//!
//! The vault lives under the per-OS application *data* directory (not the cache
//! or config dir), as `vault.spk`. Only the binary's `main` calls
//! [`resolve_vault_path`]; tests inject an explicit tempdir path into
//! [`crate::AppState::new`] and never touch the real data directory.

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::AppError;

/// The vault file name within the data directory.
pub const VAULT_FILE_NAME: &str = "vault.spk";

/// The local-socket file name used for the Core ↔ native-host IPC leg.
pub const AUTOFILL_SOCKET_NAME: &str = "spooky-pass.sock";

/// Resolve the absolute path to the vault file, creating the application data
/// directory if necessary.
///
/// Uses `ProjectDirs::from("dev", "SpookyPass", "SpookyPass")`, which maps to a
/// platform-appropriate location (e.g. `~/.local/share/SpookyPass` on Linux,
/// `~/Library/Application Support/dev.SpookyPass.SpookyPass` on macOS).
///
/// # Errors
///
/// Returns [`AppError::Internal`] if the OS home/data directory cannot be
/// determined or the data directory cannot be created.
pub fn resolve_vault_path() -> Result<PathBuf, AppError> {
    let dirs = ProjectDirs::from("dev", "SpookyPass", "SpookyPass").ok_or_else(|| {
        AppError::Internal("could not determine the application data directory".to_string())
    })?;

    let data_dir = dirs.data_dir();
    std::fs::create_dir_all(data_dir)
        .map_err(|e| AppError::Internal(format!("could not create data directory: {e}")))?;

    Ok(data_dir.join(VAULT_FILE_NAME))
}

/// Resolve the local-socket path the Core IPC server binds and the native-host
/// connects to, for the `native-host ↔ Core` autofill leg.
///
/// ## Per-user, owner-only intent
///
/// This socket carries unlocked-vault traffic (including, on a successful
/// `getCredential`, a password), so it must be reachable **only by the user who
/// owns the running Core** — never world-accessible. Two layers enforce that:
///
/// * **Location.** We prefer `$XDG_RUNTIME_DIR` (e.g. `/run/user/<uid>`), which
///   on Linux is a per-user `0700` tmpfs created by the login session — anything
///   placed there is already private to the user. If it is unset (non-Linux, or
///   a stripped environment) we fall back to the per-user application data
///   directory from [`ProjectDirs`], which is likewise under the user's home.
/// * **Permissions.** The *binder* (the Core, in `crates/app/src/ipc.rs`) is
///   responsible for creating the socket owner-only (`0600` on Unix / a
///   per-user named pipe on Windows); this helper only decides *where* it lives.
///
/// Returning a [`PathBuf`] (rather than an `interprocess` name) keeps app-core
/// free of the `interprocess` dependency: both the Core and the native-host take
/// this path and build their own local-socket name from it.
///
/// Unlike [`resolve_vault_path`] this never creates directories and never fails:
/// `$XDG_RUNTIME_DIR` already exists when set, and the data-dir fallback is best
/// effort (the binder surfaces any real bind error). If even `ProjectDirs`
/// cannot be resolved we fall back to the OS temp dir so a path is always
/// produced.
pub fn autofill_socket_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let runtime_dir = PathBuf::from(runtime_dir);
        if !runtime_dir.as_os_str().is_empty() {
            return runtime_dir.join(AUTOFILL_SOCKET_NAME);
        }
    }

    let base = ProjectDirs::from("dev", "SpookyPass", "SpookyPass")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir);

    base.join(AUTOFILL_SOCKET_NAME)
}

/// The base name of the **Windows named pipe** used for the autofill IPC leg.
///
/// On Unix the IPC endpoint is a filesystem socket ([`autofill_socket_path`]);
/// on Windows, AF_UNIX socket paths are unreliable, so the Core and the
/// native-host rendezvous on a named pipe instead (the OS maps this to
/// `\\.\pipe\<name>`). The name is per-user so two accounts on one machine
/// don't collide, and both ends derive it from this single function so they
/// always agree.
pub fn autofill_pipe_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    format!("spooky-pass-{user}.sock")
}
