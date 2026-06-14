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
//! * [`clock`] — the [`Clock`] abstraction (real and, in tests, fake time).
//! * [`state`] — [`AppState`]: the mutex-guarded session the Tauri layer manages.
//! * `service` — the operations the Tauri commands wrap (methods on `AppState`).
//! * `autolock` — pure, clock-driven idle auto-lock decision logic.
//! * [`paths`] — vault-file location on disk.
//! * [`testing`] — a deterministic [`testing::FakeClock`] for unit/integration
//!   tests (no real sleeps, no real data dir).

#![forbid(unsafe_code)]

pub mod clock;
pub mod dto;
pub mod error;
pub mod paths;
pub mod state;
pub mod testing;

// Autofill: the `contract` submodule holds the frozen IPC / native-messaging
// message types (mirrored by `extension/src/protocol.ts`); the module also adds
// the registrable-domain matching methods to `AppState`.
pub mod autofill;

// These modules add inherent methods to `AppState` (and the auto-lock decision
// logic); they export no new types of their own.
mod autolock;
mod service;

pub use clock::{Clock, SystemClock};
pub use dto::{
    EntryInput, EntryPatchInput, EntrySummary, EntryView, GeneratorOptionsDto, StatusDto,
};
pub use error::{AppError, Result};
pub use paths::{autofill_socket_path, resolve_vault_path};
pub use state::AppState;

pub use autofill::contract::{
    AutofillStatusDto, CredentialDto, IpcRequest, IpcResponse, MatchCandidate, RequestEnvelope,
    ResponseEnvelope,
};
