//! Input ownership only. No manuscripts, authentication, locking or power actions.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(target_os = "linux", feature = "native"))]
mod native;
#[cfg(target_os = "linux")]
pub use linux::{Input, Paused, WORKER_ARGUMENT, worker_main};
#[cfg(all(target_os = "linux", feature = "native"))]
pub use {linux::native_worker_main, native::NativeConfig};
