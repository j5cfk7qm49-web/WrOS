{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
let
  # Trim linux-firmware to the supported Wi-Fi chipsets (os/config/wifi-firmware.json)
  # plus Intel UHD 600 graphics (i915), excluding server NICs, cameras, and Bluetooth (S1).
  wrosFirmware = pkgs.runCommand "wros-firmware" {} ''
    mkdir -p "$out/lib/firmware"
    for dir in \
      iwlwifi rtlwifi rtw88 rtw89 \
      ath9k_htc ath10k ath11k \
      brcm mediatek \
      i915; do
      if [ -e "${pkgs.linux-firmware}/lib/firmware/$dir" ]; then
        cp -rL "${pkgs.linux-firmware}/lib/firmware/$dir" "$out/lib/firmware/"
      fi
    done
    for file in "${pkgs.linux-firmware}"/lib/firmware/iwlwifi-* "${pkgs.linux-firmware}"/lib/firmware/regulatory*; do
      if [ -e "$file" ]; then
        cp -aL "$file" "$out/lib/firmware/"
      fi
    done
  '';
in
{
  boot.kernelPackages = pkgs.linuxPackages;

  hardware.enableRedistributableFirmware = false;
  hardware.firmware = [
    wrosFirmware
    pkgs.wireless-regdb
    pkgs.sof-firmware
  ];
  hardware.wirelessRegulatoryDatabase = true;

  hardware.graphics.enable = true;

  # Early KMS graphics and generic UEFI laptop + QEMU virtio storage/keyboard drivers
  boot.initrd.availableKernelModules = [
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
    bindings = [
      { keys = [ "BRIGHTNESSUP" ]; event = "press"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} brightness-up"; }
      { keys = [ "BRIGHTNESSUP" ]; event = "hold"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} brightness-up"; }
      { keys = [ "BRIGHTNESSDOWN" ]; event = "press"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} brightness-down"; }
      { keys = [ "BRIGHTNESSDOWN" ]; event = "hold"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} brightness-down"; }
      { keys = [ "VOLUMEUP" ]; event = "press"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} volume-up"; }
      { keys = [ "VOLUMEUP" ]; event = "hold"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} volume-up"; }
      { keys = [ "VOLUMEDOWN" ]; event = "press"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} volume-down"; }
      { keys = [ "VOLUMEDOWN" ]; event = "hold"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} volume-down"; }
      { keys = [ "MUTE" ]; event = "press"; cmd = "${wrosPkgs.paths.helperPaths.mediaKey} mute"; }
    ];
  };
}
