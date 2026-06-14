//! Length-prefixed-JSON framing.
//!
//! Both wire legs use the same frame: a **4-byte little-endian `u32` length
//! prefix** followed by exactly that many bytes of **UTF-8 JSON**. This is the
//! Chrome native-messaging wire format on the browser leg, and we reuse it
//! verbatim on the Core-IPC leg so there is a single codec to test.
//!
//! ## Safety properties
//!
//! * **1 MiB cap.** A length prefix greater than [`MAX_MESSAGE_LEN`] is rejected
//!   *before* any buffer is allocated, so a hostile or buggy peer can never make
//!   the host reserve gigabytes from a 4-byte header.
//! * **Clean EOF.** A clean stream end *between* messages (zero bytes read for
//!   the prefix) is reported as [`CodecError::Eof`] so the relay loop can exit
//!   0; a stream that dies *mid-frame* is [`CodecError::Truncated`].
//! * **No partial/invalid data leaks.** Non-UTF-8 and invalid-JSON bodies are
//!   surfaced as typed errors, never panics, and the payload bytes are never
//!   logged.

use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Maximum accepted frame body size: 1 MiB. Chrome's own native-messaging limit
/// for host→browser messages is 1 MiB, and our payloads are tiny, so anything
/// larger is treated as a protocol error rather than allocated.
pub const MAX_MESSAGE_LEN: usize = 1024 * 1024;

/// An error framing or parsing a length-prefixed-JSON message.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// The stream ended cleanly between messages (a normal end-of-input, not a
    /// failure): zero bytes were available when the next length prefix was
    /// expected.
    #[error("end of stream")]
    Eof,

    /// The stream ended in the middle of a frame (after a partial length prefix
    /// or a partial body) — the peer disconnected mid-message.
    #[error("truncated message")]
    Truncated,

    /// The declared body length exceeds [`MAX_MESSAGE_LEN`]; rejected without
    /// allocating the body.
    #[error("message length {0} exceeds the {MAX_MESSAGE_LEN}-byte limit")]
    TooLarge(u32),

    /// The body was not valid UTF-8.
    #[error("message body is not valid UTF-8")]
    NotUtf8,

    /// The body was valid UTF-8 but not a valid JSON value of the expected type.
    #[error("message body is not valid JSON")]
    InvalidJson,

    /// An underlying I/O error on the stream.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

impl CodecError {
    /// Map a prefix/body read that may have hit end-of-stream: a clean EOF at a
    /// frame boundary is [`CodecError::Eof`]; an unexpected EOF mid-frame is
    /// [`CodecError::Truncated`]; any other I/O error passes through.
    fn from_read(err: io::Error, at_frame_boundary: bool) -> Self {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            if at_frame_boundary {
                CodecError::Eof
            } else {
                CodecError::Truncated
            }
        } else {
            CodecError::Io(err)
        }
    }
}

/// Read one length-prefixed-JSON message from `reader` and deserialize it into
/// `T`.
///
/// Returns [`CodecError::Eof`] when the stream is cleanly exhausted at a frame
/// boundary, so a caller's read loop can treat that as "done". The 1 MiB cap is
/// enforced on the decoded length **before** the body buffer is allocated.
pub fn read_message<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<T, CodecError> {
    let mut len_buf = [0u8; 4];

    // Distinguish a clean boundary EOF (no bytes) from a truncated prefix.
    match read_exact_or_eof(reader, &mut len_buf)? {
        ReadOutcome::Eof => return Err(CodecError::Eof),
        ReadOutcome::Filled => {}
    }

    let len = u32::from_le_bytes(len_buf);
    if len as usize > MAX_MESSAGE_LEN {
        // Reject oversize frames without ever allocating `len` bytes.
        return Err(CodecError::TooLarge(len));
    }

    let mut body = vec![0u8; len as usize];
    reader
        .read_exact(&mut body)
        .map_err(|e| CodecError::from_read(e, false))?;

    let text = std::str::from_utf8(&body).map_err(|_| CodecError::NotUtf8)?;
    serde_json::from_str(text).map_err(|_| CodecError::InvalidJson)
}

/// Serialize `value` as JSON and write it as one length-prefixed frame to
/// `writer`, flushing afterward.
///
/// A serialized value larger than [`MAX_MESSAGE_LEN`] is refused with
/// [`CodecError::TooLarge`] rather than emitting a frame the peer would reject.
pub fn write_message<W: Write, T: Serialize>(writer: &mut W, value: &T) -> Result<(), CodecError> {
    let body = serde_json::to_vec(value).map_err(|_| CodecError::InvalidJson)?;
    if body.len() > MAX_MESSAGE_LEN {
        // u32 cast is safe: MAX_MESSAGE_LEN (1 MiB) fits in u32.
        return Err(CodecError::TooLarge(body.len() as u32));
    }

    let len = body.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Whether a prefix read hit a clean boundary EOF or was fully filled.
enum ReadOutcome {
    Eof,
    Filled,
}

/// Read exactly `buf.len()` bytes, but report a clean EOF (zero bytes read on the
/// very first read) distinctly from a truncated read (some bytes, then EOF).
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<ReadOutcome, CodecError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                return if filled == 0 {
                    // Nothing at all was waiting: a clean end between frames.
                    Ok(ReadOutcome::Eof)
                } else {
                    // The prefix was cut short mid-read.
                    Err(CodecError::Truncated)
                };
            }
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(CodecError::Io(e)),
        }
    }
    Ok(ReadOutcome::Filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use app_core::{IpcRequest, RequestEnvelope};
    use assert_matches::assert_matches;

    /// Frame `body` bytes with a little-endian length prefix.
    fn frame(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn round_trips_an_envelope() {
        let env = RequestEnvelope {
            req_id: 7,
            request: IpcRequest::GetMatches {
                url: "https://example.com".into(),
            },
        };
        let mut buf = Vec::new();
        write_message(&mut buf, &env).unwrap();

        let mut cursor = Cursor::new(buf);
        let back: RequestEnvelope = read_message(&mut cursor).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn write_emits_exact_little_endian_prefix() {
        let value = serde_json::json!({ "a": 1 });
        let body = serde_json::to_vec(&value).unwrap();

        let mut buf = Vec::new();
        write_message(&mut buf, &value).unwrap();

        // First four bytes are the LE length of the JSON body...
        assert_eq!(&buf[..4], &(body.len() as u32).to_le_bytes());
        // ...followed by the body verbatim.
        assert_eq!(&buf[4..], &body[..]);
    }

    #[test]
    fn reads_back_the_exact_bytes_written() {
        // `{"a":1}` is 7 bytes -> prefix 07 00 00 00.
        let value = serde_json::json!({ "a": 1 });
        let mut buf = Vec::new();
        write_message(&mut buf, &value).unwrap();
        assert_eq!(&buf[..4], &[7, 0, 0, 0]);
    }

    #[test]
    fn multiple_messages_read_in_sequence() {
        let mut buf = Vec::new();
        write_message(&mut buf, &serde_json::json!({ "n": 1 })).unwrap();
        write_message(&mut buf, &serde_json::json!({ "n": 2 })).unwrap();
        write_message(&mut buf, &serde_json::json!({ "n": 3 })).unwrap();

        let mut cursor = Cursor::new(buf);
        for expected in 1..=3 {
            let v: serde_json::Value = read_message(&mut cursor).unwrap();
            assert_eq!(v["n"], expected);
        }
        // Clean EOF after the last message.
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::Eof)
        );
    }

    #[test]
    fn empty_stream_is_clean_eof() {
        let mut cursor = Cursor::new(Vec::new());
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::Eof)
        );
    }

    #[test]
    fn truncated_prefix_is_truncated_error() {
        // Only two of the four prefix bytes.
        let mut cursor = Cursor::new(vec![1u8, 0]);
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn truncated_body_is_truncated_error() {
        // Prefix says 10 bytes, but only 3 follow.
        let mut data = 10u32.to_le_bytes().to_vec();
        data.extend_from_slice(b"abc");
        let mut cursor = Cursor::new(data);
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn oversize_prefix_is_rejected_without_allocating() {
        // Declare a 4 GiB body but supply no body bytes. If the reader tried to
        // allocate `len` first, this would OOM; instead it must reject on the
        // prefix alone. We give it only the 4 prefix bytes.
        let huge = u32::MAX; // ~4 GiB, well over the 1 MiB cap
        let mut cursor = Cursor::new(huge.to_le_bytes().to_vec());
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::TooLarge(n)) if n == huge
        );
    }

    #[test]
    fn one_byte_over_the_cap_is_rejected() {
        let over = (MAX_MESSAGE_LEN + 1) as u32;
        let mut cursor = Cursor::new(over.to_le_bytes().to_vec());
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::TooLarge(_))
        );
    }

    #[test]
    fn exactly_the_cap_is_allowed_through_to_the_body_read() {
        // A length of exactly MAX_MESSAGE_LEN is *not* TooLarge; with no body it
        // becomes a Truncated read rather than a length rejection.
        let at = MAX_MESSAGE_LEN as u32;
        let mut cursor = Cursor::new(at.to_le_bytes().to_vec());
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn non_utf8_body_is_rejected() {
        // Valid prefix, but the body bytes are not UTF-8 (0xff is never valid).
        let cursor_data = frame(&[0xff, 0xfe, 0xfd]);
        let mut cursor = Cursor::new(cursor_data);
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::NotUtf8)
        );
    }

    #[test]
    fn invalid_json_body_is_rejected() {
        // Valid UTF-8, but not JSON.
        let cursor_data = frame(b"this is not json {");
        let mut cursor = Cursor::new(cursor_data);
        assert_matches!(
            read_message::<_, serde_json::Value>(&mut cursor),
            Err(CodecError::InvalidJson)
        );
    }

    #[test]
    fn well_formed_json_of_wrong_shape_is_invalid_json() {
        // Valid JSON, but not a RequestEnvelope (missing fields).
        let cursor_data = frame(b"{\"unrelated\":true}");
        let mut cursor = Cursor::new(cursor_data);
        assert_matches!(
            read_message::<_, RequestEnvelope>(&mut cursor),
            Err(CodecError::InvalidJson)
        );
    }
}
