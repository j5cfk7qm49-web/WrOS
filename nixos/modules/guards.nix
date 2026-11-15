{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
let
  # Read "Key=Value" lines of one reviewed drop-in (os/config/60-wros-*.conf) as an attribute set,
  # so the file in the repository stays the single source for both bases.
  iniValues = file:
    lib.listToAttrs (map
      (line:
        let parts = lib.splitString "=" line;
        in lib.nameValuePair (lib.head parts) (lib.concatStringsSep "=" (lib.tail parts)))
      (lib.filter (line: lib.hasInfix "=" line && !(lib.hasPrefix "#" line))
        (lib.splitString "\n" (builtins.readFile file))));
in
{
  # No general sudo or passwordless privilege escalation
  security.sudo.enable = false;
  security.sudo.wheelNeedsPassword = true;

  # Setuid wrappers in /run/wrappers/bin: pkexec comes from security.polkit.enable and passwd from
  # NixOS's own shadow module (nixos/modules/programs/shadow.nix). Neither is redefined here.

  # PAM services: greetd, vlock, wros-recovery
  security.pam.services.greetd = {};

  # Exact PAM text expected by os/scripts/wros-lock (verified in validate_caller / worker)
  environment.etc."pam.d/vlock".source = lib.mkForce "${wrosRoot}/os/config/vlock.pam";
  environment.etc."pam.d/wros-recovery".source = lib.mkForce "${wrosRoot}/os/config/wros-recovery.pam";
  environment.etc."pam.d/common-auth".text = ''
    auth required ${pkgs.linux-pam}/lib/security/pam_unix.so nullok
  '';
  environment.etc."pam.d/common-account".text = ''
    account required ${pkgs.linux-pam}/lib/security/pam_unix.so
  '';

  # Power, lid, sleep and reboot guards (os/config/60-wros-*-guard.conf)
  services.logind.settings.Login = iniValues "${wrosRoot}/os/config/60-wros-power-guard.conf";
  systemd.sleep.settings.Sleep = iniValues "${wrosRoot}/os/config/60-wros-sleep-guard.conf";
  systemd.settings.Manager = iniValues "${wrosRoot}/os/config/60-wros-reboot-guard.conf";

  # Suspend requires the console lock proof; hibernate and hybrid-sleep are masked
  systemd.services.systemd-suspend.serviceConfig.ExecStartPre = [
    "${wrosPkgs.paths.helperPaths.lock} assert-suspend"
  ];
  systemd.targets.hibernate.enable = false;
  systemd.targets.hybrid-sleep.enable = false;
  systemd.targets.suspend-then-hibernate.enable = false;

  # Kernel sysctls (os/config/60-wros-sysrq.conf and 60-wros-quiet-console.conf)
  boot.kernel.sysctl = {
    "kernel.sysrq" = 0;
    "kernel.printk" = "3 4 1 3";
  };

  systemd.coredump.enable = false;
}
