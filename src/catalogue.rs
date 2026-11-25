//! Catalogue requests and replies.

use std::fmt::Write;

use grimoire_core::sources::Record;

// Search and edition paging get their callers with the search state machine.
#[cfg_attr(not(test), allow(dead_code))]
pub mod crossref;
#[cfg_attr(not(test), allow(dead_code))]
pub mod openlibrary;

// Name resolution gets its first app caller with catalogue picks.
#[cfg_attr(not(test), allow(dead_code))]
pub mod names;

// Page and HEAD fetches get callers in later catalogue tasks.
#[allow(dead_code)]
pub mod fetch;

// The classifier gets its first app caller with the search state machine.
#[cfg_attr(not(test), allow(dead_code))]
pub mod query;

#[allow(unused_imports)]
pub use fetch::{Fetch, OFFLINE, default_fetch};
#[allow(unused_imports)]
pub use query::Query;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub record: Record,
    pub work: Option<String>,
    pub editions: Option<u32>,
    pub authors: Vec<String>,
}

fn encode(value: &str, form: bool) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b' ' if form => encoded.push('+'),
            b'*' if form => encoded.push('*'),
            b'~' if !form => encoded.push('~'),
            b'-' | b'_' | b'.' => encoded.push(char::from(byte)),
            byte if byte.is_ascii_alphanumeric() => encoded.push(char::from(byte)),
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}
