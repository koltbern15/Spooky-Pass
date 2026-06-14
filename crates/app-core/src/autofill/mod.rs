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
//! * The matching methods on [`crate::AppState`] (`find_matches`,
//!   `get_credential`, `autofill_status`) are added by the Phase 3 backend
//!   implementation.

pub mod contract;
