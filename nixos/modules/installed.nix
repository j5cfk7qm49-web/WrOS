{ config, lib, pkgs, wrosPkgs, version, nixpkgsRev, wrosRoot, ... }:
let
  firmware = "${wrosPkgs.wros-helpers}/bin/wros-firmware";
in
{
  imports = [
    ./base.nix
    ./hardware.nix
    ./network.nix
    ./audio.nix
    ./session.nix
    ./guards.nix
    ./polkit.nix
    ./boot.nix
    ./recovery.nix
    ./updates.nix
  ];

  boot.kernelParams = [ "firmware_class.path=/wros-firmware" ];

  system.activationScripts.wrosFirmware = lib.stringAfter [ "udevd" ] ''
    if firmwareResult=$(${firmware} link "$systemConfig" 2>&1); then
      printf '%s\n' "$firmwareResult" || true
    else
      printf '%s\n' "Could not link the firmware set for $systemConfig: $firmwareResult" \
        | ${config.systemd.package}/bin/systemd-cat -t wros-firmware -p warning \
        || printf '<4>wros-firmware: %s\n' "$firmwareResult" > /dev/kmsg \
        || printf '%s\n' "$firmwareResult" >&2 || true
    fi
  '';

  assertions = [{
    assertion = config.boot.initrd.secrets == { };
    message = "WrOS uses GRUB's initrd secrets appender for firmware. Combine both archives before configuring boot.initrd.secrets.";
  }];

  # GRUB enables initrd secrets support and invokes this through the generation's path.
  system.build.initialRamdiskSecretAppender = lib.mkForce (pkgs.writeShellScriptBin "append-initrd-secrets" ''
    system=$(${pkgs.coreutils}/bin/readlink -f -- "''${0%/*}") || exit 1
    exec ${firmware} initrd "$system" "$1"
  '');

  # Keep the Stream's seed on the kernel firmware path if the per-machine set cannot be linked.
  hardware.firmware = with config.wros.firmware; [ rtw88 i915-glk sof ];

  # Keep the Stream's seed available when an older update engine stages this system.
  system.extraDependencies = with config.wros.firmware; [ rtw88 i915-glk sof ];
  system.systemBuilderCommands = ''
    cp ${config.system.build.wrosFirmwareTable} "$out/wros-firmware.json"
    ln -s ${config.system.build.initialRamdiskSecretAppender}/bin/append-initrd-secrets "$out/append-initrd-secrets"
  '';
}
