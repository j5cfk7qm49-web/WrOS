//! Google enrollment core for WrOS. No network code lives here: the OAuth device
//! flow talks through an injected [`device::Transport`], so every path (cancel,
//! expiry, slow-down, revocation, offline) is testable without an account.
//!
//! Nothing in this crate runs unless the writer starts enrollment. Local writing,
//! boot and login never depend on it.

pub mod device;
pub mod store;

pub use device::{DeviceFlow, Flow, PollOutcome, Transport};
pub use store::{Credentials, CredentialStore};

/// Scopes the device flow can grant and WrOS is willing to request. Restricted
/// Drive scopes (`drive`, `drive.readonly`, ...) are refused here on purpose: they
/// need Google's app verification and a different approved flow.
pub const ALLOWED_SCOPES: [&str; 5] = [
    "openid",
    "email",
    "profile",
    "https://www.googleapis.com/auth/drive.file",
    "https://www.googleapis.com/auth/drive.appdata",
];

/// Errors never carry tokens, codes or response bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A requested scope is not in [`ALLOWED_SCOPES`].
    ScopeNotAllowed(String),
    /// The sign-in was refused or the code is no longer valid.
    Denied,
    Expired,
    /// Offline, rate limited or provider failure; local writing is unaffected.
    Unavailable(&'static str),
    /// Provider answered something this client does not understand.
    Protocol(&'static str),
    /// The credential file or its directory is unsafe or damaged.
    Store(&'static str),
    Cancelled,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScopeNotAllowed(_) => f.write_str("requested Google permission is not supported"),
            Self::Denied => f.write_str("Google sign-in was declined or the code is invalid"),
            Self::Expired => f.write_str("the Google sign-in code expired"),
            Self::Unavailable(why) => write!(f, "Google is unavailable: {why}"),
            Self::Protocol(why) => write!(f, "unexpected Google response: {why}"),
            Self::Store(why) => write!(f, "credential store: {why}"),
            Self::Cancelled => f.write_str("Google sign-in was cancelled"),
        }
    }
}
impl std::error::Error for Error {}
