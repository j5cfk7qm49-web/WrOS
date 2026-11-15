//! System tool and XKB data paths for `wros-input`.
//!
//! Defaults remain the Debian 13 paths; Nix supplies immutable NixOS paths via
//! `WROS_PATH_*` environment variables at compile time.

pub const SAFE_PATH: &str = match option_env!("WROS_PATH_SAFE_PATH") {
    Some(v) => v,
    None => "/usr/sbin:/usr/bin:/sbin:/bin",
};

pub const BIN_FALSE: &str = match option_env!("WROS_PATH_BIN_FALSE") {
    Some(v) => v,
    None => "/bin/false",
};

pub const XKB_DIR: &str = match option_env!("WROS_PATH_XKB_DIR") {
    Some(v) => v,
    None => "/usr/share/X11/xkb",
};

pub const X11_COMPOSE: &str = match option_env!("WROS_PATH_X11_COMPOSE") {
    Some(v) => v,
    None => "/usr/share/X11/locale/en_US.UTF-8/Compose",
};
