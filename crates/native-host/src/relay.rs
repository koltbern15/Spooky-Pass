//! The relay: pump native-messaging frames from the browser to the Core and
//! back.
//!
//! [`run`] is the production entry point — it reads the shared socket path from
//! `app-core`, then loops over stdin, forwarding each request to a freshly
//! connected Core stream and writing the reply to stdout until stdin reaches
//! EOF. The forwarding logic itself ([`relay_once`]) is generic over the
//! browser-side streams and a *connector* closure, so the unit tests drive it
//! with in-memory buffers and a fake Core stream — no real socket, no real
//! browser.
//!
//! ## Error mapping
//!
//! Failures never crash the host mid-session; they become a single
//! [`IpcResponse::Error`] frame so the extension can react:
//!
//! * Core socket absent / refused → `kind: "coreUnavailable"`.
//! * Unframable / non-JSON request, or a Core that replies with garbage →
//!   `kind: "protocol"`.
//!
//! The host never inspects or logs message *bodies*, so the one secret-bearing
//! reply ([`IpcResponse::Credential`]) passes through opaquely.

use std::io::{self, Read, Write};

use app_core::{IpcResponse, RequestEnvelope, ResponseEnvelope};

use crate::codec::{self, CodecError};

/// A top-level failure of the relay loop (as opposed to a per-message error,
/// which is reported in-band as an [`IpcResponse::Error`] frame).
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    /// An I/O error reading from or writing to the browser streams that the loop
    /// cannot recover from.
    #[error("browser I/O error: {0}")]
    BrowserIo(#[from] io::Error),

    /// Writing a reply frame to the browser failed.
    #[error("failed to write reply to the browser: {0}")]
    Reply(CodecError),
}

/// The `kind` reported when the Core cannot be reached.
const KIND_CORE_UNAVAILABLE: &str = "coreUnavailable";
/// The `kind` reported for a malformed/unframable request or a malformed Core
/// reply.
const KIND_PROTOCOL: &str = "protocol";

/// Build an `IpcResponse::Error` with the given kind and message.
fn error_response(kind: &str, message: impl Into<String>) -> IpcResponse {
    IpcResponse::Error {
        kind: kind.into(),
        message: Some(message.into()),
    }
}

/// Forward one request from `input` to the Core and write the reply to `output`.
///
/// Returns `Ok(true)` after handling exactly one message (whether it produced a
/// real reply or an in-band error frame), `Ok(false)` at a clean stdin EOF, and
/// `Err(_)` only for an unrecoverable browser-side I/O failure.
///
/// `connect` is invoked to obtain a fresh Core connection for this request; an
/// `Err` from it (socket missing/refused) is mapped to a `coreUnavailable`
/// reply rather than aborting the loop.
pub fn relay_once<I, O, C, S>(
    input: &mut I,
    output: &mut O,
    connect: &mut C,
) -> Result<bool, RelayError>
where
    I: Read,
    O: Write,
    C: FnMut() -> io::Result<S>,
    S: Read + Write,
{
    // Read the next framed request from the browser.
    let envelope: RequestEnvelope = match codec::read_message(input) {
        Ok(env) => env,
        Err(CodecError::Eof) => return Ok(false),
        Err(CodecError::Io(e)) => return Err(RelayError::BrowserIo(e)),
        Err(other) => {
            // Truncated / non-UTF-8 / invalid-JSON / too-large request: we have
            // no req_id to echo, so reply with req_id 0 and a protocol error.
            let reply = ResponseEnvelope {
                req_id: 0,
                response: error_response(KIND_PROTOCOL, other.to_string()),
            };
            write_reply(output, &reply)?;
            return Ok(true);
        }
    };

    let req_id = envelope.req_id;
    let response = forward_to_core(&envelope, connect);

    write_reply(output, &ResponseEnvelope { req_id, response })?;
    Ok(true)
}

/// Connect to the Core, send the inner request, and read the reply — mapping any
/// transport failure to an [`IpcResponse::Error`] rather than propagating it.
fn forward_to_core<C, S>(envelope: &RequestEnvelope, connect: &mut C) -> IpcResponse
where
    C: FnMut() -> io::Result<S>,
    S: Read + Write,
{
    let mut core = match connect() {
        Ok(stream) => stream,
        Err(e) => {
            return error_response(
                KIND_CORE_UNAVAILABLE,
                format!("cannot reach the Spooky-Pass Core: {e}"),
            );
        }
    };

    // Send the *inner* request over the Core leg (the Core speaks bare
    // IpcRequest/IpcResponse, not the native-messaging envelope).
    if let Err(e) = codec::write_message(&mut core, &envelope.request) {
        return error_response(
            KIND_CORE_UNAVAILABLE,
            format!("failed to send the request to the Core: {e}"),
        );
    }

    match codec::read_message::<_, IpcResponse>(&mut core) {
        Ok(resp) => resp,
        Err(CodecError::Eof | CodecError::Truncated) => error_response(
            KIND_CORE_UNAVAILABLE,
            "the Core closed the connection before replying",
        ),
        Err(e) => error_response(KIND_PROTOCOL, format!("malformed reply from the Core: {e}")),
    }
}

/// Write a response envelope to the browser, mapping a framing failure to
/// [`RelayError::Reply`].
fn write_reply<O: Write>(output: &mut O, reply: &ResponseEnvelope) -> Result<(), RelayError> {
    codec::write_message(output, reply).map_err(RelayError::Reply)
}

/// Run the relay loop over the given browser streams, connecting to the Core
/// afresh for each request via `connect`, until stdin reaches EOF.
///
/// Returns `Ok(())` on a clean EOF shutdown.
pub fn run_loop<I, O, C, S>(mut input: I, mut output: O, mut connect: C) -> Result<(), RelayError>
where
    I: Read,
    O: Write,
    C: FnMut() -> io::Result<S>,
    S: Read + Write,
{
    while relay_once(&mut input, &mut output, &mut connect)? {}
    Ok(())
}

/// Production entry point: relay between the browser (stdin/stdout) and the
/// resident Core (local socket at [`app_core::autofill_socket_path`]) until
/// stdin EOF.
///
/// A fresh Core connection is made per request; if the Core is down, each
/// request simply gets a `coreUnavailable` reply and the loop keeps serving.
pub fn run() -> Result<(), RelayError> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, Stream as LocalStream};

    let socket_path = app_core::autofill_socket_path();

    let stdin = io::stdin();
    let stdout = io::stdout();
    // Lock once for the lifetime of the loop; native messaging is strictly
    // request/response on a single connection.
    let input = stdin.lock();
    let output = stdout.lock();

    run_loop(input, output, move || {
        // Build the local-socket name from the shared path and connect. A
        // missing/refused socket surfaces here as an io::Error, which the relay
        // turns into a `coreUnavailable` reply.
        let name = socket_path
            .clone()
            .to_fs_name::<GenericFilePath>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        LocalStream::connect(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Cursor;
    use std::rc::Rc;

    use app_core::{
        AutofillStatusDto, CredentialDto, IpcRequest, IpcResponse, MatchCandidate, RequestEnvelope,
    };
    use assert_matches::assert_matches;

    /// A fake Core stream: a `Cursor` of canned reply bytes to read, and a
    /// **shared** buffer capturing what the host wrote (the request leg), so a
    /// test can inspect the forwarded request after moving the stream into the
    /// relay. Implements `Read + Write`.
    struct FakeCore {
        to_read: Cursor<Vec<u8>>,
        written: Rc<RefCell<Vec<u8>>>,
    }

    impl FakeCore {
        /// A fake Core that will reply with `response` (one framed message). The
        /// returned capture handle records every byte the host writes to it.
        fn replying(response: &IpcResponse) -> (Self, Rc<RefCell<Vec<u8>>>) {
            let mut buf = Vec::new();
            codec::write_message(&mut buf, response).unwrap();
            let written = Rc::new(RefCell::new(Vec::new()));
            let core = FakeCore {
                to_read: Cursor::new(buf),
                written: Rc::clone(&written),
            };
            (core, written)
        }

        /// A fake Core whose reply stream contains exactly `bytes` (for malformed
        /// / truncated reply tests).
        fn with_reply_bytes(bytes: Vec<u8>) -> Self {
            FakeCore {
                to_read: Cursor::new(bytes),
                written: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    /// Decode the single framed [`IpcRequest`] captured in a write buffer.
    fn captured_request(written: &Rc<RefCell<Vec<u8>>>) -> IpcRequest {
        let mut cursor = Cursor::new(written.borrow().clone());
        codec::read_message(&mut cursor).unwrap()
    }

    impl Read for FakeCore {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.to_read.read(buf)
        }
    }

    impl Write for FakeCore {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Frame a request envelope into browser-side stdin bytes.
    fn framed_request(env: &RequestEnvelope) -> Vec<u8> {
        let mut buf = Vec::new();
        codec::write_message(&mut buf, env).unwrap();
        buf
    }

    /// Decode the single response envelope the host wrote to stdout.
    fn decode_reply(stdout: &[u8]) -> ResponseEnvelope {
        let mut cursor = Cursor::new(stdout.to_vec());
        codec::read_message(&mut cursor).unwrap()
    }

    #[test]
    fn forwards_request_and_relays_reply_echoing_req_id() {
        let request = RequestEnvelope {
            req_id: 99,
            request: IpcRequest::GetMatches {
                url: "https://github.com".into(),
            },
        };
        let core_reply = IpcResponse::Matches {
            matches: vec![MatchCandidate {
                id: "1".into(),
                title: "GitHub".into(),
                username: "octocat".into(),
                primary_url: Some("https://github.com".into()),
            }],
        };

        let mut input = Cursor::new(framed_request(&request));
        let mut output: Vec<u8> = Vec::new();

        let (core, _written) = FakeCore::replying(&core_reply);
        let mut core = Some(core);
        let mut connect = || core.take().ok_or_else(|| io::Error::other("one-shot core"));

        let more = relay_once(&mut input, &mut output, &mut connect).unwrap();
        assert!(more, "handled one message");

        let reply = decode_reply(&output);
        assert_eq!(reply.req_id, 99, "req_id echoed");
        assert_matches!(reply.response, IpcResponse::Matches { matches } if matches.len() == 1);
    }

    #[test]
    fn inner_request_is_what_reaches_the_core() {
        // The host must forward the *inner* `IpcRequest` (bare, no envelope) to
        // the Core, and relay the password-bearing reply through opaquely.
        let request = RequestEnvelope {
            req_id: 5,
            request: IpcRequest::GetCredential { id: "abc".into() },
        };
        let core_reply = IpcResponse::Credential(CredentialDto {
            id: "abc".into(),
            username: "u".into(),
            password: "p".into(),
        });

        let mut input = Cursor::new(framed_request(&request));
        let mut output: Vec<u8> = Vec::new();

        // `written` is shared with the FakeCore, so after the relay moves the
        // stream in and drives it we can still inspect what it forwarded.
        let (core, written) = FakeCore::replying(&core_reply);
        let mut core = Some(core);
        let mut connect = || core.take().ok_or_else(|| io::Error::other("one-shot core"));

        relay_once(&mut input, &mut output, &mut connect).unwrap();

        // The bytes the relay wrote to the Core decode to the bare GetCredential
        // — the envelope was stripped before forwarding.
        assert_matches!(
            captured_request(&written),
            IpcRequest::GetCredential { id } if id == "abc"
        );

        // And the relayed reply carries the password through opaquely.
        let reply = decode_reply(&output);
        assert_matches!(reply.response, IpcResponse::Credential(c) if c.password == "p");
        assert_eq!(reply.req_id, 5);
    }

    #[test]
    fn core_connect_failure_maps_to_core_unavailable() {
        let request = RequestEnvelope {
            req_id: 3,
            request: IpcRequest::GetStatus,
        };
        let mut input = Cursor::new(framed_request(&request));
        let mut output: Vec<u8> = Vec::new();

        // Connector always fails: simulates the Core being down.
        let mut connect =
            || Err::<FakeCore, _>(io::Error::new(io::ErrorKind::ConnectionRefused, "no core"));

        relay_once(&mut input, &mut output, &mut connect).unwrap();

        let reply = decode_reply(&output);
        assert_eq!(reply.req_id, 3, "still echoes the req_id");
        assert_matches!(
            reply.response,
            IpcResponse::Error { kind, .. } if kind == "coreUnavailable"
        );
    }

    #[test]
    fn core_closing_before_reply_maps_to_core_unavailable() {
        let request = RequestEnvelope {
            req_id: 8,
            request: IpcRequest::GetStatus,
        };
        let mut input = Cursor::new(framed_request(&request));
        let mut output: Vec<u8> = Vec::new();

        // Core accepts the connection but sends back nothing (clean EOF).
        let mut connect = || Ok::<_, io::Error>(FakeCore::with_reply_bytes(Vec::new()));

        relay_once(&mut input, &mut output, &mut connect).unwrap();

        let reply = decode_reply(&output);
        assert_matches!(
            reply.response,
            IpcResponse::Error { kind, .. } if kind == "coreUnavailable"
        );
    }

    #[test]
    fn malformed_core_reply_maps_to_protocol_error() {
        let request = RequestEnvelope {
            req_id: 11,
            request: IpcRequest::GetStatus,
        };
        let mut input = Cursor::new(framed_request(&request));
        let mut output: Vec<u8> = Vec::new();

        // A well-framed but non-JSON body from the Core.
        let mut bad = Vec::new();
        let body = b"not json at all";
        bad.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bad.extend_from_slice(body);
        let mut connect = || Ok::<_, io::Error>(FakeCore::with_reply_bytes(bad.clone()));

        relay_once(&mut input, &mut output, &mut connect).unwrap();

        let reply = decode_reply(&output);
        assert_matches!(
            reply.response,
            IpcResponse::Error { kind, .. } if kind == "protocol"
        );
    }

    #[test]
    fn malformed_request_frame_maps_to_protocol_error_with_req_id_zero() {
        // A well-framed but non-envelope JSON body from the browser.
        let mut input_bytes = Vec::new();
        let body = b"{\"not\":\"an envelope\"}";
        input_bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        input_bytes.extend_from_slice(body);
        let mut input = Cursor::new(input_bytes);
        let mut output: Vec<u8> = Vec::new();

        // The connector must never be called for a malformed request.
        let mut connect = || -> io::Result<FakeCore> {
            panic!("connect must not be called on a malformed request");
        };

        let more = relay_once(&mut input, &mut output, &mut connect).unwrap();
        assert!(more);

        let reply = decode_reply(&output);
        assert_eq!(reply.req_id, 0, "no req_id available -> 0");
        assert_matches!(
            reply.response,
            IpcResponse::Error { kind, .. } if kind == "protocol"
        );
    }

    #[test]
    fn clean_eof_ends_the_loop() {
        let mut input = Cursor::new(Vec::new());
        let mut output: Vec<u8> = Vec::new();
        let mut connect = || -> io::Result<FakeCore> { panic!("never connects on empty input") };

        let more = relay_once(&mut input, &mut output, &mut connect).unwrap();
        assert!(!more, "empty stdin is a clean shutdown");
        assert!(output.is_empty(), "no reply written on clean EOF");
    }

    #[test]
    fn run_loop_handles_multiple_then_eof() {
        // Two requests back to back, then EOF.
        let r1 = RequestEnvelope {
            req_id: 1,
            request: IpcRequest::GetStatus,
        };
        let r2 = RequestEnvelope {
            req_id: 2,
            request: IpcRequest::GetMatches {
                url: "https://example.com".into(),
            },
        };
        let mut input_bytes = framed_request(&r1);
        input_bytes.extend(framed_request(&r2));
        let input = Cursor::new(input_bytes);
        let mut output: Vec<u8> = Vec::new();

        let status = IpcResponse::Status(AutofillStatusDto {
            unlocked: true,
            has_vault: true,
        });
        let matches = IpcResponse::Matches {
            matches: Vec::new(),
        };
        // Alternate the canned reply per call.
        let mut call = 0u32;
        let connect = || {
            call += 1;
            let resp = if call == 1 { &status } else { &matches };
            let (core, _written) = FakeCore::replying(resp);
            Ok::<_, io::Error>(core)
        };

        run_loop(input, &mut output, connect).unwrap();

        // Two reply frames, in order, echoing req_ids 1 and 2.
        let mut cursor = Cursor::new(output);
        let reply1: ResponseEnvelope = codec::read_message(&mut cursor).unwrap();
        let reply2: ResponseEnvelope = codec::read_message(&mut cursor).unwrap();
        assert_eq!(reply1.req_id, 1);
        assert_matches!(reply1.response, IpcResponse::Status(_));
        assert_eq!(reply2.req_id, 2);
        assert_matches!(reply2.response, IpcResponse::Matches { .. });
        assert_matches!(
            codec::read_message::<_, ResponseEnvelope>(&mut cursor),
            Err(CodecError::Eof)
        );
    }
}
