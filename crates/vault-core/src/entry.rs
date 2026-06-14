//! Vault entry types and the serializable vault body.
//!
//! An [`Entry`] is a stored login credential. The crate owns each entry's `id`
//! and its `created` / `updated` timestamps; callers supply the remaining
//! fields via [`NewEntry`] (on insert) and [`EntryPatch`] (on update).
//!
//! Timestamps are RFC 3339 strings *supplied by the caller* — `vault-core`
//! deliberately depends on no time crate, so the app layer decides what "now"
//! means.

use serde::{Deserialize, Serialize};

/// The current vault-body schema version (the JSON `version` field).
pub const VAULT_BODY_VERSION: u32 = 1;

/// A stored login credential.
///
/// `id`, `created`, and `updated` are managed by `vault-core`; the other fields
/// come from the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable, crate-assigned identifier: 32 lowercase hex chars (16 random
    /// bytes).
    pub id: String,
    /// Human-readable title (e.g. "GitHub").
    pub title: String,
    /// The login username / email.
    pub username: String,
    /// The login password.
    pub password: String,
    /// Associated URLs for autofill matching.
    pub urls: Vec<String>,
    /// Free-form notes.
    pub notes: String,
    /// RFC 3339 creation timestamp, supplied by the caller at insert time.
    pub created: String,
    /// RFC 3339 last-modified timestamp, bumped on each update.
    pub updated: String,
}

/// Caller-supplied fields for a new entry.
///
/// The crate fills in `id`, `created`, and `updated`; the caller provides the
/// "now" timestamp it wants recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    /// Human-readable title.
    pub title: String,
    /// The login username / email.
    pub username: String,
    /// The login password.
    pub password: String,
    /// Associated URLs.
    pub urls: Vec<String>,
    /// Free-form notes.
    pub notes: String,
    /// RFC 3339 timestamp to record as both `created` and `updated`.
    pub now: String,
}

/// A partial update to an existing entry.
///
/// Every field is optional; `Some(_)` replaces the corresponding field and
/// `None` leaves it untouched. The crate refreshes `updated` itself using the
/// supplied `now` timestamp; `id` and `created` are never changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryPatch {
    /// New title, if changing.
    pub title: Option<String>,
    /// New username, if changing.
    pub username: Option<String>,
    /// New password, if changing.
    pub password: Option<String>,
    /// New URL list, if changing.
    pub urls: Option<Vec<String>>,
    /// New notes, if changing.
    pub notes: Option<String>,
    /// RFC 3339 timestamp to record as the new `updated` value.
    pub now: String,
}

/// The decrypted, serializable vault body.
///
/// This is exactly what is JSON-encoded and then sealed into the vault file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultBody {
    /// Schema version of the body (see [`VAULT_BODY_VERSION`]).
    pub version: u32,
    /// All stored entries, in insertion order.
    pub entries: Vec<Entry>,
}

impl Default for VaultBody {
    fn default() -> Self {
        VaultBody {
            version: VAULT_BODY_VERSION,
            entries: Vec::new(),
        }
    }
}
