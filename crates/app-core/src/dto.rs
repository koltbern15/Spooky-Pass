//! Data-transfer objects that cross the Tauri command boundary.
//!
//! These types are the **canonical contract** between the Rust backend and the
//! TypeScript frontend. Their serde representation is camelCase and is mirrored
//! field-for-field by `crates/app/ui/src/types.ts` — keep the two in lockstep.

use serde::{Deserialize, Serialize};
use vault_core::PasswordOptions;

/// High-level application status, used by the UI to choose its screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    /// Whether a vault file exists on disk (`false` → first-run/create screen).
    pub has_vault: bool,
    /// Whether a vault is currently unlocked in memory.
    pub unlocked: bool,
    /// The idle auto-lock window, in seconds (for an optional UI countdown).
    pub idle_timeout_secs: u64,
}

/// A compact entry record for the list view. Deliberately omits the password so
/// secrets are not shipped to the UI on every list refresh.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    /// Stable, crate-assigned entry id.
    pub id: String,
    /// Entry title.
    pub title: String,
    /// Login username.
    pub username: String,
    /// First associated URL, if any.
    pub primary_url: Option<String>,
    /// RFC 3339 last-modified timestamp.
    pub updated: String,
}

/// A full entry record for the detail/edit view. Includes the password, which
/// the detail pane must be able to reveal and copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    /// Stable, crate-assigned entry id.
    pub id: String,
    /// Entry title.
    pub title: String,
    /// Login username.
    pub username: String,
    /// Login password.
    pub password: String,
    /// Associated URLs.
    pub urls: Vec<String>,
    /// Free-form notes.
    pub notes: String,
    /// RFC 3339 creation timestamp.
    pub created: String,
    /// RFC 3339 last-modified timestamp.
    pub updated: String,
}

/// Fields supplied by the UI to create a new entry. The backend assigns the id
/// and the created/updated timestamps.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    /// Entry title.
    pub title: String,
    /// Login username.
    pub username: String,
    /// Login password.
    pub password: String,
    /// Associated URLs.
    pub urls: Vec<String>,
    /// Free-form notes.
    pub notes: String,
}

/// A partial update supplied by the UI. `None` fields are left unchanged; the
/// backend refreshes the `updated` timestamp itself.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPatchInput {
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
}

/// Serializable mirror of [`vault_core::PasswordOptions`] (which has no serde
/// derive), used by the password-generator command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorOptionsDto {
    /// Number of characters to generate.
    pub length: usize,
    /// Include lowercase letters.
    pub lowercase: bool,
    /// Include uppercase letters.
    pub uppercase: bool,
    /// Include digits.
    pub digits: bool,
    /// Include symbols.
    pub symbols: bool,
    /// Exclude visually ambiguous characters (`l`, `I`, `O`, `0`, `1`).
    pub exclude_ambiguous: bool,
}

impl Default for GeneratorOptionsDto {
    fn default() -> Self {
        // Mirror `vault_core::PasswordOptions::default()` field-for-field so the
        // UI's defaults match the generator's. (Asserted by a unit test.)
        let d = PasswordOptions::default();
        GeneratorOptionsDto {
            length: d.length,
            lowercase: d.lowercase,
            uppercase: d.uppercase,
            digits: d.digits,
            symbols: d.symbols,
            exclude_ambiguous: d.exclude_ambiguous,
        }
    }
}

impl From<GeneratorOptionsDto> for PasswordOptions {
    fn from(o: GeneratorOptionsDto) -> Self {
        PasswordOptions {
            length: o.length,
            lowercase: o.lowercase,
            uppercase: o.uppercase,
            digits: o.digits,
            symbols: o.symbols,
            exclude_ambiguous: o.exclude_ambiguous,
        }
    }
}
