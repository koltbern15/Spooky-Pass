//! # spooky-pass-app-core
//!
//! The GUI-independent core of the Spooky-Pass desktop app. It wraps
//! [`vault_core`] with the application session model, the auto-lock decision
//! logic (driven by an injectable clock), vault-file location, and the service
//! functions that the Tauri commands are thin wrappers over.
//!
//! This crate has **no `tauri` dependency**, so it builds and is unit-tested on
//! any platform — including CI and dev environments without a webview toolchain.
//!
//! ## Module map
//!
//! * [`dto`] — the serializable data-transfer objects that cross the Tauri
//!   command boundary. Mirrored by `crates/app/ui/src/types.ts`.
//! * [`error`] — [`AppError`], the serializable error surfaced to the UI.
//!
//! The session/service/auto-lock/clock/paths modules are added by the Phase 2
//! backend implementation; this file currently anchors the frozen DTO + error
//! contract shared with the frontend.

#![forbid(unsafe_code)]

pub mod dto;
pub mod error;

pub use dto::{
    EntryInput, EntryPatchInput, EntrySummary, EntryView, GeneratorOptionsDto, StatusDto,
};
pub use error::{AppError, Result};
