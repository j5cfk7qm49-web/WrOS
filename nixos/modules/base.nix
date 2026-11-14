{ config, lib, pkgs, wrosPkgs, version, nixpkgsRev, wrosRoot, ... }:
let
  releaseText = ''
    WROS_NAME="WrOS"
    WROS_VERSION=${version}
    WROS_BASE="NixOS 26.05 (${nixpkgsRev})"
  '';
  legacyReleaseText = ''
    WROS_NAME="WrOS"
    WRITINGDECK_VERSION=${version}
    WROS_BASE="NixOS 26.05 (${nixpkgsRev})"
  '';
  # Exclude mbrola and mbrola-voices (645 MiB) which Debian rc13 never shipped (S1)
  espeakNgLean = pkgs.espeak-ng.override { mbrolaSupport = false; };
in
{
  system.stateVersion = "26.05";
  system.nixos.distroName = "WrOS";
  system.nixos.distroId = "wros";
  system.nixos.variantName = "Writer's OS";
  system.nixos.variant_id = "appliance";

  # Appliance model: no channels, no nixos-rebuild, no nixpkgs source, no flake registry, no compiler.
  nix.enable = true;
  nix.channel.enable = false;
  nix.registry = lib.mkForce {};
  nix.nixPath = lib.mkForce [];
  nix.settings = {
    experimental-features = [];
    substituters = lib.mkForce [];
    trusted-public-keys = lib.mkForce [];
  };
  system.disableInstallerTools = true;
  system.switch.enable = true;

  # Do not let NixOS manage /etc/hostname so wros-install-core's chosen hostname persists (H3).
  networking.hostName = lib.mkOverride 1400 "";  # weaker than mkDefault: the VM test driver names its machines

  # Lean closure (S1): no default packages, docs, desktop font packs, or XDG desktop themes/menus.
  environment.defaultPackages = lib.mkForce [];
  documentation.enable = false;
  documentation.man.enable = false;
  documentation.info.enable = false;
  documentation.doc.enable = false;
  documentation.nixos.enable = false;

  fonts.enableDefaultPackages = false;
  fonts.packages = [ pkgs.dejavu_fonts ];

  xdg.sounds.enable = lib.mkForce false;
  xdg.icons.enable = lib.mkForce false;
  xdg.menus.enable = lib.mkForce false;
  xdg.mime.enable = lib.mkForce false;
  xdg.autostart.enable = lib.mkForce false;
  # NixOS turns speech-dispatcher on with any graphical session. It is what pulled in
  # mbrola-voices (645 MiB) and flite (60 MiB); read-aloud calls espeak-ng directly.
  services.speechd.enable = lib.mkForce false;

  # Link /libexec and /share/wros into /run/current-system/sw (H4).
  environment.pathsToLink = [ "/libexec" "/share/wros" ];

  i18n.defaultLocale = "C.UTF-8";

  # Mutable timezone (not managed via environment.etc)
  time.timeZone = null;

  # Accounts are mutable and created by the installer; root stays locked.
  users.mutableUsers = true;
  users.users.root = {
    hashedPassword = "!";
  };

  users.groups.wros = {};
  users.groups.writingdeck = {};
  users.groups._greetd = {};
  users.groups.wros-keys = {};

  users.users._greetd = {
    isSystemUser = true;
    group = "_greetd";
    extraGroups = [ "video" ];
    home = "/var/empty";
    shell = "${pkgs.shadow}/bin/nologin";
  };

  users.users.wros-keys = {
    isSystemUser = true;
    group = "wros-keys";
    description = "WrOS brightness and volume keys";
    home = "/var/empty";
    shell = "${pkgs.shadow}/bin/nologin";
    extraGroups = [ "video" "audio" ];
  };

  environment.systemPackages = [
    wrosPkgs.grimoire
    wrosPkgs.wros-greeter
    wrosPkgs.wros-boot
    wrosPkgs.wros-console-reset
    wrosPkgs.wros-display-hw
    wrosPkgs.wros-cage
    wrosPkgs.wros-keymaps
    wrosPkgs.wros-prose-runtime
    wrosPkgs.wros-helpers
    wrosPkgs.wros-licenses
    pkgs.foot
    pkgs.kbd
    pkgs.vlock
    pkgs.brightnessctl
    pkgs.triggerhappy
    pkgs.git
    pkgs.restic
    pkgs.rclone
    pkgs.gnupg
    pkgs.cacert
    pkgs.wordnet
    pkgs.util-linux
    pkgs.coreutils
    pkgs.shadow
    pkgs.less
    pkgs.nano
    pkgs.pciutils
    pkgs.usbutils
    pkgs.python3
    espeakNgLean
    pkgs.playerctl
    pkgs.xkeyboard_config
  ];

  environment.variables = {
    WROS_WORDNET_DIR = "${pkgs.wordnet}/dict";
    WROS_TOOLS_PYTHON = "${wrosPkgs.wros-prose-runtime}/bin/python3";
  };

  # Read-only release identity, installed version (U3), and seed defaults.
  environment.etc."wros/version".text = "${version}\n";
  environment.etc."wros/release".text = releaseText;
  environment.etc."writingdeck/release".text = legacyReleaseText;
  environment.etc."wros/defaults/keyboard.json".source = wrosRoot + "/os/config/keyboard.json";

  # Seed mutable state once so updates and rollbacks never overwrite writer choices.
  systemd.tmpfiles.rules = [
    "d /etc/wros 0755 root root -"
    "d /etc/writingdeck 0755 root root -"
    "C /etc/wros/keyboard.json 0644 root root - /etc/wros/defaults/keyboard.json"
    "L+ /etc/writingdeck/keyboard.json - - - - ../wros/keyboard.json"
    "d /etc/default 0755 root root -"
    "f /etc/default/keyboard 0644 root root - XKBLAYOUT=\"us\"\\nXKBVARIANT=\"\"\\nXKBMODEL=\"pc105\"\\nXKBOPTIONS=\"\"\\n"
    "f /etc/hostname 0644 root root - wros\\n"
    "d /etc/NetworkManager/system-connections 0700 root root -"
    "d /var/lib/wros 0755 root root -"
    "d /var/lib/wros/boot-screen 0700 root root -"
    "d /var/lib/wros-update 0711 root root -"
    "d /var/lib/wros-update/public 0755 root root -"
    "d /var/cache/wros-update 0755 root root -"
    "d /var/cache/wros-update/online 0755 root root -"
    "d /nix/var/nix/gcroots/wros 0755 root root -"
    "d /boot/wros-boot 0755 root root -"
    "d /run/wros-keys 0755 wros-keys wros-keys -"
    "w /sys/module/video/parameters/brightness_switch_enabled - - - - N"
  ];
}
