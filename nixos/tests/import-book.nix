{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
  projectId = "12345678-1234-4234-8234-123456789abc";
  books = pkgs.runCommand "wros-import-books" {} ''
    mkdir -p "$out/usb/manuscript" "$out/drive/manuscript"
    printf '%s\n' 'title = "USB Book"' > "$out/usb/novel.toml"
    printf '%s\n' 'Words brought in from the USB stick.' > "$out/usb/manuscript/01-Scene.md"
    printf '%s\n' 'title = "Clip says Hello World."' > "$out/drive/novel.toml"
    printf '%s\n' 'Words brought in from Google Drive.' > "$out/drive/manuscript/01-Scene.md"
    printf '%s\n' '{"version":1,"project_id":"${projectId}"}' > "$out/drive/.wros-sync.json"
  '';
  usbImage = pkgs.runCommand "wros-import-books-usb.img" {
    nativeBuildInputs = [ pkgs.dosfstools pkgs.mtools pkgs.util-linux ];
  } ''
    # The media helper accepts partitions of removable USB disks, not whole-disk filesystems.
    truncate -s 32M "$out"
    printf 'label: dos\nstart=2048, type=6\n' | sfdisk "$out"
    truncate -s 31M partition.img
    mkfs.vfat -F 16 -n BOOKS partition.img
    mmd -i partition.img '::/USB Book' '::/USB Book/manuscript'
    mcopy -i partition.img ${books}/usb/novel.toml '::/USB Book/novel.toml'
    mcopy -i partition.img ${books}/usb/manuscript/01-Scene.md '::/USB Book/manuscript/01-Scene.md'
    dd if=partition.img of="$out" bs=1M seek=1 conv=notrunc
  '';
in
pkgs.testers.runNixOSTest {
  name = "wros-import-book";

  nodes = {
    installer = { nodes, ... }: {
      imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ];
    };
    installed = encryptedDisk.mkInstalledNode {
      virtualisation.qemu.options = [
        "-device qemu-xhci,id=import-xhci"
        "-drive if=none,id=import-books,format=raw,readonly=on,file=${usbImage}"
        "-device usb-storage,bus=import-xhci.0,drive=import-books,removable=on"
      ];
    };
  };

  testScript = ''
    ${encryptedDisk.pythonPreamble}

    def wait_screen(pattern, timeout=300):
        deadline = time.monotonic() + timeout
        text = ""
        while time.monotonic() < deadline:
            text = screen(installed)
            if re.search(pattern, text):
                return text
            time.sleep(1)
        raise AssertionError(f"timed out waiting for {pattern!r}:\n{text}")

    def enter_row(pattern):
        choose(installed, pattern)
        installed.send_key("ret")

    def open_import(from_dashboard=False):
        # to_editor() waits for the demo book's GRIMOIRE title; it only leaves the dashboard
        # the session may open on. After an import the editor shows the imported book.
        if from_dashboard:
            to_editor(installed)
        installed.send_key("esc")
        installed.send_chars(colemak_keys("f"))
        wait_screen("Open another project")
        enter_row(r"^Open another project")
        wait_screen("YOUR PROJECTS")
        enter_row(r"^Import a project")
        wait_screen("A folder on this laptop")

    try:
        disk_img = build_encrypted_disk(installer)
        prepare_installed_disk(installed, disk_img)
        installed.start(allow_reboot=True)
        wait_for_disk_prompt(installed)
        unlock_at_prompt(installed, colemak_keys("colemak-passphrase"))
        installed.wait_for_unit("multi-user.target", timeout=300)
        installed.wait_for_unit("wros-boot-ok.service", timeout=300)
        installed.wait_until_succeeds("test -b /dev/disk/by-label/BOOKS", timeout=300)

        # The real backup/sync helpers share this private alias config, just as in the
        # Python helper tests. Its presence marks Google Drive as linked; no OAuth or network.
        drive = "/home/writer/test-drive"
        drive_book = drive + "/WrOS/Clip says Hello World."
        backup_state = "/home/writer/.local/share/grimoire/wros/backup"
        # Made as the writer: every parent (~/.local/share/grimoire ...) must be theirs too,
        # or the app cannot write its own data there.
        installed.succeed("runuser -u writer -- mkdir -p -m 0700 " + shlex.quote(backup_state))
        installed.succeed("runuser -u writer -- mkdir -p " + shlex.quote(drive + "/WrOS"))
        installed.succeed("cp -r ${books}/drive " + shlex.quote(drive_book))
        installed.succeed("chmod -R u+w " + shlex.quote(drive))
        config = "[wros-google]\ntype = alias\nremote = " + drive + "\n"
        installed.succeed(
            "umask 077; printf %s " + shlex.quote(config) + " > "
            + shlex.quote(backup_state + "/rclone.conf")
        )
        installed.succeed(
            "chown -R writer:users " + shlex.quote(drive) + " " + shlex.quote(backup_state)
        )

        log_in(installed, "writer", "writer-secret-password", colemak=True, console=True)
        wait_screen("Resume writing|GRIMOIRE")

        with subtest("import a book from the BOOKS USB stick"):
            open_import(from_dashboard=True)
            wait_screen('USB stick "BOOKS"')
            enter_row(r'^USB stick "BOOKS"$')
            wait_screen(r"USB Book.*project")
            # Check that it really mounted, through the writer's polkit session, read-only.
            options = installed.succeed(
                "findmnt -n -o OPTIONS /run/wros-import/media/1"
            ).strip().split(",")
            assert {"ro", "nodev", "nosuid", "noexec"} <= set(options), options
            enter_row(r"^USB Book.*project")
            wait_screen(r"Copied to ~/Documents/USB Book and checked\.")
            installed.send_key("ret")
            # The tree's title is the open book's, so the imported book is open.
            wait_screen(r"(?s)\A(?!.*IMPORT A PROJECT).*┌ USB BOOK ")
            installed.wait_until_fails("findmnt /run/wros-import/media/1", timeout=300)
            usb_local = "/home/writer/Documents/USB Book/"
            installed.succeed("cmp ${books}/usb/manuscript/01-Scene.md "
                              + shlex.quote(usb_local + "manuscript/01-Scene.md"))
            # rc22 (#164): the desk renames the imported novel.toml to project.toml, unchanged.
            installed.wait_until_succeeds(
                "test ! -e " + shlex.quote(usb_local + "novel.toml"), timeout=120)
            installed.succeed("cmp ${books}/usb/novel.toml " + shlex.quote(usb_local + "project.toml"))

        with subtest("import a synced book from Google Drive"):
            open_import()
            # Status loads asynchronously on opening Import. Do not choose the unlinked row.
            wait_screen(r"Google Drive\s+│")
            enter_row(r"^Google Drive$")
            wait_screen(r"Clip says Hello World..*project.*synced")
            enter_row(r"^Clip says Hello World..*project.*synced")
            wait_screen(r"Copied to ~/Documents/Clip says Hello World\. and checked\.")
            installed.send_key("ret")
            wait_screen(r"(?s)\A(?!.*IMPORT A PROJECT).*┌ CLIP SAYS HELLO WORLD\. ")
            local = "/home/writer/Documents/Clip says Hello World."
            scene = "manuscript/01-Scene.md"
            installed.succeed("cmp ${books}/drive/" + scene + " " + shlex.quote(local + "/" + scene))
            installed.succeed(
                "cmp ${books}/drive/" + scene + " " + shlex.quote(drive_book + "/" + scene))
            # rc22 (#164): renamed on the laptop, unchanged; Drive has it under whichever name
            # its last sync left.
            installed.wait_until_succeeds(
                "test ! -e " + shlex.quote(local + "/novel.toml"), timeout=120)
            installed.succeed("cmp ${books}/drive/novel.toml " + shlex.quote(local + "/project.toml"))
            installed.succeed(
                "cmp ${books}/drive/novel.toml " + shlex.quote(drive_book + "/novel.toml")
                + " || cmp ${books}/drive/novel.toml " + shlex.quote(drive_book + "/project.toml"))
            marker = json.loads(installed.succeed(
                "cat " + shlex.quote(drive_book + "/.wros-sync.json")
            ))
            identity = json.loads(installed.succeed(
                "cat " + shlex.quote(local + "/.grimoire/sync.json")
            ))
            assert marker["project_id"] == "${projectId}", marker
            assert identity["project_id"] == marker["project_id"], identity
            assert identity["version"] == 1, identity
            assert identity["drive_folder"] == "Clip says Hello World.", identity
            installed.fail("findmnt /run/wros-import/media/1")
    except Exception:
        # Include the actual console even when choose(), a helper wait or an assertion fails.
        # Before stage 2 the backdoor may be absent: keep that diagnostic attempt bounded.
        try:
            print(installed.succeed(
                "python3 -c " + shlex.quote(SCREEN_PY % ("1", "1")), timeout=15
            ))
        except Exception as error:
            print(f"could not read the installed console: {error}")
        raise
  '';
}
