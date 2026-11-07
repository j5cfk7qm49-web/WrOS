//! WrOS start screens drawn with Ratatui (docs/RATATUI_BOOT.md): the disk
//! password in the boot image, and later the progress bar up to the login.
//! Debian's own prompt always stands behind them.

pub mod keys;
pub mod logo;
pub mod unlock;

use wros_shell::grimoire_theme::{Theme, default_theme, from_config};

/// The active theme's copy for the start screens, which can't read the writer's
/// settings before the disk is unlocked.
pub const THEME: &str = "/etc/wros/boot-theme.toml";

/// Exit codes the boot image's `askpass` wrapper reads.
pub mod exit {
    /// The password went to standard output.
    pub const UNLOCK: i32 = 0;
    /// The writer pressed Esc: use Debian's text prompt.
    pub const TEXT: i32 = 10;
    /// The screen couldn't run; nothing was written. Debian's prompt takes over.
    pub const FAILED: i32 = 20;
}

/// The theme in `path`, or WrOS's default when it is missing or unreadable.
pub fn theme(path: &std::path::Path) -> Theme {
    std::fs::read_to_string(path).map_or_else(|_| default_theme(), |s| from_config(&s))
}
