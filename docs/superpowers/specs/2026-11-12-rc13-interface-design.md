# WrOS rc13: one interface

Status: approved by the owner on 2026-11-12.

Starting point: rc12 (tag `v0.1.0-rc12`, commit `d438fe4`), on the local branch
`rc13-build`. Where this document compares against "Grimoire", it means upstream
GrimoireTUI 0.7.5, commit `f3ca1e8`, the last upstream commit in this history.

## Why

The owner is ready to use WrOS daily for writing. What still feels wrong is that
WrOS is visibly two programs. Esc opens Grimoire's small menu over the page, while
WrOS Settings wipes the whole screen and draws with a different border, title,
highlight and key hints. The owner's words: "I don't really want to feel like one
exits the editor to get into settings, I want it to all feel like part of the
whole experience."

rc13 makes the menus, Settings, status and the pins sidebar read as one program,
puts the WrOS name where Grimoire's is, and switches the graphical session to the
font the owner prefers.

## Goals

1. One way into everything: a menu bar across the top with dropdowns.
2. Settings opens over the page in the same style as every other box.
3. Status the owner looks at stays visible and uncluttered.
4. WrOS branding replaces Grimoire branding where a writer sees it.
5. The graphical session looks like the console the owner prefers, with symbols
   rather than words in indicators and controls.
6. The pins sidebar behaves like Grimoire's own side panes.

## Not in rc13

- Moving off Debian. The owner said "save the rebase for later".
- Restyling the other WrOS-added screens (writing checks, dictionary, Story
  Spark, productivity, journal, scratchpad, continuity, milestones, revision
  notes, prose dashboard). They keep their look and gain shared keys only.
  Restyling them is rc14.
- Paring down to one renderer (deferred by the owner on 2026-11-12).
- Installer work.

## What would make this a failure

- Any screen where the writer feels they left the editor to change a setting.
- A menu row that worked in rc12 and cannot be reached in rc13.
- Symbols that are clipped, overlap their neighbours, or show as blanks.
- A saved Settings position, saved shortcut or book that rc12 could read and
  rc13 cannot, or that rc12 cannot read again after a go-back.

## Decided by the owner

Each item here was stated or explicitly approved by the owner on 2026-11-12.

### 1. Top menu bar

A one-line bar across the top of the screen replaces the Esc box menu
(`Overlay::Menu` and `Overlay::Sub`, drawn by `src/app/overlays/menu.rs`). Losing
one line of the page is acceptable. Focus mode hides the bar as it hides
everything else.

Titles, left to right: **WrOS, File, Edit, View, Book, Tools, Settings, Help**.
There is no Insert or Format menu.

Tools has three sub-groups that open to the side: **Revision, Notes, Practice**.

The dropdowns should look like Grimoire: the theme's border, the shaded
highlight row with `▸`, lit key letters. No drop shadows and no colours outside
the theme. The owner's reference was a classic ncurses menu bar, with the note
"I want something better than that, but akin to it."

### 2. Keys

- **Esc** opens the menu bar and closes it. Esc opens the menus only.
- **Alt + a letter** opens that menu directly in the graphical session. The
  letter is underlined in each title.
- **F10** also opens the bar. "Show or hide the outline" moves off F10.
- **Super, tapped on its own**, brings up Find anything (today's Ctrl+K box)
  together with the menu bar, "kinda like windows". Ctrl+K keeps working.
- Find anything has no menu row.

### 3. Status

- **Top right:** save state and a sync/backup indicator.
- **Bottom left:** the book's word count, the progress bar and today's words.
  These are what the owner looks at most.
- **Bottom right:** Wi-Fi, volume, battery and the clock.
- Wi-Fi shows signal strength if that can be read cheaply, and a compact
  connected/not-connected mark if it cannot.
- Indicators and controls use symbols, not words: "I would prefer useful symbols
  in all cases rather than replacing them with words."

### 4. Settings, the dashboard, and other screens

- Settings pages open as a box over the page instead of taking the whole screen.
- The Settings menu keeps the five groups approved for rc9 (`docs/SETTINGS_MENU_PLAN.md`).
- The F12 dashboard becomes **Overview**, under the WrOS menu, without its
  launcher rows.
- Key hints leave the bottom bar.
- The pins sidebar is fixed to behave like Grimoire's side panes (section 8).
- The other added screens gain shared keys only (section 9).

### 5. Branding

- The spellbook art leaves the outline. "The floating book thing in the sidebar
  is currently a waste of space."
- Grimoire's name gives way to WrOS wherever a writer sees it.
- The Donate screen stays.
- The license and the credit to Grimoire's author stay. The `grimoire` command
  and the `.grimoire` folders inside books keep their names.

### 6. Font

- The graphical session switches from WrOS Mono (DejaVu Sans Mono) to **GNU
  Unifont**. The owner's laptop console uses Debian's "Fixed" 8×16 font, which
  Debian builds from Unifont, and the owner prefers it.
- Where Unifont's own drawing of a symbol is two cells wide, WrOS adds a
  one-cell version: "add the narrow unifont stuff as needed."

### 7. Bug reported

- [Issue 123](https://github.com/j5cfk7qm49-web/WrOS/issues/123): the owner
  reported that "next" in the spelling box does not work. F8 reopens the same
  word.

## The menus, row by row

The owner approved the eight titles, the three Tools sub-groups and a summary of
what each menu holds. The exact placement of each row below is mine, worked out
from that summary.

Every row in rc12's Esc menu and its groups has a place below. Rows keep their
current conditions (for example, writing rows only appear with a scene open).
Shortcut labels stay beside rows as they are today.

A row that cannot work right now but belongs on the menu (Sleep in the graphical
session is one in rc12) is shown dim, cannot be chosen, and says why in the
description line, as Settings rows already do.

**WrOS**
- Overview (F12)
- About WrOS
- Install WrOS to disk (live media only)
- Lock
- Sleep
- Restart…
- Shut down…
- Quit (only where rc12 offers "Quit Grimoire")

The power rows trigger the same actions and confirmations as the rows on
Settings › System › Battery and power, which stay where they are.

**File**
- Settle conflicts… (only while there are conflicts)
- Start a new book…
- Open another book…
- Export…
- Compile to Markdown
- Update the project map
- Back up now
- Writing sessions…
- Save this session now
- Move writing history out…

**Edit**
- Undo, Redo
- Cut, Copy, Paste, Select all
- Find and replace…
- Search the whole book…
- Italic, Bold
- Insert footnote here

**View**
- Focus mode
- Outline (show or hide)
- Pins sidebar (show or hide)
- Open a scene beside…
- Look up the name here
- Second pane (show or hide)
- Choose second document…
- Corkboard…
- Themes…
- Workspaces…

**Book**
- New scene…, New chapter…, New part…, New folder…
- Rename…, Delete…, Restore from the Trash
- Move up, Move down
- Scene details…
- Scene history…
- Goals…

**Tools**
- Start a sprint… / Stop the sprint
- Start the timer / Pause the timer
- Reset the timer
- Read aloud, Read chapter aloud, Read manuscript aloud
- Music player…
- Dictionary and thesaurus…
- Writing checks…
- Revision ›
- Notes ›
- Practice ›

*Revision:* Notes and TKs…, Next TK or note, Next scene still in draft, Echo
words, Check names…, Footnote navigator…, Read footnote at caret…, Continuity
tracker…, Draft milestones…, Revision notes…, Prose and revision dashboard…

*Notes:* Pinboard…, Pin this document, Writing journal…, Scratchpad…, Session
handoff…

*Practice:* Story Spark…, Daily challenges…, Progress…, Recorded edit ledger…,
Advanced goals…, Calendar and rest days…, Advanced sprints…, Milestone
collection…

**Settings**
- Writing ›
- Look and keyboard ›
- System ›
- Backups and accounts ›
- Updates and recovery ›

Choosing a group opens the Settings box on that group's page.

**Help**
- Help… (?)
- Interactive tutorial…
- License…
- Donate…

**Rows that go away**
- "Find anything…": reached by Super or Ctrl+K.
- The Grimoire "Settings" group. Every row in it already exists inside WrOS
  Settings. "WrOS settings…" and "System settings…" are replaced by the Settings
  menu itself.
- "WrOS dashboard": replaced by Overview.
- The "Productivity tools" step: its rows sit in Tools › Practice.

No dropdown is taller than 12 rows. A dropdown taller than the screen scrolls
around the highlight, as the box menu does today.

## My calls

These are choices I made to fill gaps. The owner has not confirmed them one by
one and should check them when reading. A builder may push back on any of them.

### Menu behaviour

1. **Esc lights the bar without opening a dropdown.** The lit title is the one
   used last, starting on File. Then ←→ move, ↓ or Enter open the lit menu, and
   a letter opens the menu it is underlined in. This makes "Esc, then B" open
   Book on every screen mode, including the console, where Alt is unreliable.
   Inside an open dropdown, a letter jumps to the row starting with it.
2. **Esc keeps its earlier duties first.** As in `App::escape()` today, Esc
   clears a selection, then closes an open notebook entry or scene beside, and
   only then goes to the bar.
3. **The highlighted row's description moves to the bottom bar.** The box menu
   shows a sentence about the highlighted row. With dropdowns, that sentence
   takes the bottom bar's message space while a menu is open. `?` still opens
   the help topic for the highlighted row.
4. **Edit gains Undo, Redo, Cut, Copy, Paste and Select all** as rows. They are
   standard and they teach their keys. They become ordinary actions, so they
   also appear in Find anything.
5. **Mouse:** click a title to open it, move across titles to switch, click a
   row to run it, click anywhere else to close.
6. **In focus mode** the bar is hidden. Esc draws it over the top line until it
   is closed.

### Super and Find anything

7. **Super, tapped alone, opens the Find anything box just under the bar and
   lights the bar.** Typing searches; ↑↓ move through results; ←→ close the box
   and open the first or last dropdown; Esc or a second Super tap closes it.
   The find box ignores ←→ today, so those keys are free.
8. **On WrOS, Super stops doubling as Ctrl**, and the `⌘` shortcut label is
   never shown. Super held with another key does nothing and never types that
   key's letter into the page. Builds that run on a Mac keep Cmd as they have it.
9. **Each Find anything result names its menu on the right** ("Tools › Notes"),
   so searching teaches where things live. Drop this if it is costly.
10. **Super works in the native graphical session only.** The console cannot see
    the key. The Foot under Cage session is unchecked; if it cannot report a
    bare tap, Ctrl+K remains the way in there.

### Status

11. **Top right, in order:** the save mark, then the backup indicator.
    - Save mark: steady and dim when saved; the existing saving, waiting and
      not-saved marks otherwise.
    - Backup indicator has four states: backed up (with the time), backing up,
      waiting (offline, queued), and failed. Failed uses the warning colour and
      stays until dealt with.
    - A conflict warning also appears here while conflicts exist.
12. **Bottom bar, left to right:** book total and target, progress bar, today's
    words, a running sprint, the one-line message; then at the right Wi-Fi,
    volume, battery, clock.
    - Focus mode keeps its current short form: scene name, its words, today, and
      a low battery only.
    - Brightness is not shown permanently. It appears for a few seconds after a
      brightness key is pressed.
    - On a narrow screen the right-hand items drop in this order: clock, volume,
      Wi-Fi. Battery goes last. The message is never truncated to make room for
      them.
13. **Wi-Fi signal is read in the background, never while drawing.** At most
    once every 30 seconds, for the connected network only, without triggering a
    scan. Three bar levels plus an off mark. If the read proves costly on the
    laptop, fall back to connected or not connected.
14. **The console fallback shows a symbol where its font has one and a word
    where it does not.** The console font holds only a few hundred characters,
    so the symbols-everywhere rule can only be fully met in the graphical
    session. The existing worded battery and volume readout is the console form.

### Settings and Overview

15. **The Settings box is centred, sized to its page, and leaves the page
    visible around it when there is room.** On an 80×24 console it may cover
    nearly everything. Pages that need the whole screen (the phone-link QR code
    is one) may still take it.
16. **Overview keeps three things:** its tiles, "Configure widgets", and on live
    media the "Install WrOS to disk" row, still selected first. Resume, Goals,
    Progress, Workspaces, Settings and Tutorial rows go, since the bar reaches
    them.

### Branding

17. **About WrOS** credits GrimoireTUI by Josh King and Catfinity Studios, names
    the MIT license, and shows the WrOS logo where the spellbook was.
18. **Wording:** "outside Grimoire" becomes "outside WrOS" in messages, and so
    on for every string a writer sees. The command-line help keeps the
    `grimoire` command name.

### Font

19. **The rule:** every character the interface itself puts on screen is drawn
    exactly as many cells wide as the layout gives it. Add a test that walks the
    interface's symbols against the shipped font, so a new symbol cannot break
    this unnoticed.
20. **Ship WrOS's own copy of Unifont**, under its own family name as WrOS Mono
    is today, with one-cell drawings added where Unifont's drawing is two cells
    wide and the layout gives one. Unifont has every character the interface
    uses; the problem is only width. What I found:
    - Five symbols in everyday controls: play/pause `⏯`, pause bar `❚`, check
      mark `✓`, warning `⚠`, star `★`. The help pages use `✓` and `⚠` too.
    - Five of the outline's ten folder icons (`Area::icon` in
      `crates/grimoire-core/src/project.rs`): `ⓘ ⌖ ❡ ❧ ✎`.
    - Thirteen decorations in the Pomodoro scenes and visualizer (flowers and
      stars). For these, a one-cell lookalike is as good as a new drawing.
    - Login, the startup screen, the installer and `wros-shell` use none.
    
    These widths came from the copy of Unifont inside Debian's console-setup
    1.242, checked against the Rust sources, help pages and tutorial. Re-check
    against the Unifont release that gets pinned.
21. **A writer's own text** may contain a character Unifont draws wider than
    the layout allows. The one-cell drawings apply there too. Any other such
    character is squeezed into the cells it was given; it never overlaps its
    neighbours. Chinese, Japanese and Korean text is two cells wide in both the
    layout and the font, so it is unaffected. Characters Unifont lacks (most
    emoji) show the missing-character box, as they do today.
22. **Bold and italic.** Unifont has one weight and no italic. Titles and key
    hints are bold today, and row descriptions and emphasised prose are italic.
    The first trial must show both still read clearly. If they do not, mark
    them with colour or underline instead.
23. **Writing cells become 8×16**, so the laptop's 1366×768 screen fits about
    170×48 characters instead of today's 136×42.

### Bug

24. **Fix issue 123 in rc13**, with a test that presses F8 twice on a line with
    two misspellings. The cause, read from the code and not yet reproduced:
    "next" leaves the cursor just after the word, and `App::spelling()` counts
    that position as still on the word.

## Pins sidebar (section 8)

Compared with Grimoire's side panes, the sidebar differs in four ways. Fix all
four.

1. **It writes to disk on every move.** Each arrow press, click or wheel step
   goes through `Store::update` in `crates/grimoire-core/src/pins.rs`, which
   rewrites `pins.json`. Selection and scroll position should live in memory and
   be written only with a real change (add, remove, reorder, recolour, resize,
   scope, anchor) or when the pane is left or the app closes. The existing
   guards against overwriting outside changes stay.
2. **Esc does not close it.** With focus on the sidebar, Esc should hide it, as
   Esc closes the notebook entry and the scene beside. Esc with focus in the
   editor must not hide it. It reopens from View › Pins sidebar and its
   existing shortcut.
3. **Its marker differs.** Use `▸` with the shaded row, not `▶`.
4. **Pinboard's help is a wall.** Three dim lines list about 25 keys. Show the
   few most-used keys in the lit-key style and put the rest behind `?`. Give the
   title and messages the plain voice the rest of the program uses.

Selection should move with the shared list helper, so `j` and `k` work.
PgUp/PgDn keep scrolling the selected card, and Home keeps returning to the
anchor.

## Shared keys for the other added screens (section 9)

In rc12 only the footnote browser uses Grimoire's list helper (`list_nav` in
`src/app/overlays/mod.rs`); the other added screens each handle arrows
themselves, so `j`/`k`, Home/End and PgUp/PgDn work in some and not others.

For rc13, every added screen that shows a list moves its selection through
`list_nav`, with the preset that fits (a typed-filter list does not take
letters). Esc already closes or steps back everywhere and stays that way. Where
a screen must differ, say why in a comment. No visual changes to these screens.

## The shared look

The bar, dropdowns, the Settings box and key hints should be drawn by one set of
parts, and login and the installer should get the same look.

- `crates/wros-shell` already draws Settings, login and the installer
  (`render_menu`, `render_panel`). It has no dependency on the editor, and must
  not gain one: the greeter cannot link the editor.
- Grimoire's border with bold title (`pane_block`), shaded highlight row and
  lit key hints (`hint_line`) live in `src/ui.rs` today. Move them, or their
  equivalents, into `wros-shell` so both sides use the same code.
- Login and installer behaviour does not change. Their appearance will.

## Things that must not break

- **The Settings map.** `MENU` in `src/app/shell.rs`, the route names, and row
  order within pages stay as they are. Saved positions are keyed by page name,
  and an older VM test reaches Battery and power as the third row under System.
- **Saved shortcuts.** F10, F11 and F12 are remappable
  (`docs/KEYBOARD_SHORTCUTS.md`). The saved file
  (`crates/wros-sys/src/shortcuts.rs`, version 1, unknown fields refused) lists
  every binding, defaults included, so a saved "F10 shows the outline" cannot be
  told apart from a deliberate choice. The rule:
  - Opening the bar becomes a new remappable action. Its default, F10, lives in
    the code.
  - A machine with no saved file gets the new default. The owner's laptop has no
    saved file as of 2026-11-12.
  - A machine with a saved file keeps exactly what the file says, even if that
    leaves F10 on the outline. Esc still opens the bar there.
  - After a go-back, rc12 must still start with whatever rc13 saved. If rc12
    cannot read a file that names the new action, rc13 must not write that
    action unless the writer binds a key to it themselves. Test this.
  - The outline has no default key once F10 is taken (see the end).
- **Names that stay.** Everything listed in `docs/NAMING_MIGRATION.md`,
  `LICENSE`, and the notices.
- **Process boundaries** in `docs/UNIFIED_SHELL.md`: nothing reads the disk,
  the network or a command while drawing; network reads happen off the main
  thread.
- **Screen sizes.** 80×24, 100×30, 120×40 and 160×50 as tested today, plus the
  new 170×48.
- **Tests that guard intent.** Keep what each of these protects, adapting it to
  the bar:
  - `nothing_in_ctrl_k_is_missing_from_the_menu`
  - `every_menu_row_says_what_it_does_and_has_a_help_topic`
  - `the_palette_runs_the_same_actions`
  - `the_first_menu_is_short_and_the_rest_is_one_step_down`
  - `src/menu_map_ui_tests.rs`
- **The VM test harness.** `grim_open_settings` in `os/tests/rc_vm_phases.py`
  reaches Settings through the dashboard's "Settings" row, which rc13 removes.
  The rc13 release chain starts from an rc12 disk, so the harness must drive
  both the old path and the new one, as it already does for the rc9 Settings
  move.
- **How tests find rows.** The VM harness and the terminal tests pick a row by
  matching a whole screen line, for example `^System(\s*›)?$`. A box drawn over
  page text puts other text on the same line, so those matches stop working.
  Either the helpers learn to read inside the box, or the box clears its full
  rows; choose one and apply it everywhere.
- **Words that teach paths.** 17 of the 39 help pages in `assets/help/` name
  `Esc ›` paths, and the 50 tutorial lessons in
  `crates/grimoire-core/assets/wros/tutorial.json` open pages and name paths.
  All must match the new menus. Every lesson must still complete by doing what
  it teaches.

## Order of work

Three short trials come first, because each could change the plan:

1. **Unifont on screen.** Draw the writing screen in Unifont through the
   existing renderer and compare it with the console on the laptop, including
   bold titles, lit key hints and italic text. If it is not crisp, draw the
   font's bitmaps directly instead.
2. **Super tap.** The native input process sees Super press and release
   (`crates/wros-input/src/native.rs`) but sends the app nothing for a modifier
   on its own. Prove a press and release with no key between can reach the app.
3. **Wi-Fi signal cost.** Time the background read on the laptop.

Then:

4. Shared look into `wros-shell`.
5. The bar, dropdowns, keys, and the description line. Remove the box menu.
6. Status: top right and the bottom bar.
7. Settings as a box.
8. Overview.
9. Pins sidebar and shared keys.
10. Branding, the book art, help pages, tutorial, documents.
11. The font swap, added symbols, and license notices.
12. Issue 123.
13. Test harness updates, the full test pass, the VM chain from the rc12 disk,
    and release.

## Left to the builder

- How the bar and dropdowns are represented in the code (new overlay states or
  otherwise), provided the guard tests above still hold.
- The exact symbols for each indicator, chosen from what Unifont draws in one
  cell. The mockups used `▂▄▆` for Wi-Fi, `♪` for volume, `▮` for battery, `✓`
  for saved and `⇅` for backup.
- The Settings box's exact size rules.
- How the one-cell symbols are added to the font, and which Unifont release is
  pinned. Record the source and version beside the font, and update
  `docs/CONTENT_LICENSES.md` and the license inventories. Unifont is offered
  under the SIL Open Font License 1.1 and under GPL 2 or later with a
  font-embedding exception.
- Whether the Foot session's own font setting follows.
- What the startup and login screens use for a text size, since Unifont is only
  crisp at multiples of 16 pixels.

## How we will know it is done

- Every rc12 menu row is reachable from the bar, and from Find anything.
- Opening any Settings page leaves the page visible behind it when the screen
  has room.
- No Grimoire name remains on a screen a writer sees, except the credit, the
  license and the Donate screen.
- Moving through the pins sidebar writes nothing to disk.
- "F8 next" in the spelling box reaches the next misspelt word, with a test.
- The Rust and Python suites pass, the terminal tests pass at all four sizes,
  and the VM chain passes from the rc12 disk.
- Measured on the laptop and recorded: memory used by the graphical session
  before and after the font change (about 39 MiB today).
- Checked by the owner on the laptop: the font's look, the Super tap, the Wi-Fi
  bars, and the brightness and volume keys. The laptop is on rc11 today, so it
  needs rc12 installed before the rc13 update.

## Answered by the owner

F10 opens the menu bar from rc13, and the outline gets no default key for now.
It stays one step away under View › Outline and in Find anything, and a key can
be bound to it in Settings › Look and keyboard › Keyboard. Focus mode stays on
Ctrl+D.

The owner approved the calls in "My calls" on 2026-11-12.
