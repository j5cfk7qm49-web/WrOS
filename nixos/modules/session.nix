{ config, lib, pkgs, wrosPkgs, ... }:
let
  defaultGreetdConfig = ''
    [terminal]
    vt = 1
    switch = true

    [general]
    source_profile = false

    [default_session]
    command = "${wrosPkgs.wros-greeter}/bin/wros-greeter"
    user = "_greetd"
  '';
in
{
  services.dbus.enable = true;
  services.dbus.implementation = "broker";

  console = {
    enable = true;
    earlySetup = true;
    keyMap = "us";
  };

  environment.etc."greetd/config.toml".text = defaultGreetdConfig;

  services.greetd = {
    enable = true;
    settings = {
      terminal = {
        vt = 1;
        switch = true;
      };
      general = {
        source_profile = false;
      };
      default_session = {
        command = "${wrosPkgs.wros-greeter}/bin/wros-greeter";
        user = "_greetd";
      };
    };
  };

  systemd.services.greetd = {
    after = [ "systemd-user-sessions.service" "wros-keyboard.service" ];
    wants = [ "wros-keyboard.service" ];
    conflicts = [ "getty@tty1.service" ];
    environment.WROS_GREETD_CONFIG = "/etc/greetd/config.toml";
    unitConfig.StartLimitIntervalSec = 0;
    serviceConfig = {
      EnvironmentFile = [ "-/run/wros-live/greetd.env" ];
      # The login screen replaces the progress screen in place (#165): the progress screen is told
      # (SIGHUP), gives the console back and stops reading it, then ends by itself once the login
      # screen's first frame is up. Stopped as before if it doesn't answer.
      ExecStartPre = [ "-${pkgs.writeShellScript "wros-splash-handover" ''
        systemctl=${pkgs.systemd}/bin/systemctl
        $systemctl -q is-active wros-boot-splash.service || exit 0
        $systemctl kill --kill-whom=main --signal=SIGHUP wros-boot-splash.service || exit 0
        for _ in $(${pkgs.coreutils}/bin/seq 30); do
          [ -e /run/wros-boot-splash/handed-over ] && exit 0
          $systemctl -q is-active wros-boot-splash.service || exit 0
          ${pkgs.coreutils}/bin/sleep 0.1
        done
        $systemctl stop wros-boot-splash.service
      ''}" ];
      ExecStart = lib.mkForce "${pkgs.greetd}/bin/greetd --config \${WROS_GREETD_CONFIG}";
      IgnoreSIGPIPE = false;
      SendSIGHUP = true;
      TimeoutStopSec = "30s";
      KeyringMode = "shared";
      Restart = lib.mkForce "always";
      RestartSec = 2;
      LimitCORE = 0;
    };
  };

  # tty1 is owned by greetd; tty2 is kept for authenticated maintenance; tty8 is reserved for vlock.
  systemd.services."getty@tty1".enable = false;
  systemd.services."autovt@tty1".enable = false;
  systemd.services."getty@tty8".enable = false;
  systemd.services."autovt@tty8".enable = false;

  # /etc/profile.d/wros.sh: start wros-session if an authenticated writer signs in on tty1 getty
  environment.extraInit = ''
    if [ "$(id -u)" -ne 0 ] && [ "$(tty 2>/dev/null)" = /dev/tty1 ] && id -nG | grep -qw wros; then
      exec ${wrosPkgs.paths.helperPaths.session}
    fi
  '';

  # Keyboard apply & 15-second preview safety rollback units (os/systemd/wros-keyboard*)
  systemd.services.wros-keyboard = {
    description = "Apply the committed WrOS console layout";
    after = [ "systemd-vconsole-setup.service" ];
    before = [ "getty@tty1.service" "greetd.service" ];
    wantedBy = [ "multi-user.target" ];
    # systemd-vconsole-setup runs again later in the boot (NixOS's reload unit, udev when a console
    # appears) and loads console.keyMap ("us") over the writer's layout, so a Colemak password
    # typed at the login screen arrived as QWERTY. Restarted with it, this applies the layout
    # after it every time.
    partOf = [ "systemd-vconsole-setup.service" ];
    unitConfig.ConditionPathExists = "${wrosPkgs.wros-keymaps}/share/wros/keymaps/qwerty.map";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${wrosPkgs.paths.helperPaths.keyboard} boot";
      RemainAfterExit = true;
      ProtectHome = true;
      ProtectSystem = "strict";
      ReadWritePaths = [ "/run/wros" "/etc/wros" "/boot" ];
      RuntimeDirectory = "wros";
      NoNewPrivileges = true;
    };
  };

  systemd.services.wros-keyboard-revert = {
    description = "Revert an unconfirmed WrOS keyboard preview";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${wrosPkgs.paths.helperPaths.keyboard} rollback";
      ProtectHome = true;
      ProtectSystem = "strict";
      ReadWritePaths = [ "/run/wros" "/etc/wros" ];
      NoNewPrivileges = true;
    };
  };

  systemd.timers.wros-keyboard-revert = {
    description = "Independent 15-second keyboard preview safety timer";
    timerConfig = {
      OnActiveSec = "15s";
      AccuracySec = "100ms";
      Unit = "wros-keyboard-revert.service";
    };
  };

  # Display warmth apply & 15-second preview safety rollback units (os/systemd/wros-display*)
  systemd.services.wros-display = {
    description = "Apply confirmed WrOS screen warmth";
    after = [ "systemd-vconsole-setup.service" "systemd-udev-settle.service" ];
    wantedBy = [ "multi-user.target" ];
    unitConfig.ConditionPathExists = "/etc/wros/display.json";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${wrosPkgs.paths.helperPaths.display} boot";
      RemainAfterExit = true;
      RuntimeDirectory = "wros-display";
      RuntimeDirectoryMode = "0700";
      RuntimeDirectoryPreserve = true;
      ProtectHome = true;
      ProtectSystem = "strict";
      ReadWritePaths = [ "/run/wros-display" "/etc/wros" ];
      NoNewPrivileges = true;
      TimeoutStartSec = "45s";
    };
  };

  systemd.services.wros-display-revert = {
    description = "Restore an unconfirmed WrOS display preview";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${wrosPkgs.paths.helperPaths.display} rollback";
      ProtectHome = true;
      ProtectSystem = "strict";
      ReadWritePaths = [ "/run/wros-display" "/etc/wros" ];
      NoNewPrivileges = true;
      TimeoutStartSec = "12s";
    };
  };

  systemd.timers.wros-display-revert = {
    description = "Independent 15-second WrOS display preview safety timer";
    timerConfig = {
      OnActiveSec = "15s";
      AccuracySec = "100ms";
      Unit = "wros-display-revert.service";
    };
  };
}
