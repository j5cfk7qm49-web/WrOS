# History & recovery

WrOS never lets go of your words. There are several nets under them.

## Autosave

Every scene is saved two seconds after you stop typing, and never waits longer than twenty. Each save goes through a temporary file, so a crash can't tear a scene in half. Closing the terminal window, or the computer shutting down, saves first too.

## History

Every scene keeps dated copies of itself as you work: at most one every five minutes, plus how it was before this session's first change and before anything drastic (a replace across the book, restoring an old version).

- `H` in the outline (or *Scene history* in `Ctrl-K`) shows the scene's history. `↑` `↓` choose a version and the pane shows what's different from now.
- `PgUp` `PgDn` or `Space` scroll the differences.
- `Enter` restores that version. Restoring is itself undoable, and today's words go into the history first.

The copies live in `.grimoire/history/`, as plain Markdown.

## Recovery

If a save fails or WrOS catches an application crash, it tries to keep unsaved words in `.grimoire/recovery/`. A power cut can lose changes that have not reached disk. Next time the book opens, WrOS offers any recovery copies back:

- **Restore them**: the saved versions go into each scene's history first.
- **Keep the saved versions**: the recovered words go to the Trash, not away.
- **Decide later**: asked again next time.

Choose with `↑` `↓` and press `Enter`; no other key decides, so a stray keystroke at launch can't lose anything.

## The Trash

Deleting moves things to the **Trash** section at the bottom of the outline, and `Ctrl-Z` brings them back to where they were. Only deleting something that is already in the Trash removes it for good, and WrOS asks first.
