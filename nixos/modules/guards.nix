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

  # A Debian PAM file in upstream Linux-PAM syntax.
  nixPam = file:
    builtins.replaceStrings
      [ "@include common-auth" "@include common-account" ]
      [ "auth include common-auth" "account include common-account" ]
      (builtins.readFile file);
in
{
  # sudo for the writer (wheel) behind their own password, as on Debian; never passwordless, and
  # root stays locked (#142). The design rule is "no passwordless general sudo", not "no sudo".
  security.sudo.enable = true;
  security.sudo.wheelNeedsPassword = true;
  security.sudo.execWheelOnly = true;

  # Setuid wrappers in /run/wrappers/bin: pkexec comes from security.polkit.enable and passwd from
  # NixOS's own shadow module (nixos/modules/programs/shadow.nix). Neither is redefined here.

  # PAM services: greetd, vlock, wros-recovery
  security.pam.services.greetd = {};

  # Exact PAM text expected by os/scripts/wros-lock (verified in validate_caller / worker)
  # Debian's Linux-PAM accepts "@include FILE"; upstream Linux-PAM on NixOS rejects it
  # ("illegal module type: @include"), which made every password fail. The same files are
  # used with the upstream spelling. nixos/pkgs/helpers.nix applies the same rewrite to the
  # copy of this text that wros-lock compares /etc/pam.d/vlock against.
  environment.etc."pam.d/vlock".text = lib.mkForce (nixPam "${wrosRoot}/os/config/vlock.pam");
  environment.etc."pam.d/wros-recovery".text = lib.mkForce (nixPam "${wrosRoot}/os/config/wros-recovery.pam");
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
