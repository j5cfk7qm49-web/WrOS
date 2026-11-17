# Book shelf and Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** File › Open another book… lists every book under `~/Documents` with no typed paths, and
one Import screen brings a book in from Google Drive, a USB stick, a laptop folder or a draft,
copying it into `~/Documents`, checking it, and opening it.

**Architecture:** Book discovery, destination naming, same-book matching, the verified copy and
the merge live in `grimoire-core` (pure, testable on disk fixtures). The app adds a rebuilt
shelf and a new Import overlay that runs copies on a worker thread (`backup_service::spawn_work`)
and shows progress. Drive goes through `wros-backup` (a new `drive-books` listing, the existing
`drive-materialize`) and `wros-sync enable`; USB sticks go through a new narrow root helper,
`wros-import-media`, modelled on `wros-update-media`.

**Tech Stack:** Rust (ratatui app `src/`, `crates/grimoire-core`, `crates/wros-sys`), Python helpers
in `os/scripts/`, NixOS modules/packages in `nixos/`.

**Spec:** `docs/superpowers/specs/2026-11-16-book-shelf-and-import-design.md`

## Global Constraints

- No path the writer must type to open or import a book. The typed-path overlay
  (`Overlay::BookPath`) leaves the shelf.
- Import copies; the source is never changed. Nothing overwrites the writer's words without a
  choice made on screen.
- Every copy shows a progress line that stays until it ends, then a result line that says where
  the book is or why it was not copied; a failed or cancelled copy leaves nothing behind.
- A book is a folder containing a `manuscript/` directory. Hidden folders (any name starting with
  `.`, including `.grimoire`, `.git`, `.wros-drive-materialize-*`) are never shown or descended
  into. Symlinks are never followed.
- Search depth under `~/Documents` is 3. Scans never hash files; stop listing after 500 books.
- Keyboard-only; every screen usable at 80×24 and at 1366×768 native (170×48); Colemak and
  QWERTY. Arrow keys/Enter/Esc everywhere, as on the existing shelf (`list_nav`).
- Offline: the shelf, laptop and USB imports work with no network; the Drive row says plainly
  when Drive can't be reached.
- USB: read-only, `nodev,nosuid,noexec`, one stick mounted at a time, unmounted when Import
  closes; root only through pkexec and an exact polkit action.
- Drive access stays in `wros-backup` (`wros-google:` remote); sync stays in `wros-sync`.
- Merge copies are named `<stem> (from <source>, <YYYY-MM-DD HH-MM>).md` so
  `grimoire_core::sync::copy_of` recognises them and the conflict screen lists them.
- Replace never deletes: the local book is renamed `<folder> (replaced <D Mon>)` only after the
  incoming copy is checked.
- Every new user-visible menu path in `assets/help/` must resolve
  (`every_path_in_help_and_the_tutorial_is_a_real_menu_path`); every palette action stays named
  in the help (`every_palette_action_is_explained_in_the_help`).

## Review Focus

1. A book folder name with spaces, a trailing full stop or non-ASCII letters (the owner's
   `Clip says Hello World.`) must list, import and open: Task 1 and Task 3 tests use that name.
2. Documents already holding a non-book folder with the incoming name: the copy goes to
   `<name> 2`, never into that folder (Task 1 `free_destination` test).
3. The stick is pulled out or the Drive call fails mid-copy: the staging folder is removed and
   the result line names the cause (Task 2 and Task 6 tests).
4. Importing the book that is open now (same project id or same folder): it must not copy over
   itself; say it is already here (Task 4 test).
5. A Documents tree with a deep or huge non-book folder (e.g. a photo dump): the shelf still
   opens fast because depth and count are bounded and no file is hashed (Task 1 test with a
   depth-5 tree and 600 folders).

---

### Task 1: Book discovery and naming (`grimoire_core::shelf`)

**Files:**
- Create: `crates/grimoire-core/src/shelf.rs`
- Modify: `crates/grimoire-core/src/lib.rs` (add `pub mod shelf;`), `src/books.rs:20` (`is_book`
  calls `grimoire_core::shelf::is_book`)
- Test: unit tests in `crates/grimoire-core/src/shelf.rs`

**Interfaces:**
- Produces:
  - `pub fn is_book(dir: &Path) -> bool` — `dir.join("manuscript").is_dir()`, false for symlinks.
  - `pub struct ShelfBook { pub root: PathBuf, pub title: String, pub project_id: Option<String> }`
  - `pub fn title_of(root: &Path) -> String` — `novel.toml` `title` when present and non-empty,
    else the folder name.
  - `pub fn project_id_of(root: &Path) -> Option<String>` — `project_id` from
    `.grimoire/sync.json` when it is a valid UUID string.
  - `pub fn find_books(top: &Path, depth: usize, limit: usize) -> Vec<ShelfBook>` — breadth-first,
    skips hidden names and symlinks, does not descend into a book, ordered by title
    (case-insensitive) then path.
  - `pub fn free_destination(documents: &Path, name: &str) -> PathBuf` — `documents/name`, or
    `name 2`, `name 3`… when taken by anything.
  - `pub fn same_book(local: &Path, incoming_name: &str, incoming_project_id: Option<&str>) -> bool`
    — both ids present and equal; otherwise (either id absent) the local folder name equals
    `incoming_name`.
  - `pub fn merge_copy_name(stem: &str, source: &str, when: chrono::NaiveDateTime) -> String` —
    `"{stem} (from {source}, {when:%Y-%m-%d %H-%M})"` (no extension).

- [ ] **Step 1: Write the failing tests** in `shelf.rs`:
  - `finds_books_to_depth_three_and_skips_hidden_and_non_books`: build `top/A/manuscript`,
    `top/WrOS/Clip says Hello World./manuscript` (with `novel.toml` `title = "Clip Says Hello World"`),
    `top/.hidden/B/manuscript`, `top/a/b/c/d/Deep/manuscript` (depth 5), `top/Notes/` (no
    manuscript), `top/A/inner/manuscript` (inside a book). Assert exactly
    `["A", "Clip Says Hello World"]` by title, and the second's root ends with
    `Clip says Hello World.`.
  - `find_books_is_bounded`: 600 empty folders plus 3 books under `top`; `find_books(top, 3, 2)`
    returns 2 entries.
  - `free_destination_numbers_any_taken_name`: `documents/Book` exists as a plain folder →
    `documents/Book 2`; with `Book 2` also present → `Book 3`.
  - `same_book_prefers_project_ids`: matching ids with different folder names → true; different
    ids with the same folder name → false; no ids and same folder name → true.
  - `merge_copy_names_round_trip_through_copy_of`:
    `sync::copy_of(&merge_copy_name("Scene One", "Google Drive", when), &|s| s == "Scene One", None)`
    is `Some` with `original == "Scene One"`.
- [ ] **Step 2:** Run `cargo test --locked -p grimoire-core shelf::` — expect compile failure
  (module missing).
- [ ] **Step 3:** Implement the functions above in `crates/grimoire-core/src/shelf.rs`; use
  `fs::symlink_metadata` to skip symlinks and `crate::project`'s existing `novel.toml` parser for
  the title if it is public, else `toml` the same way `Project::load` reads `title`.
- [ ] **Step 4:** Run `cargo test --locked -p grimoire-core shelf::` — PASS; then
  `cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings`.
- [ ] **Step 5:** Commit `feat(shelf): find books under Documents and name imported copies`.

### Task 2: Verified copy, replace and merge (`grimoire_core::book_copy`)

**Files:**
- Create: `crates/grimoire-core/src/book_copy.rs`
- Modify: `crates/grimoire-core/src/lib.rs`
- Test: unit tests in `book_copy.rs`

**Interfaces:**
- Consumes: Task 1 `is_book`, `free_destination`, `merge_copy_name`.
- Produces:
  - `pub struct Progress { pub files: u64, pub bytes: u64 }`
  - `pub fn copy_book(source: &Path, dest: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<()>`
    — refuses unless `is_book(source)` and `dest` does not exist; copies into a staging folder
    `dest.parent()/.wros-import-<random hex>` (regular files and directories only; symlinks and
    hidden `.wros-*` staging folders skipped), then compares every file's size and SHA-256 with
    the source, then renames staging to `dest`. Any error or `cancel` removes staging; the error
    names the file.
  - `pub fn replace_with_copy(source: &Path, local: &Path, today: chrono::NaiveDate, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<PathBuf>`
    — `copy_book` to a free staging name beside `local`; only after it succeeds, rename `local`
    to `free_destination(parent, "<folder> (replaced <D Mon>)")` (e.g. `Book (replaced 16 Nov)`),
    then rename the copy to `local`'s path. Returns the replaced book's new path.
  - `pub struct MergeReport { pub added: Vec<PathBuf>, pub conflicts: Vec<PathBuf>, pub kept_local: Vec<PathBuf> }`
  - `pub fn merge_into(source: &Path, local: &Path, source_label: &str, when: chrono::NaiveDateTime, cancel: &AtomicBool) -> Result<MergeReport>`
    — never reads or writes `local/.grimoire`; a file only in `source` is copied (`added`); a
    differing `.md` file is written beside the local one as `merge_copy_name(stem, source_label, when).md`
    (`conflicts`); any other differing file keeps the local version (`kept_local`). Identical
    files are untouched. The source is never written.

- [ ] **Step 1: Write the failing tests**:
  - `a_copied_book_matches_its_source_file_for_file` (names with spaces and `é`; nested scenes;
    a symlink in the source is not copied; `dest` exists afterwards; no `.wros-import-*` left).
  - `a_cancelled_or_failed_copy_leaves_nothing`: `cancel` set before the call → `Err`, `dest` and
    staging absent; a source file made unreadable (`chmod 000`, skip the assertion when running as
    root) → `Err` naming that file, nothing left.
  - `copy_refuses_a_folder_that_is_not_a_book_or_an_existing_destination`.
  - `replace_keeps_the_old_book_beside_the_new_one`: after `replace_with_copy`, `local` holds the
    source's scenes and `Book (replaced 16 Nov)` holds the old ones (date fixed in the test).
  - `merge_adds_new_files_parks_differing_scenes_and_keeps_local_state`: source and local differ
    in one scene, the source has one extra scene, `novel.toml` differs, local `.grimoire/x` exists.
    Assert the extra scene is added, `<scene> (from Google Drive, 2026-11-17 09-30).md` exists with
    the source text, the local scene and `novel.toml` are unchanged (`kept_local` lists
    `novel.toml`), `.grimoire/x` untouched, and the source tree's bytes are unchanged.
- [ ] **Step 2:** `cargo test --locked -p grimoire-core book_copy::` — fails to compile.
- [ ] **Step 3:** Implement; hash with `sha2` (already a dependency; check `Cargo.toml` before
  adding anything).
- [ ] **Step 4:** Tests and clippy pass.
- [ ] **Step 5:** Commit `feat(shelf): checked book copies, replace and merge`.

### Task 3: The shelf lists every book in Documents

**Files:**
- Modify: `src/app/overlays/shelf.rs` (`other_books`, `on_books_key`, `draw_books`),
  `src/books.rs` (add `pub fn documents() -> PathBuf` = `home().join("Documents")`),
  `src/app.rs` (`Overlay::Books` keeps `{ sel }`)
- Test: `src/ui_tests.rs`

**Interfaces:**
- Consumes: Task 1 `find_books(documents, 3, 500)`, `ShelfBook`.
- Produces: `App::shelf_rows(&self) -> Vec<ShelfRow>` with
  `enum ShelfRow { Book(ShelfBook, bool /* open now */), Import, NewBook }`; Task 4 opens Import
  from `ShelfRow::Import`.

Rows: books under Documents ordered by last opened (`books::recent` order) then title, the open
book included and marked "open now"; then recent books outside Documents; then
**Import a book…** and **New book…**. The "a book in another folder" and "Bring in a draft…"
rows and `Overlay::BookPath` go (drafts move to Task 7). `x`/Delete keeps its confirmation and is
refused on the open book. Each row shows the title, then `books::pretty` of its folder.

- [ ] **Step 1: Write the failing tests**:
  - `the_shelf_lists_every_book_in_documents_without_typing`: HOME with
    `Documents/WrOS/Clip says Hello World./manuscript` never opened before, plus the open book.
    Open File › Open another book…; the screen shows `Clip says Hello World.`, `Import a book…`,
    `New book…`, and the open book marked `open now`; Enter on the first row opens it.
  - Update `a_book_is_deleted_from_open_another_book_after_saying_so` and any test that used
    `BookPath` (grep `BookPath`) to the new rows.
- [ ] **Step 2:** Run `cargo test --locked -p grimoire-tui --no-default-features shelf` and the two
  named tests — fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass; run the whole `grimoire-tui` suite (help-path and palette tests).
- [ ] **Step 5:** Commit `feat(shelf): Open another book lists every book in Documents`.

### Task 4: Import from a folder on this laptop

**Files:**
- Create: `src/app/overlays/import.rs` (screen logic and drawing; register in
  `src/app/overlays/mod.rs`)
- Modify: `src/app.rs` (`Overlay::Import(ImportState)`, key dispatch, draw dispatch)
- Test: `src/ui_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces (Tasks 5–7 add sources to these):
  - `pub(crate) enum ImportSource { Drive, Usb { label: String, mount: PathBuf }, Laptop, Draft }`
  - `pub(crate) struct ImportState { stage: ImportStage, sel: usize }` with
    `enum ImportStage { Sources, Browse { source: ImportSource, dir: PathBuf, entries: Vec<Entry> }, Same { incoming: Incoming, local: PathBuf, choice: usize }, Copying { job: Job<ImportDone>, what: String }, Done(String) }`
  - `pub(crate) struct Incoming { source: ImportSource, path: PathBuf, name: String, project_id: Option<String> }`
  - `fn App::start_import(&mut self, incoming: Incoming, how: How)` with
    `enum How { New, Replace(PathBuf), Merge(PathBuf), KeepBoth }`, running the copy on
    `backup_service::spawn_work("import", …)`.

Behaviour: Sources lists only **A folder on this laptop** in this task (it starts at `$HOME`).
Browse lists directories only, books first labelled `book`, then folders, then **Up one folder**
below the source's top. Enter on a folder opens it; Enter on a book imports it. If
`same_book` matches a book on the shelf (Task 3's list) → `Same` with **Replace**, **Merge**,
**Keep both**, **Cancel**, showing each side's newest file date; if the incoming folder is the
open book's own folder → `Done("This book is already open here.")`. Copying shows
`Copying <title> from <source>…` until done; Esc asks "Stop copying? Nothing will be kept." and
cancels through the job's flag. Done shows `Copied to <pretty path> and checked. Opening it.` and
opens the book; Merge opens the conflict screen (`open_conflicts`) and the line lists
`kept_local` files. A failure shows the error and leaves the screen open.

- [ ] **Step 1: Write the failing tests** (laptop fixtures under a temporary HOME):
  - `a_laptop_book_is_copied_into_documents_checked_and_opened`: progress line seen, result line
    seen, `Documents/<name>` matches, the source unchanged, the book is open.
  - `a_folder_that_is_not_a_book_opens_instead_of_importing`.
  - `the_same_book_offers_replace_merge_keep_both_and_cancel` (one subtest per choice: Replace
    leaves `… (replaced <date>)`; Merge opens the conflict screen with one copy; Keep both
    creates `<name> 2`; Cancel changes nothing).
  - `importing_the_open_book_says_it_is_already_here`.
  - `escape_while_copying_asks_then_leaves_nothing` (a large fixture file so the copy is still
    running).
- [ ] **Step 2:** Run those tests — fail.
- [ ] **Step 3:** Implement; the shelf's Import row opens `Overlay::Import`.
- [ ] **Step 4:** Tests pass; full `grimoire-tui` suite and clippy.
- [ ] **Step 5:** Commit `feat(import): bring a book in from a folder on this laptop`.

### Task 5: Import from Google Drive, keeping sync

**Files:**
- Modify: `os/scripts/wros-backup` (new operation `drive-books`), `src/backup_service.rs`
  (`Request::DriveBooks { folder: String }`, reply type), `src/app/overlays/import.rs`,
  `os/tests/test_backup.py`, `os/scripts/wros-sync` (no change expected; `enable` already keeps
  an existing identity)
- Test: `os/tests/test_backup.py`, `src/ui_tests.rs` or `src/backup_ui_tests.rs`

**Interfaces:**
- Produces: `wros-backup drive-books <folder>` → JSON
  `{"entries": [{"name": str, "path": str, "book": bool, "synced": bool, "project_id": str|null}]}`
  for the folder's subfolders only, from one `rclone lsjson --max-depth 2` call (a child is a
  book when `<child>/manuscript` is a directory; synced when `<child>/.wros-sync.json` exists; its
  `project_id` is read with one `rclone cat` per synced child, capped at 50). Caps: 1,000 children.

Behaviour: the Drive row appears when Google is linked; otherwise it reads
**Google Drive · link it in Settings first** and opens Settings › Backups and accounts ›
Google Drive. Browsing starts at `WrOS/`. Importing runs `DriveMaterialize { remote, destination: free_destination(...) }`.
After it succeeds for a synced book: write `.grimoire/sync.json`
`{"version": 1, "project_id": <id>, "drive_folder": <name under WrOS/>}`, then run the existing
sync-enable request, so the first sync finds both sides equal. A Drive folder without a marker
imports as an ordinary copy. If a shelf book already syncs to that same Drive folder → `Done`
"<title> is already here and syncs with this Drive folder." with Enter to open it. Keep both on
a synced Drive book imports without sync identity. When Drive can't be reached, the row's
listing says so and nothing is copied.

- [ ] **Step 1: Write the failing tests**:
  - `test_backup.py::DriveBooks` against the rclone `alias` remote: a book with a marker, a book
    without, a plain folder; assert `book`/`synced`/`project_id`; a remote failure returns the
    helper's usual error.
  - App test with a stubbed backup helper (follow the stubs in `src/backup_ui_tests.rs`):
    `a_synced_drive_book_imports_with_its_sync_identity` (sync.json written with the marker's id
    and folder; the enable request sent), and `a_drive_book_already_synced_here_is_not_copied_again`.
- [ ] **Step 2:** Run them — fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Python and Rust tests pass.
- [ ] **Step 5:** Commit `feat(import): bring a book in from Google Drive with sync kept`.

### Task 6: Import from a USB stick (`wros-import-media`)

**Files:**
- Create: `os/scripts/wros-import-media`, `os/tests/test_import_media.py`
- Modify: `nixos/pkgs/helpers.nix` (install it with its `/usr/bin` tools substituted, like
  `wros-update-media`), `nixos/pkgs/paths.nix` (`helperPaths.importMedia`, env
  `WROS_PATH_HELPER_IMPORT_MEDIA`), `nixos/modules/polkit.nix` (action `org.wros.import.media`,
  exact `org.freedesktop.policykit.exec.path` annotation), `os/config/50-wros-update.rules` →
  allow the new action for the same subjects (active local session, groups `wros` and the
  `writingdeck` compatibility group), `crates/wros-sys/src/{lib.rs,linux.rs,paths.rs}`
  (`Request::ListImportMedia`, `Request::MountImportMedia(String)`, `Request::UnmountImportMedia`,
  `Reply::ImportMedia(Vec<ImportStick>)` with `ImportStick { device: String, label: String, mount: Option<String> }`),
  `src/app/overlays/import.rs`, `nixos/tests/helpers-no-fhs.nix` if it lists helpers.
- Test: `os/tests/test_import_media.py`, `crates/wros-sys` unit tests, an app test with the
  simulated backend.

**Interfaces:**
- Produces: `pkexec wros-import-media list|mount <device>|unmount`. `list` prints JSON of
  removable, partitioned USB block devices with a mountable filesystem (`lsblk -J -o
  PATH,TRAN,RM,TYPE,FSTYPE,LABEL`), excluding the disk holding `/` and anything LUKS. `mount`
  accepts only a device from `list`, unmounts any previous import mount, and mounts
  `ro,nodev,nosuid,noexec` at `/run/wros-import/media/1`. `unmount` is idempotent.

Behaviour: Sources shows one row per stick: **USB stick "<label>"** (or "USB stick" when
unlabelled). Choosing it mounts it, then browses from the mount point. Leaving Import, opening a
different stick, or the app exiting the Import overlay sends `UnmountImportMedia`. A stick
pulled out mid-copy fails the copy cleanly (Task 2) and says "The USB stick was removed."

- [ ] **Step 1: Write the failing tests**:
  - `test_import_media.py`: with fake `lsblk`/`mount`/`umount` on a private PATH (follow
    `os/tests/test_update*.py` media seams if present, else inject a runner like
    `wros-update-media`'s `main(..., **seams)`): `list` hides the root disk and LUKS devices;
    `mount` refuses a device not listed and uses exactly `-o ro,nodev,nosuid,noexec`; `unmount`
    twice succeeds.
  - `wros-sys`: invocation tests that the three requests become
    `pkexec <helper> list|mount <dev>|unmount` with the 60-second budget used by
    `FindUpdateMedia`.
  - App: `a_usb_stick_is_listed_by_label_and_unmounted_when_import_closes` (simulated backend).
- [ ] **Step 2:** Run them — fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass; `nix build .#checks.x86_64-linux.helpers-no-fhs` and
  `.#checks.x86_64-linux.python-suite` pass in NixOS-WSL.
- [ ] **Step 5:** Commit `feat(import): bring a book in from a USB stick`.

### Task 7: Drafts move into Import; Settings points books to Import

**Files:**
- Modify: `src/app/overlays/import.rs` (Draft source), `src/app/overlays/shelf.rs` (remove
  `bring_in`'s typed path; reuse its body), `src/app/shell.rs` (Drive files page: on a book
  folder, the folder copy row becomes **Import this book…**, which opens Import's Drive browse at
  that folder), `assets/help/` (the Books help topic and any page naming "Bring in a draft" or
  "a book in another folder"), `docs/` pages naming them.
- Test: update `a_draft_brought_in_becomes_a_book_split_at_its_chapters`; add
  `the_drive_files_page_sends_a_book_folder_to_import`.

**Interfaces:**
- Consumes: Task 4 `ImportSource::Draft`; `grimoire_core::import::{is_draft, read, make_book}`.

Behaviour: **A draft (Word, Markdown, text)** browses from `$HOME`, listing folders and files for
which `import::is_draft` holds; Enter on a file makes a new book at
`free_destination(documents, <file stem>)` and opens it.

- [ ] **Step 1:** Update/add the two tests — fail.
- [ ] **Step 2:** Implement; update help and docs; `python tools/check-wros.py` passes.
- [ ] **Step 3:** Full `grimoire-tui` suite passes (help paths, palette names).
- [ ] **Step 4:** Commit `feat(import): drafts come in through Import; Settings sends books there`.

### Task 8: VM check for USB and Drive import

**Files:**
- Create: `nixos/tests/import-book.nix`
- Modify: `flake.nix` (`checks.import-book`), `docs/NIXOS_QUALIFICATION.md` (list it)

Behaviour: an installed-system VM with a second virtual disk (vfat, label `BOOKS`, holding a
book) attached as USB storage, and an rclone `alias` remote standing in for Drive with one
synced book. The test drives the app's CLI or PTY to import both, then asserts both books are in
`~/Documents`, match their sources, the stick is unmounted, and the Drive book's
`.grimoire/sync.json` has the marker's id.

- [ ] **Step 1:** Write the test; run `nix build -L .#checks.x86_64-linux.import-book` — fails
  before Task 6's helper is packaged, passes after.
- [ ] **Step 2:** Commit `test: VM check imports a book from a USB stick and from Drive`.
