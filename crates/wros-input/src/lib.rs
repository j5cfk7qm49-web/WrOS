//! Input ownership only. No manuscripts, authentication, locking or power actions.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Input, Paused, WORKER_ARGUMENT, worker_main};
