# Interactive tutorial

## Current implementation

The compiled catalog at `crates/grimoire-core/assets/writingdeck/tutorial.json` lists **all 39 requested lessons**. Fifteen have actual application-event hooks; the other 24 are explicitly marked **planned** and cannot be started/completed. This is a continuously developed framework, not a claim of full tutorial completion.

Open F12 → Interactive tutorial, or use the menu/command palette. Arrow keys select a lesson; Enter shows its instructions and then starts the real feature; `h` opens linked searchable offline help; `s` skips; Escape returns. Completed lessons can be replayed. Progress is schema-versioned, atomically guarded and stored with the project. There is no fake Next/Done button.

Press `d` to create a **new** safe demonstration project and switch to it after saving the current project. Existing directories are never reused. Demo scenes, Markdown formatting, autosave, goals, corkboard and history use actual Grimoire code. Writing exercises only complete against a project with the safe demo marker. Ordinary novels are not rewritten to satisfy lessons.

```sh
grimoire writingdeck tutorial-demo /parent/NEW-demo
grimoire /parent/NEW-demo
```

The demo contains original short text and a deliberate spelling error. Nothing is copied from an unreviewed exercise source. To return, use Open another book; your saved novel remains an ordinary separate folder. No routine deletes a previous demo to make room for a new one.

## Coverage inventory

| # | Lesson | State / actual completion event |
|---|---|---|
| 1 | Starting the OS/home | Wired: actual dashboard opened; boot itself separately VM-tested |
| 2 | Keyboard layout selection | Wired: helper-confirmed layout; only installed console service can verify it |
| 3 | Creating a novel | Planned tutorial event (existing new-book feature retained) |
| 4 | Writing and Markdown | Wired: actual demo editor text changes |
| 5 | Scene/chapter navigation | Wired: actual open document changes |
| 6 | Corkboard | Wired: actual corkboard overlay |
| 7 | Workspaces | Wired: chosen saved workspace applied |
| 8 | Focus mode | Wired: strict focus entered |
| 9 | Panes | Wired foundation: outline toggled; secondary editing still planned |
| 10 | Custom shortcuts | Wired: valid alias actually saved |
| 11 | Sticky pins | Planned |
| 12 | Sticky scrolling | Planned |
| 13 | Notes beside manuscript | Planned editable-pane lesson; existing reference is not misrepresented |
| 14 | Writing journal | Planned |
| 15 | Scratchpad | Planned |
| 16 | Session handoff | Planned |
| 17 | Daily goals | Wired: actual saved target changes in demo |
| 18 | NaNoWriMo tracking | Planned advanced calendar |
| 19 | Sprints | Wired: actual baseline sprint starts |
| 20 | Achievements | Planned |
| 21 | Daily challenges | Planned |
| 22 | Story Spark plot | Planned |
| 23 | Random writing challenge | Planned |
| 24 | Combined roulette | Planned |
| 25 | Dictionary/thesaurus | Planned |
| 26 | Grammar/spelling | Wired baseline spelling suggestions; grammar part planned |
| 27 | Hemingway-style analysis | Planned |
| 28 | Nabokov | Planned |
| 29 | Proselint | Planned |
| 30 | Author voice | Planned |
| 31 | Continuity | Planned |
| 32 | Inline revision notes | Planned |
| 33 | Draft comparisons | Wired baseline history view; named variants/diff extension planned |
| 34 | Footnotes | Planned |
| 35 | Export | Existing export retained; verified tutorial event planned |
| 36 | Google Drive backup | Planned; live credentials required for provider acceptance |
| 37 | Mobile access | Planned |
| 38 | Wi-Fi/system settings | Wired: actual settings screen; hardware changes separately verified |
| 39 | Recovery/troubleshooting | Wired: actual offline recovery manual; not a destructive crash simulation |

## Colemak and accessibility

Instructions use arrows, Enter/Escape and function keys for physical-layout-independent discovery. Aliases use logical letters after OS translation. The layout lesson opens the real safe preview, not an application remapping game. Planned lessons remain searchable so coverage is visible, but are disabled until their feature is real.

## Tests and continuation rule

Core tests verify 39 stable lesson IDs, unavailable lesson rejection, event matching, safe-demo gating, skip/replay, persisted progress and exclusive new-demo creation. TUI tests type through the actual routing/editor and validate persisted completion, plus links to real help topics and required terminal sizes. Add new completion evidence and safe-demo tasks in the same change that implements each future feature. A feature's tutorial must never be marked complete simply because its informational page exists.
