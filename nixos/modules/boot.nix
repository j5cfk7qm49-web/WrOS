{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
{
  # Fixed disk labels written by the installer; never a baked-in disk UUID.
  fileSystems."/" = {
    device = "/dev/disk/by-label/WROS-ROOT";
    fsType = "ext4";
    options = [ "noatime" ];
  };

  fileSystems."/boot" = {
    device = "/dev/disk/by-label/WROS-ESP";
    fsType = "vfat";
    options = [ "fmask=0077" "dmask=0077" ];
  };

  # Systemd stage-1 initrd (B1, B2, B5):
  # wros-initrd-unlock.service runs before systemd-cryptsetup@wros_crypt.service, reads untrusted
  # keyboard/theme/boot-screen files from /boot/wros-boot with strict size and allowlist checks,
  # loads the console keymap, and invokes wros-boot unlock to write /crypt-ramfs/passphrase.
  # On wros.unlock=text, boot-screen != ratatui, Esc, or any non-zero exit, /crypt-ramfs/passphrase
  # is left absent and systemd-cryptsetup@wros_crypt.service automatically falls back to systemd's
  # standard text passphrase prompt (fallbackToPassword is implied by systemd stage 1 in luksroot.nix).
  boot.initrd.enable = true;
  boot.initrd.systemd = {
    enable = true;
    emergencyAccess = false;

    extraBin = {
      wros-boot = "${wrosPkgs.wros-boot}/bin/wros-boot";
      wros-console-reset = "${wrosPkgs.wros-console-reset}/bin/wros-console-reset";
      loadkeys = "${pkgs.kbd}/bin/loadkeys";
    };

    storePaths = [
      wrosPkgs.wros-boot
      wrosPkgs.wros-console-reset
      pkgs.kbd
      wrosPkgs.wros-keymaps
    ];

    contents."/etc/wros/keymaps".source = "${wrosPkgs.wros-keymaps}/share/wros/keymaps";

    services.wros-initrd-unlock = {
      description = "WrOS graphical disk unlock before systemd-cryptsetup";
      wantedBy = [ "systemd-cryptsetup@wros_crypt.service" ];
      before = [
        "systemd-cryptsetup@wros_crypt.service"
        "initrd-switch-root.target"
        "shutdown.target"
      ];
      after = [
        "systemd-modules-load.service"
        "systemd-udev-settle.service"
      ];
      wants = [ "systemd-udev-settle.service" ];
      conflicts = [
        "initrd-switch-root.target"
        "shutdown.target"
      ];
      unitConfig.DefaultDependencies = "no";
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        StandardInput = "tty-force";
        StandardOutput = "tty";
        StandardError = "tty";
        TTYPath = "/dev/console";
        ExecStop = "-${config.boot.initrd.systemd.package.util-linux}/bin/umount /crypt-ramfs";
      };
      script = ''
        mkdir -p /crypt-ramfs /etc/wros /run/wros-boot-esp
        mount -t ramfs -o mode=0700 none /crypt-ramfs
        umask 077

        # Mount unencrypted WROS-ESP read-only and read untrusted config files with 8 KiB caps.
        if mount -t vfat -o ro,nodev,nosuid,noexec /dev/disk/by-label/WROS-ESP /run/wros-boot-esp 2>/dev/null; then
          for name in keyboard.json boot-theme.toml boot-screen; do
            src="/run/wros-boot-esp/wros-boot/$name"
            if [ -f "$src" ] && [ ! -L "$src" ]; then
              head -c 8192 "$src" > "/etc/wros/$name" 2>/dev/null || rm -f "/etc/wros/$name"
            fi
          done
          umount /run/wros-boot-esp 2>/dev/null || true
        fi

        # Apply console keymap strictly from the 4 compiled layouts so both graphical and text prompts use it.
        if [ -f /etc/wros/keyboard.json ]; then
          layout=$(sed -n 's/.*"layout"[[:space:]]*:[[:space:]]*"\([a-z0-9-]*\)".*/\1/p' /etc/wros/keyboard.json | head -n 1)
          case "$layout" in
            qwerty|colemak|colemak-dh-ansi|colemak-dh-iso)
              /bin/loadkeys --unicode "/etc/wros/keymaps/$layout.map" >/dev/null 2>&1 || true
              ;;
          esac
        fi

        # Fall back to systemd-cryptsetup's text prompt on wros.unlock=text or when boot-screen != ratatui.
        case " $(cat /proc/cmdline 2>/dev/null) " in
          *" wros.unlock=text "*)
            exit 0
            ;;
        esac
        if [ -f /etc/wros/boot-screen ] && [ "$(cat /etc/wros/boot-screen 2>/dev/null)" != "ratatui" ]; then
          exit 0
        fi

        if /bin/wros-boot unlock --tty /dev/console --theme /etc/wros/boot-theme.toml > /crypt-ramfs/passphrase.tmp; then
          if [ -s /crypt-ramfs/passphrase.tmp ]; then
            mv /crypt-ramfs/passphrase.tmp /crypt-ramfs/passphrase
            exit 0
          fi
        fi
        rm -f /crypt-ramfs/passphrase.tmp /crypt-ramfs/passphrase
        /bin/wros-console-reset >/dev/null 2>&1 || true
        exit 0
      '';
    };
  };

  boot.initrd.luks.devices."wros_crypt" = {
    device = "/dev/disk/by-label/WROS-LUKS";
    keyFile = "/crypt-ramfs/passphrase";
    keyFileTimeout = 1;
    allowDiscards = true;
  };

  boot.kernelParams = [
    "loglevel=3"
    "rd.systemd.unit=initrd.target"
    "panic=10"
  ];
  boot.consoleLogLevel = lib.mkDefault 3;

  # GRUB UEFI bootloader (B3, B4): removable EFI path (EFI/BOOT/BOOTX64.EFI) so newly installed
  # disks boot without relying on host NVRAM variables, hidden 2s menu (Esc shows it), 30s menu
  # after a boot that never reached login (wros_boot_pending=1 in /boot/grub/grubenv), and
  # specialisations named "WrOS (text start)" and "WrOS recovery".
  boot.loader.efi.canTouchEfiVariables = false;
  boot.loader.efi.efiSysMountPoint = "/boot";
  boot.loader.timeout = 2;

  boot.loader.grub = {
    enable = true;
    device = "nodev";
    efiSupport = true;
    efiInstallAsRemovable = true;
    timeoutStyle = "hidden";
    configurationLimit = 3;
    extraConfig = ''
      # WrOS: a boot that never reached the login screen shows this menu for 30 seconds.
      if [ "''${wros_boot_pending}" = "1" ]; then
        set timeout_style=menu
        set timeout=30
      else
        set timeout_style=hidden
        set timeout=2
      fi
      set wros_boot_pending=1
      save_env wros_boot_pending
    '';
  };

  specialisation.text-start = {
    inheritParentConfig = true;
    configuration = {
      boot.loader.grub.configurationName = "WrOS (text start)";
      boot.kernelParams = [ "wros.unlock=text" "wros.session=console" ];
    };
  };

  specialisation.recovery = {
    inheritParentConfig = true;
    configuration = {
      boot.loader.grub.configurationName = "WrOS recovery";
      boot.kernelParams = [ "wros.unlock=text" "systemd.unit=wros-recovery.target" ];
    };
  };

  # Ensure /boot/grub/grubenv exists and clear wros_boot_pending once greetd starts (B3).
  systemd.services.wros-boot-ok = {
    description = "Record that this boot reached the WrOS login screen";
    after = [ "greetd.service" ];
    wantedBy = [ "multi-user.target" "system-update.target" ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.bash}/bin/sh -ec 'mkdir -p /boot/grub; if [ ! -s /boot/grub/grubenv ]; then ${pkgs.grub2}/bin/grub-editenv /boot/grub/grubenv create; fi; ${pkgs.grub2}/bin/grub-editenv /boot/grub/grubenv unset wros_boot_pending'";
      ProtectHome = true;
      ProtectSystem = "strict";
      ReadWritePaths = [ "/boot" ];
      NoNewPrivileges = true;
      PrivateNetwork = true;
    };
  };

  # Start screen splash until greetd takes tty1 (os/systemd/wros-boot-splash.service)
  systemd.services.wros-boot-splash = {
    description = "WrOS start screen: the logo and boot progress until the login screen";
    unitConfig = {
      DefaultDependencies = false;
      ConditionKernelCommandLine = "!wros.unlock=text";
    };
    after = [ "systemd-vconsole-setup.service" ];
    before = [ "shutdown.target" ];
    conflicts = [ "shutdown.target" ];
    wantedBy = [ "sysinit.target" ];
    serviceConfig = {
      Type = "simple";
      ExecCondition = "${pkgs.bash}/bin/sh -c 'test ! -e /etc/wros/boot-screen || test \"$(cat /etc/wros/boot-screen)\" = ratatui'";
      ExecStart = "${wrosPkgs.wros-boot}/bin/wros-boot splash --tty /dev/tty1";
      TimeoutStopSec = 3;
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      PrivateTmp = true;
    };
  };

  # Sync start-screen settings to /boot/wros-boot when changed (no initramfs compilation)
  systemd.paths.wros-boot-rebuild = {
    description = "Watch for a WrOS start-screen change";
    wantedBy = [ "multi-user.target" ];
    pathConfig = {
      PathExists = [
        "/var/lib/wros/boot-screen/pending"
        "/var/lib/wros/boot-screen/building"
      ];
      Unit = "wros-boot-rebuild.service";
    };
  };

  systemd.services.wros-boot-rebuild = {
    description = "Prepare the WrOS start screen for the next restart";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${wrosPkgs.paths.helperPaths.bootSettings} rebuild";
      TimeoutStartSec = "2min";
      UMask = "0022";
    };
  };
}
