//! # vault-core
//!
//! The cryptographic core of [Spooky-Pass](https://github.com/) — a local-first
//! password manager. This crate owns the **encrypted vault file format**, the
//! **Argon2id + XChaCha20-Poly1305** key-derivation and encryption, the
//! lock/unlock **typestate**, entry CRUD, and a CSPRNG-backed **password
//! generator**. It performs no UI, no IPC, and runs no background timers.
//!
//! ## Quick tour
//!
//! ```
//! use vault_core::{LockedVault, KdfParams, NewEntry};
//!
//! // Create a fresh, unlocked vault (uses a random salt + nonce).
//! let mut vault = LockedVault::create("correct horse battery staple",
//!                                     KdfParams::default()).unwrap();
//!
//! // Add a credential. The caller supplies the RFC 3339 "now" timestamp; the
//! // crate assigns the id and the created/updated stamps.
//! let id = vault.add_entry(NewEntry {
//!     title: "GitHub".into(),
//!     username: "octocat".into(),
//!     password: "hunter2".into(),
//!     urls: vec!["https://github.com".into()],
//!     notes: String::new(),
//!     now: "2026-06-14T00:00:00Z".into(),
//! }).unwrap().id.clone();
//!
//! // Serialize + seal (fresh nonce each call).
//! let bytes = vault.to_bytes().unwrap();
//!
//! // Re-open and unlock.
//! let locked = LockedVault::open_from_bytes(&bytes).unwrap();
//! let reopened = vault_core::UnlockedVault::unlock(&locked,
//!                    "correct horse battery staple").unwrap();
//! assert_eq!(reopened.get_entry(&id).unwrap().title, "GitHub");
//! ```
//!
//! ## Security model
//!
//! * The master password is borrowed as `&str` and **never** copied to the heap
//!   or stored by this crate; the caller owns and zeroizes it.
//! * The derived key lives only inside an [`UnlockedVault`] and is zeroized on
//!   drop / [`UnlockedVault::lock`].
//! * Every AEAD-open failure during [`UnlockedVault::unlock`] is reported as
//!   [`VaultError::WrongPassword`]; a wrong password and a tampered vault are
//!   cryptographically indistinguishable and we do not leak which occurred.
//! * Memory hardening of individual entry fields is **out of scope** per the
//!   project threat model; the aggregate plaintext JSON buffer is wiped, but
//!   entry `String`s are plain.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod crypto;
mod entry;
mod error;
mod format;
mod generator;
mod session;
mod vault;

// ---- Public format constants -------------------------------------------------

pub use format::{
    ARGON2_VERSION, CIPHER_ID_XCHACHA20POLY1305, FORMAT_VERSION, HEADER_LEN, KDF_ID_ARGON2ID,
    MAGIC, NONCE_LEN, SALT_LEN,
};

// ---- Public API --------------------------------------------------------------

pub use crypto::KdfParams;
pub use entry::{Entry, EntryPatch, NewEntry};
pub use error::{Result, VaultError};
pub use generator::{generate_password, PasswordOptions};
pub use vault::{LockedVault, UnlockedVault};
