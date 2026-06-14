//! Spooky-Pass native-messaging host.
//!
//! A thin relay launched by the browser: it frames Chrome native-messaging
//! traffic on stdin/stdout (4-byte little-endian length prefix + JSON, ≤ 1 MiB)
//! and forwards each request to the resident Core over a local socket, echoing
//! the correlation id back. It holds **no vault and no key**.
//!
//! Implemented in Phase 3. This entry point is a placeholder so the crate
//! compiles as part of the workspace scaffold.

fn main() {
    // Phase 3 implementation goes here.
}
