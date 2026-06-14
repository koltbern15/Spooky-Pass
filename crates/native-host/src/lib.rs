//! Spooky-Pass native-messaging host — the stdio ↔ Core-IPC relay.
//!
//! The browser launches this binary and speaks **Chrome native messaging** to it
//! on stdin/stdout: each message is a 4-byte little-endian `u32` length prefix
//! followed by that many bytes of UTF-8 JSON. The host frames that traffic,
//! forwards the inner [`IpcRequest`] to the resident Core over a local socket
//! (length-prefixed JSON on that leg too), and relays the Core's [`IpcResponse`]
//! back, echoing the request's correlation id.
//!
//! The host holds **no vault and no key** — it depends on `app-core` only for the
//! frozen [`contract`](app_core::autofill::contract) message types and the shared
//! socket-path helper. It never logs message bodies, so a password (which can
//! ride along in exactly one reply, [`IpcResponse::Credential`]) is never written
//! anywhere.
//!
//! ## Module layout
//!
//! * [`codec`] — the length-prefixed-JSON framing used on **both** legs, with the
//!   1 MiB cap and clean errors on truncation / non-UTF-8 / invalid JSON. Pure
//!   and exhaustively unit-tested over in-memory buffers.
//! * [`relay`] — wires stdin/stdout to a Core connection (or any `Read + Write`),
//!   mapping a missing/refused Core to a `coreUnavailable` error and looping until
//!   stdin EOF.

pub mod codec;
pub mod relay;

pub use codec::{CodecError, MAX_MESSAGE_LEN};
pub use relay::{run, RelayError};
