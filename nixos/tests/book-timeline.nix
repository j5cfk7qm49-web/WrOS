{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
  book = pkgs.runCommand "wros-timeline-book" {} ''
    mkdir -p "$out/manuscript" "$out/notes"
    printf '%s\n' 'title = "Timeline Book"' > "$out/novel.toml"
    printf '%s\n' 'The river rose.' > "$out/manuscript/01-Riverbank.md"
    printf '%s\n' 'The tower stood.' > "$out/manuscript/02-Watchtower.md"
    printf '%s\n' 'Outline for tomorrow.' > "$out/notes/Outline.md"
    printf '%s\n' '{"version":1,"project_id":"12345678-1234-4234-8234-123456789abc"}' \
      > "$out/.wros-sync.json"
  '';
in
pkgs.testers.runNixOSTest {
  name = "wros-book-timeline";

  nodes = {
    installer = { nodes, ... }: {
      imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ];
    };
    installed = encryptedDisk.mkInstalledNode {};
  };

  testScript = ''
    ${encryptedDisk.pythonPreamble}

    home = "/home/writer"
    local = home + "/Documents/Timeline Book"
    data = home + "/.local/share/grimoire"
    drive = home + "/test-drive"
    drive_book = drive + "/WrOS/Timeline Book"
    timeline = drive + "/WrOS-History/Timeline Book/timeline"
    scenes = {
        "Riverbank": "manuscript/01-Riverbank.md",
        "Watchtower": "manuscript/02-Watchtower.md",
    }

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

    def open_shelf():
        # Through Ctrl-K, which works from any focus (Esc then f types the f into a page when
        # the menu is slow to open). A book can open on its Overview, where Ctrl-K is not Find
        # anything: leave it for the editor first.
        if "WrOS · Overview" in screen(installed):
            installed.send_key("esc")
            wait_screen(r"(?s)\A(?!.*WrOS · Overview)")
        installed.send_key("ctrl-k")
        wait_screen("FIND ANYTHING")
        installed.send_chars("Open another project")
        enter_row(r"^Open another project")
        wait_screen("YOUR PROJECTS")

    def book_header(title):
        return "┌ " + title.upper() + " "

    def import_book():
        open_shelf()
        enter_row(r"^Import a project")
        wait_screen(r"Google Drive\s+│")
        enter_row(r"^Google Drive$")
        wait_screen(r"Timeline Book.*project.*synced")
        enter_row(r"^Timeline Book.*project.*synced")
        wait_screen(r"Copied to ~/Documents/Timeline Book and checked\.")
        installed.send_key("ret")
        wait_screen(r"(?s)\A(?!.*IMPORT A PROJECT).*" + book_header("Timeline Book"))

    def switch_book(title):
        header = book_header(title)
        for _ in range(10):
            open_shelf()
            # A shelf row is the title, then its folder.
            enter_row(r"^" + re.escape(title) + r"(?:\s|$)")
            # The book opens in its editor, or on its Overview ("Grimoire · Drafting").
            overview = "│" + title + " · "
            text = wait_screen(
                r"(?s)\A(?!.*YOUR PROJECTS).*(?:" + re.escape(header) + "|"
                + re.escape(overview) + "|Syncing before leaving)"
            )
            if header in text or overview in text:
                return
            # The first attempt starts the final sync. Retry once it has had time to run.
            time.sleep(1)
        raise AssertionError(f"could not switch to {title}:\n{text}")

    def open_scene(title):
        # Find the scene by name, as in the driven writer's terminal test.
        installed.send_key("ctrl-k")
        wait_screen("FIND ANYTHING")
        installed.send_chars(title)
        enter_row(r"^" + title + r"\s+scene")
        wait_screen(r"(?si)\A(?!.*FIND ANYTHING).*┌ (?:[^┐\n]+ / )?" + title + " ")

    def write_scene(title, words):
        open_scene(title)
        installed.send_key("home")
        installed.send_chars(words + " ")
        installed.send_key("ctrl-s")
        installed.wait_until_succeeds(
            "grep -Fq " + shlex.quote(words) + " " + shlex.quote(local + "/" + scenes[title])
        )

    def git(repo, *args):
        return shlex.join(["runuser", "-u", "writer", "--", "git", "--git-dir", repo, *args])

    def repository():
        # A project brought in has no id yet; WrOS gives it one under the rc22 name.
        book_id = local + "/.grimoire/project-id"
        installed.wait_until_succeeds("test -s " + shlex.quote(book_id))
        identifier = installed.succeed("cat " + shlex.quote(book_id)).strip()
        return data + f"/sessions/{identifier}.git"

    def wait_bundle(repo, number):
        ref = f"refs/wros/uploaded/{number}"
        installed.wait_until_succeeds(git(repo, "rev-parse", "--verify", ref), timeout=120)
        machine = installed.succeed("cat " + shlex.quote(data + "/machine-id")).strip()
        assert re.fullmatch(r"[a-z0-9-]+-[0-9a-f]{4}", machine), machine
        find = ("find " + shlex.quote(timeline + "/" + machine)
                + " -maxdepth 1 -type f -name " + shlex.quote(f"{number:06} *.bundle"))
        paths = installed.succeed(find).splitlines()
        assert len(paths) == 1, paths
        assert re.fullmatch(rf"{number:06} \d{{4}}-\d{{2}}-\d{{2}} \d{{4}}\.bundle",
                            Path(paths[0]).name), paths
        return paths[0]

    disk_img = build_encrypted_disk(installer, keyboard_layout="qwerty")
    prepare_installed_disk(installed, disk_img)
    installed.start(allow_reboot=True)
    wait_for_disk_prompt(installed)
    unlock_at_prompt(installed, "colemak-passphrase")
    installed.wait_for_unit("multi-user.target", timeout=300)
    installed.wait_for_unit("wros-boot-ok.service", timeout=300)

    # Use the same private rclone alias as import-book, with parents owned by the writer.
    backup = data + "/wros/backup"
    installed.succeed("runuser -u writer -- mkdir -p -m 0700 " + shlex.quote(backup))
    installed.succeed("runuser -u writer -- mkdir -p " + shlex.quote(drive + "/WrOS"))
    installed.succeed("cp -r ${book} " + shlex.quote(drive_book))
    installed.succeed("chmod -R u+w " + shlex.quote(drive_book))
    config = f"[wros-google]\ntype = alias\nremote = {drive}\n"
    installed.succeed("umask 077; printf %s " + shlex.quote(config)
                      + " > " + shlex.quote(backup + "/rclone.conf"))
    installed.succeed("chown -R writer:users " + shlex.join([drive, backup]))

    log_in(installed, "writer", "writer-secret-password", colemak=False, console=True)
    wait_screen("Resume writing|GRIMOIRE")
    to_editor(installed)

    with subtest("save two scenes and close the book to publish its timeline"):
        import_book()
        repo = repository()
        installed.wait_until_succeeds(git(repo, "rev-parse", "--verify", "HEAD"))
        # Edit only once the imported book's first sync has recorded its base: an edit made
        # before it is parked as a conflict by that first sync (a separate, older issue).
        installed.wait_until_succeeds(
            "test -s " + shlex.quote(data + "/wros/sync/12345678-1234-4234-8234-123456789abc/base.json"),
            timeout=180,
        )
        # rc22: the project came with novel.toml, and the desk renames it once this version
        # has said it starts (#164). Its words are unchanged.
        installed.wait_until_succeeds(
            "test -f " + shlex.quote(local + "/project.toml")
            + " && test ! -e " + shlex.quote(local + "/novel.toml"),
            timeout=120,
        )
        assert installed.succeed("cat " + shlex.quote(local + "/project.toml")).strip() \
            == 'title = "Timeline Book"'
        write_scene("Riverbank", "Water at the gate.")
        write_scene("Watchtower", "Light in the window.")
        switch_book("Grimoire")
        first_bundle = wait_bundle(repo, 1)

        tags = installed.succeed(git(repo, "tag", "--list", "session/*")).splitlines()
        assert tags, "closing the book did not mark a session"
        heads = installed.succeed(git(repo, "bundle", "list-heads", first_bundle))
        assert any(f" refs/tags/{tag}\n" in heads for tag in tags), heads
        files = installed.succeed(git(repo, "ls-tree", "-r", "--name-only", "HEAD")).splitlines()
        assert {*scenes.values(), "project.toml", "notes/Outline.md", ".grimoire/project-id",
                ".grimoire/resume.md"} <= set(files), files
        for folder in ("journal", "ledger"):
            assert any(p.startswith(f".grimoire/writingdeck/{folder}/") for p in files), files
        installed.succeed("test ! -e " + shlex.quote(local + "/.gitignore"))

        before_import = {}
        for scene in scenes.values():
            pair = shlex.join([local + "/" + scene, drive_book + "/" + scene])
            same, _ = installed.execute("cmp " + pair)
            if same != 0:
                for command in ("diff " + pair, "ls -la " + shlex.quote(local + "/manuscript"),
                                "cat " + shlex.quote(data) + "/wros/sync/*/pending.json"):
                    print(command, installed.execute(command)[1])
            assert same == 0, f"{scene} differs between the laptop and Drive after leaving"
            installed.succeed(git(repo, "show", "HEAD:" + scene)
                              + " | cmp - " + shlex.quote(local + "/" + scene))
            history = installed.succeed(git(repo, "log", "--format=%H %s", "HEAD", "--", scene))
            before_import[scene] = [line.split(" ", 1) for line in history.splitlines()]
            assert len(before_import[scene]) >= 2, history

    with subtest("import after losing the local book and read every version in scene history"):
        installed.succeed("rm -rf -- " + shlex.join([local, data + "/sessions"]))
        import_book()
        for title, scene in scenes.items():
            open_scene(title)
            # Scene history for the open scene, through Ctrl-K (H needs the outline's focus).
            installed.send_key("ctrl-k")
            wait_screen("FIND ANYTHING")
            installed.send_chars("Scene history")
            enter_row(r"^Scene history")
            labels = [label for _, label in before_import[scene]]
            frame = wait_screen(f"HISTORY · {title.upper()} · {len(labels)} versions")
            rows = re.findall(r"\d{2} [A-Za-z]{3} \d{2}:\d{2}  (\S(?:.*?\S)?)(?: {2,}|$)", frame)
            assert rows == labels, frame
            installed.send_key("esc")

    with subtest("publish a fresh full bundle after Drive loses the timeline"):
        # Import rebuilds the Git history. Mark a session while Drive is still intact
        # so the new local repository has an upload receipt before we empty Drive.
        repo = repository()
        installed.send_key("ctrl-k")
        wait_screen("FIND ANYTHING")
        installed.send_chars("Save this session now")
        enter_row(r"^Save this session now")
        receipts = git(repo, "for-each-ref", "--format=%(refname:strip=3)", "refs/wros/uploaded/")
        installed.wait_until_succeeds(receipts + " | grep -q .", timeout=120)
        previous = max(map(int, installed.succeed(receipts).splitlines()))
        assert previous >= 2, previous
        wait_bundle(repo, previous)
        installed.succeed("rm -rf -- " + shlex.quote(timeline))
        write_scene("Riverbank", "The ferry returned.")
        switch_book("Grimoire")
        full_bundle = wait_bundle(repo, previous + 1)

        # Cloning only the new bundle proves it has no earlier bundle prerequisites.
        clone = home + "/full-timeline.git"
        installed.succeed(shlex.join(["runuser", "-u", "writer", "--", "git", "clone",
                                      "--bare", full_bundle, clone]))
        for scene, versions in before_import.items():
            for commit, _ in versions:
                installed.succeed(git(clone, "merge-base", "--is-ancestor", commit, "HEAD"))
            installed.succeed(git(clone, "show", "HEAD:" + scene)
                              + " | cmp - " + shlex.quote(local + "/" + scene))
        assert "The ferry returned." in installed.succeed(
            git(clone, "show", "HEAD:" + scenes["Riverbank"])
        )
  '';
}
