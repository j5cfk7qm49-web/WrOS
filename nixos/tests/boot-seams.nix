{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
# #165: the start-up hands the screen from one picture to the next with no black screen between
# them: unlock → progress (initrd) → progress (installed system) → login. QEMU screendumps are
# taken as fast as they come (about every 100 ms) and any fully black frame after the unlock
# screen fails the test. (The VM has no i915 and a fast key derivation, so the laptop's gaps are
# longer; the boot menu → unlock gap is only reported here.) No screendump is taken while keys are
# typed: QEMU's monitor carries both, and a burst of screendumps once lost an Enter.
#
# A wrong password at the unlock screen still ends in systemd's text prompt, readable on screen,
# with the password typed there not echoed, and the right password typed there boots.
pkgs.testers.runNixOSTest {
  name = "wros-boot-seams";
  enableOCR = true;

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode { };
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}
    import hashlib, os, struct, threading, zlib

    BLACK = 0.0005  # share of sampled pixels lit: below this a frame is black
    SHOTS = os.path.join(installed.out_dir, "seams")
    os.makedirs(SHOTS, exist_ok=True)
    scratch = os.path.join(installed.out_dir, "frame.ppm")

    def png(path, w, h, rgb):
        rows = b"".join(b"\x00" + rgb[y * w * 3:(y + 1) * w * 3] for y in range(h))
        def chunk(kind, data):
            body = kind + data
            return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
        with open(path, "wb") as out:
            out.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                      + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b""))

    def film(label, done, limit=600):
        # Screendumps until `done` is set (or `limit` seconds). Keeps a PNG of each new picture.
        frames, last = [], None
        start = time.time()
        while not done.is_set() and time.time() - start < limit:
            installed.send_monitor_command(f"screendump {scratch}")
            moment = time.time()
            data = open(scratch, "rb").read()
            parts = data.split(maxsplit=4)
            w, h, rgb = int(parts[1]), int(parts[2]), parts[4]
            lit = sum(1 for i in range(0, len(rgb), 3 * 7) if max(rgb[i:i + 3]) > 24) / (w * h / 7)
            digest = hashlib.sha256(rgb).hexdigest()[:10]
            if digest != last:
                png(os.path.join(SHOTS, f"{label}-{len(frames):04d}-{digest}.png"), w, h, rgb)
                last = digest
            frames.append((moment, lit, digest))
        print(f"{label}: {len(frames)} frames in {time.time() - start:.1f}s")
        return frames

    def report(label, frames):
        # The film as a list of pictures and how long each was on screen.
        runs = []
        for moment, lit, digest in frames:
            if runs and runs[-1][2] == digest:
                runs[-1][1] = moment
            else:
                runs.append([moment, moment, digest, lit])
        t0 = frames[0][0]
        for first, last, digest, lit in runs:
            kind = "BLACK" if lit < BLACK else f"lit={lit:.4f}"
            print(f"  {label} {first - t0:7.2f}s-{last - t0:7.2f}s {digest} {kind}")

    def black_runs(frames, after=0.0):
        runs, current = [], None
        for moment, lit, _ in frames:
            if moment < after:
                continue
            if lit < BLACK:
                current = current or [moment, moment]
                current[1] = moment
            elif current:
                runs.append(current)
                current = None
        if current:
            runs.append(current)
        return runs

    def in_background(work):
        done = threading.Event()
        def run():
            try:
                work()
            finally:
                done.set()
        threading.Thread(target=run, daemon=True).start()
        return done

    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="qwerty",
        luks_passphrase="seams-passphrase",
        username="writer",
        user_password="writer-secret-password",
        hostname="wros-book",
    )
    prepare_installed_disk(installed, disk_img)

    with subtest("a wrong password ends in the text prompt, readable and not echoing"):
        installed.start(allow_reboot=True)
        prompt = in_background(lambda: wait_for_disk_prompt(installed, settle=3))
        report("menu-to-unlock", film("menu-to-unlock", prompt))
        installed.send_chars("not-the-passphrase")
        installed.send_key("ret")
        installed.wait_for_console_text("password handed to systemd-cryptsetup", timeout=60)
        installed.wait_for_console_text("Please enter passphrase for disk", timeout=120)
        time.sleep(3)
        text = installed.get_screen_text()
        print(f"screen at the text prompt: {text!r}")
        assert "passphrase" in text.lower(), "the text prompt is not on screen"
        installed.send_chars("seams-passphrase")
        time.sleep(2)
        text = installed.get_screen_text()
        assert "seams" not in text.lower(), f"the typed password is shown: {text!r}"
        installed.send_key("ret")
        wait_for_greeter(installed)

    with subtest("unlock → progress → login with no black screen"):
        restart(installed)
        prompt = in_background(lambda: wait_for_disk_prompt(installed, settle=3))
        before = film("boot", prompt)
        report("boot", before)
        unlock_at_prompt(installed, "seams-passphrase")
        greeter = in_background(lambda: wait_for_greeter(installed, settle=4))
        after = film("unlock-to-login", greeter)
        report("unlock-to-login", after)
        menu_gaps = black_runs(before)
        print(f"black before the unlock screen: {[(round(b - a, 2)) for a, b in menu_gaps]}")
        gaps = black_runs(after)
        assert not gaps, (
            "black screens between the unlock and the login screen: "
            + ", ".join(f"{a - after[0][0]:.2f}s-{b - after[0][0]:.2f}s" for a, b in gaps)
        )
        assert after[-1][1] >= BLACK, "the login screen is not on screen"
        print(installed.succeed(
            "journalctl -b --no-pager -o short-monotonic"
            " -u wros-initrd-unlock -u wros-initrd-splash -u wros-boot-splash -u greetd | tail -40"
        ))
  '';
}
