# WrOS and upstream Grimoire

WrOS's writing app is a fork of [GrimoireTUI](https://github.com/kelsierbot/GrimoireTUI) by
Catfinity Studios (MIT). WrOS takes upstream's releases as real git merges, so each merge only
has to deal with what's new since the last one.

## Where WrOS stands

| | |
|---|---|
| Forked at | v0.7.5 (f3ca1e8) |
| Merged | v0.8.3 (rc32, merge c180121), then upstream/main at cf6415c, which is v0.9.1 plus two commits (rc32, merge 929ca56) |
| Workspace version | `0.9.1` in `Cargo.toml`. It records the Grimoire base; WrOS's own version is `os/VERSION`. |

## Taking the next upstream release

```sh
git fetch upstream --tags
git merge-tree --write-tree --name-only HEAD upstream/main   # preview the conflicts
git merge --no-commit --no-ff upstream/main
```

1. Resolve these mechanically, every time:
   - `site/`: upstream's (WrOS doesn't publish it);
   - `README.md` and `CHANGELOG.md`: WrOS's;
   - `.github/workflows/ci.yml`: stays deleted;
   - `Cargo.toml`: WrOS's dependency lines with upstream's `grimoire-core` version;
   - `Cargo.lock`: `cargo update -w`.
2. Resolve the rest by the rules below.
3. All checks must pass (`rc14-argon/nixos-wsl/README.md`) before the merge is committed.

## Dependencies

- **Crates Grimoire locks stay at Grimoire's versions.** Run `python tools/upstream-crates.py
  --check` to verify every upstream version after a lockfile change.
- **Everything Grimoire doesn't cover moves to its newest compatible release each WrOS release.**
  Update WrOS-only crates within their major version and Cargo's semver compatibility rules.
  Move nixpkgs to the head of its release branch and Rust to the newest stable release.
- **Major bumps are a separate, deliberate step.** So is moving an exact pin (`=x.y.z`): the
  Brain's stores (`fsrs`, `loro`, `automerge`, `rusqlite` in `crates/grimoire-brain`) are pinned
  so their formats only change on purpose. A pinned crate's own family stays with it, so the
  `loro-*` crates are left at `loro`'s version. `tools/upstream-crates.py` without `--check`
  lists the crates to update; leave those families out of the `cargo update` call.
- **Shared manifests stay in upstream's form.** That means the root package's dependency lists,
  `crates/grimoire-core/Cargo.toml` and `crates/grimoire-app/Cargo.toml`: plain version strings,
  not `workspace = true`, so upstream's version bumps merge without conflict. WrOS's own crates
  share versions through `[workspace.dependencies]` in the root `Cargo.toml`.
- **One intentional difference:** `grimoire-core` keeps `serde_json`. Upstream dropped it as
  unused (905640c), but WrOS's code in that crate uses it.

## Which files are whose

A file is **shared** if it exists in the latest upstream tag WrOS has merged
(`git ls-tree -r upstream/main`). Every other file, and the `wros-*` crates, is **WrOS's**.

- **Shared files** get bug fixes and small local changes only: no restructuring, renames or
  reformatting. Every such change is a conflict at the next merge.
- **WrOS code that lives in a shared file** stays put: the fields WrOS added to `App` in
  `src/app.rs`, and WrOS's lines in `src/music/mod.rs` and `setup.rs`. Whole WrOS-only functions
  may move out into WrOS modules: fewer WrOS lines in shared files means fewer conflicts.
- **WrOS's own files** are restructured freely.

## How conflicts are resolved

1. Upstream's fix or feature is taken wherever WrOS didn't deliberately change the same
   behaviour.
2. Where WrOS did, WrOS's version stays and upstream's fix is re-applied on top.
3. Where both sides add, both are kept.
4. Upstream's new user-visible text says **project**, not book (WrOS's rc22 rename). The help
   follows upstream's rule: what the app can do, the help says (`CLAUDE.md`).

## Intentional differences

| Upstream | WrOS | Why |
|---|---|---|
| A book; *Your books* (`26-your-books.md`) | A project; *Your projects* (`26-your-projects.md`) | rc22 (#164): WrOS writes essays and notes as well as novels. |
| Books folder `~/Documents/Grimoire Books` (0.9) | `~/Documents`, the folder WrOS already keeps projects in. `~/Documents/Grimoire` is the first project. `grimoire_core::shelf::home()` is the one source of truth, and `src/books.rs::documents()` calls it. | WrOS's projects list, import, Drive sync and backups already work there; a second folder would split them. |
| Copies in `<books folder>/Copies` | `~/Documents/Copies`. A copy leaves out the project's Drive link (`.grimoire/sync.json`) and recovery files. | A copy opened elsewhere mustn't claim the original's Drive folder. |
| First start makes *My First Book*, or opens the newest book found (0.9.1) | Declined. WrOS's first start is its own (live session, tutorial, `~/Documents/Grimoire`). `grimoire new` with no folder makes *New Project*. | WrOS's start-up and tutorial flow. |
| *Where this book is kept*: folder, files, synced, last copy | *Where this project is kept*: the same, plus WrOS's Drive link and last backup. The last copy shows when there's no backup. | WrOS knows more about where the words are safe. |
| `grimoire copy --to <folder>` | No `--to`: copies always go in `~/Documents/Copies`. | One place to find them. |
| The menu (upstream's Esc menu) | WrOS's menu and bars (`src/app/overlays/bar.rs`). *This project* holds Where kept, Save a copy and Goals. *Open a copy…* is on the projects list. Right-click in the outline opens WrOS's Project menu. | WrOS's interface since rc30. |
| Taught words in upstream's settings list | *Settings › Writing › Editor › Taught words* | WrOS's five-group Settings. |
| Bring in a draft with upstream's path box | WrOS's import picker (Drive, USB, folders) | WrOS's import (rc22). |
| Writing sessions can push to a git remote, with remote backup status | Local sessions and the project timeline only; no automatic push. | WrOS is offline-first; backups and Drive sync are WrOS's own. |
| Music: upstream's music toggle, shared temporary downloads, desktop Spotify help | WrOS's music sources: local files, YouTube Music, Jellyfin/Plex. Downloads stay private. Spotify was removed in rc21. | WrOS's music (rc20–rc21). |
| Help topics as upstream wrote them | WrOS's topics and wording, with upstream's new facts and keys added | WrOS's help covers the whole appliance. |
| `README.md`, `CHANGELOG.md` | WrOS's | WrOS is the product. Upstream's README: https://github.com/kelsierbot/GrimoireTUI#readme |
| `.github/workflows/ci.yml` | Deleted; `nix-ci.yml` runs the checks | rc17. |
| `site/` | Upstream's, unchanged | WrOS doesn't publish it. |

## Fixes WrOS made to upstream's code

These were found while merging 0.9 and fixed in WrOS (merge 929ca56), with a test each. They
were offered upstream as well.

- **Empty the Trash** deletes only the entries the prompt named. Words a sync parks in the Trash
  while the prompt is open stay. (`empty_trash_deletes_only_what_the_prompt_named`)
- **Save a copy** waits while a save is pending, instead of making a copy without the latest
  words. (`save_a_copy_waits_for_pending_saves`)
- **Opening a copy** refuses two entries that land on one file (case-insensitive disks).
  (`open_copy_refuses_two_entries_for_one_file`)
- **Two copies saved at once** never replace each other: a copy is published by hard link,
  which can't overwrite. (`copy_publish_never_replaces_an_existing_copy`)
- **A copy can always be opened again:**
  - a copy of a project with an empty manuscript (every chapter in the Trash) keeps its folders;
  - a project folder named `manuscript` is found correctly.

  (`copy_round_trip_with_empty_manuscript_and_a_folder_named_manuscript`)

## Smaller calls made while merging

- **Front matter:** the stricter "only real front matter" rule (1d968ab) applies to WrOS's
  import too. WrOS's test `frontmatter_requires_a_complete_closing_fence` follows it.
- **Replace one match at a time:** goes through WrOS's measured, undoable replacement path.
- **Paste:** only reaches text fields, in WrOS's own overlays too (Settings, sources, pins).
- **Corkboard:** gives focus back after its prompts.
- **Read aloud:** its speed carries across queued scenes.
- **Highlight contrast:** upstream's fix applies below the top bar. The top bar's tiles use the
  selection colour as a fixed background.
- **Exports:** keep WrOS's footnotes and structured Markdown alongside upstream's export fixes.
- **Session commits:** keep WrOS's commit helper.
- **Imports:** keep WrOS's Word footnotes and project-map safeguards alongside upstream's
  encoding and paragraph fixes.
