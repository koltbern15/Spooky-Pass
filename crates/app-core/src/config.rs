//! Persisted, **non-secret** application configuration.
//!
//! This holds only user preferences (currently the idle auto-lock timeout) — it
//! never contains vault data or key material. It lives as `config.json`
//! alongside the vault file in the application data directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Config file name (sits next to the vault).
pub const CONFIG_FILE_NAME: &str = "config.json";

/// Default idle auto-lock window, in seconds (15 minutes).
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 15 * 60;

/// Non-secret, persisted user preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// Idle window before the vault auto-locks, in seconds. `0` means "never
    /// auto-lock" (the vault still locks on quit / explicit lock).
    pub idle_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
        }
    }
}

/// The config-file path that sits alongside `vault_path`.
pub fn config_path_for_vault(vault_path: &Path) -> PathBuf {
    match vault_path.parent() {
        Some(dir) => dir.join(CONFIG_FILE_NAME),
        None => PathBuf::from(CONFIG_FILE_NAME),
    }
}

/// Load config from `path`, falling back to [`AppConfig::default`] if the file
/// is missing, unreadable, or malformed. Config is a convenience, never a
/// security boundary, so a bad file degrades gracefully rather than failing.
pub fn load(path: &Path) -> AppConfig {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist `config` to `path`, creating the parent directory if needed.
pub fn save(path: &Path, config: &AppConfig) -> Result<()> {
    let json = serde_json::to_vec_pretty(config)
        .map_err(|_| AppError::Internal("could not serialize config".to_string()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| AppError::Internal(format!("could not create config directory: {e}")))?;
    }
    std::fs::write(path, json)
        .map_err(|e| AppError::Internal(format!("could not write config: {e}")))?;
    Ok(())
}
