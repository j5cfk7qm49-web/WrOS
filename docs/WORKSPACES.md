# Dashboard, workspaces and independent editors

## Home and workspace slots

F12 opens the home dashboard with existing book data: resume/current manuscript, daily net growth and target, novel target, recorded weekly progress, sprint, recent scenes, battery, backup indication and system status. Missing devices/services show unavailable. Local saving/history is not mislabeled off-device backup.

Arrow keys/Enter navigate; `h` hides a widget; `J`/`K` reorder it; Configure widgets restores hidden widgets. Home scrolls to the selection at 80×24 and is tested at 100×30, 120×40 and 160×50. Start-at-dashboard is optional in System settings; conventional development defaults to writing, appliance first launch to home.

Workspaces provides Drafting, Planning, Research, Revision and Focus slots. `n` clones a custom slot; `+`/`-` adjust outline width (18–60 columns); `<`/`>` adjust the secondary proportion by five percentage points (25–75%). Slots initially retain a safe empty pane structure except Focus. **M4c adds explicit starter application (`b`) and per-workspace pin sets (`p`)**; neither silently rewrites prose while loading a slot. See below.

## M3b: two genuinely editable documents — Tested

- **F11**, with no secondary assigned, opens Choose secondary document. Type a scene, character, note or research title and press Enter.
- With a secondary assigned, F11 hides/shows it without discarding its text, cursor, selection, free scroll or in-memory undo history. Use **Writing tools → Choose secondary document**, or the command palette, to replace its document after the existing save/recovery checks.
- Tab/Shift-Tab and clicks change the focused pane. Typing, bracketed paste, selection, undo/redo, cut/copy, find/replace and spelling act on that pane. Each document has its own complete `Editor`; the clipboard, dictionary, find preferences and project-level operations remain shared.
- Selecting an already-open document focuses its owner instead of creating a competing buffer. Canonical paths and Unix device/inode identity cover symlink and hard-link aliases. Only project-contained regular files can seed a new editable view; unreadable/non-UTF-8 documents cannot seed a new secondary editor.
- Both views use **the original Project guarded saves, autosave, history, unavailable-file handling, conflict parking and crash recovery**. Raw YAML frontmatter remains raw. There is no second direct-write path.

### Layout and focus

An editing area at least 72 columns wide splits horizontally. Narrower editing areas stack the editors vertically when at least 12 rows are available, so an ordinary 80×24 terminal with an outline still supports both. Each pane records separate editor/prose geometry, caret tracking and mouse state. Wheel scrolling over the inactive view does not steal focus or snap its scroll back to its cursor.

If a very small viewport cannot show both, the secondary is retained but not drawn or focusable. Focus normalizes to Primary; enlarging the terminal reveals the retained view. This is not a permanently closed document.

F10 hides/shows the outline independently. Ctrl-D strict focus always shows **Primary only**, hiding outline, scenery/music controls, Secondary and legacy reference panes. Leave focus with Ctrl-D before using F11. Tab never enters a hidden pane. Escape clears a real selection first, otherwise opens the existing menu; it does not close an invisible reference.

The legacy `v` beside and Ctrl-O Codex panes remain **read-only references**, not aliases for the new secondary editor. They are retained but not drawn simultaneously with Secondary. Explicitly opening a legacy reference hides the secondary without discarding it. No third editable pane is implemented.

## Persistence and migration

Each workspace stores:

- primary and optional secondary project-relative document path;
- independent logical line/character cursor and selection anchor (including selection direction);
- independent visual-row scroll;
- secondary visibility, proportion and active editor;
- outline visibility/width and strict-focus preference.

State is schema **v3** JSON under `.grimoire/writingdeck/layout.json`. V1/v2 files migrate in memory; inspecting them with `grimoire writingdeck check-config DIR` does not rewrite them. The next explicit guarded layout save writes v3. V2 pane positions/visibility/ratio remain intact; missing pin sets mean follow all. Future versions, invalid paths, unknown fields, corruption and external conflicts are preserved rather than overwritten. Bad UI state must not prevent manuscript saves.

Startup gives the upstream Primary resume record precedence: stale workspace coordinates/selection are not applied to another document or a newer cursor, and focus remains Primary when that resume differs. Explicitly choosing a workspace instead restores that workspace's positions. A workspace can swap which two documents occupy Primary/Secondary without duplicating their editors or losing in-process per-document undo.

Cursor/selection/scroll/layout persist across restart; **the transient undo stack is not serialized**. Dated disk history remains available separately, as upstream. Scroll is currently a visual-row offset, not a semantic heading/quote anchor: wrapping after a resize may change which words occupy that row. Pin-card semantic anchors are implemented separately in M4a; editor scroll itself remains a visual-row offset.

## Failure and external-change behavior

An external edit to an open document is adopted only through the existing guarded Project path. A concurrent local version is parked and retained; the matching active or inactive editor is updated so a later flush cannot put stale text back. Bulk book replacement, book undo/redo, notebook-name fixes and recovered-word restoration update the owning editor too.

A clean external rename is followed only when an unambiguous unchanged fingerprint identifies the moved file, excluding trash/parked versions and another view's document. Dirty missing-file text is first parked by the existing disk-check path; a waiting save is not permission to detach it. Ambiguous duplicate contents are not guessed between. Deleting/trashing a file or folder closes every affected editable view and stashes its in-process undo for recoverable tree operations.

If two previously distinct open paths become aliases, saves, reload adoption and editor replacement pause; both buffers remain in memory and recovery copies are attempted. Repair the external path/alias before continuing. A genuine save failure also blocks unsafe replacement, workspace switching and ordinary quit; the existing explicit emergency-quit/recovery behavior remains available. Unreadable/non-UTF-8 recovery targets remain in recovery rather than being falsely reported restored.

**Boundary:** the registry is in-process ownership, not an OS file lock, cross-process transaction or atomic compare-and-swap. The upstream fingerprint check-to-rename race against unrelated writers is not eliminated. Keep independent backups; no power-loss or hardware qualification is implied by these software tests.

## Verification

`src/ui_tests.rs` adds **28 `writingdeck_secondary_*` tests**, retaining the original home/layout/help/menu/key regressions. They cover all four terminal sizes, real editing, independent selection/undo/clipboard routing, hidden/tiny/wide layouts, mouse/free scroll, find/spelling, autosave of an inactive hidden buffer, YAML round-trip, alias ownership/revalidation, clean and dirty external changes/renames, ambiguous moves, book-wide replace/undo, save failure, crash rescue and dual recovery, non-UTF-8 refusal, history restore, trash/settled-conflict closure, workspace role swaps/restart and actual lesson 13 completion.

Core registry tests cover canonical/device-inode ownership, rejected paths, failed claim retention, missing files and atomic replacement. Workspace tests cover v1 migration, both selections/positions and unknown/future/conflicting data preservation. Editor tests verify Unicode/multiline selection replacement as one undo step.

`os/tests/test_secondary_pty.py` additionally drives the **compiled unprivileged application in a real PTY at 80×24, 100×30, 120×40 and 160×50**: actual F11 escape input, bracketed paste, both file saves, isolated undo/redo, hide/show, normal quit and persisted role/path assertions. The tiny PTY shim answers the startup cursor-position query; it is not physical-keyboard or full terminal-emulator certification. The test refuses to launch interactively as root.

See [TESTING](TESTING.md), [STATUS](STATUS.md) and retained [test records](evidence/test-results.json). The whole-VM test separately covers boot/system/console keyboard behavior, not these F11 keystrokes.

## M4a: source-linked pins — Tested

Pinboard, a responsive writing sidebar and dashboard-scoped cards now reference the same original Project documents. Each card has independent free/semantic-anchor scroll, scope/context, color/category, size/collapse and order; card archive/Trash is recoverable and never deletes its source. Enter explicitly opens/focuses the original through the existing exclusive editor registry. Ambiguous retained editor identities are not guessed. Metadata is guarded, transactional schema-v1 state, separate from layout/text, with unknown/corrupt/future/unwritable/external-conflict preservation.

See [PINS](PINS.md) and compiled Sticky-note pins help for controls, exact layout thresholds and limits. The sidebar is hidden in strict/tiny layouts and cannot steal hidden focus. Source cursor/selection/undo do not follow card scrolling. All four required terminal sizes, 31 pin UI cases, 14 core cases, one alias case, actual pin PTY/restart/paste-guard tests and read-only CLI validation supplement the retained M3b tests. Ordinary boot probes do not exercise pins.

## M4b: journal — Tested initial slice

The Writing journal widget and dashboard `j` open a separate reader for sampled saved-Project facts and an ordinary date-path reflection. Automatic immutable observations and editable reflection text never share a generated prose body. Enter/e uses the same exclusive editors; note-only/view operations invent no writing events. Known incomplete totals stay unavailable, not zero-filled. Guarded per-date metadata handles conflicts/future data/pending retry; all four required sizes, actual-binary PTY/restart/force-exit and real lesson 14 are covered. See [JOURNAL](JOURNAL.md). No gross-word/time ledger is implemented.

## Remaining M4 services — Planned

Scratchpad, advanced calendar/challenge/achievement/revision widgets and next-session handoff are not implemented widgets. Pins and the initial journal are tested; richer journal/edit-ledger metrics remain separate work. Do not invent metrics to fill them. Full preset content, per-workspace pin sets, category filters, bounded-section excerpts and free-position/dragged cards remain outside M4a.

Journal automatic facts are now separate from editable reflections; they remain sampled cached Project observations, not gross/active-time facts. Scratchpad promotion must preserve the original and use guarded project operations. Handoff is an ordinary user-authored note shown next session. The tested document registry/pins are dependencies, not implementations of those services.


## M4c explicit starter content and pin sets

In Workspaces select a destination slot, then **b** and a named starter. This is an explicit operation, not an implicit template on workspace selection/startup. It first requires clean settled original buffers and current guarded layout metadata; an external/future/conflicting layout is neither adopted nor overwritten.

| Starter | Original content/layout |
|---|---|
| Drafting | Empty ordinary `notes/workspaces/drafting.md` in Secondary; outline on; 30% Secondary |
| Planning | Empty ordinary `notes/workspaces/planning.md` in Secondary; outline on; 45% Secondary |
| Research | Empty ordinary `research/workspace.md` in Secondary; outline on; 45% Secondary |
| Revision | Empty ordinary `notes/workspaces/revision.md` in Secondary; outline on; 40% Secondary |
| Focus | Primary-only strict focus; retained Secondary hidden, not discarded; no new source |

Existing originals are opened without replacing their prose, YAML or undo. Creation is exclusive and empty; there is no generated outline, research or intention. A newly created original is deliberately retained if a later layout save fails. Layout rollback/preflight and the final guarded save are not a cross-file transaction or OS lock. The selected slot keeps its name/Primary resume and pin-set choice. Applying a preset can assign Secondary without replacing the original manuscript.

**p** opens a draft pin-set editor: Up/Down select, Space toggles an ID, `a` selects all current cards, `z` selects none, Enter saves the explicit set, **A** follows all current/future cards, Esc cancels. Paste/cut/undo do not execute its letter commands or touch a hidden source. Unknown positive IDs are retained rather than fabricated into cards. IDs must be positive, unique, at most 256.

`pin_ids: null` follows all cards (including future ones); `[]` shows none; an explicit list filters the existing sidebar/dashboard scopes. The global Pinboard, card lifecycle and original bodies are unchanged. These are references to global project cards, not copied note bodies. Selection/commands normalize to cards actually in the visible set, so changing a set cannot edit a hidden old selection.

The layout reader/preflight and save enforce a 1 MiB limit. Invalid/oversized/future/externally replaced layouts remain byte-preserved; source saves remain independent. Four-size real compiled-app PTYs verify actual Research Secondary editing, explicit empty pin-set persistence and refusal to create a Planning original after an external future-layout replacement. Rust UI tests exercise all five starters, existing-source/alias/Waiting/I/O behavior, persistence/migration, sidebar/dashboard filtering and selected-card safety. See [WORKFLOW](WORKFLOW.md) and [TESTING](TESTING.md).
