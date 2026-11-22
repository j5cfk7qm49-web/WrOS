{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-recovery-menu";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode {};
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    # 1. Build and boot the encrypted disk
    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="qwerty",
        luks_passphrase="recovery-disk-passphrase",
        username="writer",
        user_password="writer-recovery-password",
    )
    prepare_installed_disk(installed, disk_img)

    installed.start(allow_reboot=True)
    wait_for_disk_prompt(installed)
    installed.send_chars("recovery-disk-passphrase")
    installed.send_key("ret")
    installed.wait_for_unit("multi-user.target")

    # 2. Verify emergency and rescue units are masked so no unauthenticated shell exists
    installed.succeed("test \"$(systemctl is-enabled emergency.service || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled emergency.target || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled rescue.service || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled rescue.target || true)\" = masked")

    # Configure graphical session preference for writer so the "Continue in the console session"
    # choice is also offered in the recovery menu.
    installed.succeed(
        "install -d -m 0700 -o writer -g users /home/writer/.config/wros && "
        "printf '{\"session\":\"graphical\"}\\n' > /home/writer/.config/wros/session.json && "
        "chown writer:users /home/writer/.config/wros/session.json"
    )

    # 3. Boot the recovery specialisation entry (systemd.unit=wros-recovery.target) (T6)
    installed.succeed("/run/current-system/specialisation/recovery/bin/switch-to-configuration boot")
    installed.succeed("sync")
    restart(installed)

    installed.wait_for_console_text("Please enter passphrase for disk")
    installed.send_chars("recovery-disk-passphrase\n")
    installed.wait_for_unit("wros-recovery.target")
    installed.wait_for_unit("wros-recovery.service")

    # 4. Wait for recovery password prompt on tty1, test wrong password refusal, then right password
    installed.wait_until_tty_matches("1", "WrOS recovery")
    installed.wait_until_tty_matches("1", "Password for writer:")
    installed.send_chars("wrong-recovery-password\n")
    installed.wait_until_tty_matches("1", "That password did not match")

    installed.send_chars("writer-recovery-password\n")
    installed.wait_until_tty_matches("1", "1  Repair system packages")
    installed.wait_until_tty_matches("1", "2  Restore a project from a backup")
    installed.wait_until_tty_matches("1", "3  Show what happened")
    installed.wait_until_tty_matches("1", "4  Continue to the normal login")
    installed.wait_until_tty_matches("1", "5  Continue in the console session")
    installed.wait_until_tty_matches("1", "Choose:")

    # 5. Exercise each offline menu choice (repair, restore check, what happened, continue in console)
    # Choice 1: Repair system packages
    installed.send_chars("1\n")
    installed.wait_until_tty_matches("1", "verified and repaired", timeout=900)

    # Choice 2: Restore a project from a backup (offline with no snapshots yet)
    installed.send_chars("2\n")
    installed.wait_until_tty_matches("1", "No backups of a project were found")

    # Choice 3: Show what happened
    installed.send_chars("3\n")
    installed.wait_until_tty_matches("1", "This computer runs WrOS")

    # Verify no root or interactive shell is offered or running on tty1
    tty1_text = installed.get_tty_text("1")
    assert "shell" not in tty1_text.lower()
    installed.fail("pgrep -f 'agetty.*tty1'")

    # Choice 5: Continue in the console session -> writes /run/wros-session/console and isolates default.target
    installed.send_chars("5\n")
    installed.wait_for_file("/run/wros-session/console")
    installed.wait_for_unit("greetd.service")
  '';
}
