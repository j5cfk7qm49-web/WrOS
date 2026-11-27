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

  hardware.firmware = with config.wros.firmware; [ rtw88 i915-glk ];
}
