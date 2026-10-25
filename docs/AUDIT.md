# GrimoireTUI audit

Audit date: 2026-10-24. Baseline: **v0.7.5**, commit `f3ca1e8f1cb09b714e71046e8ed991051793d33a`, MIT. Findings below describe that baseline, not hypothetical README features. See [FEATURE_MATRIX](FEATURE_MATRIX.md) for the additive WritingDeck implementation and verification state.

## Executive finding

Grimoire already supplies a substantial, safety-conscious novel editor. Keep its file model, editors, export walkers, recovery logic and mobile service layer. The missing product is an appliance integration layer and several new domain services, not a new kernel or wholesale editor rewrite.

The initial checkout built successfully after installing a Rust toolchain and ALSA development headers. Its workspace tests passed before modification. The default build includes audio; `--no-default-features` removes rodio/cpal and is suitable for the minimal appliance. Ratatui 0.30.2 provides the UI; Rust 2024 is used. The build toolchain used here is recorded in the test evidence; it is newer than Debian's packaged Rust and is a build dependency, not an installed guest runtime.

## Repository and responsibility map

| Area | Actual responsibility | Reuse / extension boundary |
|---|---|---|
| `src/main.rs` | CLI, real terminal setup, event-reading thread, modifier/AltGr normalization, overlay-first routing, periodic work, signal/panic cleanup | Add appliance commands and event observations without duplicating routing |
| `src/app.rs` | Project, current editor, focus, selection, dirty state, autosave, resume, actions, timer/sprint, reference panes, history and export jobs | Preserve the mature editor/save path; add an independent OS/UI state object |
| `src/ui.rs` | Responsive tree, editor, scenery/music, status line and reference panes | Hide unavailable panes geometrically and in focus traversal; do not just paint over them |
| `src/app/overlays/` | Menu, palette, help, spelling, corkboard, recover, export, history, reading and other dialogs | New features must participate in menu/help/palette invariants |
| `crates/grimoire-core/` | Plain-file project, text buffer, spelling, counts, history, export, conflict checks, recovery, sessions | Terminal-independent new domain modules belong here |
| `crates/grimoire-app/` | Stateless front-end service API, scene load/save stamps and project/shelf access | Retain for mobile; do not entangle Linux privileges with this crate |
| `apps/mobile/` | Tauri Android prototype using the service layer | Not an implemented iPhone client or Google Docs integration |
| `assets/help/` | 27 compiled, searchable offline help topics at baseline | Add tutorials alongside, with links to real topics |
| `assets/dict/` | SCOWL/Hunspell locale dictionaries and notices | Keep locale selection and personal/book vocabulary when adding Harper |

## Manuscripts and editing

A book is an ordinary directory with `novel.toml`, `manuscript/`, notes/notebook material and generated `.grimoire/` state. Manuscript structure supports parts, chapters, scenes and configurable part terminology. `project.rs` walks the tree, computes manuscript membership and compilation eligibility, and preserves raw YAML frontmatter while editing body text. Unknown manuscript metadata must not be normalized through a new serializer.

`editor.rs` stores logical lines, wraps them into visual rows, handles Unicode character indices, selections and undo/redo. Per-scene undo history is retained while changing scenes. It is the right basis for a future secondary editor, but a second `Editor` alone is insufficient: independent save fingerprints, focus, source indices, external-change handling and recovery are also necessary.

Baseline `BesidePane` and `CodexPane` provide references, scrolling and links. **They are not independent simultaneously editable document buffers.** Opening one for writing changes the main editor. Do not advertise these as the requested F11 editing engine.

`Ctrl-D` already exists as focus mode. The audited renderer could still place an open reference alongside the manuscript in focus mode. WritingDeck now tightens both rendering and focus traversal so references remain stored but invisible and unfocusable. The upstream shortcut is retained, not reassigned.

## Data safety and sessions

- `atomic.rs`: sibling temporary file, complete write and `sync_all`, atomic rename with bounded backoff, parent directory sync where supported. No in-place truncation of the destination. Tests cover transient and permanent rename failures.
- `sync.rs` and `project.rs`: content fingerprints, stat fast paths, a two-second racily-clean window, explicit `Written` versus `Conflict`, external-file reload, conflict copies, deletion parking and unavailable-file behavior. FNV-1a here detects accidental changes; it is **not** a cryptographic backup verifier. There is a check-to-rename race against an unrelated concurrent writer; do not claim a cross-program transactional filesystem.
- `history.rs`: dated ordinary Markdown copies, five-minute ordinary snapshot spacing, pre-session/pre-destructive snapshots and history tracking through renames. This is local history, not off-device backup or named milestone/variant management.
- `recovery.rs`: unsaved full scene text under `.grimoire/recovery/<relative-scene-path>`; differing copies are offered at startup, matching copies cleared. Missing-scene recovery files are retained rather than discarded.
- `resume.rs`: cross-machine Markdown handoff with scene/cursor information. Preserve it; workspace startup must not replace a newer upstream resume with an older layout cursor.
- `sessions.rs`: Git writing-session storage outside the synced book where possible, with legacy in-book history migration and optional existing Git remote push behavior. This is not a general Google Drive OAuth backup queue.
- `grimoire-app`: opaque scene `Stamp` values reject a phone save based on a stale version and retain frontmatter. Existing mobile conflict integration tests are particularly valuable regressions.

Normal application exit, failed save, panic and Unix hang-up/termination have recovery paths. A complete hardware power-loss guarantee still depends on filesystem, storage and firmware behavior. Do not infer it solely from atomic-rename tests.

## Goals, notes and writing aids

- Daily and book targets, preset goal editing, scene targets, a Pomodoro timer and timed/word-goal sprint functionality exist.
- `days.rs` persists local calendar-date totals/net growth, aggregates weeks, estimates recent pace and forecasts a finish. It does not measure gross typed/deleted words or keyboard-active writing time.
- `notes.rs` recognizes `%% ... %%` and standalone TK/TKTK marks; these are excluded from relevant counts and stripped appropriately for export. Unclosed notes are bounded safely. Research and non-manuscript areas are not novel prose.
- Corkboard, find/replace, notebook-name lookup, spelling suggestions, revision draft flags and a nearby repeated-word/echo detector exist.
- There is no baseline pinboard with source-linked Markdown cards, semantic scroll anchors, writing journal, multi-note scratchpad, achievements, local daily challenges or Story Spark corpus.

## Language and speech

`spell.rs` embeds spellbook/Hunspell dictionaries for US, British, Canadian and Australian English, loads asynchronously and has personal vocabulary and notebook-name integration. Preserve these affordances while adding grammar checking. Harper, Nabokov and standalone proselint are **not baseline integrations**.

`voice.rs` already segments and reads a scene sentence by sentence, including highlighting/navigation. It discovers local Piper, Speech Dispatcher or eSpeak, platform voices, and a silent test engine. A full-chapter/multi-scene queue, durable reading progress and mark-for-revision workflow are additional work. Piper models have their own licensing; no voice model has been bundled here.

## Export

`export.rs` derives DOCX (standard manuscript formatting), EPUB 3 with compatibility navigation, and manuscript Markdown from a shared book representation. Scope/sample selection, compile flags, generated/explicit title matter, typography, scene/chapter breaks and note stripping already exist. `export_print.rs` handles print-oriented output. `pdf.rs` converts DOCX through a separately installed headless LibreOffice, with an isolated profile and explicit missing-tool reporting; PDF generation runs outside the input loop.

There is no audited genuine Markdown `[^id]` to OOXML footnote implementation in these exporters. Do not certify bottom-of-page DOCX/PDF footnotes or Google Docs round-trip preservation without new package/XML/pagination tests. The minimal VM deliberately does not bundle LibreOffice; existing DOCX/EPUB/Markdown export remains native, while PDF requires an optional installation.

## Input/guard-test contracts

F1 help, F2-F3 timer, F4-F7 music, F8 spelling and F9 themes are established. F10-F12 were unassigned at baseline. WritingDeck now uses F10 for outline, F11 for the tested M3b independent editable secondary buffer, and F12 for home. This is a fork addition, not an upstream capability at the audited base.

The following tests remain authoritative, not obstacles to delete:

1. Every menu row has a nonempty description and valid help topic. `about_row` descriptions contain **no em dash**.
2. Palette action labels, after normalization, must appear in help text.
3. `EVERY_KEY` exactly matches the offline Every key table, and each documented key has an observable effect.
4. Every non-exempt palette action appears in a menu/submenu.
5. Every help topic fits/scrolls at the tested small and large terminal sizes.

No `Sub` variant was added. The Every key list and its test list were extended together for F10/F12. New interaction tests cover 80×24, 100×30, 120×40 and 160×50.

## Packaging and product gaps

Upstream has `.deb`/`.rpm` metadata, release workflows and a portable CLI, but no appliance image, login policy, console layout provisioning, authenticated recovery console, OS settings boundary, disk installer, OS updater/snapshots or hardware qualification. An app package is not an OS image.

The added `os/` workflow now creates a VM-testable Debian UEFI GPT image without opening host block devices. Its explicitly passwordless **VM demo** profile is not a hardware installer or secure release. The remaining safety gates are recorded in [RISKS](RISKS.md), [INSTALLATION](INSTALLATION.md) and [ROADMAP](ROADMAP.md).

## M4a addendum (2026-10-24)

The absence of a pinboard described above is a baseline finding, not the current branch state. M4a adds portable metadata-only source references, responsive sidebar/Pinboard/dashboard readers, free/semantic-anchor scrolling, recoverable card lifecycle and explicit source editing through the M3b registry/guarded Project path. No third-party creative text, new text-storage format or second writer was added. There are 46 new Rust tests (14 core, 31 UI, one alias), actual non-root pin PTY/restart/paste-guard coverage and read-only CLI pin validation. Journal/scratchpad/handoff, full content presets and large-note/power-loss qualification are still unfinished. See [PINS](PINS.md) and [STATUS](STATUS.md) for scope and current evidence.
