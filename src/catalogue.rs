//! Catalogue requests and replies.

// The fetch seam gets its first app caller when catalogue search is wired in.
#[allow(dead_code)]
pub mod fetch;

// The classifier gets its first app caller with the search state machine.
#[cfg_attr(not(test), allow(dead_code))]
pub mod query;

#[allow(unused_imports)]
pub use fetch::{Fetch, OFFLINE, default_fetch};
#[allow(unused_imports)]
pub use query::Query;
