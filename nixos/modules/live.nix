{ config, lib, pkgs, modulesPath, wrosPkgs, version, nixpkgsRev, wrosRoot, installedSystemToplevel, ... }:
let
  installedClosureInfo = pkgs.closureInfo {
    rootPaths = [ installedSystemToplevel ];
  };
in
{
  imports = [
    "${modulesPath}/installer/cd-dvd/iso-image.nix"
    ./base.nix
    ./hardware.nix
    ./network.nix
    ./audio.nix
    ./session.nix
    ./guards.nix
    ./polkit.nix
    ./recovery.nix
    ./updates.nix
  ];

  # Distinct ISO volume label so live media never collides with WROS-ESP, WROS-LUKS or WROS-ROOT.
  isoImage.volumeID = lib.mkForce "WROS_LIVE";
  isoImage.makeEfiBootable = true;
  isoImage.makeUsbBootable = true;

  # A stick written by copying the ISO's files (Rufus's default "ISO image mode") holds the same
  # files and label on FAT32, not ISO 9660. Mount whatever filesystem carries the label (#132);
  # vfat and the code pages it needs are in the initrd (hardware.nix). nixpkgs defines the whole
  # entry with mkImageMediaOverride (priority 60), which drops any ordinary definition of the
  # entry, so overriding one field is silently lost: replace the entry, and assert it held.
  fileSystems."/iso" = lib.mkForce {
    device = "/dev/disk/by-label/${config.isoImage.volumeID}";
    fsType = "auto";
    options = [ "ro" ];
    neededForBoot = true;
    noCheck = true;
  };
  # VM tests replace the whole filesystem layout (qemu-vm.nix, priority 10); judge only the
  # medium's own mount, where it exists.
  assertions = [{
    assertion =
      let iso = config.fileSystems."/iso" or null;
      in iso == null || iso.device != "/dev/disk/by-label/${config.isoImage.volumeID}" || iso.fsType == "auto";
    message = "the live medium must mount whatever filesystem carries WROS_LIVE (#132)";
  }];

  # WrOS branding on the medium: file name, boot menu entry (rc13's wording, now with the version),
  # the logo above the UEFI menu, and the logo and colours behind the legacy-BIOS menu.
  image.baseName = lib.mkForce "wros-${config.system.nixos.label}-x86_64";
  isoImage.prependToMenuLabel = "Start ";
  isoImage.appendToMenuLabel = " (try it, or install it from the dashboard)";
  isoImage.grubTheme = "${wrosPkgs.wros-branding}/grub-theme";
  isoImage.efiSplashImage = "${wrosPkgs.wros-branding}/bios-menu.png";
  isoImage.splashImage = "${wrosPkgs.wros-branding}/bios-menu.png";
  isoImage.syslinuxTheme = ''
    MENU TITLE WrOS
    MENU RESOLUTION 640 480
    MENU CLEAR
    MENU VSHIFT 17
    MENU ROWS 6
    MENU CMDLINEROW -4
    MENU TIMEOUTROW -3
    MENU TABMSGROW  -2
    MENU HELPMSGROW -1
    MENU HELPMSGENDROW -1
    MENU MARGIN 4

    #                                FG:AARRGGBB  BG:AARRGGBB   shadow
    MENU COLOR BORDER       30;40      #00000000    #00000000   none
    MENU COLOR SCREEN       37;40      #FFDEDCD4    #00000000   none
    MENU COLOR TABMSG       37;40      #FF6C6C7A    #00000000   none
    MENU COLOR TIMEOUT      1;37;40    #FFDEDCD4    #00000000   none
    MENU COLOR TIMEOUT_MSG  37;40      #FF6C6C7A    #00000000   none
    MENU COLOR CMDMARK      1;33;40    #FFD6AD60    #00000000   none
    MENU COLOR CMDLINE      37;40      #FFDEDCD4    #00000000   none
    MENU COLOR TITLE        1;33;40    #FFD6AD60    #00000000   none
    MENU COLOR UNSEL        37;40      #FFDEDCD4    #00000000   none
    MENU COLOR SEL          7;33;40    #FF000000    #FFD6AD60   std
  '';

  # Include the complete installed-system closure inside the ISO squashfs for offline installation.
  isoImage.storeContents = [
    installedSystemToplevel
    installedClosureInfo
  ];

  environment.etc."wros/installed-system-path".text = "${installedSystemToplevel}\n";
  environment.etc."wros/installed-system-closure".text = "${installedSystemToplevel}\n";
  environment.etc."wros/installed-closure-info".text = "${installedClosureInfo}\n";

  boot.kernelParams = [
    "boot=live"
    "wros.live=1"
    "loglevel=3"
  ];

  specialisation.rescue = {
    inheritParentConfig = true;
    configuration = {
      # The ISO uses the distro name and system label in both boot menus.
      system.nixos.label = lib.mkOverride 40 "rescue"; # above mkForce: NixOS tests force "test"
      isoImage.prependToMenuLabel = lib.mkForce "";
      isoImage.appendToMenuLabel = lib.mkForce "";
      boot.kernelParams = [ "systemd.unit=wros-live-rescue.target" ];

      systemd.services.greetd.enable = false;
      systemd.services.wros-live-session.enable = false;

      systemd.targets.wros-live-rescue = {
        description = "WrOS live rescue";
        requires = [ "basic.target" "wros-live-rescue.service" ];
        after = [ "basic.target" "wros-live-rescue.service" ];
      };

      systemd.services.wros-live-rescue = {
        description = "WrOS live rescue menu";
        after = [ "systemd-udev-trigger.service" "systemd-vconsole-setup.service" ];
        serviceConfig = {
          Type = "idle";
          User = "root";
          StandardInput = "tty";
          StandardOutput = "tty";
          StandardError = "tty";
          TTYPath = "/dev/tty1";
          TTYReset = true;
          TTYVHangup = true;
          TTYVTDisallocate = true;
          Restart = "always";
          RestartSec = 1;
          LimitCORE = 0;
        };
        script = ''
          trap ':' INT
          printf '\nWrOS rescue\n'
          ${pkgs.systemd}/bin/udevadm settle --timeout=30 || true
          ${wrosPkgs.paths.helperPaths.rescue} || true

          while true; do
            printf '\n1  Run rescue again\n2  Roll back to the previous generation\n3  Power off\n'
            printf 'Choose: '
            IFS= read -r choice || exit 0
            case "$choice" in
              1) ${wrosPkgs.paths.helperPaths.rescue} || true ;;
              2) ${wrosPkgs.paths.helperPaths.rescue} --rollback || true ;;
              3) exec ${pkgs.systemd}/bin/systemctl poweroff ;;
            esac
          done
        '';
      };
    };
  };

  # Live session account (UID 1000) and locked frontend worker account (_wros-installer)
  users.users.wros-live = {
    isNormalUser = true;
    uid = 1000;
    group = "wros-live";
    extraGroups = [ "wros" "writingdeck" "video" "audio" ];
    home = "/home/wros-live";
    createHome = true;
    # Live session marker checks `passwd -S wros-live` == `P`
    initialPassword = "wros-live-session-only";
  };
  users.groups.wros-live = { gid = 1000; };

  users.users._wros-installer = {
    isSystemUser = true;
    group = "_wros-installer";
    home = "/var/empty";
    shell = "${pkgs.shadow}/bin/nologin";
    hashedPassword = "!";
  };
  users.groups._wros-installer = {};

  # Reserve tty7 for the supervised live installer frontend.
  systemd.services."getty@tty7".enable = false;
  systemd.services."autovt@tty7".enable = false;

  environment.systemPackages = [
    wrosPkgs.wros-installer
    pkgs.nixos-install-tools
    pkgs.cryptsetup
    pkgs.e2fsprogs
    pkgs.dosfstools
    pkgs.gptfdisk
    pkgs.parted
    pkgs.lvm2
  ];

  security.polkit.extraConfig = lib.mkAfter ''
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-installer.rules"}
  '';

  systemd.services.wros-live-session = {
    description = "Prepare WrOS live session marker and automatic greetd entry";
    before = [ "greetd.service" ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${wrosPkgs.paths.helperPaths.liveSession}";
    };
  };

  systemd.services.greetd.after = [ "wros-live-session.service" ];
  systemd.services.greetd.wants = [ "wros-live-session.service" ];
  systemd.services.greetd.serviceConfig.EnvironmentFile = [ "-/run/wros-live/greetd.env" ];
}
