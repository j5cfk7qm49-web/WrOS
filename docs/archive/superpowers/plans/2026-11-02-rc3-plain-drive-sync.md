# rc3: Plain-Text Google Drive Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship rc3: two-way, versioned, readable sync of the open book with Google Drive, with paragraph merge for edits on both sides, published online and installed through Update WrOS.

**Architecture:** A Python helper (`wros-sync`) compares each file's last-synced base, its local state and its Drive state using the existing private `wros-google` rclone remote. It uploads, downloads, moves replaced or deleted versions to a dated history folder, and parks Drive's version beside a file changed on both sides. It writes into the book only as a sync app would, using the compare-and-swap rule Grimoire already applies to Dropbox and Syncthing folders. The app runs it in the background after saves, on project open and every 5 minutes, then three-way merges parked text files. Clean merges save themselves; clashing paragraphs wait for the writer in the Conflicts screen.

**Tech Stack:** Python 3 + rclone 1.60 (helper), Rust (grimoire-core merge, grimoire-tui app), GitHub Actions (publish).

**Spec:** `docs/superpowers/specs/2026-11-01-plain-drive-sync-design.md`; online updates: `docs/superpowers/specs/2026-11-01-online-wros-updates-design.md`.

## Global Constraints

- Per book, opt-in, off by default: Settings › Accounts › Backups › **Sync this book with Google Drive (readable files)**; the row states the files in Drive are not encrypted.
- Only the open book syncs. Triggers: 10 s after the last save, on project open, every 5 min while switched on. Never while writing blocks: it runs on its own worker like backups.
- Drive: `WrOS/<book folder name>/`; history: `WrOS-History/<book folder name>/<YYYY-MM-DD>/<path> (<HH-MM-SS> from WrOS|from Drive)<ext>`.
- Synced: every regular file in the book except dot-names at any depth (`.grimoire/`, `.git/`), parked copies (` (from `), and Grimoire temporary/rescue files. Symlinks and special files are refused, as in backups.
- Identity: `.grimoire/sync.json` `{"version":1,"project_id":"<uuid4>","drive_folder":"<name>"}`; Drive marker `WrOS/<name>/.wros-sync.json` with the same id. A marker with another id refuses: "This Drive folder belongs to another WrOS book or computer".
- Private state: `$XDG_DATA_HOME/grimoire/wros/sync/<project_id>/` (0700): `base.json`, `base/<path>` text copies, `merge-base/<path>`, `pending.json`.
- A parked Drive copy is named `<stem> (from Drive, <YYYY-MM-DD HH-MM>)<ext>`, which the existing `sync::copy_of` reads as a parked copy (shown, never compiled or counted).
- Mass-delete guard: a run that would delete more than 5 files on either side, or finds either folder empty while the base is not, deletes nothing and reports `confirm`.
- First sync (no base): never deletes; equal files become the base; one-sided files copy across; differing files park Drive's copy without a merge.
- Helper protocol like `wros-backup`: JSON request on stdin, one JSON object on stdout, progress lines on stderr; started in its own session with no terminal.
- Version `0.1.0~rc3`, signed here with the pinned primary (`--use-primary`), published from a GitHub release asset while the CI secret is absent.

## Review Focus

1. A Drive change arriving for a file open with unsaved words: never overwritten (the editor's existing external-change and parked-copy handling), and the words survive.
2. A Drive folder emptied or renamed by mistake: nothing local is deleted (guard + first-sync rule).
3. Network loss mid-run: no partial file in the book, the base not advanced for files not finished, the next run resumes.
4. Two edits to the same paragraph: neither is lost; both are reachable until the writer chooses.
5. Files that are not UTF-8 text, or very large: synced as files, never merged.

---

### Task 1: Paragraph three-way merge (grimoire-core)

**Files:** Create `crates/grimoire-core/src/merge.rs` (+ `pub mod merge;`); tests in the same file.

**Interfaces:**
- `pub fn paragraphs(text: &str) -> Vec<&str>`: blocks separated by blank lines; YAML frontmatter (leading `---` … `---`) is one block; joining with `"\n\n"` reproduces normalized text.
- `pub struct Clash { pub mine: String, pub theirs: String }`
- `pub enum Piece { Same(String), Clash(Clash) }`
- `pub fn three_way(base: &str, mine: &str, theirs: &str) -> Vec<Piece>`: LCS-aligned on paragraphs. A block changed on one side takes that side; changed identically on both is taken once; changed differently is a `Clash`; deletions on one side with no change on the other apply.
- `pub fn assemble(pieces: &[Piece], choices: &[Choice]) -> String` with `pub enum Choice { Mine, Theirs, Both }` (one per clash, in order).

- [ ] **Step 1:** Failing tests: different paragraphs edited on each side merge with no clash; the same paragraph edited differently gives one clash with both texts; identical edits give no clash; a paragraph deleted on one side and untouched on the other is deleted; frontmatter is a single block; `assemble` with Mine/Theirs/Both produces the expected text.
- [ ] **Step 2:** Run `cargo test -p grimoire-core merge`; expect FAIL (module missing).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run; expect PASS.
- [ ] **Step 5:** Commit `feat: paragraph three-way merge`.

### Task 2: The sync helper

**Files:** Create `os/scripts/wros-sync`, `os/tests/test_sync.py`; manifest entry `usr/libexec/wros-sync` (0755, code).

**Interfaces:**
- Request `{"operation":"sync","project":"<abs>","allow_deletes":false}` → `{"ok":true,"data":{"status":"synced|confirm|offline|refused","uploaded":n,"downloaded":n,"deleted":n,"parked":["<rel>"...],"pending":[{"path","parked","merge_base"}],"deletes":n,"message":"..."}}`.
- `{"operation":"enable","project":"<abs>"}` writes `.grimoire/sync.json` (new uuid, drive folder = book folder name, ` (2)` when another id owns it in Drive) and the Drive marker; `{"operation":"disable","project":...}` removes `.grimoire/sync.json` only; `{"operation":"status","project":...}` reads local state only.
- `plan(base, local, remote, first) -> list[(path, action)]`, pure, with actions `upload, download, delete_remote, delete_local, park, adopt, none`, tested directly.
- Uses `rclone lsjson -R --hash --files-only` for Drive (md5), local md5 and sha256, `copyto`, `moveto` (to history), `deletefile`, all with `--config <private rclone.conf>`; every local write is a temp file in the same folder + `os.replace` after re-checking that the local file still has the scanned md5 (else skip and report).

- [ ] **Step 1:** Failing tests, against an rclone `alias` remote standing in for Drive (as in `test_backup_pty`):
  - `plan` covers every row of the spec's table, the first-sync rules and the guard.
  - A first sync uploads a book. A Drive edit downloads, with the old local version in history. A local edit uploads, with the old Drive version in history. A local delete deletes in Drive (to history).
  - Edits on both sides to a `.md` park `name (from Drive, …).md` locally, keep the merge base, report it pending, and set the base to Drive's version.
  - Deleting 6 files reports `confirm` and changes nothing; `allow_deletes` proceeds.
  - Dot paths and parked copies are never uploaded.
  - A marker with a different id refuses.
  - A local file changing between scan and write is skipped, not overwritten.
- [ ] **Step 2:** Run; expect FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `test_sync`; expect PASS.
- [ ] **Step 5:** Commit `feat: two-way readable Drive sync helper`.

### Task 3: App: switch, schedule, status

**Files:** Create `src/sync_service.rs` (job spawn as in `backup_service.rs`: own session, progress, cancel). Modify `src/app/backup.rs` (sync state), `src/app/shell.rs` (Backups rows), `src/app.rs`/`src/app/writingdeck.rs` (triggers), tests in `src/backup_ui_tests.rs` or `src/shell_ui_tests.rs`.

**Interfaces:** `SyncState { enabled: bool, job: Option<Job>, last: Option<Summary>, next: Option<Instant>, confirm_deletes: Option<usize> }`. Rows: switch (`Sync this book with Google Drive (readable files)` / `Stop syncing this book`), `Sync now`, a description row (`Synced 14:02 · 2 uploaded, 1 downloaded`, `Waiting: offline`, `2 files need merging`, `Stopped: would delete 7 files`), and `Sync and delete 7 files` when stopped by the guard.

- [ ] **Step 1:** Failing UI tests with a stub helper (a test script path, as backup tests use): switching on runs `enable` then `sync`; a save schedules a sync 10 s later; the guard state offers the delete option; the status row shows each summary.
- [ ] **Step 2:** Run; expect FAIL. **Step 3:** Implement. **Step 4:** Run; PASS. **Step 5:** Commit.

### Task 4: Merging after a sync

**Files:** Modify `src/app/overlays/conflicts.rs` (fourth choice `Merge paragraph by paragraph` for `(from Drive, …)` copies with a merge base), `src/app/backup.rs` (post-sync processing), tests.

- [ ] **Step 1:** Failing tests:
  - After a sync reports a pending merge with no clashes, the file holds the merged text, the parked copy goes to the trash, and the notice says it merged.
  - With clashes, nothing changes until the writer opens Conflicts. The merge screen steps through each clash (Mine / Drive's / Both). Finishing saves the merged text, trashes the parked copy, and is undoable with Ctrl-Z.
  - A file open with unsaved words is not merged until saved.
- [ ] **Step 2:** FAIL. **Step 3:** Implement with `grimoire_core::merge`. **Step 4:** PASS. **Step 5:** Commit.

### Task 5: Update check says when nothing is published

- [ ] **Step 1:** Failing test: an HTTP 404 for `InRelease` gives status `unpublished` ("No WrOS updates have been published yet"), not `unreachable`; Rust accepts and shows it.
- [ ] **Step 2–5:** FAIL, implement (`check_online`, Rust parser and line), PASS, commit.

### Task 6: Publish a locally signed update without the CI secret; docs

- [ ] **Step 1:** Failing test (`test_update_workflow`): with no `WROS_UPDATE_SIGNING_SUBKEY`, a tag run (or dispatch input `release`) downloads the release asset `wros-updates.tar`, verifies it with `publish-updates.py check-bundle` against the committed keyring and pin, and deploys it unchanged. A scheduled run without the secret republishes the live files unchanged after verifying them.
- [ ] **Step 2–5:** FAIL; implement (`publish-updates.py check-bundle DIR`, workflow branches); PASS; docs: `SYNC_AND_BACKUP.md` (bidirectional sync now implemented), `NATIVE_BACKUP.md` (sync rows), `API_AND_NETWORK_USE.md` (sync traffic); commit.

### Task 7: Release rc3

- [ ] **Step 1:** `os/VERSION` → `0.1.0~rc3`; full suites; fresh whole-branch review; fix pass; commit `release: 0.1.0~rc3`; push `development` and fast-forward `main`.
- [ ] **Step 2:** Build and sign the bundle here (`rc-x-rc2-bundle.sh` with the version from the tree; `--use-primary`); verify against the laptop's rc2 trust.
- [ ] **Step 3:** `gh release create v0.1.0-rc3 wros-updates.tar`; let the workflow publish; `curl` the Pages `InRelease` and verify it.
- [ ] **Step 4:** Over SSH: `wros-update check-online` on the laptop → `available 0.1.0~rc3`. The writer makes a backup from the menu (the gate now sees it), then chooses Update WrOS and restarts.
