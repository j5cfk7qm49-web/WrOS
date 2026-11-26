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
    import os

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
    installed.wait_until_tty_matches("1", "File  Edit  View")

    # 4. Settings -> System -> Battery and power -> Lock writing session, the way rc13's own VM
    #    chain does it (os/tests/rc_vm_phases.py grim_open_settings, grim_to_panel, choose).
    #    In rc13 Esc opens the menu bar, so it is not pressed first.

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

    # 5. wros-lock switches to tty8 and starts WrOS's lock screen in vlock-main's place (#158):
    #    it runs as the writer (root only opened tty8 and the display), SysRq is off, the
    #    screen is drawn (DRM, so read it with a screendump and OCR), and VT switches are refused.
    installed.wait_until_succeeds("[ $(fgconsole) = 8 ]")
    installed.wait_until_succeeds("pgrep -f '^[^ ]*wros-lock-screen --user'")
    installed.fail("pgrep -x vlock-main")
    pid = installed.succeed("pgrep -f '^[^ ]*wros-lock-screen --user'").strip()
    writer = installed.succeed("id -u writer").strip()
    ids = installed.succeed(f"grep -E '^(Uid|Gid):' /proc/{pid}/status")
    print(ids)
    for line in ids.splitlines():
        values = line.split()[1:]
        expected = writer if line.startswith("Uid") else installed.succeed("id -g writer").strip()
        assert values == [expected] * 4, f"the lock screen is not running as the writer: {line}"
    installed.succeed("[ $(cat /proc/sys/kernel/sysrq) = 0 ]")
    time.sleep(3)
    # The lock screen draws through DRM in Unifont, which OCR reads poorly: keep a picture for
    # the record and check the screen is drawn (neither black nor the text console's few lines).
    installed.screenshot("lock-screen")
    dump = os.path.join(installed.out_dir, "lock-screen.ppm")
    installed.send_monitor_command(f"screendump {dump}")
    parts = open(dump, "rb").read().split(maxsplit=4)
    w, h, rgb = int(parts[1]), int(parts[2]), parts[4]
    lit = sum(1 for i in range(0, len(rgb), 21) if max(rgb[i:i + 3]) > 24) / (w * h / 7)
    print(f"lock screen: {w}x{h}, {lit:.4f} of the screen lit")
    assert lit > 0.02, f"the lock screen is not drawn ({lit:.4f} lit)"
    installed.send_key("ctrl-alt-f1")
    installed.send_key("ctrl-alt-f2")
    time.sleep(1)
    installed.succeed("[ $(fgconsole) = 8 ]")

    # 6. Type characters while locked (must not reach the editor), then a wrong password: still
    #    locked, by the same lock screen.
    installed.send_chars("LEAKED_WHILE_LOCKED_wrongpass\n")
    time.sleep(5)
    installed.succeed("[ $(fgconsole) = 8 ]")
    assert installed.succeed("pgrep -f '^[^ ]*wros-lock-screen --user'").strip() == pid
    installed.screenshot("lock-screen-after-wrong-password")

    # 7. Type the right password to unlock and return to tty1
    installed.send_chars("writer-lock-password\n")
    installed.wait_until_succeeds("[ $(fgconsole) = 1 ]")
    installed.wait_until_fails("pgrep -f '^[^ ]*wros-lock-screen --user'")
    installed.succeed("pgrep -u writer grimoire")

    # 8. Confirm keys typed while locked never reached tty1 or any manuscript file
    tty1_text = installed.get_tty_text("1")
    assert "LEAKED_WHILE_LOCKED" not in tty1_text, tty1_text
    assert "writer-lock-password" not in tty1_text, tty1_text
    installed.fail("grep -rn 'LEAKED_WHILE_LOCKED' /home/writer")

    # 9. Killing the lock screen never unlocks: the lock fails and the writing session closes
    #    through its save/rescue path, so the writer has to log in again. After the unlock the
    #    app is back where it was: Settings > System > Battery and power.
    installed.wait_until_tty_matches("1", "Lock writing session")
    choose(installed, r"^Lock writing session$")
    installed.send_key("ret")
    installed.wait_until_succeeds("pgrep -f '^[^ ]*wros-lock-screen --user'")
    installed.succeed("pkill -KILL -f '^[^ ]*wros-lock-screen --user'")
    installed.wait_until_fails("pgrep -u writer -x grimoire", timeout=60)
    installed.wait_until_succeeds("pgrep -x wros-greeter", timeout=60)
    installed.fail("pgrep -f '^[^ ]*wros-lock-screen --user'")

    # 10. The writer's usual session draws through DRM (the steps above used the console session
    #     so the editor's text can be read). There the app gives the card up while locked, the
    #     lock screen holds DRM master throughout, and the app, unprivileged, must get the card
    #     back. Locked and unlocked blind: Ctrl-K, "lock", Enter in the command palette.
    def lit(name):
        dump = os.path.join(installed.out_dir, f"{name}.ppm")
        installed.send_monitor_command(f"screendump {dump}")
        parts = open(dump, "rb").read().split(maxsplit=4)
        w, h, rgb = int(parts[1]), int(parts[2]), parts[4]
        share = sum(1 for i in range(0, len(rgb), 21) if max(rgb[i:i + 3]) > 24) / (w * h / 7)
        print(f"{name}: {share:.4f} lit")
        return share, rgb
    installed.succeed("rm -f /home/writer/.config/wros/session.json")
    log_in(installed, "writer", "writer-lock-password", colemak=False)
    app = installed.succeed("pgrep -u writer -x grimoire -o").strip()
    assert "WROS_SESSION=drm" in installed.succeed(f"tr '\\0' '\\n' < /proc/{app}/environ")
    def sketch(name):
        # A coarse picture of the screen in the log (DRM drawing has no tty text to read).
        dump = os.path.join(installed.out_dir, f"{name}.ppm")
        installed.send_monitor_command(f"screendump {dump}")
        parts = open(dump, "rb").read().split(maxsplit=4)
        w, h, rgb = int(parts[1]), int(parts[2]), parts[4]
        rows = []
        for y in range(0, h, h // 40):
            row = ""
            for x in range(0, w, w // 120):
                i = 3 * (y * w + x)
                row += "#" if max(rgb[i:i + 3]) > 60 else ("+" if max(rgb[i:i + 3]) > 24 else " ")
            rows.append(row.rstrip())
        print(f"--- {name}")
        for row in rows:
            print(row)
    time.sleep(5)
    sketch("desk-at-login")
    # The session opens on the desk (rc23 has no dashboard), where Ctrl-K is the command
    # palette. Esc would open the menu bar instead, so none is pressed.
    installed.send_key("ctrl-k")
    time.sleep(1)
    installed.send_chars("lock")
    time.sleep(1)
    sketch("palette-lock")
    installed.send_key("ret")
    try:
        installed.wait_until_succeeds("[ $(fgconsole) = 8 ]", timeout=30)
    except Exception:
        sketch("not-locked")
        print(installed.succeed("journalctl -b --no-pager -t wros-lock -t pkexec | tail -20"))
        raise
    installed.wait_until_succeeds("pgrep -f '^[^ ]*wros-lock-screen --user'")
    time.sleep(3)
    lock_lit, lock_rgb = lit("drm-lock-screen")
    assert lock_lit > 0.02, f"the lock screen is not drawn ({lock_lit:.4f})"
    # #175 (rc30): a wrong password leaves the same lock screen drawn the whole time. On the HP
    # Stream on rc29 the screen went black for about 5 s before the lock screen came back.
    # Sampled after typing (a screendump while keys are being sent once lost an Enter).
    locker = installed.succeed("pgrep -f '^[^ ]*wros-lock-screen --user'").strip()
    installed.send_chars("wrong-lock-password\n")
    time.sleep(0.3)
    samples = []
    end = time.time() + 8
    while time.time() < end:
        share, _ = lit(f"drm-wrong-{len(samples):02d}")
        samples.append(share)
        time.sleep(0.2)
    print("after a wrong password, lit:", " ".join(f"{s:.3f}" for s in samples))
    installed.succeed("[ $(fgconsole) = 8 ]")
    assert installed.succeed("pgrep -f '^[^ ]*wros-lock-screen --user'").strip() == locker
    assert min(samples) > 0.02, f"the lock screen went dark after a wrong password: {samples}"
    installed.send_chars("writer-lock-password\n")
    installed.wait_until_succeeds("[ $(fgconsole) = 1 ]")
    installed.wait_until_fails("pgrep -f '^[^ ]*wros-lock-screen --user'")
    time.sleep(5)
    assert installed.succeed("pgrep -u writer -x grimoire -o").strip() == app, "the app restarted"
    mode = installed.succeed("${pkgs.kbd}/bin/kbdinfo -C /dev/tty1 getmode")
    assert "graphics" in mode, f"the app did not get the display back: {mode}"
    back_lit, back_rgb = lit("desk-after-unlock")
    assert back_lit > 0.005 and back_rgb != lock_rgb, f"the desk is not back ({back_lit:.4f})"
  '';
}
