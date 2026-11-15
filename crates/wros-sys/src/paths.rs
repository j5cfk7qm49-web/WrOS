//! System tool, helper, and data paths for WrOS.
//!
//! Defaults remain the Debian 13 paths so existing Debian builds and tests
//! pass unchanged; Nix supplies immutable NixOS store and wrapper paths via
//! `WROS_PATH_*` environment variables at compile time.

pub const SAFE_PATH: &str = match option_env!("WROS_PATH_SAFE_PATH") {
    Some(v) => v,
    None => "/usr/sbin:/usr/bin:/sbin:/bin",
};

pub const MINIMAL_PATH: &str = match option_env!("WROS_PATH_MINIMAL_PATH") {
    Some(v) => v,
    None => "/usr/bin:/bin",
};

pub const SH: &str = match option_env!("WROS_PATH_SH") {
    Some(v) => v,
    None => "/bin/sh",
};

pub const BIN_FALSE: &str = match option_env!("WROS_PATH_BIN_FALSE") {
    Some(v) => v,
    None => "/bin/false",
};

pub const USR_BIN_FALSE: &str = match option_env!("WROS_PATH_USR_BIN_FALSE") {
    Some(v) => v,
    None => "/usr/bin/false",
};

pub const PKEXEC: &str = match option_env!("WROS_PATH_PKEXEC") {
    Some(v) => v,
    None => "/usr/bin/pkexec",
};

pub const NMCLI: &str = match option_env!("WROS_PATH_NMCLI") {
    Some(v) => v,
    None => "/usr/bin/nmcli",
};

pub const TIMEDATECTL: &str = match option_env!("WROS_PATH_TIMEDATECTL") {
    Some(v) => v,
    None => "/usr/bin/timedatectl",
};

pub const SYSTEMCTL: &str = match option_env!("WROS_PATH_SYSTEMCTL") {
    Some(v) => v,
    None => "/usr/bin/systemctl",
};

pub const SYSTEMD_INHIBIT: &str = match option_env!("WROS_PATH_SYSTEMD_INHIBIT") {
    Some(v) => v,
    None => "/usr/bin/systemd-inhibit",
};

pub const SYSTEMD_RUN: &str = match option_env!("WROS_PATH_SYSTEMD_RUN") {
    Some(v) => v,
    None => "/usr/bin/systemd-run",
};

pub const BUSCTL: &str = match option_env!("WROS_PATH_BUSCTL") {
    Some(v) => v,
    None => "/usr/bin/busctl",
};

pub const FINDMNT: &str = match option_env!("WROS_PATH_FINDMNT") {
    Some(v) => v,
    None => "/usr/bin/findmnt",
};

pub const BRIGHTNESSCTL: &str = match option_env!("WROS_PATH_BRIGHTNESSCTL") {
    Some(v) => v,
    None => "/usr/bin/brightnessctl",
};

pub const POWERPROFILESCTL: &str = match option_env!("WROS_PATH_POWERPROFILESCTL") {
    Some(v) => v,
    None => "/usr/bin/powerprofilesctl",
};

pub const PYTHON3: &str = match option_env!("WROS_PATH_PYTHON3") {
    Some(v) => v,
    None => "/usr/bin/python3",
};

pub const APT_GET: &str = match option_env!("WROS_PATH_APT_GET") {
    Some(v) => v,
    None => "/usr/bin/apt-get",
};

pub const OPENVT: &str = match option_env!("WROS_PATH_OPENVT") {
    Some(v) => v,
    None => "/usr/bin/openvt",
};

pub const CHVT: &str = match option_env!("WROS_PATH_CHVT") {
    Some(v) => v,
    None => "/usr/bin/chvt",
};

pub const UNSHARE: &str = match option_env!("WROS_PATH_UNSHARE") {
    Some(v) => v,
    None => "/usr/bin/unshare",
};

pub const VLOCK_MAIN: &str = match option_env!("WROS_PATH_VLOCK_MAIN") {
    Some(v) => v,
    None => "/usr/sbin/vlock-main",
};

pub const CAGE: &str = match option_env!("WROS_PATH_CAGE") {
    Some(v) => v,
    None => "/usr/bin/cage",
};

pub const FOOT: &str = match option_env!("WROS_PATH_FOOT") {
    Some(v) => v,
    None => "/usr/bin/foot",
};

pub const GRIMOIRE: &str = match option_env!("WROS_PATH_GRIMOIRE") {
    Some(v) => v,
    None => "/usr/bin/grimoire",
};

pub const WROS_INSTALLER: &str = match option_env!("WROS_PATH_WROS_INSTALLER") {
    Some(v) => v,
    None => "/usr/bin/wros-installer",
};

pub const LIBEXEC_DIR: &str = match option_env!("WROS_PATH_LIBEXEC_DIR") {
    Some(v) => v,
    None => "/usr/libexec",
};

pub const WROS_BACKUP: &str = match option_env!("WROS_PATH_HELPER_BACKUP") {
    Some(v) => v,
    None => "/usr/libexec/wros-backup",
};

pub const WROS_BOOT_SETTINGS: &str = match option_env!("WROS_PATH_HELPER_BOOT_SETTINGS") {
    Some(v) => v,
    None => "/usr/libexec/wros-boot-settings",
};

pub const WROS_CLOCK: &str = match option_env!("WROS_PATH_HELPER_CLOCK") {
    Some(v) => v,
    None => "/usr/libexec/wros-clock",
};

pub const WROS_DISPLAY: &str = match option_env!("WROS_PATH_HELPER_DISPLAY") {
    Some(v) => v,
    None => "/usr/libexec/wros-display",
};

pub const WROS_INSTALL_SESSION: &str = match option_env!("WROS_PATH_HELPER_INSTALL_SESSION") {
    Some(v) => v,
    None => "/usr/libexec/wros-install-session",
};

pub const WROS_KEYBOARD: &str = match option_env!("WROS_PATH_HELPER_KEYBOARD") {
    Some(v) => v,
    None => "/usr/libexec/wros-keyboard",
};

pub const WROS_LOCK: &str = match option_env!("WROS_PATH_HELPER_LOCK") {
    Some(v) => v,
    None => "/usr/libexec/wros-lock",
};

pub const WROS_PASSWD: &str = match option_env!("WROS_PATH_HELPER_PASSWD") {
    Some(v) => v,
    None => "/usr/libexec/wros-passwd",
};

pub const WROS_SESSION: &str = match option_env!("WROS_PATH_HELPER_SESSION") {
    Some(v) => v,
    None => "/usr/libexec/wros-session",
};

pub const WROS_SOUND: &str = match option_env!("WROS_PATH_HELPER_SOUND") {
    Some(v) => v,
    None => "/usr/libexec/wros-sound",
};

pub const WROS_SYNC: &str = match option_env!("WROS_PATH_HELPER_SYNC") {
    Some(v) => v,
    None => "/usr/libexec/wros-sync",
};

pub const WROS_UPDATE: &str = match option_env!("WROS_PATH_HELPER_UPDATE") {
    Some(v) => v,
    None => "/usr/libexec/wros-update",
};

pub const WROS_UPDATE_APPLY: &str = match option_env!("WROS_PATH_HELPER_UPDATE_APPLY") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-apply",
};

pub const WROS_UPDATE_CANCEL: &str = match option_env!("WROS_PATH_HELPER_UPDATE_CANCEL") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-cancel",
};

pub const WROS_UPDATE_FETCH: &str = match option_env!("WROS_PATH_HELPER_UPDATE_FETCH") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-fetch",
};

pub const WROS_UPDATE_HEALTHY: &str = match option_env!("WROS_PATH_HELPER_UPDATE_HEALTHY") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-healthy",
};

pub const WROS_UPDATE_MEDIA: &str = match option_env!("WROS_PATH_HELPER_UPDATE_MEDIA") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-media",
};

pub const WROS_UPDATE_REPAIR: &str = match option_env!("WROS_PATH_HELPER_UPDATE_REPAIR") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-repair",
};

pub const WROS_UPDATE_ROLLBACK: &str = match option_env!("WROS_PATH_HELPER_UPDATE_ROLLBACK") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-rollback",
};

pub const WROS_UPDATE_STAGE: &str = match option_env!("WROS_PATH_HELPER_UPDATE_STAGE") {
    Some(v) => v,
    None => "/usr/libexec/wros-update-stage",
};

pub const QWERTY_KEYMAP: &str = match option_env!("WROS_PATH_QWERTY_KEYMAP") {
    Some(v) => v,
    None => "/usr/share/wros/keymaps/qwerty.map",
};

pub const WORDNET_DIR: &str = match option_env!("WROS_PATH_WORDNET_DIR") {
    Some(v) => v,
    None => "/usr/share/wordnet",
};

pub const TOOLS_PYTHON: &str = match option_env!("WROS_PATH_TOOLS_PYTHON") {
    Some(v) => v,
    None => "/opt/writingdeck/python/bin/python3",
};

pub const XKB_DIR: &str = match option_env!("WROS_PATH_XKB_DIR") {
    Some(v) => v,
    None => "/usr/share/X11/xkb",
};

pub const X11_COMPOSE: &str = match option_env!("WROS_PATH_X11_COMPOSE") {
    Some(v) => v,
    None => "/usr/share/X11/locale/en_US.UTF-8/Compose",
};

pub const LIVE_MEDIUM_SQUASHFS: &str = match option_env!("WROS_PATH_LIVE_SQUASHFS") {
    Some(v) => v,
    None => "/run/live/medium/live/filesystem.squashfs",
};

pub const LIVE_SESSION_JSON: &str = "/run/wros-live/session.json";

pub const SUSPEND_LOCK_DROPIN: &str = match option_env!("WROS_PATH_SUSPEND_LOCK_DROPIN") {
    Some(v) => v,
    None => "/etc/systemd/system/systemd-suspend.service.d/60-wros-lock.conf",
};

pub const LIVE_NIXOS_SQUASHFS: &str = "/iso/nix-store.squashfs";
