{ pkgs, wrosPackages }:
# #165 experiment: what QEMU's screendump shows of WrOS's native drawing, and what stays on screen
# when a start screen ends (stopped, killed, or replaced by the next one while it still runs).
# It prints its findings and keeps every frame in $out; it asserts only that drawing is visible.
pkgs.testers.runNixOSTest {
  name = "wros-start-frames";

  nodes.machine = { ... }: {
    environment.systemPackages = [ wrosPackages.wros-boot ];
    systemd.services."getty@tty1".enable = false;
    systemd.services."autovt@tty1".enable = false;
    virtualisation.memorySize = 1024;
  };

  testScript = ''
    import hashlib, os

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

    def splash(unit, figure):
        machine.succeed(f"echo {figure} > /tmp/{unit}")
        machine.succeed(
            f"systemd-run --unit={unit} wros-boot splash --tty /dev/tty1 --progress-file /tmp/{unit}"
        )

    machine.wait_for_unit("multi-user.target")
    machine.succeed("chvt 1")
    print(machine.succeed("cat /proc/fb; ls -l /dev/dri; lsmod | grep -E 'bochs|virtio_gpu|simpledrm|drm' || true"))
    frame("00-console")

    splash("a", "0.2")
    machine.sleep(3)
    a1 = frame("01-splash-a")
    machine.succeed("echo 0.7 > /tmp/a")
    machine.sleep(2)
    a2 = frame("02-splash-a-moved")
    print(f"RESULT drawing visible: {a1[0] > 0.005}; frame changed with progress: {a1[1] != a2[1]}")

    # Killed: nothing puts the console back.
    machine.succeed("systemctl kill -s KILL a; sleep 1")
    frame("03-after-kill")
    machine.sleep(2)
    frame("04-after-kill-2s")

    # Stopped normally: the screen restores the console on its way out.
    splash("b", "0.4")
    machine.sleep(3)
    frame("05-splash-b")
    machine.succeed("systemctl stop b; sleep 1")
    frame("06-after-stop")

    # Overlap: a second screen starts while the first still runs, then the first is stopped.
    splash("c", "0.1")
    machine.sleep(3)
    c = frame("07-splash-c")
    splash("d", "0.9")
    machine.sleep(3)
    d = frame("08-c-and-d")
    print(machine.succeed("systemctl is-active c d || true"))
    machine.succeed("systemctl stop c; sleep 1")
    e = frame("09-c-stopped-d-runs")
    machine.sleep(2)
    frame("10-d-later")
    print(f"RESULT overlap: d's frame replaced c's: {d[1] != c[1]}; "
          f"after c stopped, screen lit={e[0]:.4f}, same as with both: {e[1] == d[1]}")
    machine.succeed("systemctl stop d || true")
    print(machine.succeed("journalctl -b --no-pager -o short-monotonic -u a -u b -u c -u d | tail -40"))

    assert a1[0] > 0.005, "WrOS's drawing is not visible in QEMU's screendump"
  '';
}
