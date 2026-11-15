{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-boot-unlock-login";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode {
      # Test only: from the moment the start screen starts, record systemd's boot progress the
      # way the start screen reads it (systemctl), and the way it used to (busctl), so the test
      # can show the bar has something to move with.
      systemd.services.wros-test-progress = {
        unitConfig.DefaultDependencies = false;
        wantedBy = [ "sysinit.target" ];
        after = [ "systemd-vconsole-setup.service" ];
        serviceConfig.Type = "simple";
        script = ''
          for i in $(seq 1 1200); do
            p=$(${pkgs.systemd}/bin/systemctl show --property=Progress --value 2>&1 | tr -d '\n')
            b=$(${pkgs.coreutils}/bin/timeout 0.3 ${pkgs.systemd}/bin/busctl get-property org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager Progress 2>&1 | tr -d '\n')
            echo "$(cut -d' ' -f1 /proc/uptime)|$p|$b" >> /run/wros-test-progress.log
            [ "$p" = 1 ] && break
            sleep 0.05
          done
        '';
      };
    };
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    # 1. Build the encrypted disk with wros-install-core using Colemak layout
    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="colemak",
        luks_passphrase="colemak-passphrase",
        username="writer",
        user_password="writer-secret-password",
        hostname="wros-book",
    )
    prepare_installed_disk(installed, disk_img)

    # 2. Boot the encrypted disk and unlock at the graphical prompt (wros-boot unlock)
    #    typing the passphrase in the configured Colemak layout (T1, B1, B2, B5).
    installed.start(allow_reboot=True)
    wait_for_disk_prompt(installed)
    installed.send_chars(colemak_keys("colemak-passphrase"))
    installed.send_key("ret")
    # The start screen shows from the unlock until the login screen: pictures for the record.
    import time
    for n in range(1, 5):
        time.sleep(1)
        installed.screenshot(f"start-screen-{n}")

    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
    installed.wait_for_unit("wros-boot-ok.service")

    # 2b. The start screen's bar has a moving figure to show: from the first moment after the
    #     unlock, systemctl reports systemd's progress and it rises through several values
    #     before the boot finishes. (busctl, which the bar used to ask, needs D-Bus.)
    installed.wait_until_fails("systemctl is-active --quiet wros-test-progress", timeout=120)
    samples = [line.split("|") for line in installed.succeed("cat /run/wros-test-progress.log").splitlines()]
    def as_float(text):
        try:
            return float(text)
        except ValueError:
            return None
    readings = [as_float(s[1]) for s in samples]
    busctl_ok = [s[0] for s in samples if as_float(s[2].removeprefix("d ")) is not None]
    print(f"progress samples: {len(samples)} from {samples[0][0]}s to {samples[-1][0]}s; "
          f"first: {samples[:3]}; busctl first answered at "
          f"{busctl_ok[0] if busctl_ok else 'never'}s ({len(samples) - len(busctl_ok)} failures)")
    print("progress readings: " + " ".join(f"{s[0]}={s[1]}" for s in samples[::max(1, len(samples) // 40)]))
    assert readings[0] is not None, f"systemctl gave no progress at the start: {samples[0]}"
    assert all(r is not None for r in readings), [s for s, r in zip(samples, readings) if r is None]
    before_done = sorted({r for r in readings if r < 1.0})
    assert len(before_done) >= 3, f"progress barely moved: {before_done}"
    assert readings[-1] == 1.0, samples[-1]

    # 2c. WrOS, not NixOS, wherever the system names itself.
    installed.succeed("grep -qx 'PRETTY_NAME=\"WrOS ${version}\"' /etc/os-release")
    installed.succeed("grep -q 'menuentry \"WrOS' /boot/grub/grub.cfg")
    installed.fail("grep -q 'menuentry \"NixOS' /boot/grub/grub.cfg")
    installed.succeed("grep -q theme.txt /boot/grub/grub.cfg")

    # 3. Verify encrypted root mount, non-empty grubenv with cleared wros_boot_pending (B3),
    #    and initrd emergency shell disabled (B5).
    installed.succeed("mount | grep -q '/dev/mapper/wros_crypt on / type ext4'")
    installed.succeed("test -s /boot/grub/grubenv")
    installed.fail("grub-editenv /boot/grub/grubenv list | grep -q '^wros_boot_pending=1$'")

    # 4. Log in through wros-greeter on tty1 as writer and verify the session is not root (T1).
    log_in(installed, "writer", "writer-secret-password", colemak=True)
    session_users = installed.succeed("ps -o user= -C grimoire").strip().splitlines()
    assert session_users and all(u == "writer" for u in session_users), session_users
    assert installed.succeed("su - writer -c 'id -u'").strip() == "1000"
    installed.succeed("loginctl list-sessions --no-legend | grep -q 'writer'")

    # 5. Sync start-screen settings to /boot/wros-boot via wros-boot-settings
    theme_toml = 'name = "Candlelight"\naccent = "#d4a373"\ntext = "#f4efe6"\n'
    installed.succeed(
        f"/run/current-system/sw/libexec/wros-boot-settings screen ratatui '{theme_toml}'"
    )
    installed.wait_until_succeeds("test -f /boot/wros-boot/boot-theme.toml")
    installed.succeed("test -f /boot/wros-boot/boot-screen")

    # 6. Activate the text-start specialisation ("WrOS (text start)", wros.unlock=text)
    #    and reboot to verify fallback to systemd's plain-text passphrase prompt (B1, B4, T1).
    installed.succeed("/run/current-system/specialisation/text-start/bin/switch-to-configuration boot")
    # Mark this boot as one that never reached login, so GRUB shows its (WrOS-themed) menu for
    # 30 seconds on the way back up: a picture of it for the record, then Enter starts the default.
    installed.succeed("${pkgs.grub2}/bin/grub-editenv /boot/grub/grubenv set wros_boot_pending=1")
    installed.succeed("sync")
    restart(installed)
    time.sleep(8)
    installed.screenshot("grub-menu")
    installed.send_key("ret")

    installed.wait_for_console_text("Please enter passphrase for disk")
    installed.send_chars(colemak_keys("colemak-passphrase") + "\n")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
    installed.succeed("grep -q 'wros.unlock=text' /proc/cmdline")
  '';
}
