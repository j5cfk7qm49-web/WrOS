{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
{
  # Fixed disk labels written by the installer; never a baked-in disk UUID.
  fileSystems."/" = {
    device = "/dev/disk/by-label/WROS-ROOT";
    fsType = "ext4";
    # The root only appears once the writer unlocks the disk, so wait for it however long they
    # take. systemd's default gave up after 5 minutes at the unlock screen, leaving the prompt
    # dead and the boot stuck (nixos/tests/unlock-waits.nix).
    options = [ "noatime" "x-systemd.device-timeout=infinity" ];
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
        # Notes go to the kernel log at info level: hidden on the laptop's screen (loglevel=3),
        # kept in the journal and shown on the serial console in the VM tests.
        note() { echo "<6>wros-initrd-unlock: $*" > /dev/kmsg 2>/dev/null || true; }

        mkdir -p /crypt-ramfs /etc/wros /run/wros-boot-esp
        mount -t ramfs -o mode=0700 none /crypt-ramfs
        umask 077

        # This unit starts before udev has probed the disk, so the ESP's label link may not
        # exist yet. Wait for it briefly; a missing ESP only costs the saved layout and theme.
        udevadm wait --timeout=15 /dev/disk/by-label/WROS-ESP 2>/dev/null || note "WROS-ESP did not appear"

        # Mount unencrypted WROS-ESP read-only and read untrusted config files with 8 KiB caps.
        if mount -t vfat -o ro,nodev,nosuid,noexec /dev/disk/by-label/WROS-ESP /run/wros-boot-esp 2>/dev/null; then
          for name in keyboard.json boot-theme.toml boot-screen; do
            src="/run/wros-boot-esp/wros-boot/$name"
            if [ -f "$src" ] && [ ! -L "$src" ]; then
              head -c 8192 "$src" > "/etc/wros/$name" 2>/dev/null || rm -f "/etc/wros/$name"
            fi
          done
          umount /run/wros-boot-esp 2>/dev/null || true
        else
          note "could not mount WROS-ESP; using the default layout"
        fi

        # Apply console keymap strictly from the 4 compiled layouts so both graphical and text prompts use it.
        # The systemd initrd has bash and coreutils but no sed, so the layout is parsed in bash.
        layout=qwerty
        if [ -f /etc/wros/keyboard.json ]; then
          json=$(head -c 8192 /etc/wros/keyboard.json)
          if [[ $json =~ \"layout\"[[:space:]]*:[[:space:]]*\"([a-z0-9-]+)\" ]]; then
            layout=''${BASH_REMATCH[1]}
          fi
        fi
        case "$layout" in
          qwerty) ;;
          colemak|colemak-dh-ansi|colemak-dh-iso)
            if /bin/loadkeys --unicode "/etc/wros/keymaps/$layout.map" >/dev/null 2>&1; then
              note "keyboard layout $layout"
            else
              note "loadkeys failed for $layout; the prompt uses qwerty"
            fi
            ;;
          *) note "unknown layout '$layout' ignored" ;;
        esac

        # Fall back to systemd-cryptsetup's text prompt on wros.unlock=text or when boot-screen != ratatui.
        case " $(cat /proc/cmdline 2>/dev/null) " in
          *" wros.unlock=text "*)
            exit 0
            ;;
        esac
        if [ -f /etc/wros/boot-screen ] && [ "$(cat /etc/wros/boot-screen 2>/dev/null)" != "ratatui" ]; then
          exit 0
        fi

        note "asking for the disk password"
        if /bin/wros-boot unlock --tty /dev/console --theme /etc/wros/boot-theme.toml > /crypt-ramfs/passphrase.tmp; then
          if [ -s /crypt-ramfs/passphrase.tmp ]; then
            mv /crypt-ramfs/passphrase.tmp /crypt-ramfs/passphrase
            note "password handed to systemd-cryptsetup"
            exit 0
          fi
        fi
        note "graphical prompt ended without a password; falling back to the text prompt"
        rm -f /crypt-ramfs/passphrase.tmp /crypt-ramfs/passphrase
        /bin/wros-console-reset >/dev/null 2>&1 || true
        exit 0
      '';
    };
  };

  # The start screen from the moment the password is accepted until the switch to the installed
  # system, where wros-boot-splash.service carries the bar on from the figure this one leaves in
  # /run. It starts alongside the unlock screen but waits, touching neither the console nor the
  # display, until that screen has handed a password over (/run/wros-boot/unlocked); then it
  # replaces it in place, so the disk's key derivation runs under the progress screen, not a black
  # one (#165). With no graphical unlock (Esc, the text start, boot-screen != ratatui) it never
  # shows. A wrong password ends in systemd's own text prompt on the console: starting that
  # prompt's agent stops this screen first (Conflicts= with an ordering), so the prompt is never
  # covered and this screen has given the console back before the agent reads from it.
  # It is not stopped at the switch to the installed system: it carries on through it (left out of
  # initrd-switch-root.target's isolate; argv[0] starting with "@" spares it the initrd's last kill,
  # as Plymouth does) until wros-boot-splash.service's first frame replaces it in place, and ends
  # by itself 8 seconds after systemd stops answering it if nothing does.
  boot.initrd.systemd.services.wros-initrd-splash = {
    description = "WrOS start screen: boot progress after the unlock";
    wantedBy = [ "initrd.target" ];
    after = [
      "systemd-modules-load.service"
      "systemd-udev-settle.service"
    ];
    before = [
      "initrd-switch-root.target"
      "shutdown.target"
      "systemd-ask-password-console.service"
    ];
    conflicts = [
      "shutdown.target"
      "systemd-ask-password-console.service"
    ];
    unitConfig = {
      DefaultDependencies = "no";
      ConditionKernelCommandLine = "!wros.unlock=text";
      IgnoreOnIsolate = true;
    };
    serviceConfig = {
      Type = "simple";
      ExecStart = "@/bin/wros-boot @wros-boot splash --stage initrd --tty /dev/console --theme /etc/wros/boot-theme.toml";
      TimeoutStopSec = 3;
    };
  };

  boot.initrd.luks.devices."wros_crypt" = {
    device = "/dev/disk/by-label/WROS-LUKS";
    keyFile = "/crypt-ramfs/passphrase";
    keyFileTimeout = 1;
    allowDiscards = true;
  };

  # quiet: systemd (in the initrd and after it) shows only failures on the screen instead of a
  # line per unit; the start screen shows the progress, and the journal keeps every line. The
  # text start and recovery entries bring the lines back with systemd.show_status=true.
  boot.kernelParams = [
    "quiet"
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
    # The menu (after a boot that never reached login, or on Esc) shows the WrOS logo and colours
    # instead of NixOS's grey artwork.
    theme = "${wrosPkgs.wros-branding}/grub-theme";
    splashImage = null;
    extraConfig = ''
      # WrOS: a boot that never reached the login screen shows this menu for 30 seconds.
      if [ "''${wros_boot_pending}" = "1" ]; then
        set timeout_style=menu
        set timeout=30
      else
        set timeout_style=hidden
        set timeout=2
        # #165: the start picture instead of a black screen during the hidden wait. With
        # gfxpayload=keep it stays on screen into Linux until the unlock screen draws over it.
        # The picture drawn for this machine's screen size (wros-boot-logo.service) when there is
        # one, else the packaged 1366x768 one: GRUB pins it to the top-left corner.
        insmod png
        if [ -f /wros-boot/boot-logo.png ]; then
          background_image --mode normal /wros-boot/boot-logo.png
        elif [ -n "''${theme}" ]; then
          insmod regexp
          if regexp --set=1:wros_theme_dir '^(.*)/theme\.txt$' "''${theme}"; then
            background_image --mode normal "''${wros_theme_dir}/boot-logo.png"
          fi
        fi
      fi
      set wros_boot_pending=1
      save_env wros_boot_pending
    '';
  };

  specialisation.text-start = {
    inheritParentConfig = true;
    configuration = {
      boot.loader.grub.configurationName = "WrOS (text start)";
      boot.kernelParams = [ "wros.unlock=text" "wros.session=console" "systemd.show_status=true" ];
    };
  };

  specialisation.recovery = {
    inheritParentConfig = true;
    configuration = {
      boot.loader.grub.configurationName = "WrOS recovery";
      boot.kernelParams = [
        "wros.unlock=text"
        "systemd.unit=wros-recovery.target"
        "systemd.show_status=true"
      ];
    };
  };

  # Ensure /boot/grub/grubenv exists and clear wros_boot_pending once greetd starts (B3).
  systemd.services.wros-boot-ok = {
    description = "Record that this boot reached the WrOS login screen";
    after = [ "greetd.service" ];
    wantedBy = [ "multi-user.target" "system-update.target" ];
    serviceConfig = {
      Type = "oneshot";
      # Stays active after it runs, so "this boot reached login" can be queried later.
      RemainAfterExit = true;
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
      # /run/wros-boot-splash/handed-over tells greetd the console is free for the login screen.
      RuntimeDirectory = "wros-boot-splash";
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      # No PrivateTmp: it orders the unit after systemd-tmpfiles-setup.service and so after every
      # local filesystem and the journal flush, which held the screen back well into the boot.
      # It reads the initrd screen's last figure from /run/wros-boot/progress.
    };
  };

  # The boot menu's start picture at this machine's screen size (#165): GRUB can't centre a
  # picture, so it is drawn here to the size of the framebuffer GRUB hands to Linux, with the logo
  # where the unlock screen draws it. Only redrawn when that size changes.
  systemd.services.wros-boot-logo = {
    description = "Fit the boot menu's start picture to this screen";
    wantedBy = [ "multi-user.target" ];
    after = [ "local-fs.target" ];
    unitConfig = {
      RequiresMountsFor = "/boot";
      ConditionPathIsMountPoint = "/boot";
      ConditionPathExists = "/sys/kernel/boot_params/data";
    };
    serviceConfig = {
      Type = "oneshot";
      Nice = 19;
      ExecStart = "${pkgs.python3}/bin/python3 -I ${wrosRoot}/nixos/scripts/wros-boot-logo ${wrosPkgs.wros-branding}/share/wros-branding/wros-logo-png ${wrosPkgs.wros-branding}/share/wros-branding/logo.rs";
      ProtectSystem = "strict";
      ReadWritePaths = [ "/boot" ];
      ProtectHome = true;
      PrivateNetwork = true;
      NoNewPrivileges = true;
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
