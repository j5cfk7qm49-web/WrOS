{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-live-session";

  nodes.live = { config, pkgs, ... }: {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 2560;  # was 4096: a 4 GiB guest starves the 16 GB qualification host
      cores = 2;
      emptyDiskImages = [ 8192 ];
    };
    # Test only: the live account's own session setting (docs/GRAPHICAL_SESSION.md) set to
    # console, so the desk is text on tty1 that the checks below can read. The live medium
    # itself keeps the graphical default.
    systemd.tmpfiles.rules = [
      "d /home/wros-live/.config 0700 wros-live wros-live -"
      "d /home/wros-live/.config/wros 0700 wros-live wros-live -"
      "f+ /home/wros-live/.config/wros/session.json 0600 wros-live wros-live - {\"version\":1,\"session\":\"console\"}"
    ];
  };

  testScript = ''
    ${encryptedDisk.pythonPreamble}
    import json

    live.start()
    live.wait_for_unit("multi-user.target")
    live.wait_for_unit("wros-live-session.service")
    live.wait_for_unit("greetd.service")

    # 1. Verify the live writing desk (grimoire) is running automatically as wros-live on tty1 (T7)
    live.succeed("grep -q 'wros.live=1' /proc/cmdline")
    marker = json.loads(live.succeed("cat /run/wros-live/session.json"))
    assert marker["user"] == "wros-live" and marker["uid"] == 1000 and marker["automatic"] is True
    live.wait_until_succeeds("pgrep -u wros-live grimoire")
    # 2. The live dashboard offers "Install WrOS to disk" selected. Enter opens the installer
    #    confirmation, whose default is to stay in the live session (rc13: src/shell_ui_tests.rs,
    #    os/tests/rc_vm_phases.py). Enter on that default changes nothing (T7).
    import time
    live.wait_until_tty_matches("1", "Install WrOS to disk")
    choose(live, r"^Install WrOS to disk")
    live.send_key("ret")
    live.wait_until_tty_matches("1", "Cancel and remain in the live writing session")
    assert any(re.search(r"Cancel and remain", row) for row in highlighted(live)), screen(live)
    live.send_key("ret")
    time.sleep(2)
    live.fail("pgrep -u _wros-installer wros-installer")
    live.succeed("[ $(fgconsole) = 1 ]")
    live.succeed("pgrep -u wros-live grimoire")

    # 3. Choose to start the installer: it runs as _wros-installer on tty7
    live.send_key("ret")
    live.wait_until_tty_matches("1", "Start encrypted WrOS installer")
    choose(live, r"Start encrypted WrOS installer")
    live.send_key("ret")
    live.wait_until_succeeds("pgrep -u _wros-installer wros-installer")
    live.wait_until_succeeds("[ $(fgconsole) = 7 ]")
    live.wait_until_tty_matches("7", "WrOS · Install")
    live.wait_until_tty_matches("7", "Keyboard layout")

    # 3. Step through installer questions to the final disk-erase confirmation
    # Step 1: Keyboard layout (default qwerty)
    live.send_key("ret")
    live.wait_until_tty_matches("7", "Appliance hostname")
    # Step 2: Hostname (default wros)
    live.send_key("ret")
    live.wait_until_tty_matches("7", "Writer display name")
    # Step 3: Full name (default Writer)
    live.send_key("ret")
    live.wait_until_tty_matches("7", "Writer account username")
    # Step 4: Username (default writer)
    live.send_key("ret")
    live.wait_until_tty_matches("7", "Writer account password")
    # Step 5: Writer password
    live.send_chars("writer-live-password\n")
    live.wait_until_tty_matches("7", "Disk encryption passphrase")
    # Step 6: LUKS passphrase
    live.send_chars("luks-live-passphrase\n")
    live.wait_until_tty_matches("7", "Erase /dev/vdb and install encrypted WrOS")

    # 4. Verify confirmation defaults to "No" (staying in the live session) and pressing Enter
    #    cancels without writing to /dev/vdb and returns cleanly to grimoire on tty1 (T7).
    confirm_screen = live.get_tty_text("7")
    assert "▸ No" in confirm_screen or "No" in confirm_screen, confirm_screen
    live.send_key("ret")

    live.wait_until_succeeds("[ $(fgconsole) = 1 ]")
    live.wait_until_fails("pgrep -f wros-installer")
    live.succeed("pgrep -u wros-live grimoire")
    live.fail("blkid -L WROS-LUKS")
  '';
}
