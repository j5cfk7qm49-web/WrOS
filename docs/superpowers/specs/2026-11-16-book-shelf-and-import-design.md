# Book shelf and import (design)

Status: approved direction (owner, 2026-11-16); target release rc17.

## Why

On a fresh rc14 install the owner tried to get their books back from Google Drive. The copy
worked, but nothing on screen said it was running or where it went; the books landed nested
in `Documents/From Drive - WrOS/`; and opening one meant typing its whole path, where a name
with spaces copied from a terminal (`'Clip says Hello World.'`) silently matched nothing.
Bringing writing into WrOS is spread over three places (File › Open another book…,
Settings › Backups and accounts › Google Drive › Your Drive files,
Settings › Backups and accounts › Backups), each with its own typed path.

The owner wants one obvious way to see their books and one obvious way to bring a book in,
from anywhere, for fresh starts and for any other import.

## Goals

1. **Shelf.** File › Open another book… lists every book in `~/Documents` automatically,
   whether or not this laptop has opened it before. No typed paths.
2. **Import.** One "Import a book…" entry on the shelf. Sources: Google Drive (readable synced
   books), any USB stick plugged in, and other folders on this laptop. Browsing a source shows
   folders; only folders with Grimoire's book structure can be picked.
3. **Copy in.** A picked book is copied into `~/Documents`, checked, then opened. The source is
   never changed.
4. **Drive books keep syncing.** A book imported from Drive comes in with readable sync already
   on, tied to the same Drive folder.
5. **Conflicts.** If `~/Documents` already holds that book: Replace, Merge (through the existing
   conflict screen), Keep both, or Cancel.
6. **Drafts.** "Bring in a draft" (Word, Markdown, text) moves into Import, as a source type,
   so every way of getting writing in is in one place.
7. **Visible progress and result.** While a copy runs, the screen says so and stays saying so;
   at the end it says where the book is and opens it, or says plainly why it did not.

## Non-goals

- Encrypted backups (restic, recovery key). The owner wants that whole system revisited
  separately; Settings › Backups and accounts › Backups stays as it is for now.
- Moving books (import copies; the source stays).
- Syncing with anything but Google Drive.
- Changing what a book is on disk.

## Anti-goals (failure even if it "works")

- Any path the writer must type to open or import a book.
- A copy, replace or merge that changes the source, or overwrites the writer's words without a
  choice they made on screen.
- A copy that runs, finishes or fails with nothing visible.
- A folder that is not a book offered as one (or a book hidden because of its name).

## Constraints

- Keyboard-only text UI (ratatui), 1366×768 at the default font, Colemak and QWERTY users; every
  screen reachable and usable without a mouse.
- HP Stream 11: 4 GB RAM, eMMC; scans must stay fast (bounded depth, no hashing to list).
- Offline-first: the shelf and laptop/USB imports work with no network; Drive rows say clearly
  when Drive can't be reached.
- Privilege: the app runs as the writer. Reading a USB stick needs a mount, which needs root:
  a narrow helper behind a polkit rule, like `wros-update-media` (read-only, nodev, nosuid,
  noexec; one stick at a time; unmounted when Import closes).
- Drive access stays in `wros-backup` (rclone, `wros-google:` remote, the verified copy that
  `drive-materialize` already does); sync stays in `wros-sync`.

## Approaches considered

- **A. The Books screen becomes the shelf plus an Import screen, reusing the helpers that already
  work (chosen).** Shelf, browsing, laptop/USB copies and conflict handling live in the app,
  which already knows what a book is (`books::is_book`) and owns the conflict screen. Drive
  copies go through `wros-backup drive-materialize`; sync is switched on with `wros-sync`. A new
  root helper mounts a USB stick read-only.
- **B. A new import helper does all the work; the app only shows lists.** Easier to test in
  isolation, but duplicates the app's knowledge of what a book is and how conflict copies are
  named; if they drift, merges stop appearing in the conflict screen.
- **C. Patch the existing pieces in place** (folder browser instead of a typed path, Drive copies
  land in Documents). Quickest, but import stays split between Books and Settings, which is what
  made restoring confusing.

The owner chose A.

## Design

### What counts as a book

A folder is a book when it contains a `manuscript/` directory (`books::is_book`). `novel.toml`
supplies the title when present; otherwise the folder name. Hidden folders (`.grimoire`,
`.git`, `.wros-drive-materialize-*`) are never shown or descended into.

### The shelf (File › Open another book…)

```
 Books
 ──────────────────────────────────────────────────────────
  Clip Says Hello World        Documents/WrOS/Clip says Hello World.
  Grimoire                     Documents/Grimoire              ← open now
  ──────────────────────────────────────────────────────────
  Import a book…
  New book…
```

- Lists every book under `~/Documents`, searched to a bounded depth (owner's own layout already
  nests one level: `Documents/WrOS/<book>`), title first, location after, the open book marked.
- Recent books outside `~/Documents` (opened before this change) still show, after the
  Documents books, until they are imported or forgotten.
- Delete (existing `x`) keeps its current confirmation.

### Import a book…

```
 Import a book                    Import from Google Drive › WrOS
 ──────────────────────────       ───────────────────────────────────
  Google Drive                     Clip says Hello World.   book · synced
  USB stick "KINGSTON"             Grimoire                 book
  A folder on this laptop          Old drafts/              folder
  A draft (Word, Markdown, text)   Up one folder
```

- Sources: Google Drive (only when linked; otherwise a row that leads to linking), each mounted
  USB stick by its label, "A folder on this laptop" (starts at the home folder), and drafts.
- Browsing: folders that are books are pickable and say so; other folders open; files are not
  listed (except in the draft source). "Up one folder" at every level below the source's top.
- Drive browsing uses `wros-backup drive-list` for names and entry types. Add bounded inspection
  to determine whether a folder contains `manuscript/` and a `.wros-sync.json` marker before
  marking it as a book or synced.

### Copying in

- Destination: `~/Documents/<book folder name>`, numbered (" 2", " 3") if taken by something
  that is not the same book. Laptop and USB sources: copy with the app, then compare every file
  (size and hash) before the book is opened; a failed check removes the partial copy and says so.
  Drive source: `wros-backup drive-materialize` into that destination (it stages, checks against
  Drive with `rclone check`, and only then publishes).
- Progress: a line on the Import screen that stays until the copy ends ("Copying Clip Says Hello
  World from Google Drive…"), then the result ("Copied to Documents/Clip says Hello World and
  checked. Opening it."). Failures name the cause and leave nothing half-copied behind.
- Escape during a copy cancels it cleanly (nothing kept), with a confirmation.

### Drive books keep syncing

After a Drive import the app writes the book's `.grimoire/sync.json` from the Drive folder's
marker (`project_id` from `.wros-sync.json`, `drive_folder` = the folder's name under `WrOS/`) and
asks `wros-sync` to enable it (which keeps that identity and folder). The first sync then finds
both sides identical and changes nothing. A Drive folder without a marker (not a WrOS-synced
book) imports as an ordinary copy; sync can be switched on from Backups as today.

### Same book already in Documents

"The same book": both have a `project_id` (synced books) and they match; otherwise the same
folder name. Builder's call, recorded here: if the local copy is already synced to the very Drive
folder being imported, there is nothing to import — say so and offer to open it (sync brings any
difference over).

Otherwise the choices, newest dates shown for each side:

- **Replace:** the incoming book is copied in first and checked; only then is the local book
  renamed beside it to `<folder> (replaced <date>)`, so it stays on the shelf as
  "<title> (replaced 16 Nov)" until the writer deletes it there. (Deleting from the shelf is
  permanent today, so Replace must not rely on it. Owner-visible; recorded as a design call.)
- **Merge:** scenes and files only one side has are added; each `.md` file that differs comes in
  beside the local one as `<name> (from <source>, <date time>).md`, the name the conflict screen
  already recognises; the book opens on the conflict screen to settle them. Other files that
  differ (`novel.toml`, images) keep the local version, and the result line lists them. The
  local `.grimoire/` state (history, progress, sync identity) is kept as it is.
- **Keep both:** the incoming book is copied in as a separate book (numbered folder name). A
  synced Drive book imported this way is not linked to sync (only one local book can own a Drive
  folder); the writer can link it later.
- **Cancel.**

### Drafts

The existing draft import (`grimoire_core::import`) becomes the "A draft" source: browse to a
`.docx`, `.md` or `.txt` file; it becomes a new book in `~/Documents`.

### What leaves Settings

The Drive files page (Settings › Backups and accounts › Google Drive › Your Drive files) keeps browsing and
copying single files, but its "copy this folder" for books points to Import; book restore from
encrypted backups stays in Backups (non-goal above).

## Left to the builder

- Search depth under `~/Documents` (suggest 3) and how the shelf orders books (suggest by last
  opened, then title).
- The trash location for Replace, reusing the shelf's delete; numbering style.
- The USB helper's name, polkit action and how a stick is chosen when several are plugged in.
- Whether laptop/USB copies run on a worker thread or a helper process (they must not block
  typing).
- Tests: unit tests for book detection, destination naming, same-book matching and merge naming
  (round-trip through `grimoire_core::sync::copy_of`); UI tests for the shelf and Import screens;
  a VM test that imports from a USB disk image and from a fake Drive (rclone `alias` remote).

## Follow-ups (separate work)

- Revisit encrypted backups: export/import as easily as plaintext, without a recovery key that
  can be lost.
