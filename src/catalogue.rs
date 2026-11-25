//! Catalogue requests and replies.

// The fetch seam gets its first app caller when catalogue search is wired in.
#[allow(dead_code)]
pub mod fetch;

#[allow(unused_imports)]
pub use fetch::{Fetch, OFFLINE, default_fetch};
