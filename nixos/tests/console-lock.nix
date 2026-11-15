{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-console-lock";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode {};
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    # 1. Build and boot the encrypted disk with qwerty layout
    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="qwerty",
        luks_passphrase="writer-lock-passphrase",
        username="writer",
        user_password="writer-lock-password",
    )
    prepare_installed_disk(installed, disk_img)

    installed.start()
    wait_for_disk_prompt(installed)
    installed.send_chars("writer-lock-passphrase")
    installed.send_key("ret")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")

    # 2. Verify suspend-lock drop-in path agreement (H7) and masked sleep/tty8 units
    installed.succeed("test -f /etc/systemd/system/systemd-suspend.service.d/overrides.conf")
    installed.succeed(
        "grep -q 'wros-lock assert-suspend' /etc/systemd/system/systemd-suspend.service.d/overrides.conf"
    )
    installed.succeed("test \"$(systemctl is-enabled hibernate.target || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled hybrid-sleep.target || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled suspend-then-hibernate.target || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled getty@tty8.service || true)\" = masked")
    installed.succeed("test \"$(systemctl is-enabled autovt@tty8.service || true)\" = masked")
    installed.fail("/run/current-system/sw/libexec/wros-lock assert-suspend")

    # 3. Log in as writer on tty1 via wros-greeter and open the writing editor
    # The console session draws the editor as text on tty1, which is what the screen
    # checks below read (the default session draws on DRM, which tty text cannot see).
    log_in(installed, "writer", "writer-lock-password", colemak=False, console=True)
    installed.wait_until_tty_matches("1", "WrOS")

    # 4. Settings -> System -> Battery and power -> Lock writing session, the way rc13's own VM
    #    chain does it (os/tests/rc_vm_phases.py grim_open_settings, grim_to_panel, choose).
    #    In rc13 Esc opens the menu bar, so it is not pressed first.
    import time

    time.sleep(2)
    to_editor(installed)
    installed.send_key("ctrl-k")
    time.sleep(1)
    installed.send_chars("wros settings")
    installed.wait_until_tty_matches("1", "WrOS settings")
    installed.send_key("ret")
    installed.wait_until_tty_matches("1", "Look and keyboard|Interface")
    choose(installed, r"^System")
    installed.send_key("ret")
    installed.wait_until_tty_matches("1", "Wi-Fi and networking")
    choose(installed, r"^Battery and power")
    installed.send_key("ret")
    time.sleep(1)
    choose(installed, r"^Lock writing session$")
    installed.send_key("ret")

    # 5. Verify wros-lock switches to tty8, runs vlock-main, blanks tty1, and blocks VT switching (T4)
    installed.wait_until_succeeds("[ $(fgconsole) = 8 ]")
    installed.wait_until_succeeds("pgrep -f vlock-main")
    installed.wait_until_tty_matches("8", "WrOS - writing session locked|Press Enter to unlock")
    installed.send_key("ctrl-alt-f1")
    installed.send_key("ctrl-alt-f2")
    installed.succeed("[ $(fgconsole) = 8 ]")

    # 6. Type characters while locked (must not reach the editor), then type a wrong password
    installed.send_key("ret")
    installed.wait_until_tty_matches("8", "Password:")
    installed.send_chars("LEAKED_WHILE_LOCKED_wrongpass\n")
    # The lock screen clears itself after a refusal (wros-lock LOCK_MESSAGE), so wait for a screen
    # with no "Password:" left on it: the old prompt text would match at once, and keys typed
    # during PAM's failure delay are lost (rc13: os/tests/rc_vm_phases.py lock-password-rejected).
    installed.wait_until_tty_matches("8", r"(?s)\A(?!.*Password:).*Press Enter to unlock")
    installed.succeed("[ $(fgconsole) = 8 ]")
    installed.succeed("pgrep -f vlock-main")

    # 7. Type the right password to unlock and return to tty1
    installed.send_key("ret")
    installed.wait_until_tty_matches("8", "Password:")
    installed.send_chars("writer-lock-password\n")
    installed.wait_until_succeeds("[ $(fgconsole) = 1 ]")
    installed.wait_until_fails("pgrep -f vlock-main")
    installed.succeed("pgrep -u writer grimoire")

    # 8. Confirm keys typed while locked never reached tty1 or any manuscript file
    tty1_text = installed.get_tty_text("1")
    assert "LEAKED_WHILE_LOCKED" not in tty1_text, tty1_text
    assert "writer-lock-password" not in tty1_text, tty1_text
    installed.fail("grep -rn 'LEAKED_WHILE_LOCKED' /home/writer")
  '';
}
