# Dashboard, workspaces and pane engine

## Implemented foundation

F12 opens the home dashboard with data from the existing book: resume/current manuscript, daily net growth and target, novel target, recorded weekly progress, sprint, recent scenes, battery, backup indication and system status. Missing battery/services show unavailable. Local saving/history is not mislabeled off-device backup.

Arrow keys/Enter navigate; `h` hides a selected widget; `J`/`K` reorder it; Configure widgets restores hidden widgets. The screen scrolls to the selection at 80×24 and is tested at 100×30, 120×40 and 160×50. Start-at-dashboard is optional in System settings; conventional development defaults to the existing writing view, appliance first launch to home.

Workspaces provides five named slots (Drafting, Planning, Research, Revision, Focus), `n` to clone a custom slot and `+`/`-` to adjust outline width (18–60 columns, adapted to terminal space). The initial slots currently share the same primary/outline structure except Focus. They are **not yet the full requested planning/research/revision multi-pane presets**.

Each slot persists primary document, logical cursor, visual scroll, outline visibility/width and strict-focus choice. Saved state is versioned JSON under `.grimoire/writingdeck/layout.json`; ordinary Markdown is untouched. Startup gives a newer upstream resume record precedence. Choosing a workspace explicitly restores its own saved position. A corrupt/future/conflicting state file is preserved and does not prevent manuscript saves.

F10 hides/shows the left outline column independently of focus mode. Ctrl-D hides outline, scenery/music controls and retained reference panes. Tab/Shift-Tab only visit visible panes. Leaving focus restores the retained reference; Escape in focus opens the menu rather than closing an invisible reference.

## Multi-document engine: next milestone, not implemented

F11 is reserved but **not assigned** until there is a real second editable buffer. Baseline beside/codex panes remain references. A secondary editor needs all of the following before release:

1. A canonical document registry, with one authoritative in-process document per file path.
2. Distinct documents with independent `Editor`, undo/redo, selection, cursor and scroll state.
3. Shared existing project save fingerprints, autosave scheduling, pre-edit/history snapshots and recovery; no separate unsafe `fs::write` path.
4. Focus switching that routes paste, selection, find, save and modifier keys to the intended buffer.
5. Dirty/external-file conflicts preserved without silently reloading over text. Opening the same file in two views must share state, not race saves.
6. Saved pane selections/ratios, minimum widths, narrow-terminal fallback and terminal resize tests.
7. F11 direct switching, Tab cycling and menu/palette/help coverage plus interactive lesson 13.

## Pins and other dashboard widgets: planned

Journal, scratchpad, pinned notes, advanced daily/weekly/NaNoWriMo calendars, challenges, achievements, revision queue, Story Spark and next-session handoff are not implemented widgets. Do not display invented metrics to fill them.

Pinboard design: a pin stores a source Markdown path and presentation metadata only (scope/category/color/order/collapse/card size/selected section/scroll/heading or quote anchor). Full editor, dashboard pin and sticky card use that same source. Scopes: Always, Novel, Chapter, Scene, Dashboard, Hidden. Archive/trash are recoverable. Free-scroll and anchored-excerpt modes keep independent card and sidebar positions. Deleted/changed anchors need a visible fallback, not silent jumps or copied text stores.

Journal design: immutable automatic session-event facts are separate from user-editable reflections; views can generate/open normal Markdown. Scratchpad supports multiple notes and scene links with recoverable promotion. Handoff is a user-authored ordinary note/reference shown on next boot. All need persistence/external-change/tutorial tests alongside implementation.

## Acceptance tests

Current: `writingdeck_dashboard_is_navigable_at_required_terminal_sizes`, `writingdeck_widgets_keep_visibility_and_order_after_reload`, `writingdeck_workspaces_restore_document_cursor_and_width`, `writingdeck_outline_toggles_without_focus_mode_and_persists`, strict-focus tests and `workspaces::tests`.

Next: two independently dirty buffers, same-file aliasing, concurrent external edits, save failure in either pane, crash recovery of both, pane ratio/cursor/selection restoration and sticky semantic anchors.
