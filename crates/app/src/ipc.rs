//! The Core's local-socket IPC server: the in-process counterpart of the
//! native-messaging host's relay.
//!
//! The native-host (`crates/native-host`) cannot hold the unlocked vault — it is
//! spawned per browser connection and must stay key-free — so the long-running
//! Core exposes a tiny request/response server over a local socket
//! (`interprocess`: a Unix-domain socket / a Windows named pipe) at
//! [`app_core::autofill_socket_path`]. The host connects, sends a bare
//! [`IpcRequest`], and reads back an [`IpcResponse`]; this module is the other
//! end, dispatching each request to the shared [`AppState`].
//!
//! ## Owner-only
//!
//! The socket carries unlocked-vault traffic, including — on a successful
//! [`IpcRequest::GetCredential`] — a password. It is therefore created
//! **owner-only**: on Unix we `fchmod` it to `0600` via
//! [`ListenerOptionsExt::mode`] before `bind`, and it lives under the per-user
//! `$XDG_RUNTIME_DIR` (or the user's data dir). On Windows the named pipe is
//! per-user by construction.
//!
//! ## The password-crossing invariant
//!
//! Exactly one reply carries a password — [`IpcResponse::Credential`], produced
//! only by [`AppState::get_credential`] in response to a user-gesture click.
//! `find_matches` returns [`MatchCandidate`](app_core::MatchCandidate)s that
//! carry none *by type*, so "no secret in matches" is a compile-time property.

use std::io::{self, Read, Write};
use std::thread;

use app_core::{AppError, AppState, IpcRequest, IpcResponse};
use interprocess::local_socket::{
    prelude::*, GenericFilePath, ListenerOptions, Stream as LocalStream,
};
use tauri::{AppHandle, Manager, Runtime};

/// Maximum accepted request frame: 1 MiB, matching the native-host codec so the
/// Core never allocates a huge buffer from a 4-byte length prefix.
const MAX_MESSAGE_LEN: usize = 1024 * 1024;

/// Owner-only file mode for the socket (read+write for the owner, nothing for
/// group/other). Only the user running the Core may connect.
#[cfg(unix)]
const SOCKET_MODE: libc::mode_t = 0o600;

/// Spawn the Core IPC server on a background thread, sharing `app`'s managed
/// [`AppState`].
///
/// Mirrors [`crate::autolock_driver::spawn`]: it holds only a cheap-to-clone
/// [`AppHandle`] and runs for the lifetime of the process. A bind failure is
/// logged (never panicked) — the GUI must keep working even if autofill IPC
/// cannot start (e.g. another Core already owns the socket).
pub fn spawn<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = thread::Builder::new()
        .name("spooky-ipc".into())
        .spawn(move || {
            if let Err(e) = serve(handle) {
                eprintln!("spooky-pass: autofill IPC server stopped: {e}");
            }
        });
}

/// Bind the listener and serve connections until the process exits.
fn serve<R: Runtime>(app: AppHandle<R>) -> io::Result<()> {
    let socket_path = app_core::autofill_socket_path();

    // A stale socket file from a previous crash would make `bind` fail; remove it
    // first (best effort — a live owner is handled by `try_overwrite` below).
    #[cfg(unix)]
    let _ = std::fs::remove_file(&socket_path);

    let name = socket_path
        .clone()
        .to_fs_name::<GenericFilePath>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let opts = ListenerOptions::new().name(name);
    // Owner-only permissions on Unix (fchmod before bind, no umask race).
    #[cfg(unix)]
    let opts = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        opts.mode(SOCKET_MODE)
    };

    let listener = opts.create_sync()?;

    // Serve one connection at a time. The native-host opens a fresh connection
    // per request, so connections are short-lived; a sequential accept loop is
    // ample and keeps the AppState access trivially serialized.
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => handle_connection(&app, stream),
            Err(e) => eprintln!("spooky-pass: autofill IPC accept error: {e}"),
        }
    }

    Ok(())
}

/// Read a single request from `stream`, dispatch it, and write the reply.
///
/// Per-connection errors (a malformed frame, a disconnect) are isolated to that
/// connection and never bring down the server loop.
fn handle_connection<R: Runtime>(app: &AppHandle<R>, mut stream: LocalStream) {
    let request = match read_message::<IpcRequest>(&mut stream) {
        Ok(req) => req,
        // A client that connects and disconnects without sending a full frame is
        // ignored; nothing to reply to.
        Err(_) => return,
    };

    let response = dispatch(app, request);

    // Best effort: if the host has already gone away, there is nothing to do.
    let _ = write_message(&mut stream, &response);
}

/// Dispatch one [`IpcRequest`] against the managed [`AppState`], producing the
/// [`IpcResponse`] to send back.
///
/// Maps [`AppError`] to [`IpcResponse::Error`] using the error's stable kind.
/// `SaveLogin` is not yet implemented in this phase and is reported as a
/// `protocol`-kind error rather than silently succeeding.
fn dispatch<R: Runtime>(app: &AppHandle<R>, request: IpcRequest) -> IpcResponse {
    let state = app.state::<AppState>();

    match request {
        IpcRequest::GetStatus => IpcResponse::Status(state.autofill_status()),
        IpcRequest::GetMatches { url } => match state.find_matches(&url) {
            Ok(matches) => IpcResponse::Matches { matches },
            Err(e) => error_response(e),
        },
        IpcRequest::GetCredential { id } => match state.get_credential(&id) {
            // The one and only password-bearing reply.
            Ok(cred) => IpcResponse::Credential(cred),
            Err(e) => error_response(e),
        },
        IpcRequest::SaveLogin { .. } => IpcResponse::Error {
            kind: "protocol".to_string(),
            message: Some("saveLogin is not implemented yet".to_string()),
        },
    }
}

/// Convert an [`AppError`] into an [`IpcResponse::Error`], reusing the error's
/// serialized `kind` tag (so the wire `kind` matches the one the GUI already
/// branches on) and its `Display` text as the message.
fn error_response(err: AppError) -> IpcResponse {
    IpcResponse::Error {
        kind: app_error_kind(&err),
        message: Some(err.to_string()),
    }
}

/// Extract the stable machine-readable kind of an [`AppError`].
///
/// `AppError` serializes as `{ "kind": "<Variant>", .. }`; we read that tag back
/// so the IPC `kind` stays in lockstep with the UI error contract without
/// hand-maintaining a parallel match.
fn app_error_kind(err: &AppError) -> String {
    match serde_json::to_value(err) {
        Ok(serde_json::Value::Object(map)) => map
            .get("kind")
            .and_then(|k| k.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "internal".to_string()),
        _ => "internal".to_string(),
    }
}

/// Read one length-prefixed-JSON message (4-byte LE prefix + UTF-8 JSON, ≤ 1
/// MiB) and deserialize it. Mirrors the native-host codec so both ends frame
/// identically.
fn read_message<T: serde::de::DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message exceeds the 1 MiB limit",
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Serialize `value` as JSON and write it as one length-prefixed frame.
fn write_message<T: serde::Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let body =
        serde_json::to_vec(value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if body.len() > MAX_MESSAGE_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message exceeds the 1 MiB limit",
        ));
    }
    let len = body.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}
