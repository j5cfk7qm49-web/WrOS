# rc21 Music Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Music becomes the third view of the clearing carousel and gets its own Settings group. Spotify leaves WrOS, and the laptop's play/pause, next and previous keys control the music.

**Architecture:**
- `scene::Mode` gains `Music`. `ui::draw_scene` draws it in the clearing's 9 rows.
- The separate music pane and its focus (`Focus::Music`) are deleted.
- `Music` starts dormant and wakes on first use, which replaces the `enabled` switch.
- Hardware keys: triggerhappy → `wros-media-key play-pause|next|previous` → a short press log in `/run/wros-keys/music` → an app watcher thread → the same commands as F4/F5/F6.

**Tech Stack:** Rust (ratatui), Python 3 helper scripts, NixOS modules and VM tests.

**Spec:** `docs/superpowers/specs/2026-11-20-rc21-music-rework-design.md`

## Global Constraints

- Branch `rc21` (worktree `WrOS/rc21`, from rc20 1c9d350).
- Work scripts are in `WrOS/rc21-work`: `check.sh` runs fmt, clippy and tests; `fmt.sh` formats; `one.sh FILTER` runs the matching tests. They run in NixOS-WSL through `rc19-work/nctest.sh`. Run `fmt.sh` after every Rust edit.
- Settings: keep the existing `Route` names and page order inside each group. VM tests reach pages by position. The new group goes **last**: Writing, Look and keyboard, System, Backups and accounts, Updates and recovery, Music.
- Carousel order: Pomodoro, Visualizer, Music. View names: `pomodoro`, `visualizer`, `music`.
- Music view rows (exact labels):
  - `Open player · F7`
  - `Source · {label}…`
  - Jellyfin/Plex only: `Sign in to {label}…` when not set up, `Sign out of {label}` when set up.
- Spotify on WrOS: the source picker shows `not available yet`, and choosing it leaves the source unchanged with the message `Spotify isn't available on WrOS yet`.
- Media-key actions: `play-pause`, `next`, `previous`. The press log `/run/wros-keys/music` holds at most 8 lines of `<seq> <action>`, newest last.
- Nothing may listen on the network for music, and no Spotify credential may remain.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No model names in commits or docs.

## Review Focus

1. **Upgrade from rc20 with `music.toml` saying `enabled = true` and `source = "spotify"`.** The desk opens, the source is Music files, `~/.cache/spotify-player/credentials.json` is gone, and nothing else in `~/.cache` is touched. Test in Task 1.
2. **Two quick presses of the hardware Next key, under 200 ms apart.** The music skips two tracks, not one. The log keeps 8 entries and the app acts on every seq it hasn't seen. Tests in Task 6.
3. **Presses recorded before the app started**, for example at the login screen or in an earlier session. They do nothing when the desk opens. Test in Task 6.
4. **A media key or F5 while music is still dormant.** Music wakes and the command reaches the backend; it isn't dropped. Test in Task 2.
5. **A narrow left column (minimum width 18) or a short terminal.** The Music view truncates titles and drops rows that don't fit, and the switcher still names the current view. Test in Task 3.

---

### Task 1: Spotify leaves the app (Rust)

**Files:**
- Delete: `src/music/spotify_signin.rs`
- Modify:
  - `src/music/spotify.rs`: delete the `cli` module, `wros_signin_available` and the WrOS branch in `state`/`command`/`search`/`playlists`/`load_playlist`; keep the MPRIS/AppleScript remote.
  - `src/music/mod.rs`: `Source::available`, the migration, and removing `mod spotify_signin`.
  - `src/app/shell.rs`: delete `spotify_rows`, and the `Source::Spotify` arm in `music_source_rows`.
  - `src/app/wros.rs:1111`: delete `tick_spotify_signin` and its fn.
  - `src/app/overlays/looks.rs`: `on_sources_key` and `draw_sources`.
  - `src/main.rs:149,190`: the CLI source list still names spotify (that's fine off WrOS).
  - `crates/wros-sys/src/paths.rs:173`: keep `WROS_MUSIC_SIGNIN` until Task 5.
- Test: `src/shell_ui_tests.rs` (replace `spotify_is_signed_in_from_the_music_page`), `src/music/mod.rs` tests.

**Interfaces:**
- Produces:
  - `music::Source::available(self) -> bool`: false only for `Spotify` when `music::on_wros()` is true.
  - `music::on_wros() -> bool`: `Path::new(wros_sys::paths::WROS_SOUND).exists()`.
  - `music::forget_spotify(home: &Path)`: removes the whole folders `home/.cache/spotify-player` and `home/.config/spotify-player`. They hold the login, the web token, `app.toml` and the player's `.log` files, which contained tokens. Nothing else is touched.
  - `music::migrate_off_spotify(home: &Path, wros: bool)`: when `wros` is true it calls `forget_spotify(home)` whatever the source. If the saved source is Spotify, it also saves `source = "files"`.
  - It runs **once, from `App::new`** (and from the `grimoire music` CLI entry), with `wros` taken from the app's injected flag. `Config::load()` stays a pure reader and never deletes or saves.
  - Every WrOS check in this plan (`Source::available`, the picker, the migration) reads the injected `App::wros_music` flag, or takes `wros: bool`. No test reads `on_wros()`, whose answer can differ between the Nix suites and `check.sh`.

- [ ] **Step 1: Write the failing tests** in `src/music/mod.rs`:
  - `a_spotify_source_on_wros_becomes_music_files_and_its_login_is_removed`:
    - temp home with `.config/grimoire/music.toml` = `source = "spotify"\nenabled = true\n`
    - present: `.cache/spotify-player/credentials.json`, `.cache/spotify-player/spotify-player.log` and `.config/spotify-player/app.toml`, plus an unrelated `.cache/other/keep`
    - call `migrate_off_spotify(home, true)`
    - assert the reloaded config's source is `Source::Files`
    - assert both spotify-player folders are gone and `.cache/other/keep` still exists
  - `a_writer_who_switched_away_from_spotify_loses_the_login_too`: the source is `files` and `.cache/spotify-player/credentials.json` exists. After `migrate_off_spotify(home, true)` the folder is gone and the source is still `files`.
  - `off_wros_spotify_stays`: the same setup with `wros = false` keeps `Source::Spotify` and every file.
  - `loading_the_config_never_deletes_anything`: after `Config::load()` on the first setup, every file still exists.
  - `spotify_is_unavailable_only_on_wros`.

  In `src/shell_ui_tests.rs`, replace the Spotify sign-in test with `spotify_is_not_available_on_wros_yet`:
  - open the source picker with `Action::MusicSource`
  - move to Spotify and press Enter
  - assert the overlay is still open and the source unchanged
  - assert `desk.shows("not available yet")`
  - Gate the test by injecting `on_wros`: give `on_sources_key` the check through `App::wros_music` (a `bool` field set from `music::on_wros()` in `App::new`), so tests can set it.
- [ ] **Step 2: Run them:** `bash ../rc21-work/one.sh spotify` → FAIL (missing fns).
- [ ] **Step 3: Implement.**
  - Delete the sign-in code.
  - `App::new` sets `wros_music = music::on_wros()`, then calls `migrate_off_spotify(home, self.wros_music)` before it loads the music config.
  - `draw_sources` notes Spotify as `not available yet` when `!available()` and dims it.
  - `on_sources_key` refuses an unavailable source with the message above.
- [ ] **Step 4: Run** `bash ../rc21-work/check.sh` → fmt-ok, clippy clean, all tests pass.
- [ ] **Step 5: Commit** `rc21: Spotify leaves WrOS's app; a Spotify source becomes Music files`.

### Task 2: Music needs no switch

**Files:**
- Modify:
  - `src/music/mod.rs`: `Config.enabled` and `Music.enabled`, `spawn`, `send`.
  - `src/app.rs`: `on_function_key` at 1198, `Action::MusicToggle` at 2409, the menu row at 5580, `set_music` at 6023.
  - `src/palette.rs`: the MusicToggle entries.
  - `src/app/overlays/menu.rs` and `src/app/overlays/bar.rs`: the toggle row.
  - `src/app/shell.rs:1014`: the `Music · on/off` row.
  - `src/bar_ui_tests.rs`, `src/ui_tests.rs`: tests that use the toggle.

**Interfaces:**
- Consumes: Task 1's `Config`.
- Produces:
  - `Music::dormant(cfg: Config) -> Music`: no thread; state `NoToken`; keeps `cfg`.
  - `Music::wake(&mut self)`: if dormant, it becomes `Music::spawn(cfg)` and does nothing on later calls.
  - `Music::is_awake(&self) -> bool`.
  - `Music::send(&mut self, c: Cmd)` wakes first, then sends.
  - `App::new` uses `Music::dormant(Config::load())`.
  - `Action::MusicToggle` and `App::set_music` are deleted.
  - `Config` no longer has `enabled`. `parse` ignores an `enabled` key, and `save` stops writing it and its comment line.

- [ ] **Step 1: Write the failing tests:**
  - `a_new_desk_starts_no_player` (ui_tests): `!d.app.music.is_awake()` after `Desk::open` and a draw.
  - `f5_wakes_music_and_sends_play` (music tests): use a dormant `Music` with a recording test backend. If none exists, add `#[cfg(test)] Music::with_backend(Box<dyn Backend>)` and assert the backend saw `Cmd::PlayPause`.
  - `the_menu_has_no_music_switch`: `page_labels(app, Route::MusicPage)` contains no label starting `Music ·`, and `bar` entries contain no `Turn music`.
- [ ] **Step 2: Run** `one.sh music` → FAIL.
- [ ] **Step 3: Implement.**
  - `select_music_source` replaces `self.music` with a woken `Music::spawn(cfg)`, because choosing a source is asking for music.
  - Remove the `!self.music.enabled` early return in `on_function_key`.
- [ ] **Step 4: Run** `check.sh` → all pass.
- [ ] **Step 5: Commit** `rc21: music needs no on/off switch; it starts on first use`.

### Task 3: Music is the carousel's third view

**Files:**
- Modify:
  - `src/scene.rs:514-539`: `Mode::Music`, `next`/`prev` over 3, `ALL` of 3, `name`.
  - `src/ui.rs`: delete `MUSIC_H`, the three-way layout in `draw_left` (lines 266–307), `draw_music` (777–875) and `app.rect_music`; add a `Mode::Music` arm in `draw_scene`; the switcher fallback in `view_switch`.
  - `src/app.rs`:
    - delete `Focus::Music`, `music_visible` and `rect_music` (fields at 238 and 243, focusable 5155, ORDER 5169, click 5383)
    - `on_clearing_key` at 5195 dispatches on `pane_mode`
    - add `music_row: usize` to `App`
  - `src/app/overlays/help.rs:87`: the Music view opens the `music` help topic.
  - `src/main.rs:795`: the visualizer check is unchanged; verify only.
  - `src/shots.rs`: if a shot showed the music pane, show the Music view instead.
  - Any `Focus::Music` match arms the compiler finds.
- Create: `src/music_view.rs`, which builds the view's lines and rows (keeps `ui.rs` from growing).
- Test: `src/ui_tests.rs`, and the switcher unit test in `src/ui.rs` (1579–1606).

**Interfaces:**
- Consumes: `Music::wake`, `Music::send` (Task 2); `Overlay::Sources` (exists).
- Produces:
  - `pub enum MusicRow { Player, Source, SignIn, SignOut }`.
  - `music_view::rows(app: &App) -> Vec<(MusicRow, String)>`, using Global Constraints' labels:
    - Player and Source always appear.
    - SignIn appears for Jellyfin/Plex with no server or `api_key`; SignOut appears when both are set.
  - `music_view::view(app: &App, t: &Theme, width: u16, height: u16) -> (String, Vec<Line<'static>>)`, returning the title `♪ {source label} · F7` and the body lines.
  - `App::on_music_view_key(&mut self, key: Key)`:
    - ↑/↓ (`k`/`j`) move `music_row`
    - Enter runs the row: Player → `open_player()`; Source → `Action::MusicSource`; SignIn → `open_settings_page(Route::MusicServerForm)`; SignOut → `music_sign_out()`
    - other keys go to `transport_key`, except ←/→/h/l, which switch views
  - `App::music_sign_out(&mut self)`: clears `server`, `api_key` and `user_id`, saves, respawns, and sets the message `signed out of {label}`.
  - The view's body, top to bottom:
    - the state line(s) that `draw_music` drew: NoToken shows `not set up`, Offline, Idle, and Playing (title, artist, progress bar)
    - a blank line
    - the rows, with the selected one marked `›` and lit when the pane is focused
  - When `height` is too small, drop the blank line first, then the artist line, then rows from the bottom.
  - Showing the Music view (`pane_mode == Music` and the scene visible) calls `app.music.wake()` from `draw_left`. A view that's drawn is a view asked for.
  - `view_switch`: if every name doesn't fit, show `◂ {current} ▸` with dots for position (`●` for the current view, `·` for the others; e.g. `◂ music · · ● ▸`), so all three positions stay discoverable at `LEFT_W` 30, where the row is 28 cells. Clicks on the arrows still step back and forward.
  - A click on the Music view's body focuses the clearing. A click on a row runs it; a click elsewhere in the view toggles play/pause, as the old pane did.

- [ ] **Step 1: Write the failing tests** (ui_tests):
  - Rename `the_pane_switches_between_pomodoro_and_visualizer_and_there_is_no_garden` to `the_pane_cycles_pomodoro_visualizer_music`. Right ×3 returns to Pomodoro; Left from Pomodoro goes to Music.
  - `the_music_view_lists_player_and_source`: at 140×42, in Music mode, the screen shows `Open player · F7` and `Source · Music files…`.
  - `jellyfin_not_set_up_offers_sign_in_from_the_view`:
    - cfg source Jellyfin with an empty server
    - the screen shows `Sign in to Jellyfin…`
    - selecting it and pressing Enter opens `Route::MusicServerForm`
  - `signing_out_from_the_view_forgets_the_server` (under `music_test_home`):
    - cfg Jellyfin with server, api_key and user_id set
    - choosing `Sign out of Jellyfin` leaves `Config::load().server` empty
  - `enter_on_source_opens_the_picker`: the overlay is `Overlay::Sources`.
  - `there_is_no_music_pane_under_the_clearing`: in Pomodoro mode no `♪` title is drawn, and `Focus::Music` no longer exists.
  - `a_narrow_column_still_names_the_view` (Review Focus 5): outline width 18, 80×24, Music mode; the screen shows `music` and the title doesn't overflow its row.
  - Update `view_switch`'s unit test to the 3-mode layout and the dots fallback.
- [ ] **Step 2: Run** `one.sh music_view` (and the others) → FAIL.
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** `check.sh` → all pass.
- [ ] **Step 5: Commit** `rc21: music is the clearing's third view; no music pane`.

### Task 4: Settings gets a Music group

**Files:**
- Modify: `src/app/shell.rs`:
  - `Category` at 15–35: add `Music` with label `Music`.
  - `MENU` at 705: add `Route::Category(Category::Music)` last in Settings, give it `&[Route::MusicPage]`, and remove MusicPage from Interface.
  - `page_entries`: the Interface info row is unchanged.
- Check: `src/app/overlays/menu.rs:257` and the tutorial paths from `MENU`; any test that walks `menu()`.
- Test: `src/shell_ui_tests.rs`.

**Interfaces:** Produces `Category::Music`. `breadcrumb(Route::MusicPage)` becomes `WrOS · Settings › Music › Music`. If the existing breadcrumb code doubles the name, a group with one page may open that page directly; that's the builder's call, and the test pins the result.

- [ ] **Step 1: Write the failing tests:**
  - `music_is_the_last_settings_group`: Settings' rows end with `Music`; Look and keyboard's rows are `Look` and `Keyboard` only.
  - `the_menu_map_still_reaches_power_third`: the existing position check keeps passing.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** Also update `music::SETTINGS_PATH` (it's in the NoToken notice) to `Esc › Settings › Music`.
- [ ] **Step 4: Run** `check.sh` → pass.
- [ ] **Step 5: Commit** `rc21: Music gets its own Settings group, last`.

### Task 5: Spotify leaves the OS (Nix)

**Files:**
- Delete:
  - `nixos/modules/music.nix`
  - `nixos/tests/music-signin.nix`
  - `os/scripts/wros-music-signin`
  - `os/config/50-wros-music.rules`
- Modify:
  - `nixos/modules/installed.nix`: drop the music.nix import.
  - `nixos/modules/polkit.nix:22-31`: drop the music-signin action.
  - `nixos/pkgs/helpers.nix:67-80`: drop the script from the loop and its substitutions.
  - `nixos/pkgs/paths.nix:55,181`.
  - `crates/wros-sys/src/paths.rs:173`: delete `WROS_MUSIC_SIGNIN`.
  - `flake.nix:135`: drop the check.
  - Any Python packaging test that lists helper scripts (`os/tests/test_packaging.py`; grep `music-signin`).
  - `playerctl`: it stays installed only if another source uses it. Grep `playerctl` in `src/music`; if Spotify's remote was its only user on WrOS, remove it from the WrOS packages.

- [ ] **Step 1:** `grep -rn "music-signin\|musicSignin\|MUSIC_SIGNIN\|librespot\|spotify-player\|wros-signin" --exclude-dir=target --exclude-dir=.git . | grep -v CHANGELOG` lists every place; remove each.
- [ ] **Step 2: Run** the evals for the installed system and the ISO (`rc14-argon/nixos-wsl/eval-each.sh`), plus the `python-suite` and `helpers-no-fhs` builds → pass. Then `grep -c spotify` over the installed toplevel's closure path list → 0.
- [ ] **Step 3: Commit** `rc21: spotify_player, librespot and the sign-in window leave WrOS`.

### Task 6: Hardware play/pause, next and previous keys

**Files:**
- Modify:
  - `os/scripts/wros-media-key`: the new actions and `record_music_key`.
  - `os/config/wros-keys-triggers.conf`: `KEY_PLAYPAUSE 1 … play-pause`, `KEY_NEXTSONG 1 … next`, `KEY_PREVIOUSSONG 1 … previous`; press only, no hold.
  - `nixos/modules/hardware.nix:89-103`: the same three bindings with `event = "press"`.
  - `os/tests/test_media_key.py`.
  - `docs/HARDWARE_CHECKLIST.md` §12: add the three keys, and add them to the probe's grep.
- Create:
  - `src/media_keys.rs`: the app watcher.
  - `nixos/tests/media-keys.nix`.
- Modify: `src/app.rs` (start the watcher in `App::new`, drain it in the tick that drains gauges) and `flake.nix` (add the `media-keys` check).

**Interfaces:**
- Produces:
  - Helper `record_music_key(state: Path, action: str) -> None`:
    - reads `state/'music'`, takes the last seq (0 if missing or garbled), appends `f'{seq+1} {action}'` and keeps the last 8 lines
    - writes atomically: a tmp file in `state`, `os.replace`, mode 0644
    - waits for the lock (`LOCK_EX`, blocking) instead of dropping the press
    - does not touch `changed`
  - Rust `media_keys::MediaKey { PlayPause, Next, Previous }`.
  - `media_keys::parse(text: &str) -> Vec<(u64, MediaKey)>`, which skips bad lines.
  - `media_keys::Watcher::start(path: PathBuf) -> Watcher`: a thread that reads the whole file (≤ 8 short lines) every 100 ms and compares seqs; it doesn't use mtime or length.
    - At start, the high-water mark is the highest seq in the file, or 0.
    - Each poll sends every entry with seq above the mark, in order, then raises the mark.
    - If the file's highest seq is **below** the mark, the log was restarted (garbled, or `/run` was cleared). The mark becomes 0 and that poll's entries are sent, so the watcher never goes deaf.
  - `Watcher::try_iter(&self) -> impl Iterator<Item = MediaKey>`.
  - The app maps PlayPause → `on_function_key(5)`, Next → 6, Previous → 4.
  - The path is `/run/wros-keys/music` on Linux. Elsewhere there is no watcher.

- [ ] **Step 1: Write the failing Python tests** (test_media_key.py):
  - `test_music_keys_are_logged_for_the_app`:
    - `press('play-pause')`, `press('next')`, `press('next')`
    - the `music` file reads `1 play-pause\n2 next\n3 next\n`
    - no `run` calls, and `changed` was not written
  - `test_the_music_log_keeps_the_last_eight`: 10 presses leave 8 lines, seq 3–10.
  - `test_a_garbled_log_starts_again_at_one`.
  - The existing `test_every_trigger_runs_the_handler_with_a_known_action` must cover the new triggers (it reads the conf).
  - A new `test_hardware_nix_binds_the_music_keys_on_press_only`: parse `nixos/modules/hardware.nix` text for the three keys with `event = "press"` and no `hold`.
- [ ] **Step 2: Write the failing Rust tests** in `src/media_keys.rs`:
  - `parse_reads_seq_and_action_and_skips_junk`.
  - `presses_from_before_start_are_ignored` (Review Focus 3): write `5 next` to a temp file, start the watcher, sleep 300 ms, assert nothing; append `6 play-pause`, assert `PlayPause` arrives within 1 s.
  - `two_quick_presses_both_arrive` (Review Focus 2): write `7 next\n8 next` in one write, assert two `Next` arrive.
  - `a_restarted_log_is_heard`: start with `40 next` in the file, then replace the file with `1 play-pause`; assert `PlayPause` arrives within 1 s.
- [ ] **Step 3: Run** `nix build .#checks.x86_64-linux.python-suite` and `one.sh media_keys` → FAIL.
- [ ] **Step 4: Implement** per Interfaces.
- [ ] **Step 5: Write the VM test** `nixos/tests/media-keys.nix`:
  - a node with the modules that define the `wros-keys` account, the `/run/wros-keys` tmpfiles rule and triggerhappy: `../modules/base.nix` and `../modules/hardware.nix` if they evaluate alone, else `installedModule`
  - `machine.wait_for_unit("triggerhappy.service")`
  - `send_key("audioplay")` → wait until `/run/wros-keys/music` ends with `1 play-pause`
  - `send_key("audionext")` → `2 next`
  - `send_key("audioprev")` → `3 previous`
  - assert the file is owned by `wros-keys`
  - assert `/run/wros-keys/changed` was not created by these presses
- [ ] **Step 6: Run** the new VM test alone (`rc21-work/vm.sh`, copied from `rc20-work`, with `CHECKS=media-keys`) and `check.sh` → pass.
- [ ] **Step 7: Commit** `rc21: the play/pause, next and previous keys control the music`.

### Task 7: Help, docs, site

**Files:**
- `assets/help/17-music.md`: the Music view, its keys and rows; media keys; Settings › Music; Spotify "not available yet" with a one-line reason.
- `assets/help/14-the-pomodoro.md` and `15-the-visualizer.md`: the three-view carousel.
- `assets/help/23-settings.md`: six groups.
- `assets/help/24-every-key.md`: media keys, the Music view's keys, and removing the music pane's keys.
- `docs/API_AND_NETWORK_USE.md`: remove the Spotify network use.
- `docs/HARDWARE_CHECKLIST.md`: done in Task 6.
- `site/src/pages/index.astro`: drop the Spotify claim.
- `CHANGELOG.md`: an rc21 entry.
- `src/app/overlays/menu.rs:257`: the source list text loses Spotify.
- `src/library.rs:6`, `src/visualizer.rs:4`: comments only; update if they now mislead.

- [ ] **Step 1:** Edit. Then `grep -rni "spotify" assets docs site src | grep -v CHANGELOG` → only the off-WrOS remote, the "not available yet" notes and the reason line.
- [ ] **Step 2:** Run `check.sh` (the help pages are compiled in, and the help tests check links).
- [ ] **Step 3: Commit** `rc21: help and docs for the music rework`.

### Task 8: Version, qualification, release

**Files:**
- `flake.nix`, `os/VERSION` and `os/tests/test_packaging.py`: `0.1.0-rc21`, as in rc20's 9cb7a14.
- Scripts: copy `rc14-argon/nixos-wsl/rc20-{qualify,release-build,release-post,channel-check}.sh` to `rc21-*`, with the baseline set to **rc20** (`~/delta-baselines/rc20`; the laptop runs rc20). Export `channel-src/rc20` to `rc21-release/`.
- Notes: `WrOS/rc21-release/release-notes.md`.

- [ ] **Step 1:** Bump the version; `check.sh` passes; commit `rc21: version`.
- [ ] **Step 2:** Run `rc21-qualify.sh` in the background: all VM tests minus music-signin plus media-keys, all suites, the ISO and boot checks. Check `iso-check-rc21/rufus` has pictures.
- [ ] **Step 3:** Laptop trial over side SSH, before publishing, without restarting the laptop. Install no build and touch no real book. With the owner's help:
  - On rc20 now, read `/proc/bus/input/devices` and confirm event1 is still the AT keyboard.
  - Then run a read-only Python probe on `/dev/input/event1`, filtered to codes 163/164/165, for 20 s. The owner presses play/pause, next and previous, and the probe prints each code once per press.
- [ ] **Step 4:** `rc21-release-build.sh` (WITH_ISO=1) → sign → `rc21-channel-check.sh` as an rc20 laptop → publish (pre-authorized). Root the baseline at `~/delta-baselines/rc21`.
- [ ] **Step 5: GitHub.**
  - File an issue: "Spotify can't play on WrOS: Spotify blocks unofficial players for newer accounts". Give the librespot #1649 link and the Soloist plan; no account details.
  - Comment on #122 and #162. Close #162 if the owner has heard music from Files or YouTube Music on the HP Stream.
- [ ] **Step 6:** Update the memory files `wros-current-state` and `rc21-music-rework`. Clean up per `clean-up-superseded-builds`.
