{ pkgs
, lib
, version
, src
, wros-display-hw
, wros-cage
, wros-keymaps
, wros-prose-runtime
}:
let
  wrapperDir = "/run/wrappers/bin";
  ytmPython = pkgs.python3.withPackages (p: [ p.ytmusicapi ]);
  safePath = lib.makeBinPath [
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
    pkgs.nix
    pkgs.zstd
    pkgs.python3
    pkgs.bash
  ] + ":${wrapperDir}:/run/current-system/sw/bin";

  helperSrc = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/os/config/foot.ini")
      (src + "/os/config/wros-keys-triggers.conf")
      (src + "/os/scripts")
      (src + "/nixos/scripts")
    ];
  };
in
pkgs.stdenv.mkDerivation {
  pname = "wros-helpers";
  inherit version;
  src = helperSrc;

  buildPhase = ''
    runHook preBuild
    mkdir -p "$out/libexec" "$out/bin" "$out/lib/wros-backup" "$out/lib/wros-update" "$out/share/wros"

    install -m 644 os/config/foot.ini "$out/share/wros/foot.ini"
    install -m 644 os/config/wros-keys-triggers.conf "$out/share/wros/wros-keys.conf"
    substituteInPlace "$out/share/wros/wros-keys.conf" \
      --replace-fail "/usr/libexec/wros-media-key" "$out/libexec/wros-media-key"

    for script in \
      wros-keyboard wros-display wros-boot-settings wros-clock \
      wros-sound wros-passwd wros-media-key wros-backup wros-sync \
      wros-session wros-lock wros-recovery wros-import-media \
      wros-update wros-update-stage wros-update-cancel wros-update-repair \
      wros-update-media wros-update-fetch wros-update-apply \
      wros-update-rollback wros-update-healthy wros-music-signin; do
      install -m 755 "os/scripts/$script" "$out/libexec/$script"
      substituteInPlace "$out/libexec/$script" \
        --replace-fail "#!/usr/bin/python3 -I" "#!${pkgs.python3}/bin/python3 -I"
    done

    # Quote-independent --replace-fail substitutions per helper script
    substituteInPlace "$out/libexec/wros-music-signin" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/sbin/ipset" "${pkgs.ipset}/bin/ipset"

    substituteInPlace "$out/libexec/wros-import-media" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/lsblk" "${pkgs.util-linux}/bin/lsblk" \
      --replace-fail "/usr/bin/findmnt" "${pkgs.util-linux}/bin/findmnt" \
      --replace-fail "/usr/bin/mount" "${pkgs.util-linux}/bin/mount" \
      --replace-fail "/usr/bin/umount" "${pkgs.util-linux}/bin/umount"

    substituteInPlace "$out/libexec/wros-keyboard" \
      --replace-fail "/usr/bin/loadkeys" "${pkgs.kbd}/bin/loadkeys" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl" \
      --replace-fail "/usr/share/wros/keymaps" "${wros-keymaps}/share/wros/keymaps" \
      --replace-fail "BOOT_SYNC_DIR = None" "BOOT_SYNC_DIR = Path('/boot/wros-boot')"

    substituteInPlace "$out/libexec/wros-display" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl" \
      --replace-fail "/usr/libexec/wros-display-hw" "${wros-display-hw}/libexec/wros-display-hw"

    substituteInPlace "$out/libexec/wros-boot-settings" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl" \
      --replace-fail "BOOT_SYNC_DIR = None" "BOOT_SYNC_DIR = Path('/boot/wros-boot')" \
      --replace-fail "UPDATE_INITRAMFS = '/usr/sbin/update-initramfs'" "UPDATE_INITRAMFS = None"

    substituteInPlace "$out/libexec/wros-clock" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/timedatectl" "${pkgs.systemd}/bin/timedatectl"

    substituteInPlace "$out/libexec/wros-passwd" \
      --replace-fail "/usr/bin/passwd" "${wrapperDir}/passwd" \
      --replace-fail "/usr/bin:/bin" "${safePath}"

    substituteInPlace "$out/libexec/wros-media-key" \
      --replace-fail "/usr/lib/wros/keys/wros-keys.conf" "$out/share/wros/wros-keys.conf" \
      --replace-fail "/usr/bin/brightnessctl" "${pkgs.brightnessctl}/bin/brightnessctl" \
      --replace-fail "/usr/libexec/wros-sound" "$out/libexec/wros-sound" \
      --replace-fail "/usr/bin:/bin" "${safePath}"

    substituteInPlace "$out/libexec/wros-backup" \
      --replace-fail "/usr/lib/wros-backup/link.py" "$out/lib/wros-backup/link.py" \
      --replace-fail "/usr/share/wros/google-client.json" "/etc/wros/google-client.json" \
      --replace-fail "/usr/bin/rclone" "${pkgs.rclone}/bin/rclone" \
      --replace-fail "/usr/bin/restic" "${pkgs.restic}/bin/restic" \
      --replace-fail "/usr/bin:/bin" "${safePath}"

    substituteInPlace "$out/libexec/wros-sync" \
      --replace-fail "/usr/bin/rclone" "${pkgs.rclone}/bin/rclone" \
      --replace-fail "/usr/bin:/bin" "${safePath}"

    install -m 755 os/scripts/wros-ytm "$out/libexec/wros-ytm"
    substituteInPlace "$out/libexec/wros-ytm" \
      --replace-fail "#!/usr/bin/python3 -I" "#!${ytmPython}/bin/python3 -I"
    ln -s ../libexec/wros-ytm "$out/bin/wros-ytm"

    substituteInPlace "$out/libexec/wros-session" \
      --replace-fail "/usr/bin/grimoire" "/run/current-system/sw/bin/grimoire" \
      --replace-fail "/usr/bin/cage" "${wros-cage}/bin/cage" \
      --replace-fail "/usr/lib/wros/cage" "${wros-cage}/bin/cage" \
      --replace-fail "/usr/bin/dpkg-query" "${pkgs.coreutils}/bin/false" \
      --replace-fail "/usr/bin/foot" "${pkgs.foot}/bin/foot" \
      --replace-fail "/usr/share/wros/foot.ini" "$out/share/wros/foot.ini" \
      --replace-fail "/usr/lib/x86_64-linux-gnu/libwayland-egl.so.1" "${pkgs.wayland}/lib/libwayland-egl.so.1" \
      --replace-fail "/usr/bin/logger" "${pkgs.util-linux}/bin/logger" \
      --replace-fail "/usr/bin/pkexec" "${wrapperDir}/pkexec" \
      --replace-fail "/usr/libexec/wros-update-healthy" "$out/libexec/wros-update-healthy" \
      --replace-fail "/usr/libexec/wros-display" "$out/libexec/wros-display"

    # Libraries loaded through ctypes by bare name. NixOS has no system library cache, so
    # find_library() returns None and dlopen("libX.so.N") fails; the recovery menu crashed on
    # the first password. Give each its store path.
    substituteInPlace "$out/libexec/wros-recovery" \
      --replace-fail "ctypes.CDLL(ctypes.util.find_library('pam') or 'libpam.so.0')" "ctypes.CDLL('${pkgs.linux-pam}/lib/libpam.so.0')"
    substituteInPlace "$out/libexec/wros-sound" \
      --replace-fail "ctypes.util.find_library('asound') or 'libasound.so.2'" "'${pkgs.alsa-lib}/lib/libasound.so.2'"
    substituteInPlace "$out/libexec/wros-lock" \
      --replace-fail "ctypes.CDLL('libsystemd.so.0')" "ctypes.CDLL('${pkgs.systemd}/lib/libsystemd.so.0')"

    # wros-lock refuses to lock unless /etc/pam.d/vlock matches its own copy of the text, so
    # that copy gets the same upstream-PAM rewrite as nixos/modules/guards.nix (nixPam).
    # NixOS masks units through a store symlink that itself points to /dev/null, so the mask
    # check resolves the whole chain instead of reading one link.
    substituteInPlace "$out/libexec/wros-lock" \
      --replace-fail "@include common-auth" "auth include common-auth" \
      --replace-fail "require(os.readlink('/etc/systemd/system/' + name) == '/dev/null')" \
                     "require(os.path.realpath('/etc/systemd/system/' + name) == '/dev/null')"

    substituteInPlace "$out/libexec/wros-lock" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/lib/wros/wros-greeter" "/run/current-system/sw/bin/wros-greeter" \
      --replace-fail "/usr/lib/wros/wros-lock-screen" \
                     "/run/current-system/sw/bin/wros-lock-screen" \
      --replace-fail "/usr/bin/openvt" "${pkgs.kbd}/bin/openvt" \
      --replace-fail "/usr/sbin/vlock-main" "${pkgs.vlock}/sbin/vlock-main" \
      --replace-fail "/usr/lib/wros/cage" "${wros-cage}/bin/cage" \
      --replace-fail "/usr/bin/cage" "${wros-cage}/bin/cage" \
      --replace-fail "/usr/bin/grimoire" "/run/current-system/sw/bin/grimoire" \
      --replace-fail "/usr/bin/loginctl" "${pkgs.systemd}/bin/loginctl" \
      --replace-fail "/usr/bin/passwd" "${wrapperDir}/passwd" \
      --replace-fail "/usr/bin/systemd-run" "${pkgs.systemd}/bin/systemd-run" \
      --replace-fail "/usr/bin/chvt" "${pkgs.kbd}/bin/chvt" \
      --replace-fail "/usr/lib/vlock/modules/all.so" "${pkgs.vlock}/lib/vlock/modules/all.so" \
      --replace-fail "/usr/lib/vlock/modules/nosysrq.so" "${pkgs.vlock}/lib/vlock/modules/nosysrq.so" \
      --replace-fail "/usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so" "${pkgs.linux-pam}/lib/security/pam_succeed_if.so" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl" \
      --replace-fail "/usr/bin/busctl" "${pkgs.systemd}/bin/busctl" \
      --replace-fail "/usr/libexec/wros-lock" "$out/libexec/wros-lock"

    substituteInPlace "$out/libexec/wros-recovery" \
      --replace-fail "/usr/lib/wros-update/engine.py" "$out/lib/wros-update/engine.py" \
      --replace-fail "/usr/libexec/wros-backup" "$out/libexec/wros-backup" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl" \
      --replace-fail "/usr/libexec/wros-session" "$out/libexec/wros-session" \
      --replace-fail "/usr/sbin/runuser" "${pkgs.util-linux}/bin/runuser" \
      --replace-fail "/usr/bin/python3" "${pkgs.python3}/bin/python3" \
      --replace-fail "/usr/bin/journalctl" "${pkgs.systemd}/bin/journalctl"

    substituteInPlace "$out/libexec/wros-update" \
      --replace-fail "/usr/lib/wros-update/engine.py" "$out/lib/wros-update/engine.py"

    for updLauncher in \
      wros-update-stage wros-update-cancel wros-update-repair \
      wros-update-media wros-update-fetch wros-update-apply \
      wros-update-rollback wros-update-healthy; do
      substituteInPlace "$out/libexec/$updLauncher" \
        --replace-fail "/usr/lib/wros-update/engine.py" "$out/lib/wros-update/engine.py" \
        --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}"
    done

    install -m 644 os/scripts/wros-link.py "$out/lib/wros-backup/link.py"
    substituteInPlace "$out/lib/wros-backup/link.py" \
      --replace-fail "/usr/libexec/wros-backup" "$out/libexec/wros-backup"

    install -m 644 os/scripts/wros-update-engine.py "$out/lib/wros-update/debian_engine.py"
    substituteInPlace "$out/lib/wros-update/debian_engine.py" \
      --replace-fail "thin entry points in /usr/libexec:" "thin entry points in $out/libexec:" \
      --replace-fail "/usr/share/keyrings/wros-archive-keyring.gpg" "/etc/wros/update-trust/wros-archive-keyring.gpg" \
      --replace-fail "/usr/share/wros/update-signers" "/etc/wros/update-trust/update-signers" \
      --replace-fail "/usr/share/wros/update-keyring.gpg" "/etc/wros/update-trust/wros-archive-keyring.gpg" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/bin/dpkg-deb" "${pkgs.coreutils}/bin/false" \
      --replace-fail "/usr/bin/dpkg" "${pkgs.coreutils}/bin/false" \
      --replace-fail "/usr/bin/apt-get" "${pkgs.coreutils}/bin/false" \
      --replace-fail "/usr/bin/gpgv" "${pkgs.gnupg}/bin/gpgv" \
      --replace-fail "/usr/bin/lsblk" "${pkgs.util-linux}/bin/lsblk" \
      --replace-fail "/usr/bin/mount" "${pkgs.util-linux}/bin/mount" \
      --replace-fail "/usr/bin/systemctl" "${pkgs.systemd}/bin/systemctl"

    install -m 644 nixos/scripts/wros-update-engine.py "$out/lib/wros-update/engine.py"
    substituteInPlace "$out/lib/wros-update/engine.py" \
      --replace-fail "/usr/lib/wros-update/debian_engine.py" "$out/lib/wros-update/debian_engine.py" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}"

    for nixScript in wros-install-core wros-installer-backend wros-install-session wros-live-session; do
      install -m 755 "nixos/scripts/$nixScript" "$out/libexec/$nixScript"
      substituteInPlace "$out/libexec/$nixScript" \
        --replace-fail "#!/usr/bin/python3 -I" "#!${pkgs.python3}/bin/python3 -I"
    done

    substituteInPlace "$out/libexec/wros-install-core" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}"

    substituteInPlace "$out/libexec/wros-installer-backend" \
      --replace-fail "/usr/libexec/wros-install-core" "$out/libexec/wros-install-core" \
      --replace-fail "/usr/bin/loadkeys" "${pkgs.kbd}/bin/loadkeys" \
      --replace-fail "/usr/share/wros/keymaps" "${wros-keymaps}/share/wros/keymaps"

    substituteInPlace "$out/libexec/wros-install-session" \
      --replace-fail "ctypes.CDLL('libsystemd.so.0')" "ctypes.CDLL('${pkgs.systemd}/lib/libsystemd.so.0')" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}" \
      --replace-fail "/usr/libexec/wros-installer-backend" "$out/libexec/wros-installer-backend" \
      --replace-fail "/usr/libexec/wros-install-session" "$out/libexec/wros-install-session"

    substituteInPlace "$out/libexec/wros-live-session" \
      --replace-fail "/usr/libexec/wros-session" "$out/libexec/wros-session"

    install -m 755 nixos/scripts/wros-rescue "$out/bin/wros-rescue"
    substituteInPlace "$out/bin/wros-rescue" \
      --replace-fail "#!/bin/bash" "#!${pkgs.bash}/bin/bash" \
      --replace-fail "/usr/sbin:/usr/bin:/sbin:/bin" "${safePath}"

    ln -s wros-keyboard "$out/libexec/writingdeck-keyboard"
    ln -s wros-session "$out/libexec/writingdeck-session"

    # Bytecode for the imported modules, made here: otherwise Python writes __pycache__ into this
    # store path the first time a module is imported where the store is writable (the installer's
    # chroot, the rescue stick's), and the installed system then fails its own
    # `nix-store --verify --check-contents` (Repair in the recovery menu, wros-rescue).
    # unchecked-hash: Python uses these files as they are and never rewrites them.
    ${pkgs.python3}/bin/python3 -m compileall -q --invalidation-mode unchecked-hash "$out/lib"
    runHook postBuild
  '';

  dontInstall = true;
}
