# Settings menu (#90)

Built for rc9 (2026-11-07) from the plan below, which the owner approved and may
adjust later.

## Moving a page

The layout lives in one place: `MENU` near the top of the Settings code
(`src/app/shell.rs`). Each line is a group page and the pages under it, in
order. Group pages, their rows, the breadcrumb titles and the paths the
tutorial opens are all built from it, so moving a page to another group, or
reordering a group, is editing that list. Each page still builds its own rows.
`src/menu_map_ui_tests.rs` checks that every page on the map can be reached by
name, has one place, and that group pages stay short.

Settings remembers its place on each page by page name, so a page that is
retired stays in the code (unreachable) rather than being deleted; deleting
it would make older saved positions unreadable.

Keep **Battery and power** third under System: the older lock VM test reaches it
by position.

## Before rc9

Counted from the rc9 build on 2026-11-07 with every feature available (lock,
sleep, Google Drive not linked).

| Page | Rows | Notes |
| --- | --- | --- |
| Settings | 4 | Writing, Interface, System, Accounts |
| Writing | 15 | Editor options, manuscript, goals, revision tools all in one list |
| Interface | 10 | Look, music and keyboard mixed |
| System | 8 | Seven pages, then "Sound and volume" tacked on at the end |
| Accounts | 5 | Google account and Google Drive are two pages for one link |
| Wi-Fi and networking | 8 | |
| Display | 5 | |
| Battery and power | 9 | |
| Storage | 5 | Has a second way into Backups |
| Date and time | 5 | |
| Updates | 9 | Has a way into Recovery |
| Recovery | 3 | |
| Synchronization | 1 | Only a note pointing to Backups |
| Google account | 4 | Has a third way into Backups |
| Google Drive | 5 | |
| Backups | about 20 | The longest page; does not fit a 25-line console with its notes |

A 25-line console fits about 21 rows inside a Settings panel.

## Built

Five groups at the top, no page over about 10 rows, and one way to each thing.

**Writing**
- Editor: spellcheck, spelling language, line width, typewriter scrolling, focus mode
- Manuscript: manuscript look, author details, footnotes
- Goals: writing goals, daily challenges, draft milestones
- Revision: revision tools, continuity tracker, revision notes
- Reading voice

**Look and keyboard** (was Interface)
- Look: themes, tree icons, workspaces, outline
- Keyboard: layout, layout wizard, shortcuts
- Music: source, on or off

**System**
- Wi-Fi and networking
- Display
- Battery and power
- Sound and volume
- Date and time
- Storage (no Backups row)

**Backups and accounts** (was Accounts)
- Backups: back up now, restore, and the state of the last backup
  - Where backups go: new local drive, new Google Drive, connect an existing one
  - Recovery key: export it, recover an interrupted lock
  - Offline queue: queue this book, run queued backups
- Google Drive: link or unlink, browse, copy a Drive item into a new folder (one page instead of Google account and Google Drive)
- Local accounts
- Synchronization folds into Backups, where a book's sync was already turned on

**Updates and recovery** (moved out of System)
- Updates
- Recovery

## What moved with it

- Tutorial lessons name the new paths and open their pages by name.
- Messages and docs that named a path (Updates, Backups) name the new one.
- The VM harness finds Updates and Recovery in their new group, and still works
  with older versions that keep them under System.
