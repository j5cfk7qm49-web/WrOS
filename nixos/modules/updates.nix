{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
let
  cfg = config.wros.updates;
in
{
  options.wros.updates.trustDir = lib.mkOption {
    type = lib.types.path;
    default = wrosRoot + "/os/config/update-trust";
    description = ''
      Directory containing the OpenPGP keyring (wros-archive-keyring.gpg) and
      pinned primary-key fingerprint list (update-signers) trusted by the WrOS
      update engine. Defaults to os/config/update-trust in the repository;
      overridden only by nixos/tests/update-goback.nix to point at nixos/tests/keys.
    '';
  };

  config = {
    # Trust anchors come directly from the running system's store path and update
    # automatically on every generation switch (U6). mode = "0444" installs regular
    # root-owned files in /etc/wros/update-trust so require_trusted_file's lstat check passes.
    environment.etc."wros/update-trust/wros-archive-keyring.gpg" = {
      source = "${cfg.trustDir}/wros-archive-keyring.gpg";
      mode = "0444";
    };
    environment.etc."wros/update-trust/update-signers" = {
      source = "${cfg.trustDir}/update-signers";
      mode = "0444";
    };

    # systemd.offline-updates(7) on NixOS. Neither half works as shipped:
    # - NixOS installs only the upstream units it lists, and system-update.target,
    #   system-update-pre.target and system-update-cleanup.service are not among them;
    # - systemd's own system-update generator links default.target to
    #   <systemd>/lib/systemd/system/system-update.target, but NixOS keeps upstream units under
    #   <systemd>/example/, so that link dangles.
    # With /system-update present the default target then fails to load, systemd falls back to
    # rescue.target, which WrOS masks, and PID 1 freezes on every boot until the marker goes.
    # Install the units, and replace the generator (same name; /etc/systemd/system-generators
    # takes precedence over systemd's own directory) with one that links the installed unit.
    systemd.additionalUpstreamSystemUnits = [
      "system-update.target"
      "system-update-pre.target"
      "system-update-cleanup.service"
    ];
    systemd.generators.systemd-system-update-generator = pkgs.writeShellScript "wros-system-update-generator" ''
      # $2 is the early generator directory, as systemd-system-update-generator uses it.
      for marker in /system-update /etc/system-update; do
        if [ -e "$marker" ] || [ -L "$marker" ]; then
          exec ${pkgs.coreutils}/bin/ln -sf /etc/systemd/system/system-update.target "$2/default.target"
        fi
      done
    '';

    systemd.services.wros-offline-update = {
      description = "Apply verified WrOS offline generation updates";
      unitConfig = {
        DefaultDependencies = false;
        ConditionPathIsSymbolicLink = "/system-update";
      };
      requires = [ "sysinit.target" ];
      after = [ "sysinit.target" "system-update-pre.target" ];
      before = [ "system-update.target" "shutdown.target" ];
      wantedBy = [ "system-update.target" ];
      serviceConfig = {
        Type = "oneshot";
        Environment = "WROS_SYSTEMD_OFFLINE_UPDATE=1";
        ExecStart = "${wrosPkgs.paths.helperPaths.update} run-offline-boot";
        ProtectHome = true;
        LimitCORE = 0;
        TimeoutStartSec = "2100s";
        StandardOutput = "journal+console";
        StandardError = "journal+console";
      };
    };

    systemd.services.wros-update-reconcile = {
      description = "Reconcile WrOS update state and generation probation before login";
      after = [ "local-fs.target" ];
      before = [ "multi-user.target" "getty.target" "greetd.service" ];
      wantedBy = [ "multi-user.target" ];
      unitConfig.ConditionPathExists = "/var/lib/wros-update";
      serviceConfig = {
        Type = "oneshot";
        Environment = "WROS_SYSTEMD_RECONCILE=1";
        ExecStart = "${wrosPkgs.paths.helperPaths.update} reconcile";
        ProtectHome = true;
        LimitCORE = 0;
        TimeoutStartSec = "1900s";
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}
