//! Error and result types for `vault-core`.

use thiserror::Error;

/// Errors returned by `vault-core` operations.
///
/// This type is intentionally NOT `PartialEq`: it wraps [`std::io::Error`],
/// which is not comparable. Tests should match on variants (e.g. with
/// `assert_matches!`) rather than compare for equality.
///
/// # Security note
///
/// [`VaultError::WrongPassword`] is returned for *every* AEAD-open failure
/// during unlock — both an incorrect master password and a tampered/corrupted
/// vault body. These cases are cryptographically indistinguishable from the
/// outside, and we deliberately do not leak which one occurred.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VaultError {
    /// The master password was wrong, or the vault body failed authentication
    /// (tampering / corruption). These cases are intentionally merged.
    #[error("incorrect master password or corrupted vault")]
    WrongPassword,

    /// A low-level cryptographic operation failed for a reason other than
    /// authentication (e.g. an unexpected sealing error).
    #[error("cryptographic operation failed")]
    Crypto,

    /// The file did not start with the expected `SPK1` magic bytes.
    #[error("not a Spooky-Pass vault (bad magic)")]
    BadMagic,

    /// The on-disk `format_version` is not supported by this build.
    #[error("unsupported vault format version: {0}")]
    UnsupportedVersion(u16),

    /// The header names a key-derivation function this build does not support.
    #[error("unsupported KDF id: {0}")]
    UnsupportedKdf(u8),

    /// The header names a cipher this build does not support.
    #[error("unsupported cipher id: {0}")]
    UnsupportedCipher(u8),

    /// The file structure is malformed (truncated, wrong lengths, trailing
    /// garbage, etc.).
    #[error("malformed vault file")]
    MalformedFile,

    /// The supplied Argon2 parameters are invalid / out of range.
    #[error("invalid KDF parameters")]
    InvalidKdfParams,

    /// Deriving the key from the password failed.
    #[error("key derivation failed")]
    KeyDerivation,

    /// (De)serialization of the vault body failed.
    #[error("vault body serialization failed")]
    Serialization,

    /// No entry with the given id exists in the vault.
    #[error("entry not found: {0}")]
    EntryNotFound(String),

    /// The password-generator options were invalid (e.g. zero length, or no
    /// character classes selected).
    #[error("invalid password-generator options")]
    InvalidPasswordOptions,

    /// An I/O error occurred while reading or writing a vault file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience alias for results produced by `vault-core`.
pub type Result<T> = core::result::Result<T, VaultError>;
