# Writing sessions

Writing sessions mark your sittings like a diary — *Tuesday evening · Act Two · 1,240 words* — so you can see what changed each time you sat down.

## On WrOS

Every project keeps a timeline of small saves, writing sessions and changes that come in from Drive. It's on from the first time you open the project. `H` in the outline shows the selected scene's versions. *Save this session now* marks a sitting whenever you like.

## Looking back

*Writing sessions* lists every session, newest first.

- `Enter` (or `→`) shows the scenes that session changed; `Enter` on one shows the words that changed.
- `←` or `Esc` goes back; `s` saves a session now; `Esc` closes.

## Where it's kept

WrOS keeps the timeline on **this laptop**, in `~/.local/share/grimoire/sessions/`. When a project syncs with Google Drive, WrOS also sends Git bundles to `WrOS-History/<project>/timeline/`. Importing the project from Drive brings those versions back. Any Git client can read the timeline. To open a full bundle, use `git clone "<file>.bundle"`. Later bundles may need the earlier ones.

Your journal, productivity figures and review notes go to Drive with the timeline, in plaintext like your project. Normal sync no longer makes dated copies in `WrOS-History/<project>/<date>/`. If a timeline save fails before sync, WrOS still keeps a dated copy of a local file it replaces or removes. Encrypted backups and restore are available in Settings › Backups and accounts › Backups.

## Outside WrOS

Writing sessions work as before and need Git installed. *Writing sessions* in `Ctrl-K` offers to turn them on for this project (`y`). Each session is kept when you quit or use *Save this session now*. History stays on this computer, outside the project's folder, in Grimoire's data folder (`~/.local/share/grimoire` on Linux, `~/Library/Application Support/grimoire` on a Mac, `%APPDATA%\grimoire` on Windows). Grimoire never pushes it automatically.

## Older projects

On WrOS, existing writing history becomes the start of the timeline. Outside WrOS, a project from an earlier version may still keep its history in a `.git` folder inside the project. *File* offers **Move writing history out…** (in `Ctrl-K`, *Move writing history out of the synced folder*), which copies it to the data folder, checks the copy, and moves the old one to the Trash. Your scenes are never touched.
