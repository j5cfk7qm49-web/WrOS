# WrOS rc13 Interface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make WrOS read as one program: a top menu bar with dropdowns replaces the Esc box menu, Settings opens as a box over the page, status splits top-right/bottom, WrOS branding replaces Grimoire's, and the graphical session draws in GNU Unifont.

**Architecture:** The bar is a new overlay state (`Overlay::Bar`) whose rows come from one table (`App::bar_rows`) that also feeds Find anything and the guard tests. The shared look (border with bold title, shaded `▸` row, lit key hints) moves into `crates/wros-shell` so the editor, Settings, login and installer draw with the same parts. The font becomes a WrOS build of Unifont with one-cell drawings added where Unifont is two cells wide, checked by a test against every symbol the interface uses.

**Tech Stack:** Rust (ratatui, soft_ratatui + cosmic-text, crossterm, xkbcommon), Python 3 (fontTools for the font build, the VM harness), Debian 13 live-build.

**Spec:** `docs/superpowers/specs/2026-11-12-rc13-interface-design.md` (approved 2026-11-12). Section and call numbers below ("call 7") refer to it.

## Global Constraints

- Branch: local `rc13-build` from `d438fe4` (tag `v0.1.0-rc12`). Do not push until the release step.
- Version file becomes `0.1.0~rc13` only in Task 17.
- `MENU` in `src/app/shell.rs`, Settings route names and row order within pages stay as they are.
- `crates/wros-shell` must not depend on the editor crate (the greeter links it).
- Nothing reads the disk, the network or a command while drawing; network reads happen off the main thread (`docs/UNIFIED_SHELL.md`).
- Screen sizes that must draw cleanly: 80×24, 100×30, 120×40, 160×50, 170×48.
- No dropdown taller than 12 rows.
- Names that stay: everything in `docs/NAMING_MIGRATION.md`, `LICENSE`, notices, the `grimoire` command, `.grimoire/` folders, the Donate screen, the credit to GrimoireTUI by Josh King and Catfinity Studios.
- Colours only from the active `Theme`; no drop shadows.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Test commands (run from Git Bash; `R` and `P` are shorthands used below):
  - `R <package> <filter>` = `MSYS_NO_PATHCONV=1 wsl.exe -u root bash /mnt/c/Users/lettuce/Documents/ChatGPT/WrOS/.audit-tools/rc12-rust-quick.sh <package> <filter>` (syncs the tree, fmt check, tests, Clippy). Copy it to `rc13-rust-quick.sh` with `rc13-power` paths in Task 0.
  - `P <modules>` = the same with `rc12-pytest.sh` (copied to `rc13-pytest.sh`).
  - If WSL crawls, check host memory first (`time wsl.exe -e echo alive` over 2 s means `wsl --shutdown`, already OK'd).

## Review Focus

1. **A machine upgraded from rc12 with a saved shortcuts file** must keep every binding it had, and a go-back to rc12 must still start. → Task 5 tests both directions.
2. **A dropdown on an 80×24 console** (Tools has 12 rows, exactly the cap; View 10) must fit or scroll around the highlight, never draw off-screen. → Task 7 `dropdowns_fit_or_scroll_at_80x24`.
3. **A writer typing fast with Super held by accident** (Super+letter) must never get the letter in the page and never open Find anything. → Task 2 `super_with_a_key_is_not_a_tap`, Task 7 `super_chord_types_nothing`.
4. **A writer's own text containing a wide-in-Unifont character** (`★` in prose, a Pomodoro flower) must not paint over its neighbours. → Task 15 `a_wide_glyph_never_paints_outside_its_cells`.
5. **The Wi-Fi read on a machine with no wireless interface or with Wi-Fi off** must show the off mark, never block, never log errors each 30 s. → Task 8 `wifi_without_an_interface_is_off`.
6. **A save that fails while the writer is in focus mode** must still show, as it does in rc12, even though the top-right marks are hidden there. → Task 8 `focus_mode_still_shows_a_failed_save_and_conflicts`.

## Builder calls to show the owner

These go beyond the spec's wording. Mention them when handing over the plan.

- **Focus mode keeps the failed-save and conflict marks** on its bottom line. Call 12 lists its short form without them, but rc12 shows them there and hiding a failed save would be a regression.
- **F10 goes to the menu bar in any saved file that leaves F10 free.** rc12 cannot read a file that names the new action, so rc13 cannot write "F10 is deliberately unbound". In practice: a writer who moved the outline off F10 in rc12 gains F10 for the bar, and in rc13 the bar's key can be moved to another key but F10 cannot be left empty.
- **Alt+letter opens menus in both graphical sessions** (native and Foot), not on the Linux console and not on a Mac, where Alt-b/Alt-f are Option-arrow word moves.
- **Test helpers read inside the box** (spec "How tests find rows"): a row is the text between the `▸` marker and the next `│`. Boxes do not clear their full rows, so the page stays visible on both sides.

---

### Task 0: Branch housekeeping

**Files:**
- Add: `docs/superpowers/specs/2026-11-12-rc13-interface-design.md`, this plan
- Create: `../../.audit-tools/rc13-rust-quick.sh`, `rc13-pytest.sh` (copies of the rc12 scripts with `rc12-power` → `rc13-power`)

- [ ] **Step 1:** `git switch rc13-build && git add docs/superpowers && git commit -m "docs: rc13 interface design and plan"`
- [ ] **Step 2:** Copy the two scripts; create `/home/builder/rc13-power` in the chroot by copying `rc12-power/target` (keeps the warm build cache).
- [ ] **Step 3:** Run `R grimoire-tui the_first_menu_is_short` → Expected: `FMT-OK`, test `ok`, Clippy `Finished`. This is the baseline.

---

### Task 1: Trial A — Unifont on screen

Decides whether the existing renderer draws Unifont crisply. If not, Task 15 draws bitmaps directly.

**Files:**
- Create: `tools/build-wros-unifont.py`, `assets/fonts/UNIFONT-SOURCE.md`, `assets/fonts/wros-narrow.hex` (empty for now)
- Modify: `crates/wros-graphics/src/lib.rs` (`TextSize::raster`)
- Output: `docs/GRAPHICAL_SESSION.md` gains a "Unifont trial" section with screenshots

**Interfaces:**
- Produces: `assets/WrOSUnifont.ttf` (family name "WrOS Unifont"); `tools/build-wros-unifont.py --source <unifont.otf> --narrow assets/fonts/wros-narrow.hex --out assets/WrOSUnifont.ttf`; `const WRITING_PX: f32 = 16.0` and `const STARTUP_PX: f32 = 16.0` in `wros-graphics`.

- [ ] **Step 1:** Download the newest Unifont release OTF from `https://unifoundry.com/pub/unifont/` into a fresh scratch dir; record version, URL and SHA-256 in `assets/fonts/UNIFONT-SOURCE.md`. If the release is not in Debian 13's `fonts-unifont`, say so there.
- [ ] **Step 2:** Write the build script with fontTools: rename family/full/PostScript names to "WrOS Unifont", then for each `U+XXXX:` line in the narrow hex file (Unifont `.hex` format, 32 hex digits = 8×16) replace that glyph's outline with pixel squares and set advance to half the em. Same pixel-square method Unifont's own OTF uses.
- [ ] **Step 3:** In a throwaway worktree only, point `TextSize::raster` at the new file with 16 px for both sizes; build the `gui-drm` app; take writing-screen screenshots with `src/shots.rs` at 1366×768 showing a bold pane title, lit key hints, an italic description and prose. The 16 px switch does not land on `rc13-build` until Task 15, after the narrow glyphs exist.
- [ ] **Step 4:** Pass condition: glyph stems are one pixel wide with no grey fringe at 100 % zoom, cells are exactly 8×16 (grid is 170×48 at 1366×768), bold and italic are distinguishable from plain. Record the result and screenshots in `docs/GRAPHICAL_SESSION.md`. If bold/italic do not read, note "mark with colour/underline" for Task 15 (call 22).
- [ ] **Step 5:** Send the screenshots to the owner (`SendUserFile`) beside a laptop console photo or description, and ask for their look. Carry on with Tasks 2–14; **only Task 15 waits on the answer.** Commit the build script, source note and doc section: `trial: draw the writing screen in Unifont`.

---

### Task 2: Trial B — a bare Super tap reaches the app

**Files:**
- Modify: `crates/wros-input/src/native.rs` (`Keyboard::key`)
- Test: same file, `mod tests`

**Interfaces:**
- Produces: on press-then-release of Super_L/Super_R with no other key between, `Keyboard::key` returns `[Event::Key(KeyEvent::new_with_kind(KeyCode::Modifier(ModifierKeyCode::LeftSuper), KeyModifiers::NONE, KeyEventKind::Press))]` on the release. A field `super_alone: bool` on `Keyboard`, cleared by `reset()` and by any other key press.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn a_bare_super_tap_is_reported_once_on_release() {
    let mut k = keyboard();
    assert!(k.key(125 /*KEY_LEFTMETA*/, true).is_empty());
    let ev = k.key(125, false);
    assert!(matches!(ev.as_slice(), [Event::Key(e)] if e.code == KeyCode::Modifier(ModifierKeyCode::LeftSuper)));
}
#[test]
fn super_with_a_key_is_not_a_tap() {
    let mut k = keyboard();
    k.key(125, true);
    assert!(k.key(30 /*KEY_A*/, true).is_empty(), "Super+letter sends nothing on WrOS");
    k.key(30, false);
    assert!(k.key(125, false).is_empty());
}
```

- [ ] **Step 2:** `R wros-input super_` → FAIL.
- [ ] **Step 3:** Implement. Super+key presses return `vec![]` (call 8: Super no longer doubles as Ctrl on WrOS); releases with Super held also return nothing. The serde wire in `linux.rs` already carries any crossterm `Event`; add a round-trip test `a_super_tap_survives_the_wire` there.
- [ ] **Step 4:** `R wros-input super_` → PASS. Then in a VM (`.audit-tools/rc12-keys-vm.sh` pattern, `sendkey meta_l`) log that the app receives the event; record in `docs/INPUT_OWNERSHIP.md`. Foot under Cage: try it; if no bare tap arrives, write "Ctrl+K is the way in under Foot" there (call 10).
- [ ] **Step 5:** Commit: `feat(input): report a bare Super tap from the native session`.

---

### Task 3: Trial C — Wi-Fi signal cost

**Files:**
- Output: `docs/GRAPHICAL_SESSION.md` section "Wi-Fi signal read"

- [ ] **Step 0:** On the laptop, check that `/proc/net/wireless` exists and has a line for the connected interface with a non-zero link field. Settings › Wi-Fi already reads through `nmcli` (`crates/wros-sys/src/linux.rs`, `active_wifi_connections`); if `/proc/net/wireless` is empty, the fallback is the existing cached `nmcli` connected/not result, never a new `nmcli` call from the gauge thread more often than every 30 s.
- [ ] **Step 1:** On the laptop over SSH (`ssh -i ~/.ssh/wros_laptop aegeorge@192.168.41.65`, read-only, nothing in the writer's HOME): time 100 reads of `/proc/net/wireless` and, if present without root, of `iw dev <if> link`. Record median and max.
- [ ] **Step 2:** Decide: `/proc/net/wireless` link-quality field → three levels if the median read is under 1 ms; otherwise connected/not only (call 13). Record. No commit beyond the doc line (fold it into Task 8's commit).

---

### Task 4: Shared look in `wros-shell`

**Files:**
- Create: `crates/wros-shell/src/look.rs`
- Modify: `crates/wros-shell/src/lib.rs` (`Theme` gains `sel: Color`, `warn: Color`; `render_menu`/`render_panel` use `look`), `src/ui.rs` (`pane_block`, `hint_line`, `hint_spans`, `key_style` delegate to `wros_shell::look`)
- Test: `crates/wros-shell/src/look.rs` tests

**Interfaces:**
- Produces (all `pub` in `wros_shell::look`):
  - `fn pane_block(title: &str, focused: bool, t: Theme) -> Block<'_>` (theme border, bold title, accent when focused)
  - `fn key_style(t: Theme) -> Style`; `fn hint_spans(text: &str, t: Theme) -> Vec<Span<'static>>` (moved `hint_parts` rules unchanged)
  - `fn highlight(t: Theme) -> Style` (bg `t.sel`, fg `t.accent`) and `const MARKER: &str = "▸ "`
  - `fn underlined_title(label: &str, letter: usize, lit: bool, t: Theme) -> Vec<Span<'static>>` (the bar's titles; letter underlined)

- [ ] **Step 1: Failing tests** — `hint_spans_light_only_the_keys` (port the existing `hint_parts` cases from `src/ui.rs` tests, assert identical output), `render_menu_uses_the_shaded_marker_row` (render to a `TestBackend`, assert the selected row starts with `▸ ` and has bg = `theme.sel`), `panel_title_is_bold`.
- [ ] **Step 2:** `R wros-shell look` → FAIL.
- [ ] **Step 3:** Move the code; `src/ui.rs` keeps thin wrappers taking `&Theme` so call sites do not change. `render_panel` draws its title with `pane_block(title, true, t)` and its hint line with `hint_spans` instead of a plain accent paragraph. Login and installer pick this up through `render_panel` (behaviour unchanged).
- [ ] **Step 4:** `R wros-shell look` and `R grimoire-tui hint` → PASS; `R wros-greeter ""` and `R wros-installer ""` → PASS (their snapshot tests may need updating for the new look; update and say so in the commit).
- [ ] **Step 5:** Commit: `refactor: one set of drawing parts for the editor, Settings, login and installer`.

---

### Task 5: The menu-bar shortcut and saved-file compatibility

**Files:**
- Modify: `crates/wros-sys/src/shortcuts.rs`, `src/app/wros.rs` (shortcut dispatch near `ShortcutAction::Outline`), `docs/KEYBOARD_SHORTCUTS.md`
- Test: `crates/wros-sys/src/shortcuts.rs` tests

**Interfaces:**
- Produces: `shortcuts::Action::MenuBar` (serde `"menu-bar"`); `Shortcuts::default()` = F10→MenuBar, F11→Secondary, F12→Dashboard (Outline has no default); `Shortcuts::effective(&self) -> Vec<Binding>` = the file's bindings plus F10→MenuBar only if no binding uses F10 and none names MenuBar; `Shortcuts::to_saved(&self) -> Shortcuts` drops a MenuBar binding equal to the code default (F10). The F10-when-free rule is a builder call shown to the owner (see "Builder calls"); the Keyboard page offers "move" for the bar key but not "unbind" while F10 would be re-added.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn no_file_gives_f10_to_the_menu_bar() // default().effective() has F10→MenuBar, no Outline
#[test] fn an_rc12_file_keeps_f10_on_the_outline() // parse rc12 default JSON (F10 outline, F11, F12) → effective() has F10→Outline and no MenuBar
#[test] fn saving_the_default_bar_key_writes_nothing_rc12_cannot_read() // serde_json of default().to_saved() contains no "menu-bar"; it parses with an RC12_ACTIONS fixture enum copied from d438fe4
#[test] fn a_bar_key_the_writer_chose_is_saved() // bind F9→MenuBar; to_saved() keeps it
```

- [ ] **Step 2:** `R wros-sys shortcuts` → FAIL.
- [ ] **Step 3:** Implement; the app dispatches `ShortcutAction::MenuBar` to `App::open_bar(None)` (Task 7; until then `open_menu`). Settings › Look and keyboard › Keyboard lists "Open the menu bar" as a bindable action. Update `docs/KEYBOARD_SHORTCUTS.md` (F10 = menu bar; Outline unbound by default).
- [ ] **Step 4:** `R wros-sys shortcuts` → PASS.
- [ ] **Step 5:** Commit: `feat: F10 opens the menu bar; saved rc12 shortcuts keep their keys`.

---

### Task 6: The menu table

One table of titles and rows. No drawing yet; the old box menu still draws from it so nothing is unreachable mid-way.

**Files:**
- Create: `src/app/bar.rs` (model only)
- Modify: `src/palette.rs` (`Action`, remove `Sub` and `Action::Submenu`/`MenuBack` at the end of Task 7), `src/app.rs` (`menu()`/`submenu()` deleted in Task 7), `src/app/overlays/menu.rs` (`about_row` gains rows for new actions)
- Test: `src/menu_map_ui_tests.rs`, `src/ui_tests.rs`

**Interfaces:**
- Produces:
  - `pub enum Title { Wros, File, Edit, View, Book, Tools, Settings, Help }` with `Title::ALL`, `fn label(self) -> &'static str`, `fn letter(self) -> char` (W F E V B T S H; index 0 of the label)
  - `pub enum Side { Revision, Notes, Practice }`
  - `pub struct BarRow { pub label: String, pub key: String, pub action: Action, pub dim: Option<&'static str> /* why it can't run now */ }`
  - `impl App { pub fn bar_rows(&self, title: Title) -> Vec<BarRow>; pub fn side_rows(&self, side: Side) -> Vec<BarRow>; pub fn bar_path(&self, action: &Action) -> Option<String> /* "Tools › Notes" */ }`
  - New `Action` variants: `Undo, Redo, Cut, Copy, Paste, SelectAll, Lock, Sleep, Restart, ShutDown, InstallToDisk, Side(Side), SettingsGroup(SettingsGroup)`; `Action::Dashboard` keeps its name, label becomes "Overview". `pub enum SettingsGroup { Writing, LookAndKeyboard, System, BackupsAndAccounts, UpdatesAndRecovery }` mapping to the existing five `Route`s.
- Rows: exactly the spec's "The menus, row by row" lists, in that order, with rc12's conditions (`retain` rules from `submenu()`), shortcut labels as today. "Quit" only where `deck.quit_offered`; "Install WrOS to disk" only on live media; Sleep dim with its existing reason in the graphical session; "Settle conflicts…" only with conflicts. Power rows run the same handlers as Settings › System › Battery and power.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn the_bar_has_eight_titles_in_order() // Title::ALL labels == ["WrOS","File","Edit","View","Book","Tools","Settings","Help"], letters unique
#[test] fn every_rc12_menu_row_has_a_place_on_the_bar() // RC12_ROWS: the Action list from d438fe4's menu()+submenu() for all Subs, minus FindAnything, Settings group rows, Dashboard, SystemSettings, Settings, Submenu/MenuBack; each appears in some bar_rows/side_rows with every condition satisfied (scene open, music on, conflicts present, in trash)
#[test] fn every_rc12_settings_group_row_lives_in_wros_settings() // each row of d438fe4's Sub::Settings (music on/off, music source, spellcheck, spelling language, tree icons, line width, typewriter, reading voice, manuscript look, author details) maps to a Settings route row; Workspaces, Themes and Toggle outline map to View rows
#[test] fn no_dropdown_is_taller_than_twelve_rows() // for each Title and Side, with everything enabled
#[test] fn every_bar_row_says_what_it_does_and_has_a_help_topic() // replaces every_menu_row_says_what_it_does_and_has_a_help_topic: about_row non-empty and topic exists in assets/help
#[test] fn nothing_in_ctrl_k_is_missing_from_the_bar() // adapts nothing_in_ctrl_k_is_missing_from_the_menu
#[test] fn every_bar_row_is_in_find_anything() // the reverse direction (spec "done": reachable from Find anything); Side and SettingsGroup rows excepted
#[test] fn bar_path_names_the_menu() // bar_path(&Action::Pinboard) == Some("Tools › Notes")
```

- [ ] **Step 2:** `R grimoire-tui bar` → FAIL.
- [ ] **Step 3:** Implement the table and the new actions (`Undo`…`SelectAll` call the editor's existing undo/redo/clipboard/select-all functions and appear in the palette). `the_first_menu_is_short_and_the_rest_is_one_step_down` becomes `the_bar_titles_are_short_and_tools_has_three_side_groups`; `the_palette_runs_the_same_actions` keeps its assertion against `bar_rows`.
- [ ] **Step 4:** `R grimoire-tui bar` and `R grimoire-tui palette` → PASS.
- [ ] **Step 5:** Commit: `feat: the menu bar's titles and rows, one table for the bar and Find anything`.

---

### Task 7: The bar on screen: drawing, keys, mouse, Super; the box menu goes

**Files:**
- Modify: `src/app/bar.rs` (state + keys), `src/app.rs` (`Overlay::Bar`, `escape()`, remove `Overlay::Menu`/`Sub`, `menu()`, `submenu()`, `open_menu`), `src/app/overlays/menu.rs` (delete; move `about_row` into `bar.rs`), `src/app/overlays/mod.rs` (dispatch), `src/ui.rs` (`draw`: top line reserved), `src/main.rs` (Alt+letter, F10, Super tap, Super chords), `src/test_driver.rs` (`menu()` reports the bar), `src/app/overlays/command.rs` (Find anything under the bar, menu path column)
- Test: `src/menu_map_ui_tests.rs`, `src/ui_tests.rs`

**Interfaces:**
- Consumes: Task 6 table, Task 4 `look`, Task 2 `KeyCode::Modifier(LeftSuper)`.
- Produces:
  - `Overlay::Bar { title: Title, open: bool, sel: usize, side: Option<(Side, usize)>, scroll: usize }`
  - `App::open_bar(title: Option<Title>)` (None = last used, starting on File; `open: false`), `App::open_bar_with_find()` (Super tap: lit bar + `Overlay::Palette { under_bar: true, .. }`)
  - `App::bar_description(&self) -> Option<(String, &'static str)>` (the highlighted row's line and topic; status bar shows it while a menu is open)
  - `ui::draw` lays out `[bar(1), main, status(1)]` when not in focus mode; in focus mode `[main, status]` and the bar draws over the top line only while `Overlay::Bar` is up.
- Keys (calls 1, 2, 5, 6, 7, 8): Esc runs `escape()` (selection → notebook entry → scene beside → `open_bar(None)`); Esc/F10 with the bar up closes it (Esc inside an open side group steps back one level first). Lit bar: ←→ move (no h/l: H is Help), ↓/Enter open, letter opens its menu. Open dropdown: ↑↓/j/k/Home/End via `list_nav(.., RING)` (j/k stay nav keys, so no row may start with J or K), → on a `Side` row opens it, ← closes a side group or moves to the previous title, Enter/Space runs (setting toggles keep the menu open as today), a letter jumps to the next row starting with it, `?` opens the row's help topic. Alt+letter opens that menu directly when `!app.linux_console && !cfg!(target_os = "macos")` (native and Foot sessions; `App::linux_console` exists today); on the console and on a Mac, Alt-b/Alt-f/Alt-d keep their word moves. A bare Super tap arrives only from the native input process (`wros_input::Input::is_native()`). Super tap → `open_bar_with_find()`; in that box ←→ close it and open the first/last dropdown, Esc or another Super tap closes. Super chords never reach the page and Super no longer counts as Ctrl except `cfg!(target_os = "macos")`; `mod_label()` returns `"^"` except on macOS.
- Mouse: click a title opens it; motion over titles while open switches; click a row runs it; click elsewhere closes. Hit rects in `App` like `view_hits`.
- Drawing: bar line in theme `border`/`dim`, titles via `look::underlined_title`, lit title with `look::highlight`. Dropdown under its title: `pane_block` border, rows `label` + right-aligned `key`, `▸` marker on the highlight, dim rows in `t.dim` with no marker action. Side group opens to the right (to the left if no room). Height = min(rows, screen − 3, 12); scroll keeps the highlight visible. Top right of the bar line is left free for Task 8.

- [ ] **Step 1: Failing tests** (TestDriver at 120×40 unless named)

```rust
#[test] fn esc_lights_the_bar_on_file_without_opening_it()
#[test] fn esc_then_b_opens_book_on_the_console_too() // linux_console = true
#[test] fn esc_then_h_opens_help() // h is a title letter, not a move
#[test] fn no_row_starts_with_j_or_k() // every Title and Side, everything enabled
#[test] fn esc_clears_a_selection_before_opening_the_bar()
#[test] fn f10_opens_and_closes_the_bar()
#[test] fn alt_v_opens_view_in_the_graphical_session_only() // graphical=false: Alt-v does nothing to the bar
#[test] fn tools_revision_opens_to_the_side_and_left_comes_back()
#[test] fn a_letter_in_a_dropdown_jumps_to_its_row()
#[test] fn the_highlighted_rows_description_is_on_the_status_line()
#[test] fn dim_rows_cannot_be_chosen_and_say_why() // Sleep in graphical session
#[test] fn dropdowns_fit_or_scroll_at_80x24() // every Title/Side: all cells inside the screen, highlight visible after End
#[test] fn the_bar_costs_one_page_line_and_focus_mode_hides_it()
#[test] fn a_super_tap_opens_find_anything_under_the_lit_bar()
#[test] fn left_in_find_anything_opens_the_last_menu()
#[test] fn super_chord_types_nothing() // Super+a in the editor: buffer unchanged, no overlay
#[test] fn find_anything_names_each_rows_menu() // result for "Pinboard" line ends with "Tools › Notes"
#[test] fn clicking_a_title_opens_it_and_clicking_away_closes()
```

- [ ] **Step 2:** `R grimoire-tui bar` → FAIL.
- [ ] **Step 3:** Implement; delete the box menu code and every `Overlay::Menu`/`Overlay::Sub`/`Sub`/`Action::Submenu`/`MenuBack` reference (compiler-guided). `test_driver::menu()` returns `(title_label, row_label, sel)` for `Overlay::Bar`. Remove bottom-bar key hints (`app.hints()`/`fit_hints` usage in `draw_status`; spec §4).
- [ ] **Step 4:** `R grimoire-tui ""` (whole crate) → PASS; fix tests that drove the old menu by moving them to the bar (do not delete what they guard).
- [ ] **Step 5:** Commit: `feat: a menu bar with dropdowns replaces the Esc box menu`.

---

### Task 8: Status — top right and the bottom bar

**Files:**
- Modify: `src/ui.rs` (`draw_status` → `draw_status_bottom`; new `draw_status_top`), `src/gauges.rs` (Wi-Fi, symbols, brightness flash), `src/backup_service.rs` / `src/sync_service.rs` (read-only accessor)
- Create: `src/wifi.rs`
- Test: `src/ui_tests.rs`, `src/gauges.rs` tests, `src/wifi.rs` tests

**Interfaces:**
- Produces:
  - `wifi::read(proc_net_wireless: &str) -> Wifi` (pure parser) and `pub enum Wifi { Off, Connected, Bars(u8 /*1..=3*/) }`; the `gauges::watch()` thread re-reads `/proc/net/wireless` at most every 30 s (Task 3 decides Bars vs Connected).
  - `Gauges::items(&self, console: bool) -> Vec<(GaugeItem, String)>` in drop order `[Battery, Wifi, Volume, Clock]` reversed for display; `GaugeItem { Wifi, Volume, Battery, Clock, Brightness }`; brightness only for 4 s after `/run/wros-keys/changed` reports a brightness change.
  - `App::backup_mark(&self) -> Option<BackupMark>`; `pub enum BackupMark { Done(String /*HH:MM*/), Running, Waiting, Failed }` from the existing backup/sync service status, read from memory only.
- Graphical symbols (all must be one cell in Task 15's font): Wi-Fi `▂▄▆` (lit bars in text, unlit in dim) / off `▂̸` → use `✗` if the combining slash is not one cell; volume `♪` + percent; battery `▮` + percent; saved `✓` dim; saving `○`; waiting `◌`; not saved `●` warn; backup `⇅` + time / `⇅…` running / `⇅◌` waiting / `⇅!` failed in warn; conflicts `⚠ N`. Console (call 14): symbol where the console font has it, else today's worded form.
- Layout (call 12): bottom = book total / target, bar, today, sprint, message … Wi-Fi, volume, battery, clock. On narrow screens drop clock, then volume, then Wi-Fi; battery last; the message is never shortened for them. Focus mode keeps its short form plus low battery, and (builder call) the failed-save `●` and conflict `⚠` marks, since the top-right marks are hidden there.

- [ ] **Step 1: Failing tests** — `wifi_parses_link_quality_into_three_bars` (fixture lines from a real `/proc/net/wireless`), `wifi_without_an_interface_is_off` (header-only file and empty string), `status_top_right_shows_save_then_backup`, `a_failed_backup_stays_in_warning_colour`, `narrow_screens_drop_clock_then_volume_then_wifi_and_keep_the_message` (80 cols with a 60-char message), `brightness_shows_briefly_after_its_key`, `focus_mode_still_shows_a_failed_save_and_conflicts`, `nothing_in_status_drawing_reads_the_disk` (draw with HOME pointed at an unreadable dir; same output).
- [ ] **Step 2:** `R grimoire-tui status` → FAIL.
- [ ] **Step 3:** Implement. The top-right items draw on the bar line, right-aligned, and stay visible whether or not the bar is lit.
- [ ] **Step 4:** `R grimoire-tui status` and `R grimoire-tui wifi` → PASS; render screenshots at all five sizes with `src/shots.rs`.
- [ ] **Step 5:** Commit: `feat: save and backup at top right; words, progress and gauges in symbols at the bottom`.

---

### Task 9: Settings as a box over the page

**Files:**
- Modify: `src/app/shell.rs` (`draw`), `src/app/wros.rs` (`draw` passes the page area), `src/app/bar.rs` (`SettingsGroup` opens the box on that page), `os/tests/*` row helpers only if Step 4 finds one that breaks
- Test: `src/shell_ui_tests.rs`

**Interfaces:**
- Produces: `fn settings_box(screen: Rect, rows: usize, widest: usize) -> Rect` in `shell.rs`: width = clamp(widest + 6, 56, screen.width − 4), height = clamp(rows + 5, 10, screen.height − 2), centred under the bar; full screen for `Route::LinkPhone` and whenever the screen is narrower than 70 or shorter than 20. The page stays visible on all four sides. **Test helpers read inside the box** (spec "How tests find rows": the chosen rule, applied everywhere, dropdowns included): a selected row is the text after `▸` up to the next `│`.
- Helper changes in this task: `selected_row` in `os/tests/rc_vm_phases.py` cuts at the next `│` after `forms.MARK`; `choose` in `os/tests/test_backup_pty.py` (and any copy of it: `grep -rn "def choose\|'▸'" os/tests tests`) scopes its "nothing above the first option" check to rows between the box's top border `┌`/`╭` and the marker, and to the box's columns. Rust tests use `test_driver` state, not screen text.
- `App::open_settings_at(group: SettingsGroup)` opens `Overlay::Deck(Page::Settings)` with `nav` reset to the group's route. Esc steps back inside the box to the Settings root, then closes to the page.

- [ ] **Step 1: Failing tests** — `settings_opens_as_a_box_with_the_page_visible_around_it` (120×40: rows above the box contain page text), `settings_covers_the_screen_at_80x24_only_if_it_must`, `the_phone_link_page_may_take_the_whole_screen`, `settings_menu_rows_open_their_group` (each `SettingsGroup` lands on its route), `the_settings_map_is_unchanged` (MENU snapshot equals d438fe4's), `battery_and_power_is_still_the_third_row_under_system`.
- [ ] **Step 2:** `R grimoire-tui settings_` → FAIL.
- [ ] **Step 3:** Implement with `wros_shell::render_menu` inside the box (Task 4 look).
- [ ] **Step 4:** `R grimoire-tui shell_` → PASS; `P` on the terminal (PTY) tests that open Settings → PASS at 80×24, 100×30, 120×40, 160×50, 170×48.
- [ ] **Step 5:** Commit: `feat: Settings opens as a box over the page`.

---

### Task 10: Overview

**Files:**
- Modify: `src/app/wros.rs` (`Page::Home` rows, heading, `home_actions`), `src/app/bar.rs` (WrOS › Overview runs `Action::Dashboard`)
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Failing tests** — `overview_keeps_tiles_and_configure_widgets_only` (no Resume/Goals/Progress/Workspaces/Settings/Tutorial rows), `overview_on_live_media_selects_install_first`, `overview_title_says_overview` (heading "Overview", no "WrOS dashboard" text anywhere on screen).
- [ ] **Step 2:** `R grimoire-tui overview` → FAIL.
- [ ] **Step 3:** Implement; F12 default stays on `Dashboard`.
- [ ] **Step 4:** `R grimoire-tui overview` → PASS.
- [ ] **Step 5:** Commit: `feat: the F12 dashboard becomes Overview under the WrOS menu`.

---

### Task 11: Pins sidebar

**Files:**
- Modify: `src/app/pins.rs` (selection/scroll in memory; Esc; marker; help), `crates/grimoire-core/src/pins.rs` (no API change unless a flush helper is needed), `docs/PINS.md`
- Test: `src/app/pins.rs` tests or `src/ui_tests.rs`

**Interfaces:**
- Produces: `struct PinsView { sel: usize, scroll: u16, dirty: bool }` on `App`; `App::flush_pins_view()` writes selection/scroll through `Store::update` only when `dirty`, called on leaving the pane, opening another overlay, and quit. Real changes (add, remove, reorder, recolour, resize, scope, anchor) still write at once through the existing guarded path.

- [ ] **Step 1: Failing tests** — `moving_through_the_pins_sidebar_writes_nothing` (20 ↓, 5 wheel steps, 3 clicks: `pins.json` bytes and mtime unchanged), `leaving_the_sidebar_saves_its_place_once`, `esc_in_the_pins_sidebar_hides_it`, `esc_in_the_editor_keeps_the_sidebar`, `j_and_k_move_pins` (via `list_nav(.., LINES)`), `pgup_pgdn_still_scroll_the_card_and_home_returns_to_the_anchor`, `the_pins_marker_is_the_shaded_triangle` (`▸`, `look::highlight`), `pinboard_shows_a_few_keys_and_the_rest_behind_question_mark` (footer ≤ 1 line, `?` opens the full list).
- [ ] **Step 2:** `R grimoire-tui pins` → FAIL.
- [ ] **Step 3:** Implement; the title and messages in the plain voice ("Pinboard", "Pinned.", "Couldn't pin this: …").
- [ ] **Step 4:** `R grimoire-tui pins` and `R grimoire-core pins` → PASS.
- [ ] **Step 5:** Commit: `fix: the pins sidebar moves in memory, closes on Esc and looks like the other panes`.

---

### Task 12: Shared list keys in the added screens

**Files:**
- Modify: key handlers in `src/app/checks.rs`, `lexicon.rs`, `spark.rs`, `productivity.rs`, `journal.rs`, `src/app/overlays/*` for scratchpad/handoff, `continuity.rs`, `milestones.rs`, `review_notes.rs`, `prose_dashboard.rs`; `list_nav` and the `Nav` presets become `pub(crate)` in `src/app/overlays/mod.rs`
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Failing test** — `every_added_list_moves_with_the_shared_keys`: table of (open action, preset) for each screen that shows a list; for each, ↓/j moves down one, ↑/k up one (letters only where the preset allows), Home/End to the ends and PgUp/PgDn by ten where `pages`. Typed-filter lists use `TYPED`. Screens that must differ carry a `// differs: …` comment and are listed in the test with the reason.
- [ ] **Step 2:** `R grimoire-tui every_added_list` → FAIL.
- [ ] **Step 3:** Route each selection change through `list_nav`. No visual change.
- [ ] **Step 4:** `R grimoire-tui ""` → PASS.
- [ ] **Step 5:** Commit: `feat: the added screens move through lists with the shared keys`.

---

### Task 13: Branding

**Files:**
- Modify: `src/ui.rs` (remove `BOOK_ART`, `draw_book_art` and its call in `draw_left`), `src/app/overlays/about.rs` (About WrOS: WrOS logo from `wros_shell::logo`, credit "Built on GrimoireTUI by Josh King and Catfinity Studios", "MIT License"), every user-visible string naming Grimoire (`grep -rn "Grimoire" src crates/grimoire-core/src crates/grimoire-app` excluding comments, CLI help, `docs/NAMING_MIGRATION.md` names, License, Donate)
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Failing tests** — `no_grimoire_name_on_a_writer_screen`: open every bar row's action plus every Settings page at 120×40 and assert no screen contains "Grimoire" except About, License and Donate; `about_credits_grimoiretui_and_names_mit`; `the_outline_has_no_book_art` (tallest outline at 160×50 has no `☾`).
- [ ] **Step 2:** `R grimoire-tui grimoire_name` → FAIL.
- [ ] **Step 3:** Replace wording ("outside Grimoire" → "outside WrOS", "Quit Grimoire" → "Quit", etc.).
- [ ] **Step 4:** `R grimoire-tui ""` → PASS.
- [ ] **Step 5:** Commit: `feat: WrOS where a writer sees the name; the spellbook art leaves the outline`.

---

### Task 14: Help pages, tutorial and documents follow the bar

**Files:**
- Modify: the 17 files in `assets/help/` that name `Esc ›` paths, `crates/grimoire-core/assets/wros/tutorial.json` (50 lessons), `docs/*.md` that name menu paths, `src/tutorial_ui_tests.rs`
- Test: `src/ui_tests.rs`, `src/tutorial_ui_tests.rs`

**Interfaces:**
- Path notation everywhere: `Title › Row` or `Title › Side › Row` (e.g. "Tools › Notes › Pinboard…"), Settings paths "Settings › System › Battery and power".

- [ ] **Step 1: Failing test** — `every_path_in_help_and_the_tutorial_is_a_real_menu_path`: extract every `X › Y(› Z)` from help pages and lesson text; each resolves through `bar_rows`/`side_rows` or the Settings `MENU`; no text contains `Esc ›`.
- [ ] **Step 2:** `R grimoire-tui real_menu_path` → FAIL.
- [ ] **Step 3:** Rewrite the paths (and lesson steps that press keys through the old menu).
- [ ] **Step 4:** `R grimoire-tui tutorial` (all 50 lessons complete by doing what they teach) and `R grimoire-tui real_menu_path` → PASS.
- [ ] **Step 5:** Commit: `docs: help, tutorial and documents name the new menus`.

---

### Task 15: The font swap

**Files:**
- Modify: `assets/fonts/wros-narrow.hex` (one-cell drawings), `assets/WrOSUnifont.ttf` (built), remove `assets/WrOSMono.ttf` and `assets/DEJAVU-LICENSE` once nothing references them, `crates/wros-graphics/src/lib.rs`, the Foot config under `os/` (follows: "WrOS Unifont" 12pt = 16 px), live-build package lists if the font is installed system-wide, `docs/CONTENT_LICENSES.md`, `docs/content-licenses.json`, `docs/software-licenses.json`, `assets/fonts/UNIFONT-SOURCE.md`
- Test: `crates/wros-graphics/tests/font_widths.rs`

**Interfaces:**
- Consumes: Task 1 build script.
- Narrow drawings for at least: `⏯ ❚ ✓ ⚠ ★`, `ⓘ ⌖ ❡ ❧ ✎`, the 13 Pomodoro/visualizer decorations (lookalikes allowed), and every Task 8 status symbol Unifont draws two cells wide.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn every_interface_symbol_is_as_wide_as_the_layout_gives_it()
// collect every non-ASCII char from string literals in non-test sources under src/
// and crates/*/src (skip *_tests.rs files and everything after a `#[cfg(test)]` line),
// assets/help/*.md and tutorial.json; for each BMP char, unicode_width(c) cells * 8 px
// == advance of c in assets/WrOSUnifont.ttf (ttf-parser), and the glyph exists.
// Chars above U+FFFF are listed in ASTRAL_ALLOWED with the reason (they show the
// missing-character box, call 21); a new one fails the test until listed.
#[test] fn a_wide_glyph_never_paints_outside_its_cells()
// rasterise "a★b" with a font lacking the narrow ★: pixels of 'a' and 'b' cells identical to "a b".
#[test] fn the_writing_grid_is_170_by_48_at_1366_by_768()
```

- [ ] **Step 2:** `R wros-graphics font` → FAIL (wide symbols listed).
- [ ] **Step 3:** Draw the narrow glyphs in hex; rebuild; clip each glyph to its cells in the raster adapter (patch the vendored soft_ratatui draw path or clip in `wros-graphics`). Startup and login: 16 px (Unifont is crisp only at multiples of 16). Bold: if Task 1 found it unreadable, draw bold as accent colour; italic as underline (call 22).
- [ ] **Step 4:** `R wros-graphics font` → PASS. Update licenses (Unifont: SIL OFL 1.1, or GPL-2.0-or-later with the font-embedding exception). Measure graphical session RSS before/after in a VM; record beside the 39 MiB baseline in `docs/GRAPHICAL_TRIAL.md`.
- [ ] **Step 5:** Commit: `feat: the graphical session draws in WrOS Unifont`.

---

### Task 16: Issue 123 — spelling "next"

**Files:**
- Modify: `src/app/overlays/spelling.rs`, `App::spelling()` in `src/app.rs`
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Failing test** — `f8_next_reaches_the_second_misspelling`: scene line `"Teh cat sat on teh mat"`, caret at start, open the spelling box, press F8 twice: first box is on word 1, second on word 5 (`teh` at byte 15).
- [ ] **Step 2:** `R grimoire-tui f8_next` → FAIL (reopens word 1).
- [ ] **Step 3:** Fix: "next" searches from the end of the current word and `spelling()` treats a caret exactly at a word's end, arrived at by "next", as past it (spec call 24's cause; confirm by reproducing first).
- [ ] **Step 4:** `R grimoire-tui f8_next` → PASS.
- [ ] **Step 5:** Commit: `fix: F8 in the spelling box goes to the next misspelt word (#123)`; comment on issue 123 per memory rules.

---

### Task 17: Harness, full pass, VM chain, release

**Files:**
- Modify: `os/tests/rc_vm_phases.py` (`grim_open_settings`, `grim_dashboard`, any `choose` over the old Esc menu), PTY test helpers under `tests/`, `VERSION`/version file, `CHANGELOG.md`
- Scripts: `.audit-tools/rc13-*.sh` copied from the rc12 release scripts

- [ ] **Step 1:** `grim_open_settings`: if the screen shows the rc13 bar (`WrOS  File  Edit` on line 1) press Esc, `s`, Enter, then Esc once to reach the Settings root; else the rc12 dashboard path. Same two-path rule for every helper that went through the Esc menu or the dashboard's launcher rows (`grep -n "choose\|grim_dashboard\|key('esc')" os/tests/*.py`). Dropdown rows are reached by keys (Esc, title letter, row letter) or by Find anything, never by scanning a dropdown with `choose`. Row reading inside boxes comes from Task 9's helper change.
- [ ] **Step 2:** Full pass: `rc13-complete-pass.sh` (Rust all, Clippy default + no-default, Python all, PTY at 80×24/100×30/120×40/160×50/170×48). Expected: everything passes except failures that also fail on the rc12 baseline (record which).
- [ ] **Step 3:** Release build with `audio`; `tools/collect-update-runtime.py` against rc12's installed status (from the rc13 start disk `/home/builder/rc-vm-runs/rc-5leyszx2/p-9rl5lv86/update-ok-20261113T020123Z/disk.qcow2`); sign; check bundle.
- [ ] **Step 4:** VM chain from the rc12 disk (`rc13-vm-chain.sh`: update-ok, features, bootscreen, plus a go-back to rc12 and confirm it starts with the shortcuts file rc13 left). Expected: all PASS.
- [ ] **Step 5:** Version `0.1.0~rc13`, CHANGELOG, commit, push `rc13-build`, fast-forward `development`/`main`, tag `v0.1.0-rc13` at the full SHA, `gh release create v0.1.0-rc13 -R j5cfk7qm49-web/WrOS ...`, dispatch `release-updates.yml`, check the live Packages hash, close #123 and comment on related issues. (Publishing is pre-authorized for tested rc builds.)
- [ ] **Step 6:** Owner checks to list in the release notes: font look, Super tap, Wi-Fi bars, brightness and volume keys, RSS on the laptop (laptop needs rc12 first).
