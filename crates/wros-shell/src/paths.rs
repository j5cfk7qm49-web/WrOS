//! System tool and helper paths for `wros-boot` and `wros-greeter`.
//!
//! Defaults remain the Debian 13 paths; Nix supplies immutable NixOS paths via
//! `WROS_PATH_*` environment variables at compile time.

pub const SAFE_PATH: &str = match option_env!("WROS_PATH_SAFE_PATH") {
    Some(v) => v,
    None => "/usr/sbin:/usr/bin:/sbin:/bin",
};

pub const BUSCTL: &str = match option_env!("WROS_PATH_BUSCTL") {
    Some(v) => v,
    None => "/usr/bin/busctl",
};

pub const PKEXEC: &str = match option_env!("WROS_PATH_PKEXEC") {
    Some(v) => v,
    None => "/usr/bin/pkexec",
};

pub const SYSTEMCTL: &str = match option_env!("WROS_PATH_SYSTEMCTL") {
    Some(v) => v,
    None => "/usr/bin/systemctl",
};

pub const WROS_KEYBOARD: &str = match option_env!("WROS_PATH_HELPER_KEYBOARD") {
    Some(v) => v,
    None => "/usr/libexec/wros-keyboard",
};

pub const WROS_SESSION: &str = match option_env!("WROS_PATH_HELPER_SESSION") {
    Some(v) => v,
    None => "/usr/libexec/wros-session",
};
