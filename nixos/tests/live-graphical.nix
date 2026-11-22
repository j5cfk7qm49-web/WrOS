{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
# The live medium's default session is the native graphical one (no session setting). #129: its
# input reader took longer than the 2-second startup allowance off the live image, the writing
# app exited, and the medium came up black. The app must still be running, in that session,
# well after it starts.
pkgs.testers.runNixOSTest {
  name = "wros-live-graphical";

  nodes.live = { config, pkgs, ... }: {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 2560;  # a 4 GiB guest starves the 16 GB qualification host
      cores = 2;
    };
  };

  testScript = ''
    ${encryptedDisk.pythonPreamble}
    live.start()
    live.wait_for_unit("multi-user.target")
    live.wait_for_unit("greetd.service")
    live.fail("test -e /home/wros-live/.config/wros/session.json")
    live.wait_until_succeeds("pgrep -u wros-live -x grimoire", timeout=120)
    first = live.succeed("pgrep -u wros-live -x grimoire -o").strip()
    environ = live.succeed(f"tr '\\0' '\\n' < /proc/{first}/environ")
    assert "WROS_SESSION=drm" in environ, environ
    live.wait_until_succeeds("test -f /run/user/1000/wros-live-install-prompt", timeout=120)
    # Well past any startup allowance: the same app, still running.
    time.sleep(30)
    still = live.succeed("pgrep -u wros-live -x grimoire -o").strip()
    assert still == first, f"the writing app restarted or exited ({first} then {still})"
    journal = live.succeed("journalctl -b --no-pager -o cat")
    assert "Input reader startup unacknowledged" not in journal, "the input reader timed out"
    # The greeter never took over tty1 from the live session.
    live.fail("pgrep -x wros-greeter")
    live.screenshot("live-graphical-install-prompt")
  '';
}
