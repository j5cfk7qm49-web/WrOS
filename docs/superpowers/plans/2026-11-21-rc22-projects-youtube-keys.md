# rc22: projects, all of YouTube, keys that act once — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship rc22:
- a triggerhappy fix so each key acts once (#171);
- YouTube playback that streams by default, plus a search of all of YouTube;
- "book" renamed to "project" on screen and on disk (#164), with old names still read.

**Architecture:**
- **Keys:** a nixpkgs patch to triggerhappy's `add_device`, wired into the triggerhappy unit's ExecStart.
- **YouTube:** the existing YouTube Music source (`src/music/ytmusic.rs`) gains a streaming loader (yt-dlp → FIFO → mpv) and a second search scope run by `yt-dlp ytsearch`.
- **Names:** one resolver in `grimoire-core` (`names.rs`) gives every on-disk name, new first and legacy second. The desk migrates a project when it opens it (gated by update probation), and Settings' Go back renames back.

**Tech Stack:** Rust (ratatui app, grimoire-core), Python helpers, NixOS modules and VM tests, yt-dlp, mpv IPC.

**Spec:** `docs/superpowers/specs/2026-11-21-rc22-projects-youtube-keys-design.md`

## Global Constraints

- New names: `project.toml`, `.grimoire/project-id`, `Project-Format.md`. Legacy names: `novel.toml`, `.grimoire/book-id`, `Novel-Format.md`. `project.md` is unchanged. The EPUB's `<dc:identifier id="book-id">` is unchanged.
- Read the new name first, then the legacy one. Never write a legacy name, except when Go back renames back.
- "Book" stays only for Paperback/EPUB/print compile wording and the `part_label` value "Book".
- An installed WrOS migrates only when the recovery status has been read and shows `probation == false` and `staged_update == false`. Off WrOS (no online updates), it migrates on open.
- A replaced old file goes to the project's Trash (`grimoire_core::project::trash`) and the status bar says so. Nothing is deleted outright.
- yt-dlp only; no YouTube API key. Live streams are left out of the results.
- No sshd or remote access in `nixos/modules`. Never drive the app against the owner's real projects. Laptop tests run in a throwaway HOME under /tmp, which is cleaned afterwards.
- After any Rust edit run `rc22-work/fmt.sh`. Before pushing, run `rc22-work/gates.sh`.

## Review Focus

1. **`novel.toml` unreadable** (no read permission, or a half-synced file): migration still renames it (rename is metadata). Its text is untouched, and the project opens as rc21 would. → Task 5 test `migrate_renames_an_unreadable_file_without_reading_it`.
2. **Project on read-only media** (an import stick, a read-only folder): it opens on the legacy names, the status bar says it couldn't rename, and there's no error loop on every tick. → Task 6 test `a_read_only_project_opens_and_says_once_it_kept_its_old_names`.
3. **Stream cut off midway** (network drop, yt-dlp exits non-zero): mpv reaches EOF early. A note says "Couldn't finish <title>", the queue moves on, and nothing hangs. → Task 3 test `a_stream_that_breaks_off_notes_it_and_moves_on` (fake yt-dlp script).
4. **All-of-YouTube results include live streams, shorts or entries with no duration**: live streams are dropped, and items without a duration show an empty length. → Task 4 test `video_search_drops_live_streams_and_keeps_unknown_lengths`.
5. **Go back pressed, then cancelled before restart**: projects keep their legacy names until the next open after the go-back is cancelled (`staged_update` false again), then migrate. They are not migrated while the go-back is staged. → Task 7 test `nothing_migrates_while_a_go_back_is_staged`.

---

### Task 1: Each key acts once (#171)

**Files:**
- Create: `nixos/pkgs/triggerhappy-no-duplicate-devices.patch`
- Modify: `nixos/modules/hardware.nix` (triggerhappy block, ~lines 88–106)
- Modify: `nixos/tests/media-keys.nix`

**Interfaces:** Produces nothing other tasks use.

- [ ] **Step 1: Extend the VM test so it fails on the double.** After `wait_for_unit("triggerhappy.service")`, run `machine.succeed("udevadm trigger --action=add --subsystem-match=input && udevadm settle")` and sleep 2 s. Then:
  - the existing three presses and their exact-log assertion stay as they are;
  - add `assert machine.succeed("pid=$(systemctl show -p MainPID --value triggerhappy); ls -l /proc/$pid/fd | grep -o '/dev/input/event[0-9]*' | sort | uniq -d").strip() == ""`.
- [ ] **Step 2: Run it unpatched** (`rc14-argon/nixos-wsl/build.sh rc22mk checks.x86_64-linux.media-keys`). Expected: FAIL. The log has `1 play-pause`, `2 play-pause`…, or the duplicate-fd assertion fails.
- [ ] **Step 3: Write the patch.** In triggerhappy 0.5.0's `devices.c`, at the start of `add_device(char *dev, int fd, int excl, char *tag)`, walk `device_list`. If an entry's `devname` equals `dev`:
  - `fprintf(stderr, "Device %s already open.\n", dev)`;
  - close `fd` if it is `>= 0`;
  - return.
- [ ] **Step 4: Wire the patch in `hardware.nix`.**
  - `thd = pkgs.triggerhappy.overrideAttrs (o: { patches = (o.patches or [ ]) ++ [ ../pkgs/triggerhappy-no-duplicate-devices.patch ]; })`.
  - Move the existing binding list into a `let` (`keyBindings`) and keep `services.triggerhappy.bindings = keyBindings`.
  - Set `systemd.services.triggerhappy.serviceConfig.ExecStart = lib.mkForce "${thd}/bin/thd --user wros-keys --socket /run/thd.socket --triggers ${triggersFile} --deviceglob /dev/input/event*"`.
  - `triggersFile = pkgs.writeText "triggerhappy.conf"` renders each binding as `KEY_<KEY> <1|2|0> <cmd>` (press = 1, hold = 2, release = 0), one per line. This is the same format the NixOS module writes.
  - Not an overlay: `nixpkgs.pkgs` is set, and the VM tests' pkgs are read-only.
- [ ] **Step 5: Run the VM test.** Expected: PASS.
- [ ] **Step 6: Commit** `rc22: each key acts once: triggerhappy skips a device it already has open (#171)`.

### Task 2: Spike: does yt-dlp → FIFO → mpv stream reliably?

**Files:**
- Create: `rc22-work/proto/stream.sh` (throwaway, outside the repo)

- [ ] **Step 1:** In NixOS-WSL, with `nix shell nixpkgs#yt-dlp nixpkgs#mpv`:
  - `mkfifo $d/a`;
  - start `mpv --no-video --ao=null --input-ipc-server=$d/sock --idle`, then `loadfile $d/a` over IPC;
  - run `yt-dlp -f bestaudio -o - --no-part -q "https://music.youtube.com/watch?v=<a public song id from a wros-ytm search>" > $d/a`.
  - Record:
    - seconds until `time-pos` > 0;
    - whether `duration` is known;
    - whether seek +10 s works;
    - what mpv reports when yt-dlp is killed mid-song (`end-file` reason).
  - Then repeat with a 1-hour video from `yt-dlp --flat-playlist -J "ytsearch5:lofi 1 hour"`.
- [ ] **Step 2: Write down the results** (`rc22-work/proto/stream-results.md`):
  - start time;
  - duration known yes/no;
  - forward seek yes/no;
  - the EOF reason on a kill.
  - **Go:** start < 10 s and a kill gives an `end-file` event. **No-go:** download-first stays the default; tell the owner.

### Task 3: YouTube plays as it arrives

**Files:**
- Modify: `src/music/ytmusic.rs` (loader: `download`, `select`, `track_state`, `command`)
- Modify: `src/music/mod.rs` (`Config`: new `youtube_play`)
- Modify: `src/app/shell.rs` (`music_source_rows` for YouTube Music: a "Play" row)
- Test: in `src/music/ytmusic.rs`, `mod tests`

**Interfaces:**
- Produces: `pub enum YoutubePlay { Stream, Download }` in `src/music/mod.rs`, `Default = Stream`. It is stored in `music.toml` as `youtube_play = "stream" | "download"`; an unknown value counts as Stream.
- Produces: `Config.youtube_play: YoutubePlay`.

- [ ] **Step 1: Failing tests:**
  - `config_reads_youtube_play_and_defaults_to_stream`: missing gives Stream, `"download"` gives Download, `"x"` gives Stream; `save` writes the line.
  - `a_stream_that_breaks_off_notes_it_and_moves_on`: the yt-dlp path is a fake script that writes 1 s of silence (an Ogg file from `assets/` or generated with ffmpeg in the test fixture) and then exits 1. Expect `Update::Note` containing "Couldn't finish" and `current` advanced.
  - `streaming_starts_without_waiting_for_the_whole_file`: the fake yt-dlp writes slowly. `loaded` becomes `Some` before the script exits.
- [ ] **Step 2: Run** `rc22-work/one.sh ytmusic`. Expected: FAIL.
- [ ] **Step 3: Implement.** For Stream, `select` / `move_to` makes a FIFO in a private temp dir (0700), tells mpv `loadfile <fifo>`, and spawns `yt-dlp -f bestaudio -o - --no-part -q <url>` with stdout opened on the FIFO (O_WRONLY, opened in the worker thread because it blocks until mpv reads).
  - mpv's `end-file` with yt-dlp's non-zero exit gives the note and the advance.
  - Cancelling (`select` of another song, quit) kills yt-dlp and removes the FIFO.
  - Duration falls back to the item's `length` when mpv has none.
  - Download keeps today's path unchanged.
- [ ] **Step 4: Settings row.** In Settings › Music, for YouTube Music: `"Play · as it arrives"` / `"Play · after downloading"`. Enter toggles it and saves `music.toml`.
- [ ] **Step 5: Run** `rc22-work/one.sh ytmusic` and `rc22-work/one.sh shell_ui`. Expected: PASS.
- [ ] **Step 6: Commit** `rc22: YouTube plays as it arrives (setting: after downloading)`.

### Task 4: Search all of YouTube

**Files:**
- Modify: `src/music/ytmusic.rs` (`search`)
- Modify: `src/music/mod.rs` (`Cmd::Search`; `Config.youtube_search`)
- Modify: `src/app/overlays/player.rs` (typing in the Search tab)
- Modify: `assets/help/17-music.md`
- Test: `src/music/ytmusic.rs` tests; `src/ui_tests.rs`

**Interfaces:**
- Consumes: the Task 3 Config pattern.
- Produces:
  - `pub enum SearchScope { YoutubeMusic, AllOfYoutube }` (in `music.toml` as `youtube_search = "music" | "all"`, default `music`);
  - `Cmd::Search { query: String, scope: SearchScope }` replaces `Cmd::Search(String)`;
  - `fn parse_video_search(v: &serde_json::Value) -> Vec<Item>` in `ytmusic.rs`.

- [ ] **Step 1: Failing tests:**
  - `video_search_reads_title_channel_and_length`: on recorded `yt-dlp --flat-playlist -J ytsearch` JSON (an `entries` array with `id`, `title`, `channel`, `duration` 258.0), expect `Item { title, artist: channel, length: "4:18", id }`.
  - `video_search_drops_live_streams_and_keeps_unknown_lengths`: `live_status: "is_live"` is dropped; `duration: null` gives `length: ""`.
  - In `src/ui_tests.rs`, `tab_while_typing_a_search_switches_to_all_of_youtube`: on the YouTube Music source, F7, `/`, type "lofi", Tab. The prompt shows "all of YouTube", Enter sends `Cmd::Search { scope: AllOfYoutube, .. }`, and `music.toml` gets `youtube_search = "all"`.
- [ ] **Step 2: Run** `rc22-work/one.sh video_search` and `rc22-work/one.sh tab_while_typing`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - **AllOfYoutube:** runs `yt-dlp --flat-playlist -J --no-warnings "ytsearch20:<query>"` (query ≤ 200 printable characters, as `wros-ytm` checks). The results replace `songs`, so Enter and `a` work as for songs.
  - **The id:** a played video uses `https://www.youtube.com/watch?v=<id>`, songs keep the music URL, and `Item.id` stays a bare id that passes the existing id check.
  - **Typing:** while typing in the Search tab, Tab/BackTab toggles the scope (it doesn't move tabs). The prompt reads `find on YouTube Music · Tab: all of YouTube` or `find on all of YouTube · Tab: YouTube Music`.
  - **Other sources:** Tab still moves tabs.
  - **Help:** the help page gets one bullet.
- [ ] **Step 4: Run** both. Expected: PASS. Also run the live `#[ignore]` test `live_video_search_finds_something` once in WSL.
- [ ] **Step 5: Commit** `rc22: search all of YouTube from the player (Tab while typing)`.

### Task 5: One resolver for a project's names

**Files:**
- Create: `crates/grimoire-core/src/names.rs` (register in `lib.rs`)
- Test: `crates/grimoire-core/src/names.rs` tests

**Interfaces:**
- Produces:
  - `pub enum NameKind { Settings, Id, Format }`.
  - `pub const fn new_name(kind: NameKind) -> &'static str` and `pub const fn legacy_name(kind: NameKind) -> &'static str`. These are paths relative to the root: `"project.toml"`/`"novel.toml"`, `".grimoire/project-id"`/`".grimoire/book-id"`, `"Project-Format.md"`/`"Novel-Format.md"`.
  - `pub fn path(root: &Path, kind: NameKind) -> PathBuf`: the new path if it exists, else the legacy path if it exists, else the new path. (Symlink metadata: a dangling symlink counts as existing.)
  - `pub struct Migration { pub renamed: Vec<NameKind>, pub trashed: Vec<(NameKind, PathBuf)>, pub kept_old: Option<String> }`.
  - `pub fn migrate(root: &Path) -> Migration`: for each kind with a legacy file, `rename(legacy, new)` if no new file exists. If both exist, `project::trash(root, legacy)` with the name `"<legacy file name> (old name, <YYYY-MM-DD HH.MM>)"`. On an error it fills `kept_old` with a one-line reason and doesn't stop the other kinds.
  - `pub fn restore_legacy(root: &Path) -> Result<usize>`: rename new back to legacy where the legacy name doesn't exist. Returns the count renamed.
- [ ] **Step 1: Failing tests:**
  - `path_prefers_the_new_name_then_the_old`
  - `migrate_renames_all_three`
  - `migrate_with_both_keeps_the_new_and_trashes_the_old` (the Trash holds the old text)
  - `migrate_renames_an_unreadable_file_without_reading_it` (chmod 000 on unix)
  - `migrate_on_a_read_only_folder_reports_and_changes_nothing`
  - `restore_legacy_puts_old_names_back_and_counts`
  - `migrate_then_restore_round_trips_byte_for_byte`
- [ ] **Step 2: Run** `rc22-work/one.sh names`. Expected: FAIL.
- [ ] **Step 3: Implement** in `names.rs`. The Trash name uses the local time.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `rc22: one resolver for a project's file names`.

### Task 6: Every reader and writer asks the resolver

**Files:**
- Modify every `root.join("novel.toml")`, `join("book-id")` and `"Novel-Format.md"` outside tests:
  - `crates/grimoire-core/src/project.rs` (`Project::load`, `keep_page_label`, `save_section_order`, `Area::Format`, the scaffold);
  - `submission.rs`, `sessions.rs` (`id_file`, the part_label read), `knowledge.rs`, `shelf.rs` (`title_of`), `sync.rs` (`drop_conflict_copy` ancestors), `create.rs`, `book_copy.rs`, `export*.rs`, `timeline/mod.rs`;
  - `crates/grimoire-app/src/lib.rs` (`books()`);
  - `src/books.rs` (`title`), `src/app.rs`.
- Modify: the user-visible strings that name the file (`"novel.toml didn't read as settings"` and others) to say `project.toml`.
- Test: the existing tests, made to pass on both names. New: `grimoire-app` `books_lists_folders_with_either_name`; project `a_new_project_has_only_the_new_names`; the `Area::Format` kit detection with either file.

**Interfaces:**
- Consumes: `names::path`, `names::new_name`.
- Produces: `Project::scaffold` writes `project.toml` and `Project-Format.md`; `sessions::assign_id` writes `.grimoire/project-id`.

- [ ] **Step 1: Write the three new tests.** Also change existing fixtures that write `novel.toml` so that half write the legacy name and half the new one. Each must still pass.
- [ ] **Step 2: Run** `rc22-work/check.sh`. Expected: the new tests FAIL.
- [ ] **Step 3: Replace** each listed join with `names::path(root, kind)`, and each creation with `root.join(names::new_name(kind))`. Leave the EPUB `book-id` alone.
- [ ] **Step 4: Run** `rc22-work/check.sh` (clippy and all suites). Expected: PASS. `git grep -n '"novel.toml"' -- src crates ':!*names.rs'` shows only tests and comments.
- [ ] **Step 5: Commit** `rc22: every reader takes project.toml or novel.toml`.

### Task 7: The desk migrates a project it opens

**Files:**
- Modify: `src/app.rs` (open path; a tick hook)
- Modify: `src/app/shell/update_flow.rs` (read the gate from `deck.shell.recovery`)
- Test: `src/ui_tests.rs`

**Interfaces:**
- Consumes: `names::migrate`, `RecoveryInfo { probation, staged_update }`.
- Produces:
  - `App::names_may_move(&self) -> bool`: true when `!deck.online_updates`, or when the recovery status has been read and shows `!probation && !staged_update`;
  - `App::migrate_open_project(&mut self)`: runs once per project per session when the gate allows, then reloads the project meta and states the result.

- [ ] **Step 1: Failing tests:**
  - `opening_an_old_project_off_wros_renames_its_files`
  - `on_wros_nothing_migrates_until_probation_is_over`
  - `nothing_migrates_while_a_go_back_is_staged`
  - `a_read_only_project_opens_and_says_once_it_kept_its_old_names`
  - `both_names_present_trashes_the_old_and_says_so`
  - Status texts:
    - "This project's files now use the new names (project.toml)."
    - "novel.toml was replaced by project.toml; the old one is in this project's Trash."
    - "Couldn't rename this project's files (<reason>); it opened as before."
- [ ] **Step 2: Run** `rc22-work/one.sh migrat`. Expected: FAIL.
- [ ] **Step 3: Implement.** Call it on open and from the existing tick (as `tick_update_health` does) until it has run for the open project.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `rc22: the desk renames a project's files when it opens it, once updates allow`.

### Task 8: Go back renames projects back

**Files:**
- Modify: `src/app/shell/update_flow.rs` (`go_back_rows`, `update_go_back`)
- Test: `src/shell_ui_tests.rs`

**Interfaces:**
- Consumes: `names::restore_legacy`, `books::recent`, `grimoire_core::shelf` (the projects under `books::documents()`).
- Produces: `fn needs_old_names(previous: Option<&str>) -> bool`. It is false only when the previous version parses as `0.1.0~rcN` with N ≥ 22, or is a later 0.x; a missing or unparseable version gives true.

- [ ] **Step 1: Failing tests:**
  - `needs_old_names_for_rc21_and_unknown_but_not_rc22`
  - `go_back_to_rc21_says_how_many_projects_it_renames_back` (a confirm row reads "Renames 2 projects' files back to the names 0.1.0~rc21 reads")
  - `go_back_renames_shelf_and_recent_projects_back_then_asks`
  - `go_back_to_rc22_or_later_renames_nothing`
- [ ] **Step 2: Run** `rc22-work/one.sh go_back`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - **Before `deck.request(GoBackToPreviousVersion)`:** run `restore_legacy` on each project on the shelf and in the recent list (de-duplicated by canonical path) and say "Renamed N projects back". An error on one project is reported and doesn't stop the go-back.
  - **The open project:** flushed first.
  - **Migration:** Task 7's gate then blocks re-migration while `staged_update` is true.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `rc22: going back to rc21 renames projects' files back first`.

### Task 9: "Book" becomes "project" on screen, in help and in docs (Codex)

**Files:**
- Modify: user-visible strings in `src/**`, `crates/wros-shell`, `crates/grimoire-core` messages, `assets/help/*.md`, `docs/*.md` (not `docs/archive`), `docs/FEATURE_MATRIX.md`, `docs/feature-status.json`, `site/src/**`, `README.md`.
- Regenerate: `docs/keyboard-reference.html` (`tools/generate-keyboard-reference.py`) and snapshots (`tests/snapshots/*`).
- Modify: tests that match the old text, and the VM tests' OCR/driver text (`nixos/tests/import-book.nix`, `book-timeline.nix`, `boot-unlock-login.nix`).

- [ ] **Step 1: Write the Codex brief** `rc22-work/issue-words.md`:
  - the rules (Global Constraints);
  - the menu names "New project…", "Open another project…", "Import a project…", the shelf title "Your projects" and "Linking the project with Google Drive…";
  - keep: Paperback, EPUB, "Book" as a part label;
  - don't rename code identifiers except where a string is built from them;
  - run `check.sh` and `fmt.sh`.
- [ ] **Step 2: Queue it** in `WrOS/rc22-codex` via `rc22-work/codex-queue.sh words`, after Tasks 5–8 are merged into `rc22`.
- [ ] **Step 3: Review the diff.** `git grep -n -i "\bbooks\?\b" -- src assets/help` shows only compile/EPUB/part-label uses, tests and comments.
- [ ] **Step 4: Run** `check.sh`, `quick.sh` (driven snapshots), `tools/check-wros.py`. Expected: PASS.
- [ ] **Step 5: Commit** `rc22: books are projects on screen, in help and in docs (Codex, reviewed)`.

### Task 10: VM proof of the migration, then release

**Files:**
- Modify: `nixos/tests/import-book.nix` (or `book-timeline.nix`, whichever already opens a project on the desk)
- Modify: `os/VERSION`, `os/tests/test_packaging.py`, `flake.nix` version
- Create: `WrOS/rc22-release/release-notes.md` and `rc14-argon/nixos-wsl/rc22-{qualify,release-build,release-post,channel-check}.sh` (baseline rc21; `channel-src/rc21` = `git archive f68a7ba`)

- [ ] **Step 1: VM step.** Copy a fixture folder with `novel.toml` and `.grimoire/book-id` into the writer's Documents, open it on the desk, then check:
  - `test -f project.toml && test ! -e novel.toml`;
  - `.grimoire/project-id` holds the old id;
  - the timeline repo is unchanged (same HEAD before and after).
- [ ] **Step 2: Version.** Bump to `0.1.0~rc22`. Run `check.sh`, `gates.sh` and the full `rc22-qualify.sh` (all checks plus the ISO and Rufus/CD boot checks; confirm `iso-check-rc22/rufus` has pictures).
- [ ] **Step 3: Laptop check** over the side SSH, in a throwaway HOME under /tmp, never the owner's Documents:
  - a copy of a fixture project with old names opens and migrates;
  - a YouTube video streams;
  - `/proc/<thd>/fd` has no duplicates after a reboot of the new build. Needs the owner to restart after staging.
- [ ] **Step 4: Release notes** (download size, the boot-menu rollback limit, the second-computer note), then build, sign, publish and run the channel check as an rc21 laptop, per `wros-release-flow`.
- [ ] **Step 5: GitHub.** Close #171 and #164 when released and seen on the laptop.
