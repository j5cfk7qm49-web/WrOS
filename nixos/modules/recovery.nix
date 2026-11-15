{ config, lib, pkgs, wrosPkgs, ... }:
{
  systemd.targets.wros-recovery = {
    description = "WrOS recovery";
    requires = [ "basic.target" "wros-recovery.service" ];
    after = [ "basic.target" "wros-recovery.service" ];
    conflicts = [ "rescue.service" "rescue.target" "greetd.service" "getty@tty1.service" ];
    unitConfig.AllowIsolate = true;
  };

  systemd.services.wros-recovery = {
    description = "WrOS recovery menu";
    wants = [ "wros-keyboard.service" ];
    after = [ "basic.target" "wros-keyboard.service" "systemd-vconsole-setup.service" ];
    conflicts = [ "getty@tty1.service" "greetd.service" "shutdown.target" ];
    before = [ "shutdown.target" ];
    unitConfig.DefaultDependencies = false;
    requires = [ "basic.target" ];
    serviceConfig = {
      Type = "idle";
      Environment = "WROS_SYSTEMD_RECOVERY=1";
      ExecStart = "${wrosPkgs.paths.helperPaths.recovery}";
      StandardInput = "tty-force";
      StandardOutput = "tty";
      StandardError = "tty";
      TTYPath = "/dev/tty1";
      TTYReset = true;
      TTYVHangup = true;
      TTYVTDisallocate = true;
      Restart = "always";
      RestartSec = 1;
      LimitCORE = 0;
    };
  };

  # Never expose an unauthenticated or root shell on emergency or rescue targets.
  systemd.services.emergency.enable = false;
  systemd.targets.emergency.enable = false;
  systemd.services.rescue.enable = false;
  systemd.targets.rescue.enable = false;
}
