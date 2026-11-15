{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-boot-unlock-login";

  nodes = {
    installer = { nodes, ... }: (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) {};
    installed = encryptedDisk.mkInstalledNode {};
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
    installed.wait_until_tty_matches("1", "Type your disk password")
    installed.send_chars(colemak_keys("colemak-passphrase"))
    installed.send_key("ret")

    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
    installed.wait_for_unit("wros-boot-ok.service")

    # 3. Verify encrypted root mount, non-empty grubenv with cleared wros_boot_pending (B3),
    #    and initrd emergency shell disabled (B5).
    installed.succeed("mount | grep -q '/dev/mapper/wros_crypt on / type ext4'")
    installed.succeed("test -s /boot/grub/grubenv")
    installed.fail("grub-editenv /boot/grub/grubenv list | grep -q '^wros_boot_pending=1$'")

    # 4. Log in through wros-greeter on tty1 as writer and verify the session is not root (T1).
    installed.wait_until_tty_matches("1", "writer|WrOS")
    installed.send_key("down")
    installed.send_chars(colemak_keys("writer-secret-password"))
    installed.send_key("ret")

    installed.wait_until_succeeds("pgrep -u writer grimoire")
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
    installed.succeed("sync")
    installed.reboot()

    installed.wait_for_console_text("Please enter passphrase for disk")
    installed.send_chars(colemak_keys("colemak-passphrase") + "\n")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
    installed.succeed("grep -q 'wros.unlock=text' /proc/cmdline")
  '';
}
