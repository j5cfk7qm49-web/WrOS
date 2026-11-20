{ config, lib, pkgs, wrosPkgs, version, nixpkgsRev, wrosRoot, ... }:
{
  imports = [
    ./base.nix
    ./hardware.nix
    ./network.nix
    ./audio.nix
    ./music.nix
    ./session.nix
    ./guards.nix
    ./polkit.nix
    ./boot.nix
    ./recovery.nix
    ./updates.nix
  ];
}
