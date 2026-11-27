# rc32: take upstream Grimoire 0.8–0.9, then clean up

## Why

The build and test pipeline was trimmed in rc31. The owner wants the code to get the same
treatment: follow good Rust and ratatui practice, and drop what's redundant. They have no single
pain point in mind. Upstream GrimoireTUI, which WrOS is built on, has also released four versions
since WrOS's base (0.7.5): 0.8.0–0.8.3 are a careful pass with about fifty fixes, and 0.9.0–0.9.1
add a books folder and saving a book as one file. The owner wants WrOS to take what it can from
upstream first, with intentional differences written down, and then clean up. Anything egregious
found upstream becomes an issue or PR on GrimoireTUI. Its text is shown to the owner before
posting.

## Goals

1. **Merge upstream** to v0.9.1 (upstream/main as of 2026-11-17, cf6415c).
   - Take every fix.
   - Take the features that fit WrOS.
   - Adapt or decline the ones that don't, each with its reason in `docs/UPSTREAM.md`.
2. **Hygiene across the codebase:**
   - workspace-wide lints;
   - no `unwrap`/`expect` that real data can reach outside tests;
   - explicit number conversions in WrOS drawing code;
   - Debian-era leftovers removed;
   - duplicate direct dependencies unified.
3. **Restructure WrOS's own code** (files that did not exist in v0.7.5, and the `wros-*` crates)
   along ratatui and Rust lines:
   - components that implement `Widget` / `StatefulWidget`;
   - smaller modules;
   - state separate from drawing.
4. Ship as **rc32**, qualified with `qualify.sh` and published as usual.

## Non-goals

- Restructuring files shared with upstream (those in `git ls-tree v0.7.5`): no splitting of
  `src/app.rs` into components, no renames, no reformatting beyond `cargo fmt`. Only real bugs and
  the hygiene items whose fix is local.
- New WrOS features, or the open issues #173, #175, #163 and #172.
- Changing what a writer sees, except what upstream's fixes and features change on purpose.

## Anti-goals (a failure even if it works)

- A merge that throws away upstream's history, so the next upstream release conflicts over the
  same ground again.
- Any change to project files on disk. Their names (`project.toml`), their folder
  (`~/Documents/Grimoire`) and the migration code from rc22 stay as they are.
- A refactor that changes behaviour without a test saying so: the screen tests (`ui_tests.rs`,
  `shell_ui_tests.rs`, `bar_ui_tests.rs` …) must pass unchanged unless a change is intended.
- Help that falls behind: upstream's rule ("what the app can do, the help says") holds for WrOS
  too.

## Constraints

- **Toolchain:** the Rust edition (2024), toolchain and nixpkgs pin do not change. `cargo update`
  only where a merge needs it.
- **Checks:** every check in flake.nix must pass on each merged step: `nix-gates.sh`, the four
  suites, and the VM tests via `qualify.sh`. CI (`nix-ci.yml`) must stay green.
- **The release:** it runs on the HP Stream (Celeron N4000, 4 GB), so no new heavy dependencies.
  The update stays thin from the version laptops run (rc31; rc30 laptops hold rc31 only after
  they update).

## Approaches considered (owner picked A, then 1 and 2)

**Taking upstream**

- **A. Two staged merges (chosen).**
  - `git merge v0.8.3` first: the fixes and small features. Resolve, adapt, run all checks.
  - Then `git merge upstream/main`: 0.9.x.
  - Real merges move git's merge base, so later upstream releases conflict only on what's new.
- **B. Cherry-pick upstream's 57 commits.** Rejected: git wouldn't record them as taken, so every
  future merge re-conflicts.

**Cleanup**

- **1. Hygiene everywhere (chosen).**
- **2. Restructure only WrOS-owned code (chosen).**
- **3. Restructure everything, `src/app.rs` included.** Rejected: it would cut WrOS off from
  upstream in practice.

## Decisions already made

### Upstream

| Upstream change | WrOS | Why |
|---|---|---|
| 0.8.0–0.8.3 fixes and small features: caret in boxes, empty the Trash, one match at a time, read-aloud speed, menu clicks, Template Sheets, taught words, corkboard chapter, flicker fix | Take | Bugs that affect writers, and features that fit. |
| 0.9 one books folder | Adapt | `shelf::home()` returns WrOS's `~/Documents/Grimoire` (`books::default_root()`), not `Documents/Grimoire Books`. |
| 0.9 save a copy / open a copy (`.zip`) | Take, in WrOS's folder | Copies go in `~/Documents/Grimoire/Copies`. |
| 0.9 "Where this book is kept" | Take, adapted | Shows WrOS's facts: the folder, backup and Drive state where WrOS knows them. Says "project", as all WrOS text does since rc22. |
| 0.9 move a book home | Take | Into WrOS's folder. |
| 0.9.1 first start makes *My First Book* | Decline | WrOS's first start is its own (the live session, the tutorial, the projects folder). |
| `grimoire where` / `grimoire copy` commands | Take | They cost nothing. |
| `.github/workflows/ci.yml` | Keep deleted | Nix CI replaced it in rc17. |
| `site/` (upstream's website) | Take upstream's version as is | WrOS doesn't publish it; it's Grimoire's site. |
| `README.md`, `CHANGELOG.md` | Keep WrOS's | Upstream's README goes to `docs/archive/GRIMOIRE_README.md`. Upstream's notes are summarized in rc32's entry. |
| `CLAUDE.md` (the help rule) | Take | Add a line pointing to `docs/UPSTREAM.md`, and that WrOS says "project". |
| Workspace version | Becomes 0.9.1 | It records the Grimoire base. WrOS's own version stays `os/VERSION`. |
| Words in upstream's new text | Book → project | "Book" → "project" wherever WrOS's rc22 sweep would have. |

`docs/UPSTREAM.md` lists each Adapt / Decline / Keep row above, plus anything found while
resolving. It also gives the commands for the next merge.

### Cleanup

- **Shared vs WrOS-owned** is decided mechanically, against the latest upstream tag merged
  (v0.9.1 after the second merge). A file is shared if it exists in `git ls-tree -r <that tag>`;
  every other file, and the `wros-*` crates, is WrOS's. *(My call: measured against v0.7.5,
  upstream's new files such as `overlays/kept.rs` would count as ours, get restructured, and
  conflict on the next merge.)*
- **WrOS code inside shared files is not restructured.** This covers the 30 fields WrOS added to
  `App` in `src/app.rs`, and WrOS's lines in `src/music/mod.rs` and `setup.rs`. Moving them would
  edit lines upstream also edits. Only bugs and local hygiene fixes are made there. *(My call,
  from the owner's "restructure only WrOS's own code".)*
- **`crates/grimoire-core/src/shelf.rs` clashes.** WrOS has its own module at that path
  (`find_books`, `free_destination`, `same_book` …, used by import, the projects list and
  `book_copy`), and upstream 0.9 adds a different one there. During the 0.9 merge, WrOS's
  functions move to their own module, `projects_shelf.rs`, and upstream's `shelf.rs` is taken
  as is apart from `home()`. *(My call.)*
- **Workspace lints** go in a `[workspace.lints]` table, which every crate inherits:
  - `unwrap_used` and `expect_used` warn outside tests (`clippy.toml`:
    `allow-unwrap-in-tests`, `allow-expect-in-tests`);
  - `dbg_macro`, `todo` and `print_stderr` in libraries warn;
  - CI's `-D warnings` makes them errors.
  - In shared files, an unwrap that real data can't reach gets a narrowly scoped `#[expect]`
    with a reason, instead of a rewrite.
- **Casts:** `cast_possible_truncation` and `cast_sign_loss` apply to WrOS-owned drawing code only
  (module-level). Conversions become `u16::try_from(..).unwrap_or(u16::MAX)`, saturating, or a
  small helper.
- **Debian leftovers:** delete the tools, os/ and packaging files only the Debian build used.
  Proven unused by grep and by the checks passing. The Pages deploy of the old Debian channel is
  a question for the owner (below).
- **Dependencies:** unify direct dependencies that appear in two versions where a bump is free.
  Transitive duplicates (wgpu, winit …) are left.
- **Restructure (WrOS-owned):**
  - the top bar, the two bottom bars and their panels, the WrOS menu, Settings and the
    view tiles become `Widget` / `StatefulWidget` types with their state structs;
  - `src/app/shell.rs` (3,456 lines), `src/app/wros.rs` (2,576), `overlays/sources.rs`,
    `productivity.rs`, and in `music/` the WrOS-only `ytmusic.rs`, `mpv.rs` and `files.rs`, are
    split by responsibility;
  - WrOS state that already lives outside `src/app.rs` gets its update and draw code next to it.

## Left to the builder

- The order of the cleanup steps, provided each lands with all checks passing.
- How each conflict is resolved, within the decisions above. Any judgment call goes in
  `docs/UPSTREAM.md`.
- Which lints beyond those listed are worth turning on. Pedantic as a whole is not.
- Where exactly the WrOS-owned modules split.

## Open questions for the owner

1. **The old Debian channel.** The Pages deploy still serves it (`updates/`, rc13 machines).
   Is any Debian WrOS laptop left? If not, the workflow and its signing steps can go.
