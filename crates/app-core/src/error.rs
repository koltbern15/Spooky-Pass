//! Application-level errors that cross the Tauri command boundary.
//!
//! [`AppError`] is `Serialize` so a failing `#[tauri::command]` rejects the JS
//! promise with an object `{ kind, message? }`. It collapses several
//! `vault-core` errors into UI-appropriate buckets and — crucially — preserves
//! the security property that a wrong password and a tampered vault are
//! indistinguishable (both surface as [`AppError::WrongPassword`]).

use serde::Serialize;
use vault_core::VaultError;

/// Result alias for app-core service operations.
pub type Result<T> = core::result::Result<T, AppError>;

/// An error surfaced to the UI. Serialized adjacently as `{ kind, message? }`
/// so the frontend can branch on `kind` (see `AppErrorKind` in `types.ts`).
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    /// Wrong master password OR a tampered/corrupt vault — intentionally merged
    /// so we never reveal which occurred.
    #[error("incorrect master password or corrupted vault")]
    WrongPassword,

    /// The operation requires an unlocked vault, but none is open.
    #[error("the vault is locked")]
    Locked,

    /// A vault file already exists; refusing to overwrite it on create.
    #[error("a vault already exists")]
    VaultAlreadyExists,

    /// No vault file was found to unlock.
    #[error("no vault found")]
    NoVault,

    /// No entry with the given id exists.
    #[error("entry not found: {0}")]
    EntryNotFound(String),

    /// The password-generator options were invalid (zero length or no class).
    #[error("invalid password options")]
    InvalidPasswordOptions,

    /// The vault file is structurally invalid (bad magic, unsupported version,
    /// malformed framing). Collapsed into one generic bucket for the UI.
    #[error("invalid vault file")]
    InvalidVaultFile,

    /// An unexpected internal error (I/O, low-level crypto, serialization).
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<VaultError> for AppError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::WrongPassword => AppError::WrongPassword,
            VaultError::EntryNotFound(id) => AppError::EntryNotFound(id),
            VaultError::InvalidPasswordOptions => AppError::InvalidPasswordOptions,
            VaultError::BadMagic
            | VaultError::UnsupportedVersion(_)
            | VaultError::UnsupportedKdf(_)
            | VaultError::UnsupportedCipher(_)
            | VaultError::MalformedFile
            | VaultError::InvalidKdfParams => AppError::InvalidVaultFile,
            // Io / Crypto / KeyDerivation / Serialization, plus any future
            // (non_exhaustive) variant, stringified into a generic bucket.
            other => AppError::Internal(other.to_string()),
        }
    }
}
