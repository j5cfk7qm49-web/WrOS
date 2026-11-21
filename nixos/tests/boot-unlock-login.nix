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
    unlock_at_prompt(installed, colemak_keys("colemak-passphrase"))
    # No screenshots of the start screen here: QEMU shows WrOS's native drawing as black, and a
    # burst of screendump commands right after the typed password went with Enter being lost.

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

    # 2b'. The start screen covers the boot from the unlock on, and systemd's lines stay off it:
    #     - systemd shows only failures on the screen (quiet), in the initrd and after it, while the
    #       text start and recovery entries keep every line;
    #     - the initrd's screen ran once the disk was open, until the switch, and left its figure;
    #     - the installed system's screen no longer waits for tmpfiles (and so for every local
    #       filesystem and the journal flush) before it starts.
    installed.succeed("grep -qw quiet /proc/cmdline")
    show_status = installed.succeed("systemctl show --property=ShowStatus --value").strip()
    assert show_status != "yes", f"systemd still prints a line per unit: ShowStatus={show_status}"
    # The text start and recovery entries are the ones that pass wros.unlock=text.
    kernel_lines = installed.succeed("grep -E '^[[:space:]]*linux ' /boot/grub/grub.cfg").splitlines()
    text_lines = [line for line in kernel_lines if "wros.unlock=text" in line]
    assert len(text_lines) >= 2, kernel_lines
    assert all("systemd.show_status=true" in line for line in text_lines), text_lines
    initrd_splash = installed.succeed("journalctl -b -o short-monotonic -u wros-initrd-splash.service")
    print("initrd start screen: " + initrd_splash)
    assert "Started" in initrd_splash, initrd_splash
    left = float(installed.succeed("cat /run/wros-boot/progress").strip())
    assert 0.0 < left <= 0.3, f"the initrd's screen left {left}"
    after = installed.succeed("systemctl show --property=After --value wros-boot-splash.service")
    assert "systemd-tmpfiles-setup.service" not in after, after
    splash_started = int(installed.succeed(
        "systemctl show --property=ExecMainStartTimestampMonotonic --value wros-boot-splash.service").strip())
    tmpfiles_done = int(installed.succeed(
        "systemctl show --property=ActiveEnterTimestampMonotonic --value systemd-tmpfiles-setup.service").strip())
    print(f"start screen started at {splash_started / 1e6:.2f}s; tmpfiles done at {tmpfiles_done / 1e6:.2f}s")

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
    #    First rerun the console setup the way the boot can (NixOS's reload unit, or udev when a
    #    console appears): it loads the configured "us" keymap, and the writer's Colemak must
    #    still be the layout the login screen reads.
    installed.succeed("systemctl restart systemd-vconsole-setup.service")
    installed.wait_until_succeeds("systemctl is-active --quiet wros-keyboard.service")
    log_in(installed, "writer", "writer-secret-password", colemak=True)
    session_users = installed.succeed("ps -o user= -C grimoire").strip().splitlines()
    assert session_users and all(u == "writer" for u in session_users), session_users
    assert installed.succeed("su - writer -c 'id -u'").strip() == "1000"
    installed.succeed("loginctl list-sessions --no-legend | grep -q 'writer'")

    # 4a. rc15: the writer administers the laptop with sudo behind their own password (#142)
    #     and root stays locked; the writer's session joins Wi-Fi without a password prompt
    #     (#141). pkcheck asks polkit about the running grimoire, which sits in the writer's
    #     active session, exactly as nmcli's request from the app is judged.
    installed.fail("su - writer -c 'sudo -n true'")
    installed.succeed("su - writer -c 'echo writer-secret-password | sudo -S -p \"\" true'")
    installed.fail("su - writer -c 'echo wrong-password | sudo -S -p \"\" true'")
    installed.succeed("grep -q '^root:!' /etc/shadow")
    grimoire_pid = installed.succeed("pgrep -u writer -x grimoire | head -1").strip()
    for action in ("settings.modify.system", "network-control", "enable-disable-wifi", "wifi.scan"):
        installed.succeed(
            f"${pkgs.polkit.bin}/bin/pkcheck --action-id org.freedesktop.NetworkManager.{action}"
            f" --process {grimoire_pid}")
    # A process outside any session (timesyncd's own system user) does not get it.
    installed.fail(
        "${pkgs.polkit.bin}/bin/pkcheck --action-id org.freedesktop.NetworkManager.settings.modify.system"
        " --process $(pgrep -x systemd-timesyn | head -1)")
    # A test build carries no Google clients: the phone QR option stays hidden (#140; the
    # release build is checked for the file before it is signed).
    installed.fail("test -e /etc/wros/google-client.json")

    # 4b. Backups work on NixOS: an encrypted backup of a book to a local folder restores whole
    #     into a new folder. Moving from rc13 to rc14 is backup, install, restore.
    as_writer = lambda cmd: installed.succeed(f"su - writer -c '{cmd}'")
    as_writer("grimoire wros tutorial-demo /home/writer/demo-book")
    as_writer("grimoire wros backup create /home/writer/backup-repo")
    as_writer("grimoire wros backup snapshot /home/writer/demo-book")
    listed = json.loads(as_writer("grimoire wros backup list"))
    assert listed["configured"] and listed["snapshots"], listed
    snapshot = listed["snapshots"][0]["id"]
    as_writer(f"grimoire wros backup restore {snapshot} /home/writer/restored-book")
    original = as_writer("cd /home/writer/demo-book && find . -type f -not -path \"./.git/*\" -print0 | sort -z | xargs -0 sha256sum")
    restored_root = as_writer("find /home/writer/restored-book -name project.toml -printf %h").strip()
    restored = as_writer(f"cd {restored_root} && find . -type f -not -path \"./.git/*\" -print0 | sort -z | xargs -0 sha256sum")
    manuscript = lambda listing: [l for l in listing.splitlines() if "manuscript/" in l]
    assert manuscript(original) and manuscript(original) == manuscript(restored), (original, restored)

    # 5. Sync start-screen settings to /boot/wros-boot via wros-boot-settings
    theme_toml = 'name = "Candlelight"\naccent = "#d4a373"\ntext = "#f4efe6"\n'
    installed.succeed(
        f"/run/current-system/sw/libexec/wros-boot-settings screen ratatui '{theme_toml}'"
    )
    installed.wait_until_succeeds("test -f /boot/wros-boot/boot-theme.toml")
    # The service writes the theme, then the screen choice: wait for both.
    installed.wait_until_succeeds("test -f /boot/wros-boot/boot-screen")

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
