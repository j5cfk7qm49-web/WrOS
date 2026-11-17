# Book timeline: every version, on the laptop and on Drive (design)

Status: approved by the owner 2026-11-16 (git timeline mirrored to Drive as git bundles; the
whole book is versioned, journal included; no off switch on WrOS). Target: rc18, after the book
shelf and Import (rc17), which it builds on.

## Why

The owner does not want to lose version data, locally or remotely. Today versions are kept in
three unconnected places, each with a gap:

- **Scene history** (`.grimoire/history/<scene>/<date>.md`): copies of Markdown scenes and notes
  before their first change in an app session, at most every five minutes while saving, and
  before drastic changes. It stays inside the book and is excluded from readable Drive sync.
  Configured encrypted project backups include it.
- **Writing sessions** (git, `~/.local/share/grimoire/sessions/<book-id>.git`): opt-in, one commit
  per sitting, covering ordinary book files allowed by `.gitignore`, including notebook files
  (`.grimoire/` ignored except `resume.md`, and `exports/` ignored); off for every book on the
  owner's laptop; never leaves the laptop.
- **`WrOS-History/` on Drive** (wros-sync): retains local replacements and deletions and remote
  deletions. Since the rc17 fix for #148, saved changes schedule a sync after a 90-second
  pause; periodic sync is due every five minutes, with a final run on book close and app power
  actions. Uploads update the existing file so Drive's revision chain accumulates. History is
  not trimmed.

The owner wants one timeline that reads like "minor fix · minor fix · small change · end of
Tuesday evening's session", covering the whole book, kept both on the laptop and on Drive, so
that losing either side loses nothing.

## Goals

1. **One timeline per book**, on by default on WrOS: small saves while writing, a marked session
   at the end of each sitting, and an entry for everything sync brings in from Drive.
2. **The whole book**: manuscript, notes, front/back matter, `novel.toml`, and Grimoire's book
   state (story data, review notes, journal, productivity, trash of cut scenes, resume point).
3. **Kept forever, both sides.** Nothing in the timeline is ever trimmed. The laptop keeps the
   full repository; Drive keeps the same history as append-only bundle files.
4. **Survives either side's loss.** Laptop lost or reinstalled: importing the book from Drive
   brings back its full timeline. Drive emptied: the laptop still has everything and re-uploads.
   Repository damaged: rebuilt from Drive.
5. **Readable without WrOS.** The repository and every bundle are standard git; any git client
   can open them (`git clone "<file>.bundle"`).
6. **Scene history keeps working** (`H` in the outline, restore with `Enter`), now reading from
   the timeline.

## Non-goals

- Encrypted backups (restic, recovery key): separate rework, unchanged here.
- Syncing a book between laptops through git. Plain-file sync through `WrOS/<book>/` stays the
  way books move between devices; the timeline records history, it does not merge.
- Pushing to GitHub or any git server.
- Grimoire on Mac/Windows without git: keeps today's plain-file scene history.

## Anti-goals (failure even if it "works")

- Any upload that overwrites or deletes an earlier history file on Drive.
- A save, sync or quit that waits on git or the network while the writer is typing.
- A damaged repository that silently stops recording, or takes the plain files with it.
- History that only one laptop can read.

## Constraints

- HP Stream 11: Celeron N4000, 4 GB RAM, eMMC. Git work off the typing thread, with the deadlines
  and no-prompt rules `grimoire_core::sessions` already applies.
- Offline-first: the timeline records with no network; uploads wait and catch up.
- Uses only git and rclone, both already in the WrOS system closure; Drive access stays in the
  private `wros-google:` rclone remote.
- Plaintext on Drive, like the books themselves: no key to lose. (Consequence, owner-visible:
  the journal, productivity figures and review notes go to Drive too, which today stay on the
  laptop.)
- The book folder itself is not changed to make this work (no `.git`, no `.gitignore` written
  into the book).

## Approaches considered

- **A. Local git timeline, mirrored to Drive as incremental git bundles (chosen).** Each upload is
  one new file holding only what is new since the last; files are never replaced. Standard git,
  nothing new to install.
- **B. git-remote-rclone (push to Drive through rclone).** A tested tool, but it stores the
  repository as an archive that is replaced on every push (each push overwrites the last), and it
  is an extra package.
- **C. Restic snapshots.** Deduplicated and already used for encrypted backups, but always
  encrypted: brings back the recovery key the owner does not want to depend on.
- **D. Drive's version history only.** Versions expire after 30 days or 100 per file. Kept as an
  extra layer (#148), not as the record.

## Design

### The repository

- Lives where Writing sessions already keeps it: `~/.local/share/grimoire/sessions/<book-id>.git`,
  bare, with the book as its work tree (`grimoire_core::sessions::Store::Outside`). This feature is
  Writing sessions grown up; an existing sessions repository is the starting point of the
  timeline, so nothing already recorded is lost.
- Turned on for every book on WrOS, on first open after the update; there is no off switch on WrOS
  (the owner's rule: no version data lost; *designer's call, owner to confirm*). Elsewhere, Writing
  sessions stays opt-in as today.
- Two laptops writing the same book each keep their own repository; changes made on the other
  laptop arrive through plain-file sync and are recorded as "Changed on Drive" entries.
- What is left out is listed in the repository's own `info/exclude`, never in the book. A
  `.gitignore` inside the book (Writing sessions wrote one into older books; a writer may have
  their own) must not hide the writer's files: the recorded set is exactly the one below, whatever
  the book's `.gitignore` says. *(Builder: choose the git mechanics, e.g. staging with
  `git add --force` over the book minus the exclusion list, and test it with a hostile
  `.gitignore`.)*
- Written durably: `core.fsync=committed`, `core.fsyncMethod=fsync` (power cuts on eMMC).

### What is recorded

Everything in the book folder except:

| Excluded | Why |
|---|---|
| `.grimoire/history/` | Superseded: the timeline holds those versions (see Scene history below) |
| `.grimoire/recovery/`, `.grimoire/retained-history/` | Crash-recovery scratch, this machine only |
| `.grimoire/sync.json` | This laptop's link to a Drive folder; Import rewrites it from Drive's marker |
| `exports/` | Compiled output, rebuilt from the book |
| `.wros-*` temporary folders, parked `… (from Drive, …)` copies *while unsettled* | Transient; the settled result is recorded |

Recorded includes `.grimoire/writingdeck/` (story data, journal, productivity, ledger),
`.grimoire/trash/` (cut scenes), `progress.toml`, `days.txt`, `resume.md`, `book-id`,
`annotations.json`.

*(Builder's call, recorded: parked conflict copies are recorded once settled, because the
settled file is the writer's decision; recording each unsettled copy would only add noise.)*

### Entries

- **Small saves.** At the moments scene history snapshots today (at most every 5 minutes while
  writing; the before-this-session version; before a restore or a replace across the book), and
  after each sync that changed local files. One commit of the whole recorded set; skipped when
  nothing changed.
- **Labels**, from the words changed since the previous entry (builder tunes the thresholds):
  "minor fix" (a few words), "small change", "change", "big change"; "Changed on Drive" for a sync
  download (names the files); "Before restoring …" / "Before replacing across the book".
- **Sessions.** When the writer quits, after 30 minutes without typing, or before sleep/shutdown,
  the session is marked with an annotated tag `session/<YYYY-MM-DD-HHMM>` on the last entry, with
  the diary label Writing sessions already writes ("Tuesday evening · Act Two · 1,240 words").
  The main line stays linear; any git client shows sessions as tags.
- Commits run on a worker; a save never waits for git. If git fails, the plain-file fallback below
  takes over and the writer is told once, plainly.

### Scene history reads the timeline

- `H` lists the versions of the selected scene from the timeline (commits that changed it), newest
  first, with each entry's label and time; sessions shown as dividers. `Enter` restores as today
  (and records "Before restoring …" first).
- Old plain-file history (`.grimoire/history/`) is still listed, after the timeline's versions, so
  nothing kept before this change disappears from the screen.
- **Fallback:** the before-this-session copy of each scene is still written as a plain file in
  `.grimoire/history/` (one per scene per session, as today). If the repository is missing or
  damaged, scene history keeps working from plain files exactly as today, and says the timeline is
  unavailable.

### On Drive

```
WrOS/<book>/                                   plain files, as today (phone-editable)
WrOS-History/<book>/timeline/<machine>/000001 2026-11-16 2140.bundle
                                      /000002 2026-11-16 2310.bundle
                                      ...
WrOS-History/<book>/<date>/…                    old per-file copies, kept, no longer added to
```

- `<machine>`: a random id made once per installed system, stored in the data folder, plus the
  hostname for people reading the folder ("hp-stream-3f9a").
- **When:** at the end of each session, every hour during a long session, and on reconnecting.
  Offline: queued; the queue survives restarts.
- **What:** `git bundle create` of every ref, excluding what earlier bundles in this machine's
  series already hold (the local repository remembers each uploaded series tip under
  `refs/wros/uploaded/<n>`). Verified with `git bundle verify` before upload, uploaded under a
  temporary name, checked by MD5 against Drive, then renamed to its final name. A name that already
  exists is never overwritten (the upload stops and says why).
- Bundles are never deleted or replaced by WrOS. (Consolidating many small bundles into one is a
  possible follow-up; at roughly one bundle per session it is not needed for years.)
- **Damaged repository:** WrOS rebuilds it by fetching every bundle of every machine in order, then
  records the current files as a new entry, then carries on.
- **Drive emptied or a bundle missing:** the next upload sends a full bundle (all history) as the
  next number, so Drive again holds everything.

### Import and fresh installs (with the book shelf design)

Importing a Drive book (shelf → Import a book… → Google Drive) also fetches
`WrOS-History/<book>/timeline/`:

- Every machine's bundles are fetched into a new local repository; each machine's line is kept
  under `refs/machines/<machine>/…`.
- This laptop's line starts from the newest entry whose files match what was imported (normally
  the last upload of whichever laptop wrote last). If the imported files differ from every entry
  (phone edits since), they are recorded as "Brought in from Drive" on top of it.
- This laptop gets its own new series on Drive; it never writes into another machine's folder.
- Scene history therefore shows a fresh install the book's whole past, from all laptops.

### Relationship to #148

- Kept: update Drive files in place (Drive's own version history as an extra layer); sync after a
  pause in typing.
- Superseded: per-file copies into `WrOS-History/<book>/<date>/` stop. Sync downloads are recorded
  in the timeline before and after, so phone edits are never only on Drive. Trimming `WrOS-History`
  is dropped (nothing new goes there and the timeline is never trimmed).

## Size and cost (estimate)

A novel's text is about 0.5–1 MB; git stores changes compressed. With small saves every 5 minutes
during writing, a year of daily writing is expected in the low tens of MB per book, locally and on
Drive. Uploads are one small file per session. The builder measures this on a real book and
reports it.

## Left to the builder

- Exact label thresholds and wording; the idle time that ends a session (suggest 30 minutes).
- Exact git invocation that records the set above regardless of the book's own `.gitignore`.
- How the worker queues commits and uploads; how a failed upload is retried and shown.
- Migration: books with Writing sessions repos (in book or outside), books with neither.
- Tests:
  - unit: recorded set (exclusions, `.gitignore` in the book cannot hide files), labels, bundle
    ranges (no gaps, no overlap), restore from bundles of two machines, damaged-repository rebuild;
  - sync: a download is recorded before and after; no `WrOS-History/<date>` copies are made;
  - VM: write, quit, upload; wipe the laptop's data; import the book from a fake Drive
    (rclone alias remote) and see every version in scene history; empty the fake Drive and see a
    full bundle re-uploaded;
  - an HP Stream timing check: a small save never delays typing.

## Follow-ups (separate work)

- A timeline view of the whole book (sessions, then saves inside them), beyond per-scene history.
- Consolidating bundles, if the count ever matters.
