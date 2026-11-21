{ pkgs
, wros-helpers
, wros-display-hw
, wros-console-reset
, wros-cage
, wros-keymaps
, wros-prose-runtime
}:
let
  wrapperDir = "/run/wrappers/bin";
  safePath = pkgs.lib.makeBinPath [
    pkgs.coreutils
    pkgs.util-linux
    pkgs.systemd
    pkgs.networkmanager
    pkgs.brightnessctl
    pkgs.power-profiles-daemon
    pkgs.kbd
    pkgs.vlock
    pkgs.gnupg
    pkgs.restic
    pkgs.rclone
    pkgs.e2fsprogs
    pkgs.dosfstools
    pkgs.cryptsetup
    pkgs.lvm2
    pkgs.gptfdisk
    pkgs.parted
    pkgs.python3
    pkgs.bash
  ] + ":${wrapperDir}:/run/current-system/sw/bin";

  minimalPath = pkgs.lib.makeBinPath [
    pkgs.coreutils
    pkgs.bash
  ];

  # Single source of truth shared between polkit policy annotations and Rust constants.
  helperPaths = {
    libexecDir = "${wros-helpers}/libexec";
    backup = "${wros-helpers}/libexec/wros-backup";
    bootSettings = "${wros-helpers}/libexec/wros-boot-settings";
    clock = "${wros-helpers}/libexec/wros-clock";
    display = "${wros-helpers}/libexec/wros-display";
    displayHw = "${wros-display-hw}/libexec/wros-display-hw";
    importMedia = "${wros-helpers}/libexec/wros-import-media";
    installSession = "${wros-helpers}/libexec/wros-install-session";
    installCore = "${wros-helpers}/libexec/wros-install-core";
    installerBackend = "${wros-helpers}/libexec/wros-installer-backend";
    keyboard = "${wros-helpers}/libexec/wros-keyboard";
    keyboardCompat = "${wros-helpers}/libexec/writingdeck-keyboard";
    liveSession = "${wros-helpers}/libexec/wros-live-session";
    lock = "${wros-helpers}/libexec/wros-lock";
    mediaKey = "${wros-helpers}/libexec/wros-media-key";
    passwd = "${wros-helpers}/libexec/wros-passwd";
    recovery = "${wros-helpers}/libexec/wros-recovery";
    rescue = "${wros-helpers}/bin/wros-rescue";
    session = "${wros-helpers}/libexec/wros-session";
    sound = "${wros-helpers}/libexec/wros-sound";
    sync = "${wros-helpers}/libexec/wros-sync";
    update = "${wros-helpers}/libexec/wros-update";
    updateApply = "${wros-helpers}/libexec/wros-update-apply";
    updateCancel = "${wros-helpers}/libexec/wros-update-cancel";
    updateFetch = "${wros-helpers}/libexec/wros-update-fetch";
    updateHealthy = "${wros-helpers}/libexec/wros-update-healthy";
    updateMedia = "${wros-helpers}/libexec/wros-update-media";
    updateRepair = "${wros-helpers}/libexec/wros-update-repair";
    updateRollback = "${wros-helpers}/libexec/wros-update-rollback";
    updateStage = "${wros-helpers}/libexec/wros-update-stage";
  };

  toolPaths = {
    sh = "${pkgs.bash}/bin/sh";
    binFalse = "${pkgs.coreutils}/bin/false";
    usrBinFalse = "${pkgs.coreutils}/bin/false";
    pkexec = "${wrapperDir}/pkexec";
    passwd = "${wrapperDir}/passwd";
    nmcli = "${pkgs.networkmanager}/bin/nmcli";
    timedatectl = "${pkgs.systemd}/bin/timedatectl";
    systemctl = "${pkgs.systemd}/bin/systemctl";
    systemdInhibit = "${pkgs.systemd}/bin/systemd-inhibit";
    systemdRun = "${pkgs.systemd}/bin/systemd-run";
    busctl = "${pkgs.systemd}/bin/busctl";
    loginctl = "${pkgs.systemd}/bin/loginctl";
    journalctl = "${pkgs.systemd}/bin/journalctl";
    findmnt = "${pkgs.util-linux}/bin/findmnt";
    lsblk = "${pkgs.util-linux}/bin/lsblk";
    mount = "${pkgs.util-linux}/bin/mount";
    umount = "${pkgs.util-linux}/bin/umount";
    runuser = "${pkgs.util-linux}/bin/runuser";
    unshare = "${pkgs.util-linux}/bin/unshare";
    logger = "${pkgs.util-linux}/bin/logger";
    brightnessctl = "${pkgs.brightnessctl}/bin/brightnessctl";
    powerprofilesctl = "${pkgs.power-profiles-daemon}/bin/powerprofilesctl";
    python3 = "${pkgs.python3}/bin/python3";
    aptGet = "${pkgs.coreutils}/bin/false";
    openvt = "${pkgs.kbd}/bin/openvt";
    chvt = "${pkgs.kbd}/bin/chvt";
    loadkeys = "${pkgs.kbd}/bin/loadkeys";
    vlockMain = "${pkgs.vlock}/sbin/vlock-main";
    cage = "${wros-cage}/bin/cage";
    foot = "${pkgs.foot}/bin/foot";
    grimoire = "/run/current-system/sw/bin/grimoire";
    wrosGreeter = "/run/current-system/sw/bin/wros-greeter";
    wrosInstaller = "/run/current-system/sw/bin/wros-installer";
    consoleReset = "${wros-console-reset}/libexec/wros-console-reset";
    qwertyKeymap = "${wros-keymaps}/share/wros/keymaps/qwerty.map";
    keymapsDir = "${wros-keymaps}/share/wros/keymaps";
    wordnetDir = "${pkgs.wordnet}/dict";
    toolsPython = "${wros-prose-runtime}/bin/python3";
    xkbDir = "${pkgs.xkeyboard_config}/share/X11/xkb";
    x11Compose = "${pkgs.libx11}/share/X11/locale/en_US.UTF-8/Compose";
    liveSquashfs = "/iso/nix-store.squashfs";
    # NixOS generates overrides.conf in systemd-suspend.service.d from systemd.services.systemd-suspend (H7)
    suspendLockDropin = "/etc/systemd/system/systemd-suspend.service.d/overrides.conf";
  };

  # Per-binary path environments (H6): wros-boot and wros-installer do not depend on wros-helpers,
  # wros-cage, or wros-prose-runtime, keeping the initrd closure small and avoiding unneeded rebuilds.
  bootPathEnv = {
    WROS_PATH_SAFE_PATH = minimalPath;
    WROS_PATH_MINIMAL_PATH = minimalPath;
    WROS_PATH_BIN_FALSE = toolPaths.binFalse;
    # The start screen reads systemd's boot progress (same systemd package as busctl).
    WROS_PATH_SYSTEMCTL = toolPaths.systemctl;
    WROS_PATH_BUSCTL = toolPaths.busctl;
    WROS_PATH_XKB_DIR = toolPaths.xkbDir;
    WROS_PATH_X11_COMPOSE = toolPaths.x11Compose;
  };

  installerPathEnv = {
    WROS_PATH_SAFE_PATH = minimalPath;
    WROS_PATH_MINIMAL_PATH = minimalPath;
    WROS_PATH_BIN_FALSE = toolPaths.binFalse;
    WROS_PATH_XKB_DIR = toolPaths.xkbDir;
    WROS_PATH_X11_COMPOSE = toolPaths.x11Compose;
  };

  greeterPathEnv = {
    WROS_PATH_SAFE_PATH = safePath;
    WROS_PATH_MINIMAL_PATH = minimalPath;
    WROS_PATH_BIN_FALSE = toolPaths.binFalse;
    WROS_PATH_PKEXEC = toolPaths.pkexec;
    WROS_PATH_SYSTEMCTL = toolPaths.systemctl;
    WROS_PATH_BUSCTL = toolPaths.busctl;
    WROS_PATH_HELPER_KEYBOARD = helperPaths.keyboard;
    WROS_PATH_HELPER_SESSION = helperPaths.session;
    WROS_PATH_XKB_DIR = toolPaths.xkbDir;
    WROS_PATH_X11_COMPOSE = toolPaths.x11Compose;
  };

  rustPathEnv = {
    WROS_PATH_SAFE_PATH = safePath;
    WROS_PATH_MINIMAL_PATH = minimalPath;
    WROS_PATH_SH = toolPaths.sh;
    WROS_PATH_BIN_FALSE = toolPaths.binFalse;
    WROS_PATH_USR_BIN_FALSE = toolPaths.usrBinFalse;
    WROS_PATH_PKEXEC = toolPaths.pkexec;
    WROS_PATH_NMCLI = toolPaths.nmcli;
    WROS_PATH_TIMEDATECTL = toolPaths.timedatectl;
    WROS_PATH_SYSTEMCTL = toolPaths.systemctl;
    WROS_PATH_SYSTEMD_INHIBIT = toolPaths.systemdInhibit;
    WROS_PATH_SYSTEMD_RUN = toolPaths.systemdRun;
    WROS_PATH_BUSCTL = toolPaths.busctl;
    WROS_PATH_FINDMNT = toolPaths.findmnt;
    WROS_PATH_BRIGHTNESSCTL = toolPaths.brightnessctl;
    WROS_PATH_POWERPROFILESCTL = toolPaths.powerprofilesctl;
    WROS_PATH_PYTHON3 = toolPaths.python3;
    WROS_PATH_APT_GET = toolPaths.aptGet;
    WROS_PATH_OPENVT = toolPaths.openvt;
    WROS_PATH_CHVT = toolPaths.chvt;
    WROS_PATH_UNSHARE = toolPaths.unshare;
    WROS_PATH_VLOCK_MAIN = toolPaths.vlockMain;
    WROS_PATH_CAGE = toolPaths.cage;
    WROS_PATH_FOOT = toolPaths.foot;
    WROS_PATH_GRIMOIRE = toolPaths.grimoire;
    WROS_PATH_WROS_INSTALLER = toolPaths.wrosInstaller;
    WROS_PATH_LIBEXEC_DIR = helperPaths.libexecDir;
    WROS_PATH_HELPER_BACKUP = helperPaths.backup;
    WROS_PATH_HELPER_BOOT_SETTINGS = helperPaths.bootSettings;
    WROS_PATH_HELPER_CLOCK = helperPaths.clock;
    WROS_PATH_HELPER_DISPLAY = helperPaths.display;
    WROS_PATH_HELPER_IMPORT_MEDIA = helperPaths.importMedia;
    WROS_PATH_HELPER_INSTALL_SESSION = helperPaths.installSession;
    WROS_PATH_HELPER_KEYBOARD = helperPaths.keyboard;
    WROS_PATH_HELPER_LOCK = helperPaths.lock;
    WROS_PATH_HELPER_PASSWD = helperPaths.passwd;
    WROS_PATH_HELPER_SESSION = helperPaths.session;
    WROS_PATH_HELPER_SOUND = helperPaths.sound;
    WROS_PATH_HELPER_SYNC = helperPaths.sync;
    WROS_PATH_HELPER_UPDATE = helperPaths.update;
    WROS_PATH_HELPER_UPDATE_APPLY = helperPaths.updateApply;
    WROS_PATH_HELPER_UPDATE_CANCEL = helperPaths.updateCancel;
    WROS_PATH_HELPER_UPDATE_FETCH = helperPaths.updateFetch;
    WROS_PATH_HELPER_UPDATE_HEALTHY = helperPaths.updateHealthy;
    WROS_PATH_HELPER_UPDATE_MEDIA = helperPaths.updateMedia;
    WROS_PATH_HELPER_UPDATE_REPAIR = helperPaths.updateRepair;
    WROS_PATH_HELPER_UPDATE_ROLLBACK = helperPaths.updateRollback;
    WROS_PATH_HELPER_UPDATE_STAGE = helperPaths.updateStage;
    WROS_PATH_QWERTY_KEYMAP = toolPaths.qwertyKeymap;
    WROS_PATH_WORDNET_DIR = toolPaths.wordnetDir;
    WROS_PATH_TOOLS_PYTHON = toolPaths.toolsPython;
    WROS_PATH_XKB_DIR = toolPaths.xkbDir;
    WROS_PATH_X11_COMPOSE = toolPaths.x11Compose;
    WROS_PATH_LIVE_SQUASHFS = toolPaths.liveSquashfs;
    WROS_PATH_SUSPEND_LOCK_DROPIN = toolPaths.suspendLockDropin;
  };
in
{
  inherit
    wrapperDir
    safePath
    minimalPath
    helperPaths
    toolPaths
    bootPathEnv
    installerPathEnv
    greeterPathEnv
    rustPathEnv;
}
