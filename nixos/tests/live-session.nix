{ pkgs, lib, version, wrosPackages, liveModule }:
pkgs.testers.runNixOSTest {
  name = "wros-live-session";

  nodes.live = { config, pkgs, ... }: {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 4096;
      cores = 2;
      emptyDiskImages = [ 8192 ];
    };
  };

  testScript = ''
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
    live.wait_until_tty_matches("1", "WrOS|Live session|Resume writing")

    # 2. Launch "Install WrOS" from the live writing desk and verify wros-installer starts on tty7
    live.send_key("esc")
    live.send_key("ctrl-k")
    live.send_chars("Install WrOS\n")

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
