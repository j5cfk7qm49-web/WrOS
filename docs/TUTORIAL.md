# Interactive tutorial

## Current implementation

The compiled catalog at `crates/grimoire-core/assets/writingdeck/tutorial.json` lists **all 39 requested lessons**. Nineteen have actual application-event hooks; the other 20 are explicitly marked **planned** and cannot be started/completed. This is a continuously developed framework, not a claim of full tutorial completion.

Open F12 → Interactive tutorial, or use the menu/command palette. Arrow keys select a lesson; Enter shows its instructions and then starts the real feature; `h` opens linked searchable offline help; `s` skips; Escape returns. Completed lessons can be replayed. Progress is schema-versioned, atomically guarded and stored with the project. There is no fake Next/Done button.

Press `d` to create a **new** safe demonstration project and switch to it after saving the current project. Existing directories are never reused. Demo scenes, Markdown formatting, autosave, goals, corkboard and history use actual Grimoire code. Writing exercises cannot even be launched against an ordinary novel; they require the safe demo marker. Opening or switching panes alone cannot count as a text edit. Ordinary novels are not rewritten to satisfy lessons.

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
| 9 | Panes | Wired: actual outline toggled; independent secondary editing is lesson 13 |
| 10 | Custom shortcuts | Wired: valid alias actually saved |
| 11 | Sticky pins | Wired: an actual metadata-only reference successfully saved in the safe demo, not merely opening the picker |
| 12 | Sticky scrolling | Wired: capture a unique heading, really scroll in anchored mode, then Home successfully saves a return to that semantic anchor in the safe demo |
| 13 | Notes beside manuscript | Wired: actual SecondaryEdited event in the safe demo, via typing or paste; a picker/navigation event is insufficient |
| 14 | Writing journal | Wired: recorded demo manuscript checkpoint reviewed, then a genuinely changed original Markdown reflection successfully saved; opening/navigation alone does not complete it |
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

Lesson 14 uses real saved manuscript observations, not fabricated session metrics: save a sentence in the demo manuscript, open Writing journal, Tab after reviewing facts, explicitly edit the original reflection and save genuine prose. Gross/active-time accounting is not claimed. A note-only save or reading the page cannot complete it.

The newly created demo ships a Pin Practice note with a unique The Courtyard heading and enough source rows to actually scroll. Lessons 11/12 use it directly, without asking the user to manufacture a heading first; another source with a unique ATX heading also works. It does not rewrite a real manuscript or silently manufacture completion. Missing/ambiguous anchors and failed metadata saves cannot complete it.

## Colemak and accessibility

Instructions use arrows, Enter/Escape and function keys for physical-layout-independent discovery. Aliases use logical letters after OS translation. The layout lesson opens the real safe preview, not an application remapping game. Planned lessons remain searchable so coverage is visible, but are disabled until their feature is real.

## Tests and continuation rule

Core tests verify 39 stable lesson IDs, unavailable lesson rejection, event matching, safe-demo gating, skip/replay, persisted progress and exclusive new-demo creation. TUI pin tests verify actual source selection/save and anchored scroll-and-return for lessons 11/12; opening a page or pressing Home without prior anchored scrolling is insufficient. TUI tests type/paste through the actual secondary routing/editor and validate persisted lesson 13 completion and replay, reject real-novel exercise launch, and prove open/navigation alone cannot complete it. Core tests and retained baseline lessons verify event matching, help topics and required sizes. Add new completion evidence and safe-demo tasks in the same change that implements each future feature. A feature's tutorial must never be marked complete simply because its informational page exists.
