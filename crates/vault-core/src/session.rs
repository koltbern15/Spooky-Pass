//! In-memory unlocked session state.
//!
//! A [`Session`] owns the [`DerivedKey`] for an open vault along with the
//! immutable key-derivation inputs (salt + [`KdfParams`]) needed to re-seal the
//! body on save. It is the *only* place the key lives at runtime.
//!
//! This module deliberately contains **no timer and no activity tracking**.
//! Auto-lock is an application-layer concern: the app decides when to call
//! [`crate::UnlockedVault::lock`], which drops the session and zeroizes the
//! key. `vault-core` only provides the boundary (explicit, instant lock); it
//! never starts background threads or holds global/static state.

use crate::crypto::{DerivedKey, KdfParams};
use crate::format::SALT_LEN;

/// The secret-bearing state of an unlocked vault.
///
/// Holds the derived key plus the inputs needed to re-derive it (salt) and to
/// re-encode the header on save ([`KdfParams`]). The key is wiped on drop via
/// [`DerivedKey`]'s `ZeroizeOnDrop`.
pub struct Session {
    key: DerivedKey,
    salt: [u8; SALT_LEN],
    params: KdfParams,
}

impl Session {
    /// Create a new session from a derived key and its derivation inputs.
    pub(crate) fn new(key: DerivedKey, salt: [u8; SALT_LEN], params: KdfParams) -> Self {
        Session { key, salt, params }
    }

    /// Borrow the derived key (e.g. to seal/open the vault body).
    pub(crate) fn key(&self) -> &DerivedKey {
        &self.key
    }

    /// The per-vault salt used to derive the key.
    pub(crate) fn salt(&self) -> &[u8; SALT_LEN] {
        &self.salt
    }

    /// The Argon2 parameters used to derive the key.
    pub(crate) fn params(&self) -> &KdfParams {
        &self.params
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // `DerivedKey` zeroizes its bytes on drop; the salt and params are not
        // secret. This explicit impl documents that dropping a `Session` is the
        // instant-lock primitive.
    }
}

// A `Session` must never reveal secrets through `Debug`.
impl core::fmt::Debug for Session {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Session")
            .field("key", &"<redacted>")
            .field("params", &self.params)
            .finish_non_exhaustive()
    }
}
