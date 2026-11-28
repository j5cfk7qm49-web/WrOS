{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
let
  mediaKey = wrosPkgs.paths.helperPaths.mediaKey;
  keyBindings = [
    { keys = [ "BRIGHTNESSUP" ]; event = "press"; cmd = "${mediaKey} brightness-up"; }
    { keys = [ "BRIGHTNESSUP" ]; event = "hold"; cmd = "${mediaKey} brightness-up"; }
    { keys = [ "BRIGHTNESSDOWN" ]; event = "press"; cmd = "${mediaKey} brightness-down"; }
    { keys = [ "BRIGHTNESSDOWN" ]; event = "hold"; cmd = "${mediaKey} brightness-down"; }
    { keys = [ "VOLUMEUP" ]; event = "press"; cmd = "${mediaKey} volume-up"; }
    { keys = [ "VOLUMEUP" ]; event = "hold"; cmd = "${mediaKey} volume-up"; }
    { keys = [ "VOLUMEDOWN" ]; event = "press"; cmd = "${mediaKey} volume-down"; }
    { keys = [ "VOLUMEDOWN" ]; event = "hold"; cmd = "${mediaKey} volume-down"; }
    { keys = [ "MUTE" ]; event = "press"; cmd = "${mediaKey} mute"; }
    { keys = [ "PLAYPAUSE" ]; event = "press"; cmd = "${mediaKey} play-pause"; }
    { keys = [ "NEXTSONG" ]; event = "press"; cmd = "${mediaKey} next"; }
    { keys = [ "PREVIOUSSONG" ]; event = "press"; cmd = "${mediaKey} previous"; }
  ];
  # The same lines services.triggerhappy writes: "KEY_<NAME> <1 press|2 hold|0 release> <command>".
  triggers = pkgs.writeText "triggerhappy.conf" (lib.concatMapStrings (b:
    "${lib.concatMapStringsSep "+" (k: "KEY_" + k) b.keys} ${
      toString { press = 1; hold = 2; release = 0; }.${b.event}
    } ${b.cmd}\n") keyBindings);
  thd = pkgs.triggerhappy.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../pkgs/triggerhappy-no-duplicate-devices.patch ];
  });
in
{
  imports = [ ./firmware.nix ];

  boot.kernelPackages = pkgs.linuxPackages;

  hardware.enableRedistributableFirmware = false;
  # NixOS adds pkgs.wireless-regdb to hardware.firmware through this option.
  hardware.wirelessRegulatoryDatabase = true;

  hardware.graphics.enable = true;

  # Early KMS graphics and generic UEFI laptop + QEMU virtio storage/keyboard drivers
  boot.initrd.availableKernelModules = [
    "amdgpu"
    "radeon"
    "xe"
    "mmc_block"
    "sdhci"
    "sdhci_pci"
    "sdhci_acpi"
    "nvme"
    "ahci"
    "sd_mod"
    "sr_mod"
    "virtio_pci"
    "virtio_blk"
    "virtio_scsi"
    "virtio_console"
    "usb_storage"
    "uas"
    "ehci_pci"
    "xhci_pci"
    "xhci_hcd"
    "usbhid"
    "hid_generic"
    "atkbd"
    "i8042"
    "vfat"
    "nls_cp437"
    "nls_iso8859_1"
    "ext4"
    "dm_mod"
    "dm_crypt"
    "cryptd"
  ];

  boot.initrd.kernelModules = [
    "i915"
    "virtio_gpu"
    "bochs"
    "simpledrm"
  ];

  # Modprobe options from os/config/60-wros-wifi-power.conf and 60-wros-brightness-keys.conf
  boot.extraModprobeConfig = ''
    ${builtins.readFile (wrosRoot + "/os/config/60-wros-wifi-power.conf")}
    ${builtins.readFile (wrosRoot + "/os/config/60-wros-brightness-keys.conf")}
  '';

  # Udev rule from os/config/90-wros-backlight.rules
  services.udev.extraRules = ''
    ACTION=="add", SUBSYSTEM=="backlight", RUN+="${pkgs.coreutils}/bin/chgrp video /sys/class/backlight/%k/brightness", RUN+="${pkgs.coreutils}/bin/chmod g+w /sys/class/backlight/%k/brightness"
  '';

  services.upower.enable = true;
  services.power-profiles-daemon.enable = true;

  services.triggerhappy = {
    enable = true;
    user = "wros-keys";
    bindings = keyBindings;
  };
  # NixOS starts thd with a glob of every input device, and its udev rule hands thd the same
  # devices again as they appear; at boot both happen, and stock thd opened such a device twice,
  # so every press acted twice (#171). Run a thd that refuses a device it already has open. The
  # module's own config file isn't exposed, so the same bindings are written here the same way.
  systemd.services.triggerhappy.serviceConfig.ExecStart = lib.mkForce
    "${thd}/bin/thd --user wros-keys --socket /run/thd.socket --triggers ${triggers} --deviceglob /dev/input/event*";
}
