//! Spooky-Pass native-messaging host (binary entry point).
//!
//! A thin relay launched by the browser: it frames Chrome native-messaging
//! traffic on stdin/stdout (4-byte little-endian length prefix + JSON, ≤ 1 MiB)
//! and forwards each request to the resident Core over a local socket, echoing
//! the correlation id back. It holds **no vault and no key**.
//!
//! All logic lives in the library crate ([`spooky_pass_host`]) so it is unit
//! testable; this entry point just runs the relay loop until stdin EOF and maps
//! the outcome to a process exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    match spooky_pass_host::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // A loop-level failure (e.g. stdout closed) is rare and never carries
            // a secret — only the error category is printed, never any message
            // body. The browser treats a host exit as a disconnect.
            eprintln!("spooky-pass-host: {e}");
            ExitCode::FAILURE
        }
    }
}
