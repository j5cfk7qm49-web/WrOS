{ config, lib, pkgs, wrosPkgs, version, nixpkgsRev, wrosRoot, ... }:
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

  # Keep the Stream's seed available when an older update engine stages this system.
  system.extraDependencies = with config.wros.firmware; [ rtw88 i915-glk sof ];
  system.systemBuilderCommands = ''
    cp ${config.system.build.wrosFirmwareTable} "$out/wros-firmware.json"
  '';
}
