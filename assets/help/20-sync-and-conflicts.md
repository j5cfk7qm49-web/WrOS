# Sync & conflicts

## Encrypted backups and restore

Open Settings › Backups and accounts › Backups. It creates encrypted Restic
snapshots on a local backup drive or Google Drive. Export the recovery key to a
private separate device: losing it makes the backup unreadable.
Open Settings › Backups and accounts › Google Drive. Choose Link Google Drive
with your phone when offered. Manual authorization with `rclone authorize drive`
on a trusted computer remains available. Local editing works offline. Configured
backups, Drive sync and update checks can use the network in the background.

Create encrypted backup saves both buffers and refuses source conflicts. It
stages a consistent saved project, creates an encrypted snapshot, restores it,
and compares every file name, size and SHA-256 before reporting success. Select
a snapshot to restore into a NEW folder; existing destinations are refused.
Connect existing repository and recovery key works after losing the original
machine. A same-disk folder is not protection from losing that disk. Refresh
snapshots explicitly after reconnecting; use stale-lock recovery after a crash.
Cancel stops the operation and preserves the active manuscript. Delete clears a
setup field. Configured backups run after saved changes and ten minutes without
editing, at most hourly, with delayed retries after failures. Readable Google
Drive sync runs on opening, 90 seconds after the last save or edit, and every five
minutes while the project is open. Closing the project or shutting down asks for a
final sync after saving; retry the action once the sync finishes. Sleep starts a
final sync and locks straight away. Offline attempts do not prevent leaving. A phone or a lost laptop may miss the last couple
of minutes of writing. Uploads update the same Drive file, keeping its revision
history. Google Docs round trips remain unavailable.
Full guidance: docs/NATIVE_BACKUP.md in the installed documentation.

## Projects on Google Drive

Open Settings › Backups and accounts › Google Drive › Your Drive files.
It lists your whole Drive, folders first. Items with a copy on this laptop say so. A file or a
whole folder copies to a new place here, and the copy is checked against Drive
before it is kept. When the folder you are in is a project, the first row is
*Import this project…*. The copy goes beside the project you have
open, and opens once the check passes. You write on that copy; the project on
Drive stays as it was.

A project is a folder, so any sync service carries it: Dropbox, Google Drive, pCloud, Box, OneDrive, iCloud Drive. WrOS checks for outside changes and keeps conflicting versions for review. Keep a separate verified backup too.

## Two devices change one scene

**Nothing is ever written over.** Before every save WrOS checks the file on disk. If it changed somewhere else, the other version stays the scene, and your words from here are kept beside it as a *parked copy*.

Sync services make their own copies when two devices clash: Dropbox's "conflicted copy", pCloud's "(conflicted)", Box's "(1)", and so on. WrOS recognises all of them. They appear right under their scene, marked with where they came from ("Dropbox copy"), and they're never counted or exported twice. They move with their scene.

While there are any, the status bar says `⚠ 1 conflict`.

## Settling a conflict

*Settle conflicts…* (menu or `Ctrl-K`) lists every copy. Choose one and it opens beside its scene, with how much longer or shorter it is. Then:

- `1` **take the copy**: the scene's own words go into its history first;
- `2` **keep the scene**: the copy goes to the Trash;
- `3` **keep both**: the copy becomes a scene of its own, right after the original.

A copy from Google Drive also offers `4` **merge paragraph by paragraph**. A paragraph changed on only one side is kept as it is. Where both sides changed the same paragraph, you see both versions with the words that differ marked. Press `1` for yours, `2` for Drive's or `3` for both. `Tab` picks which version the arrow keys scroll; the mouse wheel scrolls the one under the pointer.

One `Ctrl-Z` takes it back. Nothing is ever deleted.

## Files that aren't here yet

A file your sync service hasn't downloaded (online-only, still arriving, or offline) is shown in the outline as *offline* and is never saved over. WrOS reads it once it arrives. Export waits until every scene can be read.

## Files that blink

Some services replace a file by removing it for a moment. A scene that vanishes briefly isn't treated as deleted, and a file still being written isn't read until it settles.

## Names

Everything WrOS names is safe on every service and computer: no characters Windows forbids, nothing too long, and never two names that differ only by capitals or accents (on a Mac, on Windows and in Box, `Wren.md` and `wren.md` are the same file). If two such files are already in a folder, both are shown marked *name clash* for you to rename.

## Good habits

- Keep the project's folder available offline ("Always keep on this device", "Make available offline").
- Don't put one project in two sync services at once.
- Let the service finish syncing before you open the project on another device.

At launch the status bar says which service the project is in.
