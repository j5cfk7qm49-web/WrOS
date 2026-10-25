# M4a: single-source Markdown pins

**Tested initial pin subsystem, not completion of all M4 work.** Journal facts/reflections, scratchpad promotion and handoff remain separate planned services. The complete appliance is still not production-ready.

## One source, several readers

Choose **Pinboard** or **Pin current document** in Writing tools or Ctrl-K; dashboard `p` opens Pinboard. `n` chooses an existing project Markdown document. A card stores a reference and presentation metadata, not another editable body. Multiple cards may reference one source and keep independent scroll/presentation state.

The cached preview reads the current authoritative Project node, preferring its existing editor owner over a stale alias node. `Enter` explicitly requests the source in Secondary; `e` requests Primary. An already-owned canonical/symlink/hard-link identity focuses its existing editor instead. Both editors keep their original undo, selection, guarded saves, YAML, history, unavailable/conflict handling and recovery. Save/identity failures block unsafe replacement; buffers stay in memory and recovery is attempted. Cards have no direct manuscript write path.

Plain typing does not edit a card body. Pasted prose in Pinboard is ignored rather than executed as card commands. Category input accepts text, but cut/undo shortcuts and mouse dragging cannot mutate an editor hidden behind it.

## Controls

| Control | Effect |
|---|---|
| Up / Down | Select card; list viewport follows selection |
| PgUp / PgDn | Scroll only the selected card by three wrapped rows |
| Mouse wheel inside a card | Scroll that reference without taking editor focus |
| Mouse wheel over Pinboard list | Move list selection, not the source cursor |
| Enter / e | Explicitly edit original source beside / in Primary, subject to ownership and save guards |
| h | Capture/cycle an ATX heading in the source; enter anchored mode |
| q | Capture an exact selected passage from this source's active editor, at most 512 characters |
| Home | Return to the unique semantic anchor |
| f | Switch free/anchored preview, resetting offset; an anchor is required |
| b | Explicitly relink a card to another existing source; retained anchor is rechecked |
| v / [ / ] | Toggle sidebar / decrease or increase its width |
| space / + / - | Collapse / increase or decrease card height |
| c / g | Cycle color / edit category |
| J / K | Reorder within the current visible collection |
| s | Cycle Always, Novel, Chapter, Scene, Dashboard, Hidden |
| a / d / u | Archive card / recoverably trash card / restore it to Active |
| Tab in Pinboard | Cycle Active, Archived, Trashed collections |
| R | Explicitly reload pin metadata; source disk adoption remains guarded separately |
| Esc | Leave Pinboard or return from category input |

Pinboard, Toggle pins sidebar and Pin current document are optional custom-alias actions. Existing F10/F11/F12 defaults remain unchanged.

## Scope and layout

All sources and settings are **project-local**. Always spans this project's writing view and dashboard; Novel excludes the dashboard. Chapter and Scene capture the current manuscript context in **Primary**, not the note being edited in Secondary. Dashboard-only and Always cards appear alongside home widgets when width permits and the Dashboard pins widget is visible; overflow is accessible in Pinboard. Hidden retains a card without displaying it in the writing/dashboard views.

The sidebar requires its configured width plus at least 34 writing columns, or 72 columns when two editable documents are visible. Height must be at least ten rows. Strict focus hides it. Narrow/tiny layouts retain all cards and normalize focus away from hidden panes; Pinboard remains available. Tab reveals the selected sidebar card instead of focusing offscreen content. These are bounded layouts, not draggable desktop windows or a third editor.

## Persistence and safe failure

Portable model/store: `crates/grimoire-core/src/pins.rs`. TUI adapter and cached rendering: `src/app/pins.rs`.

Schema-v1 `.grimoire/writingdeck/pins.json` stores stable card IDs, project-relative source paths, explicit scope/context, category/color/lifecycle, vector order, collapse/height, free/anchored mode, optional heading/quote anchor, per-card scroll, selection and separate sidebar/list positions. Limits include 256 retained cards, 24–60 sidebar columns, 4–20 card rows and 1 MiB metadata. A quote is bounded anchor metadata, not an independent body to edit or synchronize.

Transactions clone state, validate it, compare the last-read fingerprint and use the existing atomic guarded writer. Memory advances only after success. Invalid, unknown, future-version, conflicting or unwritable metadata is retained and reported, never silently reset. Existing ancestors are checked before directory creation so a `.grimoire` symlink cannot write outside the project. The metadata reader is size-bounded. Read-only `grimoire writingdeck check-config DIR` validates pin metadata without creating or rewriting files; source availability is checked in the UI separately.

Canonical containment and Unix device/inode checks resolve aliases. Internal history/recovery/Trash and outside-project symlink targets cannot become pin sources. Known application moves follow references; unknown/missing/unreadable/non-UTF-8/conflict-copy sources remain explicit unavailable cards. No arbitrary replacement is guessed. Archiving/trashing a **card** never deletes, moves or rewrites its **source**. Restoring the source at its original path can make its retained card readable again; `b` is the explicit alternative.

The draw path performs no file reads, metadata saves or system commands. Identity/cache checks run on input, reload and the existing two-second disk tick. External text still uses the existing settling/conflict rules, so a changed body may take two checks to adopt. An absent or duplicated anchor is visibly unresolved, not silently treated as a match. A shortened body reports an out-of-range saved offset; the next scroll clamps it without editing the source.

## Anchors and limits

Free offsets start at the body top; anchored offsets start at a uniquely matching heading/quote, so inserting text above it does not turn a saved line number into a wrong target. ATX labels and quotes are case-sensitive. Fenced/indented code is excluded from heading capture, and overlapping duplicate quotes are ambiguous. Missing/ambiguous anchors require explicit free mode, recapture or relink. Previews are plain Markdown from anchor to source end, not a full CommonMark renderer or a bounded-section editor. Setext heading anchors are not implemented. Reflow can change the text at a saved wrapped-row offset.

This is not an OS file lock, hostile-filesystem-race defense, cross-process compare-and-swap, backup or power-cut qualification. Hard links that an external/atomic replacement separates become distinct filesystem identities. Per-workspace pin sets, cross-project global pins, free-position canvas/dragging, category filtering and permanent card purging are not implemented. Large-document/card-count input/idle CPU and synchronous metadata-save/flash-wear budgets still require benchmarks; the ordinary boot RSS is not a pin stress result.

## Verification

- **14 core pin tests:** metadata-only round-trip, independent state, scopes/lifecycle, unknown nested/future/corrupt data, limits, actual I/O failure/conflict rollback, deleted sources, canonical/alias containment, early and late metadata symlink escape refusal, heading/fence/Unicode/quote behavior and bounded retained preview rows.
- **31 `writingdeck_pins_*` UI tests:** actual controls and source save/undo/YAML; all four required sizes plus tiny/strict fallback; both editors' cursor/selection preservation; sidebar/list/dashboard scrolling; alias ownership; external source adoption/conflict/rename/Trash/undo; failed-source-save recovery; non-UTF-8 refusal; category input safety; presentation/restart; metadata corruption/conflicts; relink; aliases; real lessons 11/12 against the shipped demo generator and unchanged Pin Practice source; no exercise against an ordinary novel.
- **One new system-alias compatibility test.** These total **46 new Rust tests** beyond M3b, not a second count of the full suite.
- `os/tests/test_pins_pty.py` uses the actual non-root compiled application/event reader: palette/source picker, real PgUp/PgDn/Home escapes, heading scroll/return, bracketed-paste command suppression, original-source save, duplicate-card independence, archive/Trash/restore, normal quit and restart loaded-state proof at **80×24, 100×30, 120×40, 160×50**; a separate 80×24 corrupt-metadata scenario refuses overwrite. A subsequent real scroll is the paste-processing barrier; differential terminal output is not mistaken for complete emitted title strings.
- New headless CLI test validates pin metadata without writes and preserves unknown/future files. The prior independent-editor PTY and all upstream regression suites remain enabled.

These PTYs answer the initial cursor query but do not emulate every terminal feature or certify physical layouts. The separate whole-VM probe covers boot/system/console keyboards, **not pin interaction**. Current aggregate results and source/binary identities are in [STATUS](STATUS.md) and [test-results.json](evidence/test-results.json).
