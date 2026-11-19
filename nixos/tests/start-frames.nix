{ pkgs, wrosPackages }:
# #165: the start screens hand the display to each other in place. Drawn with QEMU's screendump
# (which shows WrOS's native drawing now that each frame is marked dirty):
#   - a screen's drawing is visible and changes with the progress;
#   - the initrd's progress screen touches nothing until the unlock screen hands over;
#   - a screen replaced by the next one ends by itself and leaves the newer frame up;
#   - VT switches still work, and the last screen to end gives back a working text console;
#   - told the login screen is starting, a screen that nothing replaces gives the console back.
pkgs.testers.runNixOSTest {
  name = "wros-start-frames";

  nodes.machine = { ... }: {
    environment.systemPackages = [ wrosPackages.wros-boot pkgs.python3 ];
    systemd.services."getty@tty1".enable = false;
    systemd.services."autovt@tty1".enable = false;
    virtualisation.memorySize = 1024;
  };

  testScript = ''
    import hashlib, os, time

    def frame(name):
        path = os.path.join(machine.out_dir, f"{name}.ppm")
        machine.send_monitor_command(f"screendump {path}")
        data = open(path, "rb").read()
        # P6 <w> <h> 255, then RGB bytes.
        parts = data.split(maxsplit=4)
        w, h, pixels = int(parts[1]), int(parts[2]), parts[4]
        lit = sum(1 for i in range(0, len(pixels), 3 * 7) if max(pixels[i:i + 3]) > 24)
        share = lit / (w * h / 7)
        digest = hashlib.sha256(pixels).hexdigest()[:12]
        print(f"FRAME {name}: {w}x{h} lit={share:.4f} sha={digest}")
        return share, digest

    def splash(unit, figure, start=None, stage="system"):
        # `start`: the initrd screen's last figure, which a system-stage screen starts its bar
        # from; it gives each screen here its own picture.
        if start is not None:
            machine.succeed(f"mkdir -p /run/wros-boot && echo {start} > /run/wros-boot/progress")
        machine.succeed(f"echo {figure} > /tmp/{unit}")
        machine.succeed(
            f"systemd-run --unit={unit} wros-boot splash --stage {stage} --tty /dev/tty1"
            f" --progress-file /tmp/{unit}"
        )

    def settled(name, timeout=10):
        # The screen stops changing (a new screen's first frame is up).
        last = frame(f"{name}-0")
        for i in range(1, timeout * 2):
            time.sleep(0.5)
            now = frame(f"{name}-{i}")
            # The text console's cursor blinks: two console frames count as the same picture.
            if now[1] == last[1] or (now[0] > 0.02 and abs(now[0] - last[0]) < 0.002):
                return now
            last = now
        return last

    def gone(unit, timeout=10):
        machine.wait_until_fails(f"systemctl is-active --quiet {unit}", timeout=timeout)

    machine.wait_for_unit("multi-user.target")
    machine.succeed("chvt 1")
    machine.succeed("mkdir -p /run/wros-boot-splash")
    console = frame("00-console")

    with subtest("drawing is visible and follows the progress"):
        splash("a", "0.2", start="0.1")
        a1 = settled("01-a")
        machine.succeed("echo 0.7 > /tmp/a")
        machine.sleep(2)
        a2 = frame("02-a-moved")
        assert a1[0] > 0.003 and a1[1] != console[1], "WrOS's drawing is not on screen"
        assert a2[1] != a1[1], "the screen did not follow the progress"
        machine.succeed("systemctl stop a")
        assert settled("03-a-stopped")[0] > 0.02, "stopping the screen did not give the console back"

    with subtest("the initrd screen waits for the unlock screen's hand-over"):
        splash("w", "0.5", stage="initrd")
        machine.sleep(3)
        assert frame("04-waiting")[0] > 0.02, "it drew before the unlock"
        machine.succeed("systemctl is-active --quiet w")
        machine.succeed("touch /run/wros-boot/unlocked")
        w = settled("05-w")
        assert w[0] > 0.003 and w[0] < 0.02, f"no progress screen after the hand-over: {w}"
        machine.succeed("systemctl stop w; rm /run/wros-boot/unlocked")

    with subtest("the login screen's hand-over: the old screen ends and leaves the new frame"):
        splash("c", "0.2", start="0.1")
        c = settled("06-c")
        machine.succeed("systemctl kill --kill-whom=main --signal=SIGHUP c")
        machine.wait_until_succeeds("test -e /run/wros-boot-splash/handed-over", timeout=5)
        splash("d", "0.2", start="0.6")
        gone("c")
        d = settled("07-d")
        assert d[1] != c[1] and d[0] > 0.003, f"d's frame is not on screen: {c} {d}"
        machine.succeed("systemctl is-active --quiet d")

    with subtest("a screen replaced without a hand-over (as at the switch to the installed system) ends by itself"):
        splash("e", "0.2", start="0.9")
        e = settled("08-e")
        assert e[1] != d[1], "e's frame did not replace d's"
        gone("d")
        assert settled("09-d-gone")[1] == e[1], "the replaced screen's end changed the screen"

    with subtest("VT switches, and the last screen gives back a text console"):
        machine.succeed("chvt 2")
        tty2 = settled("10-tty2")
        machine.succeed("chvt 1")
        back = settled("11-tty1")
        assert back[1] == e[1], f"e did not come back after the VT switch: {back} {e}"
        machine.succeed("systemctl stop e")
        text = settled("12-e-stopped")
        assert text[0] > 0.02, f"no text console after the last screen ended: {text}"
        machine.succeed("echo console-check > /dev/tty1")

    with subtest("told the login screen is starting, with nothing to replace it, it gives the console back"):
        machine.succeed("rm -f /run/wros-boot-splash/handed-over")
        splash("f", "0.2", start="0.3")
        settled("13-f")
        machine.succeed("systemctl kill --kill-whom=main --signal=SIGHUP f")
        machine.wait_until_succeeds("test -e /run/wros-boot-splash/handed-over", timeout=5)
        gone("f", timeout=10)
        assert settled("14-f-gone")[0] > 0.02, "no console after the hand-over timed out"

    with subtest("what gives the display to the console under a start screen (greetd did both)"):
        splash("g", "0.2", start="0.4")
        g = settled("15-g")
        machine.succeed(
            "python3 -c 'import fcntl, os; "
            "fcntl.ioctl(os.open(\"/dev/tty1\", os.O_RDWR | os.O_NOCTTY), 0x4B3A, 0)'"
        )
        text_mode = [frame(f"16-text-mode-{i}") for i in range(3)]
        machine.succeed("printf '\\033[H\\033[2J' > /dev/tty1")
        cleared = [frame(f"17-cleared-{i}") for i in range(3)]
        machine.succeed("systemctl stop g")
        note = machine.succeed("dmesg | grep 'wros-boot: progress screen' | tail -1")
        print(f"RESULT text mode: {[f[0] > 0.02 for f in text_mode]}; "
              f"clear: {[f[0] > 0.02 for f in cleared]}; {note.strip()}")
        assert text_mode[-1][1] == g[1] and cleared[-1][1] == g[1], "the start screen did not come back"

    print(machine.succeed("journalctl -b --no-pager -o short-monotonic -u a -u c -u d -u e -u f | tail -30"))
  '';
}
