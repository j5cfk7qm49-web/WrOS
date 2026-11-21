# rc22: books become projects, all of YouTube, and keys that act once (design)

Owner decisions 2026-11-21 (chat), after rc21 shipped. Calls marked "(my call)" are new. Laptop: HP Stream 11 on rc21.

## Why

- **#164:** WrOS calls everything a writer works on a *book*. That fits novels, not essays, articles, scripts, poems or a thesis. The owner wants *project* everywhere, files on disk included (option C).
- **YouTube:** the YouTube Music source searches only YouTube Music's songs, albums and playlists. The owner wants an option to search all of YouTube, and wants playback to stream rather than download first.
- **#171:** on the laptop every key press acts twice:
  - play/pause looks dead (a double toggle);
  - next and previous skip two;
  - volume and brightness move two steps (since #80).

## Goals

1. **The word.** "Book" becomes "project" wherever the writer reads it: menus, the palette, Settings, the shelf, Import, sync and backup messages, the status bar, the tutorial, the start screen, help, docs and the site. "Book" stays where it means a book:
   - Paperback, EPUB and the print compile;
   - "Book" as a `part_label` value;
   - the EPUB's internal `<dc:identifier id="book-id">`.
2. **The files.** Each project's own files are renamed:

   | Old | New |
   |---|---|
   | `novel.toml` | `project.toml` |
   | `.grimoire/book-id` | `.grimoire/project-id` |
   | `Novel-Format.md` | `Project-Format.md` |

   `project.md` (the map made on request) keeps its name.
3. **Old names keep working.** Every reader (desk, shelf, phone library `grimoire-app`, sync root lookup, knowledge, submission, sessions/timeline) reads the new name first and the old name second. A Grimoire folder with `novel.toml`, a backup restored from before rc22, or a copy arriving from another computer still opens. The owner's "backwards compatibility with Grimoire" means exactly this. It does not mean that an older Grimoire can open renamed projects.
4. **Migration.** The desk renames a project's old names to the new ones when it opens that project.
   - (my call) On an installed WrOS, not before this version has reported that it started (probation over), so an automatic go-back after a failed first start never meets renamed projects.
   - Off WrOS (Grimoire builds), on first open.
   - Renames are atomic (`rename(2)` within the folder) and happen before anything else writes the files.
   - If a name can't be renamed (read-only, online-only file not downloaded), the project opens anyway on whatever it can read, and the status bar says why. It tries again next open.
5. **Both names present.** The new name wins. The old file moves to the project's Trash as `novel.toml (old name, <date time>)` and the status bar says so. Same for `Novel-Format.md`.
   - For the ids, `project-id` wins. A stray `book-id` (an rc21 boot-menu rollback creates one) goes to the Trash.
   - (my call) Its separate history repository under the data folder is left where it is, not merged.
6. **Going back.** **Settings › Updates › Go back**, when the version it goes back to is older than rc22, first renames every project on the shelf (`~/Documents`) and in the remembered list back to the old names. Then it asks for the go-back.
   - The confirm row says how many projects it will rename back.
   - If the go-back fails, the next open migrates them again.
   - An unknown target version counts as older (renaming back is always safe, because newer versions read old names).
   - A boot-menu rollback can't run this: rc21 then opens a project under its folder name, without its targets or export settings, until rc22 is back. Said in the release notes.
7. **Drive.** Nothing new is uploaded except the renamed files.
   - Sync sees `novel.toml` and `Novel-Format.md` deleted (two deletes, under the five-delete stop; Drive keeps them in its bin) and their new names added.
   - `.grimoire/` doesn't sync, so the id rename never reaches Drive.
   - `WrOS-History/<name>/` and the Drive folder use the project's folder name, so they don't change.
   - Another computer on an older WrOS loses `novel.toml` at its next sync and opens with defaults until it is updated. Said in the release notes.
8. **All of YouTube.** In the player's search (F7 › search), `Tab` switches between **YouTube Music** and **All of YouTube**.
   - The choice is remembered in `music.toml`.
   - All of YouTube lists videos (title, channel, length) from yt-dlp's own search (`ytsearch`, no API key).
   - Choosing one plays its audio through the YouTube player the Music view, F4–F7 and the keys already drive.
   - Not included: YouTube playlists or channels, sign-in, pictures.
9. **Streaming by default.** YouTube Music songs and YouTube videos start playing while yt-dlp fetches them, instead of after the whole file has downloaded. (my call) Settings › Music › YouTube Music keeps today's download-first way as an option: "Play: as it arrives (default) / after downloading".
10. **Keys act once (#171).** triggerhappy is patched so `add_device` skips a device path it already has open and closes the fd it was passed. NixOS's start-up glob and its udev rule can then both add a device without doubling it.

## Non-goals

- Renaming code identifiers wholesale (about 1,900 "book" names). Only the names that point at these files are renamed (e.g. the id-file helper).
- Merging the history of a stray rc21-era `book-id` into the project's history.
- #163 (an Essay kind), Bluetooth (#172), Soloist (#170).
- Video pictures, YouTube sign-in, YouTube playlists or channels from the all-of-YouTube search.

## Anti-goals

- A project that won't open, or opens empty, because of a rename.
- Any writing, settings or history lost silently: what is replaced goes to the Trash and is said.
- Drive re-uploading a project's scenes.
- A rename running while this version could still be rolled back automatically.
- A key press that acts twice, or a key that stops working after a keyboard is unplugged and plugged in again.

## Approaches considered

- **Rename depth:** A (screen only), B (new projects only, read both) and C (rename everywhere, read both, migrate on open). The owner chose **C**.
- **Going back:** (1) say nothing and let rc21 open on defaults; (2) rename back in Settings' Go back (chosen; covers the go-back the writer chooses); (3) also keep the old files as copies (rejected: it defeats the rename the owner asked for).
- **YouTube search:** (1) ytmusicapi's Videos filter (only what YouTube Music indexes); (2) yt-dlp `ytsearch` (all of YouTube, no key; chosen); (3) the YouTube Data API (needs a key and quota; rejected).
- **Streaming:** (1) mpv's own ytdl hook or direct URLs (403 on some tracks in rc20, rejected); (2) yt-dlp writes to a pipe that mpv plays as it arrives (chosen, to be proven by a spike first); (3) keep download-first (kept as the option).
- **Double keys:** (1) drop NixOS's `--deviceglob` (misses devices udev added before the socket listened); (2) drop the udev rule (misses keyboards plugged in later); (3) de-duplicate in `wros-media-key` by time (guesswork; a fast double press is real); (4) patch `add_device` to skip an open path (chosen: fixes the cause, keeps both routes).

## Decided details

- **Order of work:**
  - #171 first, since it's small and the laptop needs it.
  - Then the streaming spike.
  - Then YouTube search.
  - Then the rename: core resolver and migration, readers, go-back, then words, help and docs.
- **Resolver:** one place in `grimoire-core` names the files (new name, legacy name) and answers "which one is here".
  - Everything that read `root.join("novel.toml")` asks it.
  - New projects are scaffolded with the new names only.
- **Kit detection:** `Area::Format` exists if either format file exists.
- **Words:** the shelf is "Your projects", along with "New project…", "Open another project…" and "Import a project…". Help pages are renamed in title and text. Help file names may keep their numbers.
- **Tests:**
  - **Units:** the resolver; migration (fresh, both present, read-only, ids); go-back rename-back; a phone `books()` that lists both names.
  - **VM tests:**
    - `import-book` and `book-timeline` keep passing with renamed files;
    - a migration step: a folder with the old names opens on the desk and has the new names;
    - `media-keys` re-adds the keyboard through `udevadm trigger --action=add --subsystem-match=input` after thd starts, still expects one entry per press, and checks that no device is open twice in thd.
  - **YouTube:** the all-of-YouTube search parser on recorded yt-dlp JSON; streaming start (live test, `#[ignore]`).

## Left to the builder

- How the pipe between yt-dlp and mpv is set up (FIFO, `fd://`, stdin).
- How the "Play" setting is worded.
- The search list's layout.
- Where the probation check is read.
- How the go-back finds the target version.
- Which code names beyond the file helpers are worth renaming.
