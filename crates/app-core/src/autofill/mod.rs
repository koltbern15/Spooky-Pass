//! Browser-autofill support.
//!
//! The security-critical "which site does this credential belong to?" decision
//! lives here, in Tauri-free Rust, so it is unit-tested exhaustively. Matching
//! is on the **registrable domain (eTLD+1)** computed from an embedded Public
//! Suffix List — the extension never bundles or consults the PSL; it sends a raw
//! page-origin URL and this module decides.
//!
//! * [`contract`] — the frozen IPC / native-messaging message types, mirrored
//!   by `extension/src/protocol.ts`.
//! * `matching` — the registrable-domain decision logic, adding the
//!   `find_matches`, `get_credential`, and `autofill_status` methods to
//!   [`crate::AppState`].

pub mod contract;

// Adds the registrable-domain matching methods (`find_matches`,
// `get_credential`, `autofill_status`) to `crate::AppState`. Exports no new
// types of its own.
mod matching;
