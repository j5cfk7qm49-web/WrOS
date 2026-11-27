# History & recovery

WrOS never lets go of your words. There are several nets under them.

## Autosave

Every scene is saved two seconds after you stop typing, and never waits longer than twenty. Each save goes through a temporary file, so a crash can't tear a scene in half. Closing the terminal window, or the computer shutting down, saves first too.

## History

On WrOS, every project keeps a timeline of small saves, writing sessions and changes from Drive. Scene history shows the scene's timeline versions newest first, with labels and dividers for marked sessions. For a project that syncs, importing it from Drive brings its timeline back. See *Writing sessions* for where the timeline is kept and what goes to Drive.

- `H` in the outline (or *Scene history* in `Ctrl-K`) shows the scene's history. `↑` `↓` choose a version and the pane shows what's different from now.
- `PgUp` `PgDn` or `Space` scroll the differences.
- `Enter` restores that version. Restoring is itself undoable, and today's words go into the history first.

Older plain Markdown copies in `.grimoire/history/` appear below the timeline. A copy from before the first change in a sitting is still kept there, and if the timeline is unavailable you can use the saved copies. Outside WrOS, scene history still keeps dated copies as you work, before the first change of a session and before a restore or a replace across the project.

## Recovery

If a save fails or WrOS catches an application crash, it tries to keep unsaved words in `.grimoire/recovery/`. A power cut can lose changes that have not reached disk. Next time the project opens, WrOS offers any recovery copies back:

- **Restore them**: the saved versions go into each scene's history first.
- **Keep the saved versions**: the recovered words go to the Trash, not away.
- **Decide later**: asked again next time.

If the scene some of those words belonged to has gone since (deleted, or renamed on another computer), they're still offered. Restoring keeps them in the Trash as a file marked *recovered*, to read, copy from, or bring back with `u`.

Choose with `↑` `↓` and press `Enter`; no other key decides, so a stray keystroke at launch can't lose anything.

## The Trash

Deleting moves things to the **Trash** section at the bottom of the outline, and `Ctrl-Z` brings them back to where they were. Only deleting something that is already in the Trash removes it for good, and WrOS asks first.
