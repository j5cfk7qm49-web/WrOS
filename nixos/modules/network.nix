{ config, lib, pkgs, wrosRoot, ... }:
{
  networking.useDHCP = false;

  # Appliance uses Wi-Fi and Ethernet only; disable ModemManager/libqmi cellular modem support (S1)
  networking.modemmanager.enable = false;

  networking.networkmanager = {
    enable = true;
    wifi.powersave = false;
    plugins = lib.mkForce [];
  };

  # os/config/60-wros-no-connectivity-check.conf: never make unsolicited connectivity probes
  environment.etc."NetworkManager/conf.d/60-wros-no-connectivity-check.conf".text =
    builtins.readFile (wrosRoot + "/os/config/60-wros-no-connectivity-check.conf");

  services.timesyncd = {
    enable = lib.mkDefault true;
    servers = [
      "0.nixos.pool.ntp.org"
      "1.nixos.pool.ntp.org"
      "2.nixos.pool.ntp.org"
      "3.nixos.pool.ntp.org"
    ];
  };

  environment.systemPackages = [
    pkgs.networkmanager
    pkgs.wpa_supplicant
    pkgs.iw
    pkgs.util-linux
  ];
}
