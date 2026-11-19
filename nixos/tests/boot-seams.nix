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
    import base64, hashlib, os, struct, threading, zlib

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

    ACCENT = (0xd6, 0xad, 0x60)
    boxes = {}
    files = {}  # picture -> its PNG

    def accent_box(w, h, rgb):
        # Where the logo's accent colour is on screen (the unlock screen types nothing in it yet).
        xs, ys = [], []
        for y in range(0, h, 2):
            row = 3 * y * w
            for x in range(0, w, 2):
                i = row + 3 * x
                if all(abs(rgb[i + k] - ACCENT[k]) < 30 for k in range(3)):
                    xs.append(x)
                    ys.append(y)
        return (min(xs), min(ys), max(xs), max(ys)) if xs else None

    def film(label, done, limit=600, until=None):
        # Screendumps until `done` is set, `limit` seconds pass, or the picture `until` has been
        # on screen for 3 s. Keeps a PNG of each new picture, and a timeline (when each
        # screendump was taken, and its picture) to play the film back.
        frames, last, shown, since = [], None, None, 0.0
        timeline = open(os.path.join(SHOTS, f"{label}.timeline"), "w")
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
                shown = f"{label}-{len(frames):04d}-{digest}.png"
                files.setdefault(digest, shown)
                png(os.path.join(SHOTS, shown), w, h, rgb)
                if digest not in boxes:
                    boxes[digest] = accent_box(w, h, rgb)
                last, since = digest, moment
            frames.append((moment, lit, digest))
            timeline.write(f"{moment:.3f} {shown}\n")
            if until and digest == until and moment - since >= 3:
                break
        timeline.close()
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
        menu = film("menu-to-unlock", prompt)
        report("menu-to-unlock", menu)
        installed.send_chars("not-the-passphrase")
        installed.send_key("ret")
        installed.wait_for_console_text("password handed to systemd-cryptsetup", timeout=60)
        # The prompt itself ends without a newline on the serial line, so wait for the agent's
        # own line instead. The prompt can land in the middle of that line ("Starting password
        # query on /Please enter passphrase ...dev/tty1"), so only its start is matched.
        installed.wait_for_console_text("Starting password query on /", timeout=120)
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
        # This boot runs at the laptop's log level. The test framework adds loglevel=7, which
        # puts the kernel's messages on screen (and so hands the display to the console, whose
        # take-over is otherwise deferred until something prints); WrOS boots quiet with
        # loglevel=3. The unlock unit's notes on the serial line go too, so the film follows the
        # boot on screen instead: it ends once the unlock screen (the picture the first boot
        # ended on) has been up for 3 s.
        installed.succeed("sed -i 's/ loglevel=7 / /' /boot/grub/grub.cfg")
        unlock_screen = menu[-1][2]
        restart(installed)
        before = film("boot", threading.Event(), limit=180, until=unlock_screen)
        report("boot", before)
        assert before[-1][2] == unlock_screen, "the unlock screen never came"
        # Filmed from just after Enter: the unlock → progress hand-over and the switch to the
        # installed system come within seconds of it.
        installed.send_chars("seams-passphrase")
        installed.send_key("ret")
        time.sleep(0.5)
        def to_login():
            wait_for_greeter(installed, settle=4)
        greeter = in_background(to_login)
        after = film("unlock-to-login", greeter)
        report("unlock-to-login", after)
        print(installed.succeed(
            "journalctl -b --no-pager -o short-monotonic -g 'wros-boot|splash|greetd|Switching root'"
            " | tail -60; journalctl -k -b --no-pager -o short-monotonic | grep wros-boot || true"
        ))
        # The login screen draws through DRM (a text login screen would mean it never got the card).
        mode = installed.succeed("${pkgs.kbd}/bin/kbdinfo -C /dev/tty1 getmode")
        assert "graphics" in mode, f"the login screen is not graphical: {mode}"
        # greetd leaves the VT alone during the hand-over (its patch): the progress screen never
        # has to take the display back, and no frame after Enter is the console's text.
        notes = installed.succeed("journalctl -k -b --no-pager | grep 'progress screen (system) ended'")
        assert "taken back from the console 0 times" in notes, notes
        console = [moment - after[0][0] for moment, lit, _ in after if lit >= 0.045]
        assert not console, f"the console showed between the unlock and the login screen at {console}"
        # The boot menu's picture was drawn for this screen on the first boot
        # (wros-boot-logo.service); this boot's GRUB showed it with the logo where the unlock
        # screen then drew it.
        print(installed.succeed("cat /boot/wros-boot/boot-logo.size; journalctl -b -u wros-boot-logo --no-pager | tail -3"))
        shown = [boxes[digest] for _, _, digest in before if boxes.get(digest)]
        assert len(shown) >= 2, f"no logo seen before the unlock: {shown}"
        grub, unlock = shown[0], shown[-1]
        centre = lambda b: ((b[0] + b[2]) / 2, (b[1] + b[3]) / 2)
        print(f"logo in the boot menu {grub}, at the unlock screen {unlock}")
        (gx, gy), (ux, uy) = centre(grub), centre(unlock)
        assert abs(gx - ux) <= 4 and abs(gy - uy) <= 4, f"the logo moves: {grub} then {unlock}"
        cmdline = installed.succeed("cat /proc/cmdline")
        assert "loglevel=7" not in cmdline, f"the filmed boot was not at the laptop's log level: {cmdline}"
        menu_gaps = black_runs(before)
        print(f"black before the unlock screen: {[(round(b - a, 2)) for a, b in menu_gaps]}")
        # The console never took the display before the unlock screen: nothing printed to it, so
        # fbcon's take-over stayed deferred and the boot menu's picture stayed up.
        kernel = installed.succeed(
            "journalctl -k -b --no-pager -o short-monotonic"
            " | grep -E 'fbcon|Console: switching|bochs|simple-framebuffer|wros-initrd-unlock|wros-boot'"
        )
        print(kernel)
        asked = kernel.find("wros-initrd-unlock: asking for the disk password")
        assert asked >= 0, "the unlock unit's note is not in the kernel log"
        assert "fbcon: Taking over console" not in kernel[:asked], (
            "the console took the display before the unlock screen")
        # From the boot menu's picture on (the firmware's own screens before it are not WrOS's),
        # the logo stays up until the unlock screen, with no console text and no black, except
        # while the unlock screen's first frame is put up. That frame is the bochs driver's first
        # modeset (simpledrm had the display until then): bochs switches the emulated VBE display
        # off and on again without NOCLEARMEM, so for an instant QEMU shows its legacy VGA
        # display and then memory it has just cleared, until the driver copies the frame in.
        # Every later screen keeps the mode and shows nothing of the kind. That switch must be
        # over within 0.25 s, and what QEMU showed during it is printed (PNG) for review.
        wros = next(moment for moment, _, digest in before if boxes.get(digest))
        first_unlock = next(moment for moment, _, digest in before if digest == unlock_screen)
        menu_last = max(moment for moment, lit, digest in before
                        if moment < first_unlock and lit < 0.045 and boxes.get(digest))
        switch = [(m, lit, d) for m, lit, d in before if menu_last < m < first_unlock]
        for d in dict.fromkeys(d for _, _, d in switch):
            m, lit = next((m, lit) for m, lit, e in switch if e == d)
            with open(os.path.join(SHOTS, files[d]), "rb") as shot:
                picture = base64.b64encode(shot.read()).decode()
            print(f"MODESET-FRAME {m - before[0][0]:.2f}s lit={lit:.4f} {files[d]} {picture}")
        assert first_unlock - menu_last < 0.25, (
            f"the boot menu's picture to the unlock screen took {first_unlock - menu_last:.2f}s")
        odd = [(round(m - before[0][0], 2), round(lit, 4)) for m, lit, _ in before
               if m >= wros and not (menu_last < m < first_unlock) and (lit < BLACK or lit >= 0.045)]
        assert not odd, f"black or console text between the boot menu and the unlock screen: {odd}"
        # A frame or two of black (under 0.1 s) where the console briefly takes the display (greetd
        # resets it before it starts the login screen) is put right at once and only reported.
        flickers = [(a, b) for a, b in black_runs(after) if b - a < 0.1]
        print("flickers: " + ", ".join(f"{a - after[0][0]:.2f}s" for a, _ in flickers))
        gaps = [(a, b) for a, b in black_runs(after) if b - a >= 0.1]
        assert not gaps, (
            "black screens between the unlock and the login screen: "
            + ", ".join(f"{a - after[0][0]:.2f}s-{b - after[0][0]:.2f}s" for a, b in gaps)
        )
        assert after[-1][1] >= BLACK, "the login screen is not on screen"
  '';
}
