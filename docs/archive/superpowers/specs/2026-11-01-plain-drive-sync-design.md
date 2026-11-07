# Plain Google Drive sync with versions and paragraph merge — design

## Intent

The writer wants their projects in Google Drive as ordinary Markdown files they
can open and edit on a phone or another computer, with edits made there coming
back into WrOS automatically. They are more comfortable with readable files in
Drive than with encrypted snapshots that only WrOS or Restic can open. Old
versions must be kept, and when both sides changed the same file, WrOS should
combine the edits itself where it can and ask only about the paragraphs both
sides changed.

This is the "bidirectional sync" guarantee in `docs/SYNC_AND_BACKUP.md` (item 4),
which until now was Planned. It sits beside the encrypted Restic backups, which
stay exactly as they are.

## Goals

- Per project, opt-in: **Sync this project with Google Drive (readable files)**.
  Off by default. The switch states plainly that the files in Drive are not
  encrypted and anyone with access to the Google account can read them.
- Two-way and automatic while WrOS is running and online: a few seconds after a
  save, when the project is opened, and every few minutes. Offline, nothing
  blocks; changes wait and go when the network returns.
- Every file a sync replaces or deletes, on either side, is kept as a dated
  version in Drive first.
- Edits on both sides to different paragraphs of a text file are combined
  without asking. Paragraphs changed differently on both sides are offered one
  at a time: **Mine**, **Drive's**, **Both** (mine first) or **Edit**.
- The writer can see where sync stands: last synced, waiting (offline),
  files needing a merge, reauthorization needed.

## Non-goals

- Google Docs conversion or round trips. Files stay Markdown in Drive.
- Syncing anything outside a project folder, or WrOS settings and credentials.
- Real-time co-editing, or syncing while WrOS is not running.
- Any change to the encrypted backup, its recovery key or its restore checks.
- Detecting renames: a rename is a delete plus a new file (both kept in history).
- A second WrOS computer syncing the same Drive folder, for now. The writer
  wants it possible later, so nothing may rule it out: the project ID lives in
  the project, the base record is per computer, and the Drive folder records
  which project ID owns it. Until then, switching sync on refuses a Drive
  folder already owned by a different computer's record and says why.

## Anti-goals (failure even if it "works")

- Losing words: no path may overwrite or delete a version that isn't first kept
  in history or still present on the other side.
- Changing text under the writer's cursor. A Drive change to a file that is
  open and unsaved waits until it is saved; it never replaces the buffer.
- Wiping Drive or the project because one side looks empty or a folder went
  missing (unmounted disk, wrong account, renamed Drive folder).
- Blocking or slowing typing on network or Google failures.
- Credentials or tokens in the project, in Drive, in logs or in evidence.
- A merge that silently picks a side where both sides changed the same paragraph.

## Constraints

- Debian trixie ships rclone **1.60.1**, whose `bisync` is beta and has no
  conflict-resolution controls. WrOS keeps its own record of the last synced
  state and uses plain rclone operations (list with hashes, copy one file,
  move one file, delete one file) against the existing private
  `wros-google` remote and `rclone.conf` from the backup feature. No new Google
  authorization step.
- Follow the rules already in `docs/SYNC_AND_BACKUP.md`: a base-version record,
  deletion records (tombstones), conflicts preserve both sides, and incoming
  changes are applied through the existing guarded save path (`Project`
  fingerprints, `seen`, parked copies), never by writing behind the editor.
- The project limits and refusals from encrypted backup apply: 5 GiB, 100,000
  entries, 64 levels; symlinks and special files are refused, not followed.
- `docs/NATIVE_BACKUP.md` says no unprompted background network upload runs
  during writing. This feature is the deliberate, opt-in exception for projects
  where the writer switched it on; the docs must say so.
- The network-disabled writing guarantees and existing tests must still pass
  with sync switched off, and with it switched on but offline.

## Decided how

**What syncs.** Everything in the project folder except hidden files and
folders (`.grimoire/`, `.git/` and any other dot-name) and WrOS's own temporary,
recovery and lock files. *(My call: `.grimoire/` holds pins, journal facts and
other project metadata that already have their own guarded conflict rules; the
writer asked to see the Markdown.)*

**Where in Drive.** `WrOS/<project folder name>/` for the files and
`WrOS-History/<project folder name>/<YYYY-MM-DD>/<path> (<HH-MM-SS> <from WrOS|from Drive>)<ext>`
for kept versions. *(My call on names; the writer only asked that versions be
kept.)* History older than **90 days** is pruned by WrOS. Google's own per-file
revision history is a bonus, not relied on.

**Base record.** Per project, in the private state directory
(`$XDG_DATA_HOME/grimoire/wros/sync/<project id>/`, mode 0700): for every synced
path, the content hash at the last successful sync, Drive's ID, size, modified
time and MD5, and, for text files, a copy of the text itself (the merge's
common starting point). Deletions are recorded so a file deleted on one side is
not resurrected by the other.

**Each run** compares three states per path — base, local, Drive:

| Local | Drive | Result |
|---|---|---|
| changed | same as base | upload; Drive's previous version to history first |
| same as base | changed | apply Drive's version locally through the guarded save path (waits if open and unsaved); local previous version to history |
| changed | changed (text) | three-way paragraph merge, below |
| changed | changed (not text) | keep both: Drive's saved beside it as `name (from Drive, 1 Nov 12:30).ext`, both uploaded |
| deleted | same as base | delete in Drive after moving it to history |
| same as base | deleted | delete locally after copying to history (through the project's own trash) |
| deleted | changed, or changed / deleted | the changed one wins and is restored on the other side |
| new | new, different | treated as both changed with an empty base |

**Mass-delete guard.** A run that trips the guard (defined below) stops,
deletes nothing and asks the writer.

**Paragraph merge.** A paragraph is a block between blank lines; YAML
frontmatter is one block. Blocks changed on only one side are taken from that
side. Blocks changed on both sides to the same text are taken once. Blocks
changed differently on both sides are a clash:

- With no clashes, the merged text is saved locally (guarded save), uploaded,
  and both pre-merge versions go to history.
- With clashes, the local file stays as the writer's version. Drive's version
  is saved beside it as a parked copy `name (from Drive, 1 Nov 12:30).md`, using
  the existing conflict-copy mechanism (`sync::CopyOf`, `Source::GoogleDrive`),
  so it is shown, never compiled or counted. The file is marked **needs
  merging**, and sync of that one file pauses until it is settled.

**Merge screen.** The existing Conflicts list and Settle overlay
(`src/app/overlays/conflicts.rs`) gain a fourth choice for Drive copies:
**Merge paragraph by paragraph**. It walks the clashing paragraphs one at a time
showing my text and Drive's, with **Mine / Drive's / Both / Edit**. Finishing
writes the merged text through the guarded save, trashes the parked copy, and
undoes with Ctrl-Z like the other settle choices. The existing Take copy, Keep
mine and Keep both stay available.

**Where it runs.** The backup helper's process (`os/scripts/wros-backup` or a
sibling helper using the same credentials) does all Google traffic: listing,
downloading into private staging, uploading and moving to history. It never
writes into the project folder. The app applies incoming files and merges
through `grimoire-core`'s guarded `Project` path. The paragraph merge is a pure
function in `grimoire-core` so it is tested without a network.

**Visible status.** Settings → Accounts → Backups shows, per project, the switch,
**Sync now**, **Pause**, last synced time and state (synced, waiting offline,
N files need merging, reauthorization needed, stopped: would delete N files).
The editor status line shows "chapter-03.md needs merging" when it applies.

**Timing.** Sync **10 s** after the last save, on project open, and every
**5 min** while online. *(My call on numbers.)* Only the project open in WrOS
syncs; another opted-in project catches up when it is next opened, so no
project registry is needed and every change goes through a loaded `Project`.
*(My call.)*

**Project identity and the first sync.** *(My calls.)* A random project ID is
stored in the project's own `.grimoire/` metadata the first time sync is
switched on, so a renamed or moved folder keeps its Drive folder and base
record. The Drive folder name is taken from the project folder at that moment
and recorded; a name already used by another project's ID gets ` (2)`.
Switching on with a non-empty Drive folder runs a careful first sync: files
identical on both sides become the base; a file on one side only is copied
to the other; a file that differs becomes a parked Drive copy beside the
local one (no merge, since there is no common starting point); nothing is
deleted on a first sync.

**Parked copies.** *(My call.)* WrOS's own parked copies (`… (from Drive, …).md`)
stay local and are never uploaded; settling one makes no Drive change except
the merged result. Conflict copies created by Google Drive itself come down
as ordinary files and appear in WrOS's existing Conflicts list. A copy named
"(from Drive, …)" is read as `Source::GoogleDrive`.

**History pruning.** *(My call.)* Versions older than 90 days are pruned,
except the newest kept version of any path that no longer exists on either
side, which is kept until the writer deletes it.

**Mass-delete guard (refined).** *(My call.)* The run stops and asks when it
would delete more than 5 files on either side at once, or when either side's
project folder is missing or empty while the base record is not.

## Left to the builder

- State file format and layout; whether transport lives in `wros-backup` or a
  new helper; which crate holds the sync state machine.
- Exact UI wording beyond the labels above, and the merge screen layout within
  the existing overlay style.
- The paragraph diff algorithm, as long as the outcomes above hold.
- Test harness: an rclone `local` backend standing in for Drive is expected,
  plus fault injection (interrupted upload, Drive change mid-run, offline).

## Not proven by tests

A real Google account, Google's own conflict behaviour on phone edits, and long
offline gaps need a live check on the writer's own account before this is
called done, as the existing Drive backup docs already require.
