{ config, lib, pkgs, ... }:
# Spotify on WrOS (#122, #162): spotify_player plays and searches; signing in needs no browser.
#
# The phone's Spotify app hands its login over Spotify Connect: for at most three minutes after the
# writer chooses "Sign in to Spotify", librespot appears on the local network as a speaker named
# WrOS, and the reusable login it receives becomes spotify_player's. The phone has to reach the
# laptop for that, so the firewall accepts the ports in the `wros-signin` set (the Connect port and
# mDNS, added by os/scripts/wros-music-signin through polkit) from local addresses only. ipset
# drops each entry after 180 s by itself, so the window closes even if the app is killed; nothing
# listens on the network afterwards (spotify_player only connects out to Spotify).
{
  environment.systemPackages = [
    pkgs.spotify-player
    pkgs.librespot
  ];

  networking.firewall.extraPackages = [ pkgs.ipset ];
  networking.firewall.extraCommands = ''
    ipset -exist create wros-signin bitmap:port range 1-65535 timeout 0
    for proto in tcp udp; do
      iptables -A nixos-fw -p "$proto" -m set --match-set wros-signin dst \
        -s 10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,169.254.0.0/16 -j nixos-fw-accept
      ip6tables -A nixos-fw -p "$proto" -m set --match-set wros-signin dst \
        -s fe80::/10,fc00::/7 -j nixos-fw-accept
    done
  '';
}
