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
