{ config, lib, pkgs, ... }:
{
  services.dbus.enable = true;

  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
    wireplumber.enable = true;
  };

  environment.systemPackages = [
    pkgs.pipewire
    pkgs.wireplumber
    pkgs.alsa-lib
    pkgs.playerctl
    (pkgs.espeak-ng.override { mbrolaSupport = false; })
  ];
}
