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
      qemu.options = [ "-device usb-mouse,bus=usb-bus.0" ];
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

    # QEMU's default VGA (bochs) has no cursor plane. Verify that the fallback arrow
    # actually moves within a cell, using the USB mouse for relative input.
    mice = live.send_monitor_command("info mice")
    mouse = re.search(r"Mouse #(\d+): QEMU (?:HID|USB) Mouse", mice)
    assert mouse is not None, mice
    live.send_monitor_command(f"mouse_set {mouse[1]}")
    dump = Path(live.out_dir) / "cursor.ppm"

    def arrow_x():
        live.send_monitor_command(f"screendump {dump}")
        magic, size, maximum, rgb = dump.read_bytes().split(b"\n", 3)
        assert (magic, maximum) == (b"P6", b"255")
        width, height = map(int, size.split())
        cx, y = (width // 8) * 4, (height // 16) * 8
        rows = ("X", "XX", "XOX", "XOOX", "XOOOX", "XOOOOX", "XOOOOOX")
        colors = {"X": b"\x00" * 3, "O": b"\xff" * 3}

        def pixel_at(x, y):
            offset = 3 * (y * width + x)
            return rgb[offset:offset + 3]

        for x in range(cx - 8, cx + 40):
            if all(
                pixel_at(x + dx, y + dy) == colors[pixel]
                for dy, row in enumerate(rows)
                for dx, pixel in enumerate(row)
            ):
                return x
        return None

    assert arrow_x() is None, "the arrow should be hidden before mouse input"
    previous = None
    observed = []
    moved_within_cell = False
    for _ in range(16):
        live.send_monitor_command("mouse_move 1 0")
        time.sleep(0.25)
        current = arrow_x()
        observed.append(current)
        if previous is not None and current is not None and current > previous and current // 8 == previous // 8:
            moved_within_cell = True
            break
        previous = current
    dump.unlink()
    journal = live.succeed(f"journalctl -b _PID={first} --no-pager -o cat")
    assert journal.count("WrOS pointer: frame sprite") == 1, journal
    assert moved_within_cell, f"the fallback arrow did not move within a cell: {observed}"
  '';
}
