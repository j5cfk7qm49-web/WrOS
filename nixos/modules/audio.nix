{ config, lib, pkgs, ... }:
{
  services.dbus.enable = true;

  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
    wireplumber.enable = true;
  };
  # Start PipeWire (and with it WirePlumber) with the writer's session, not only when something
  # first connects: WirePlumber needs a second or two to set up the speakers, and a recorder that
  # connects first finds no output (#161). The sockets stay, so a restart is still on demand.
  systemd.user.services.pipewire.wantedBy = [ "default.target" ];

  environment.systemPackages = [
    pkgs.pipewire
    pkgs.wireplumber
    pkgs.alsa-lib
    pkgs.playerctl
    (pkgs.espeak-ng.override { mbrolaSupport = false; })
  ];
}
