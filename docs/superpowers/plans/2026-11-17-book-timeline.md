# Book Timeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every book on WrOS keeps one git timeline of the whole book (small saves, marked sessions,
"Changed on Drive" entries) on the laptop and, as append-only git bundles, on Drive; scene history
reads it; Import and a damaged repository rebuild it from Drive.

**Architecture:** A new core module `grimoire_core::timeline` grows Writing sessions' repository
(`sessions.rs`, same location and git runner) into the timeline: recording, labels, session tags,
reading versions, bundles and rebuilds. The app drives it from one long-lived worker thread
(`src/app/timeline.rs`) so typing never waits on git. Drive access stays in the Python helper
`os/scripts/wros-sync` (rclone, `wros-google:`), which gains three timeline operations.

**Tech Stack:** Rust 2024 (grimoire-core, grimoire-tui), git CLI, Python 3 helper + rclone, NixOS VM test.

**Spec:** `docs/superpowers/specs/2026-11-16-book-timeline-design.md` (read it first; this plan
argues from it and does not repeat it).

## Global Constraints

- On WrOS only (`crate::installed_session()` in the app); elsewhere Writing sessions stays opt-in and unchanged.
- No off switch on WrOS. Nothing in the timeline is ever trimmed, locally or on Drive.
- The book folder is never changed to make this work: no `.git`, no `.gitignore` written; exclusions live in the repository's `info/exclude` and in pathspecs.
- Repository: `~/.local/share/grimoire/sessions/<book-id>.git` (`sessions::outside_dir`), `core.fsync=committed`, `core.fsyncMethod=fsync`.
- Every git call goes through the sessions runner (no prompts, no stdin, deadlines `LOCAL_TIMEOUT` 30 s); every rclone call through `wros-sync`'s `Service.rclone`.
- A save never waits for git; git failures fall back to plain-file history and tell the writer once.
- Drive layout: `WrOS-History/<book>/timeline/<machine>/<NNNNNN> <YYYY-MM-DD> <HHMM>.bundle` (6-digit number, space-separated, local time). Bundles are never replaced or deleted.
- Machine name: `<hostname>-<4 hex>`, e.g. `hp-stream-3f9a`, made once, stored in `paths::data_dir()/machine-id`.
- Labels: "minor fix", "small change", "change", "big change", "Changed on Drive: <files>", "Before restoring <scene title>", "Before replacing across the book", "Brought in from Drive". Session tags `session/<YYYY-MM-DD-HHMM>` (annotated, message = the Writing sessions diary label).
- Session ends: quit, book close/switch, 30 minutes without typing, before suspend/lock, before Power actions.
- Uploads: at each session end, every hour during a long session, and when Drive becomes reachable again.

## Review Focus

1. A book with its own hostile `.gitignore` (`*`, `*.md`, `.grimoire/`) must still have every recorded file in each entry (Task 1 test `hostile_gitignore_cannot_hide_files`).
2. A failed or interrupted upload (network drop after the temporary file is written) must never leave a final-named bundle that is incomplete, and the next run must retry under the same number, not skip one (Task 4 test `interrupted_upload_retries_same_number`).
3. A sync that downloads over a scene while the timeline is broken must still keep the replaced text somewhere: `WrOS-History/<date>` copies are skipped only when the app says the pre-sync entry was recorded (Task 4 test `history_copies_only_without_timeline_entry`).
4. Two quick restores, or a restore while the worker is busy uploading, must still record "Before restoring" before the restored text reaches disk (Task 5 test `before_restore_is_recorded_before_the_restored_text`).
5. A laptop clock that moves backwards (no RTC battery, no network yet) must not reorder or rename bundles: numbering comes from the series, never from the time in the name (Task 3 test `numbers_follow_the_series_not_the_clock`).

---

### Task 1: Recording the whole book

**Files:**
- Create: `crates/grimoire-core/src/timeline.rs` (+ `pub mod timeline;` in `lib.rs`)
- Modify: `crates/grimoire-core/src/sessions.rs` (make `git()`, `Git`, `Store`, `store()`, `assign_id`, `label`, `Tally`, `body_words` `pub(crate)`; no behaviour change)
- Test: `crates/grimoire-core/src/timeline.rs` (`#[cfg(test)] mod tests`, temp books as in sessions.rs tests)

**Interfaces:**
- Produces:
  - `pub fn enable(root: &Path) -> Result<()>`: as `sessions::enable` but no `.gitignore` is written; an existing outside repository is kept (its history is the timeline's start); a `.git` inside the book is first moved out with `sessions::move_history_out`; sets the fsync config; writes `info/exclude` with the excluded set (for any git client opening the repo); first entry "Timeline begins" when the repo is new.
  - `pub fn is_enabled(root: &Path) -> bool` (outside repository present and claimed)
  - `pub enum Entry { Save, ChangedOnDrive(Vec<String>), BeforeRestoring(String), BeforeReplacing, BroughtIn }`
  - `pub fn record(root: &Path, entry: &Entry) -> Result<Option<String>>`: stage the recorded set into the repository's index, commit if anything changed, return the short hash; `Ok(None)` when nothing changed.
  - `pub fn label(entry: &Entry, words_changed: usize) -> String`
  - `pub const EXCLUDED: &[&str]` (pathspec patterns, below)

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn records_the_whole_book_except_the_excluded_set()
// files: chapters/01.md, novel.toml, notes/x.md, .grimoire/writingdeck/journal.json,
// .grimoire/trash/cut.md, .grimoire/resume.md, .grimoire/book-id, annotations.json
// excluded: .grimoire/history/a/1.md, .grimoire/recovery/r, .grimoire/retained-history/h,
// .grimoire/sync.json, exports/book.pdf, .wros-stage-1/x, "chapters/01 (from Drive, 2026-11-16).md"
// assert `git ls-tree -r --name-only HEAD` == exactly the first list
#[test] fn hostile_gitignore_cannot_hide_files()   // book .gitignore "*\n" and "*.md\n.grimoire/\n"
#[test] fn nothing_changed_records_nothing()        // second record() == Ok(None)
#[test] fn enable_writes_no_gitignore_and_keeps_an_existing_sessions_history()
#[test] fn enable_moves_an_in_book_git_out_first()
#[test] fn fsync_is_configured()                    // git config core.fsync == "committed"
#[test] fn labels_follow_the_words_changed()
// label(&Entry::Save, 3) == "minor fix"; 40 → "small change"; 400 → "change"; 4000 → "big change"
// label(&Entry::ChangedOnDrive(vec!["a.md".into(), "b.md".into()]), 0) == "Changed on Drive: a.md, b.md"
// more than 3 files: "Changed on Drive: a.md, b.md, c.md and 2 more"
// BeforeRestoring("Chapter One") → "Before restoring Chapter One"; BeforeReplacing → "Before replacing across the book"
```

- [ ] **Step 2: Run, expect failure** — `cargo test -p grimoire-core timeline::` (does not compile: module missing).
- [ ] **Step 3: Implement.** Staging: `git add --all --force -- . <EXCLUDED as :(exclude,glob) pathspecs>` with `GIT_INDEX_FILE` the repository's own index; `--force` is what defeats the book's `.gitignore`. `EXCLUDED` = `.grimoire/history/**`, `.grimoire/recovery/**`, `.grimoire/retained-history/**`, `.grimoire/sync.json`, `exports/**`, `**/.wros-*/**`, `**/* (from *`. Word count for the label: added + removed words of `git diff --cached --word-diff=porcelain -M` (lines starting `+`/`-`, not `+++`/`---`), counted with `sessions::body_words`. Thresholds: ≤ 10 minor fix, ≤ 150 small change, ≤ 1500 change, else big change. Commit identity as `sessions::commit` does.
- [ ] **Step 4: Run, expect pass** — same command; also `cargo test -p grimoire-core sessions::` still passes.
- [ ] **Step 5: Commit** — `feat(timeline): record the whole book in the sessions repository`

### Task 2: Sessions and scene versions

**Files:** Modify `crates/grimoire-core/src/timeline.rs`; tests there.

**Interfaces:**
- Consumes: Task 1 `record`, `enable`.
- Produces:
  - `pub fn mark_session(root: &Path, now: DateTime<Local>) -> Result<Option<String>>`: if HEAD has entries since the last `session/*` tag (or no tag yet), make annotated tag `session/<%Y-%m-%d-%H%M of now>` on HEAD with the diary label for the range (reuse `sessions::label` with a `Tally` built between the last tag and HEAD); returns the tag name. A name already taken gets `-2`, `-3`.
  - `pub struct SceneVersion { pub hash: String, pub when: DateTime<Local>, pub label: String, pub session: Option<String>, pub text: String }` (`session` = the diary label of the session tag on or after this entry, if it ends one, for drawing a divider)
  - `pub fn scene_versions(root: &Path, scene: &Path, limit: usize) -> Result<Vec<SceneVersion>>`: newest first, commits that changed the scene (`git log --follow -M -z --format=%H%x1f%at%x1f%s -- <rel>`), text via `git show <hash>:<path at that commit>`.

- [ ] **Step 1: Failing tests** — `marks_a_session_once` (two calls, second `Ok(None)`), `session_label_reads_like_the_diary` (contains " · " and the word delta), `same_minute_gets_a_suffix`, `scene_versions_newest_first_with_labels`, `scene_versions_follow_a_rename`, `session_divider_on_the_last_entry_of_a_session`.
- [ ] **Step 2: Run, expect failure.** `cargo test -p grimoire-core timeline::`
- [ ] **Step 3: Implement** with `git tag -a -m`, `git for-each-ref refs/tags/session/ --sort=-creatordate`, `git log` as above.
- [ ] **Step 4: Run, expect pass.**
- [ ] **Step 5: Commit** — `feat(timeline): session tags and scene versions`

### Task 3: Bundles, machine name, rebuild

**Files:** Create `crates/grimoire-core/src/timeline/bundles.rs` (move `timeline.rs` to `timeline/mod.rs`); tests there.

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:
  - `pub fn machine_name() -> String` (`paths::data_dir()/machine-id`, created once: `resume::machine_name()` lowercased, non-`[a-z0-9-]` → `-`, + `-` + 4 random hex)
  - `pub struct Bundle { pub number: u32, pub path: PathBuf, pub name: String, pub full: bool, pub tip: String }` (`name` = the Drive file name from Global Constraints; `tip` = HEAD's full hash when bundled)
  - `pub fn next_bundle(root: &Path, out_dir: &Path, on_drive: &[u32], now: DateTime<Local>) -> Result<Option<Bundle>>`: `on_drive` = numbers of this machine's bundles Drive holds. Number = 1 + highest of `refs/wros/uploaded/*` and `on_drive`. Full bundle (`--all`) when any `refs/wros/uploaded/<n>` is missing from `on_drive` or there are none; else `--all` minus `^<uploaded tip>` of the highest. `None` when nothing new. Runs `git bundle verify`.
  - `pub fn mark_uploaded(root: &Path, bundle: &Bundle) -> Result<()>`: `refs/wros/uploaded/<number>` = `bundle.tip`.
  - `pub fn rebuild(root: &Path, bundles: &[(String, PathBuf)]) -> Result<Option<String>>`: `(machine, path)` pairs in number order per machine. Moves a damaged/old repository aside to `<repo>.damaged-<stamp>` (never deleted), creates a fresh one, fetches every bundle into `refs/machines/<machine>/*` (`git fetch <bundle> '+refs/*:refs/machines/<machine>/*'`), then sets this machine's `main` to the newest entry whose tree equals the current files' recorded tree (`git write-tree` after staging), or, if none matches, to the newest entry overall and records `Entry::BroughtIn` on top. Returns the new HEAD.
  - `pub fn is_damaged(root: &Path) -> bool` (`git fsck --connectivity-only --no-dangling` fails, or HEAD unreadable)
- [ ] **Step 1: Failing tests** — `incremental_bundles_have_no_gap_or_overlap` (3 bundles; a fresh clone from all three has every commit, each bundle's prerequisites are the previous tip), `full_bundle_when_drive_lost_one`, `numbers_follow_the_series_not_the_clock` (now goes backwards; numbers still rise), `nothing_new_no_bundle`, `rebuild_from_two_machines_keeps_both_lines_and_matches_current_files`, `rebuild_records_brought_in_when_files_differ`, `damaged_repository_is_moved_aside_not_deleted`, `machine_name_is_made_once`.
- [ ] **Step 2: Run, expect failure.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, expect pass.**
- [ ] **Step 5: Commit** — `feat(timeline): bundles for Drive and rebuilding from them`

### Task 4: Drive side in wros-sync

**Files:** Modify `os/scripts/wros-sync`, `src/sync_service.rs`; Test `os/tests/test_sync.py` (rclone `alias` remote as in `setUp`).

**Interfaces:**
- Produces (JSON requests on stdin, `{ok, data|error}` replies, as the existing operations):
  - `{"op": "timeline-list", "project": P}` → `{"machines": {"<machine>": [numbers…]}}` from `WrOS-History/<folder>/timeline/`; final names only (temporary `.part-*` files ignored).
  - `{"op": "timeline-upload", "project": P, "machine": M, "bundle": PATH, "name": NAME}` → `{"uploaded": NAME}`. Copies to `timeline/<M>/.part-<random>`, compares MD5 (`rclone md5sum`/`lsjson --hash`) with the local file, refuses if `NAME` already exists (error "A timeline file with this name is already on Drive; nothing was replaced."), then `moveto` the part file to `NAME`. A leftover `.part-*` from an earlier run is left alone (never deleted).
  - `{"op": "timeline-fetch", "project": P, "into": DIR}` → `{"bundles": [{"machine": M, "number": N, "path": FILE}…]}` copying every final-named bundle.
  - `sync` request gains `"timeline": bool`; reply gains `"changed": [relative paths downloaded or deleted locally]`. When `timeline` is true, `download` over an existing file and `delete_local` skip the `WrOS-History/<date>` copy; `delete_remote` keeps relying on Drive's trash as today. When false or absent: unchanged.
  - Rust: `sync_service::Reply` gains `pub changed: Vec<String>`; the request builder gains a `timeline: bool` argument.
- [ ] **Step 1: Failing tests** — `upload_uses_a_part_file_then_the_final_name`, `upload_never_replaces_an_existing_name`, `interrupted_upload_retries_same_number` (patch rclone to fail on `moveto`; part file stays, no final name, second upload with same name succeeds), `list_ignores_part_files`, `fetch_copies_every_machine`, `history_copies_only_without_timeline_entry` (download over a local file with `timeline: true` → no WrOS-History copy; `false` → copy as today), `reply_lists_changed_files`.
- [ ] **Step 2: Run, expect failure** — `python3 -m unittest test_sync` in `os/tests`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, expect pass**, plus `cargo test -p grimoire-tui sync_service`.
- [ ] **Step 5: Commit** — `feat(sync): timeline bundles on Drive; no History copies once the timeline recorded`

### Task 5: The worker and when entries are made

**Files:**
- Create: `src/app/timeline.rs` (worker), tests in it.
- Modify: `src/app.rs` (`commit_saves` :1385 snapshot sites, `try_quit` :1811, `replace_book_sources` :2746, `restore_version` :3883, book open where `sessions_on` is set :1061), `src/app/review_notes.rs:168`, `src/app/wros.rs` (`request_suspend` :451, Power page ~:1606, `tick_deck` :866), `src/app/backup.rs` (`request_sync` :636, `tick_sync` :706), `src/app/overlays/shelf.rs:391` (already via `try_quit`).

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  - `pub struct Timeline` (on `App` as `timeline: Option<Timeline>`, `Some` only on WrOS):
    - `pub fn start(root: PathBuf) -> Self` — spawns the worker; first job enables the timeline (Task 1 `enable`), checks `is_damaged` and, if damaged, asks for a rebuild (Task 6 wires the Drive fetch; here: move aside and start fresh from the current files).
    - `pub fn record(&self, entry: Entry)` — queued, never waits.
    - `pub fn record_now(&self, entry: Entry, wait: Duration) -> bool` — queued ahead of other jobs, waits up to `wait` (use 2 s) for it to finish; `false` on timeout or failure (the plain-file copy still exists).
    - `pub fn end_session(&self, now: DateTime<Local>)` — record `Save`, `mark_session`, then upload.
    - `pub fn upload(&self)` — `timeline-list`, `next_bundle`, `timeline-upload`, `mark_uploaded`; on failure keep quiet and retry at the next trigger.
    - `pub fn poll(&mut self) -> Option<Notice>` — called each tick; returns at most one "The book's timeline isn't recording (…); scene history keeps plain copies." per app run.
    - `pub fn last_record_ok(&self) -> bool` — whether the newest finished record succeeded (for the sync request's `timeline` flag).
- Triggers:
  - `Save` where `history::snapshot(.., Some(history::GAP))` returns `Ok(true)` (`app.rs:1436`), and after the before-session copy (`app.rs:1410`) once per session.
  - `record_now(BeforeRestoring(title))` before `restore_version`'s text change and in `snapshot_before_story_restore`; `record_now(BeforeReplacing)` in `replace_book_sources` before writing.
  - Sync: `record_now(Save, 2 s)` before `request_sync` spawns; pass `timeline: ok`; after a reply with `changed` non-empty, `record(ChangedOnDrive(changed))`.
  - Session end: `try_quit` (replaces `commit_session` on WrOS), `request_suspend`, Power actions (before `sync_before_leave`), 30 minutes after the last key that changed text (checked in `tick_deck`), book switch (via `try_quit`).
  - Upload: `end_session`, hourly while open (from `tick_deck`), and when `tick_sync` sees a successful sync after an `offline` status.
  - The plain-file before-this-session copy at `app.rs:1410` stays (fallback); the 5-minute `history::snapshot` at :1436 stays only when the timeline is off or its last record failed.
- [ ] **Step 1: Failing tests** in `src/app/timeline.rs` with a temp book: `records_run_off_the_calling_thread` (record returns in < 5 ms while a job sleeps), `before_restore_is_recorded_before_the_restored_text`, `record_now_times_out_and_reports_false`, `end_session_marks_and_uploads` (fake upload closure), `one_notice_per_run_on_failure`, `idle_half_hour_ends_a_session` (injected clock).
- [ ] **Step 2: Run, expect failure** — `cargo test -p grimoire-tui --bin grimoire timeline::`.
- [ ] **Step 3: Implement** as one `std::thread` with an `mpsc` queue (urgent jobs on a second channel checked first), results back on a channel polled in `poll`; uploads go through `sync_service`'s helper runner (make a `run_timeline(request) -> Result<Value>` there).
- [ ] **Step 4: Run, expect pass**, then the whole `cargo test -p grimoire-tui`.
- [ ] **Step 5: Commit** — `feat(timeline): record while writing, mark sessions, upload to Drive`

### Task 6: Scene history from the timeline; Import and rebuild from Drive

**Files:** Modify `src/app.rs` (`open_history` :3857, `Overlay::History` :821), `src/app/overlays/versions.rs`, `src/app/overlays/import.rs` (`start_import` :644, after `DriveMaterialize`), `src/app/timeline.rs` (rebuild job); tests in `src/argon_writing_ui_tests.rs` style screen tests and `src/app/timeline.rs`.

**Interfaces:**
- Consumes: Task 2 `scene_versions`, Task 3 `rebuild`, Task 4 `timeline-fetch`, Task 5 `Timeline`.
- Produces: `Overlay::History` versions become `Vec<HistoryItem>` with `pub enum HistoryItem { Version { when, label: Option<String>, text, from_timeline: bool }, Session(String) }`; timeline versions first (newest first, `Session` dividers), then plain-file versions; when the timeline is unavailable, a first line "Timeline unavailable; showing saved copies." `Enter` on a version restores as today.
- Import: after a successful Drive import into a New/Replace book on WrOS, run `timeline-fetch` into a temp dir and `rebuild`; the shelf shows "Bringing in the book's history…" while it runs; failure leaves the book imported with a fresh timeline and a notice.
- Damaged repository at open: `timeline-fetch` then `rebuild` when the book syncs; otherwise move aside and start fresh.
- [ ] **Step 1: Failing tests** — `history_lists_timeline_then_plain_copies_with_dividers`, `history_says_when_the_timeline_is_unavailable`, `restore_from_a_timeline_version`, `import_brings_back_every_version` (fake fetch returning bundles made by Task 3 helpers), `damaged_repository_rebuilds_from_drive_bundles`.
- [ ] **Step 2: Run, expect failure.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, expect pass** (`cargo test -p grimoire-tui`).
- [ ] **Step 5: Commit** — `feat(timeline): scene history reads the timeline; Import brings it back`

### Task 7: End to end, docs, measurement

**Files:** Create `nixos/tests/book-timeline.nix` (shape of `nixos/tests/import-book.nix`, alias remote for Drive), add to `flake.nix` checks; modify `assets/help/` page on scene history and `docs/` page that describes Writing sessions / WrOS-History (find with `git grep -l "WrOS-History"`); `CHANGELOG.md` is release-time, not here.

- [ ] **Step 1: VM test** — write two scenes with the driven writer, quit (session tag + upload), check the fake Drive has `000001 … .bundle`; wipe `~/.local/share/grimoire/sessions` and the book; Import the book from the fake Drive; scene history (`H`) lists every version; empty the fake Drive's `timeline/` and end a session: a full bundle is uploaded as the next number.
- [ ] **Step 2: Run** `nix build .#checks.x86_64-linux.book-timeline` — expect pass.
- [ ] **Step 3: Docs** — help page and docs describe the timeline, the Drive folder, and that the journal/productivity/review notes now go to Drive (owner-visible consequence in the spec); run `python tools/check-wros.py`.
- [ ] **Step 4: Measure** on a copy of a real-size book (≈ 1 MB of text, 2,000 entries scripted): repository and bundle sizes, and a record's time on the HP Stream if available; write the figures into the spec's "Size and cost" section.
- [ ] **Step 5: Commit** — `test(timeline): end to end on a fake Drive; docs`
