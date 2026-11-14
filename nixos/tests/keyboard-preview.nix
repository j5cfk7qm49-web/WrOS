{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-keyboard-preview";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode {};
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    # 1. Build encrypted disk starting with qwerty layout and boot it
    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="qwerty",
        luks_passphrase="colemak-passphrase",
        username="writer",
        user_password="writer-secret-password",
    )
    prepare_installed_disk(installed, disk_img)

    installed.start(allow_reboot=True)
    installed.wait_until_tty_matches("1", "Type your disk password")
    installed.send_chars("colemak-passphrase")
    installed.send_key("ret")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("wros-keyboard.service")

    # 2. Preview colemak without confirming: verify wros-keyboard-revert.timer is armed
    #    and reverts the console layout back to qwerty after 15 seconds (T3).
    preview_raw = installed.succeed("/run/current-system/sw/libexec/wros-keyboard preview colemak")
    preview = json.loads(preview_raw)
    assert "token" in preview
    installed.succeed("test -f /run/wros/keyboard-preview.json")
    installed.succeed("systemctl is-active wros-keyboard-revert.timer")
    installed.succeed("grep -q '\"layout\": \"qwerty\"' /etc/wros/keyboard.json")
    installed.succeed("dumpkeys --console=/dev/tty1 --keys-only --numeric | grep -E '^\\s*keycode\\s+18\\s*=\\s*\\+?0x0*66\\b'")

    # Wait for the 15-second wros-keyboard-revert.timer to fire wros-keyboard-revert.service
    installed.sleep(16)
    installed.wait_until_succeeds("test ! -e /run/wros/keyboard-preview.json")
    installed.succeed("dumpkeys --console=/dev/tty1 --keys-only --numeric | grep -E '^\\s*keycode\\s+18\\s*=\\s*\\+?0x0*65\\b'")
    installed.succeed("grep -q '\"layout\": \"qwerty\"' /etc/wros/keyboard.json")

    # 3. Preview colemak again and confirm it with its token before the timer expires
    preview2 = json.loads(
        installed.succeed("/run/current-system/sw/libexec/wros-keyboard preview colemak")
    )
    confirm = json.loads(
        installed.succeed(f"/run/current-system/sw/libexec/wros-keyboard confirm {preview2['token']}")
    )
    assert confirm["status"] == "committed" and confirm["layout"] == "colemak"
    installed.succeed("grep -q '\"layout\": \"colemak\"' /etc/wros/keyboard.json")
    installed.succeed("grep -q '\"layout\": \"colemak\"' /boot/wros-boot/keyboard.json")
    installed.succeed("sync")

    # 4. Reboot into the encrypted disk prompt and prove the confirmed Colemak layout
    #    is active at the pre-mount LUKS passphrase prompt (T3).
    installed.reboot()
    installed.wait_until_tty_matches("1", "Type your disk password")
    installed.send_chars(colemak_keys("colemak-passphrase"))
    installed.send_key("ret")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
  '';
}
