# Brain, Library and Review (design)

Status: direction approved by the owner, 2026-11-22. Calls marked "(my call)" are mine and
need the owner's review. This design replaces the rc25 Zotero client
([rc25 design](2026-11-21-rc25-zotero-library-pdfs-design.md)) and removes Zotero from rc24
([rc24 design](2026-11-21-rc24-essays-mla-zotero-design.md)). rc24's essays, MLA 9 export,
per-project `sources.json` and `[@key, p. 12]` citations stay and are what this design's
citations write into.

## Why

The owner reads, takes notes, studies and writes. They have tried Zotero, RemNote, Logseq
(file and DB versions), SuperMemo 20 and Nostos. Each did part of the job, none felt complete,
and none could be extended by the owner:

- Zotero and RemNote box notes and sources into folders and hierarchies. Their calendar and task
  features are weak.
- Logseq's linking is good, but its spaced repetition is mediocre and it is hard to bend to a workflow.
- SuperMemo has incremental reading but handles PDFs badly. RemNote's incremental-reading
  plugins are mediocre.
- Nostos (Christian-Gennari/Nostos) is closest in spirit: a library where one book holds all its
  formats, notes on the page, `@` to quote with a citation. But it is a self-hosted .NET/Angular web
  app (AGPL), the owner doesn't want to host or pay for it, and it can't connect to WrOS.

So WrOS gets its own, built into Grimoire, where it can keep growing to fit the owner. Once it
works on WrOS the owner wants it everywhere, so a phone must be able to join later without a redesign.

What success looks like to the owner: it feels **complete** for their workflow, and anything
missing can be added because it's part of WrOS.

## What it is

Three new full-screen **views** next to today's writing screen, sharing one store of linked material:

| View | What it is |
|---|---|
| **Write** | Grimoire as today. Projects and their layout slots (Drafting, Research, …) unchanged. |
| **Library** | Every work (book, article, lecture, audio) with search and many sorts; open one to read it with its notes in a side pane. |
| **Brain** | A Tana/Logseq-style outliner, full screen, for notes on their own. |
| **Review** | Due cards, optionally filtered by tag. |

Views behave like window-manager workspaces: each keeps its own state, and you flip between them.
Any view can pull another in as a **side pane**: the brain or a book beside a manuscript, the book a note points
to beside the brain.

```
 Write │ Library │ Brain │ Review                                     ← view bar
┌ Discipline and Punish · p.200 ─────────────┬ Notes ────────────────┐
│ …the Panopticon is a marvellous machine    │ - Panopticon #concept  │
│ which, whatever use one may wish to put    │   - self-discipline    │
│ it to, produces homogeneous effects of     │ - ▌                    │
│ ████ power. ████                           │                        │
```

## Goals

1. **Brain (outliner).** Every line is a node: it can be folded, moved, indented, linked to, tagged
   and turned into a card. Links run both ways: every node shows what links to it. Notes are linked across all
   projects. A tag (e.g. `#essay-ethics`) marks a project's slice without walling it off.
2. **Tags and supertags.** A supertag gives the nodes it tags fields. `#book` brings author, year,
   ISBN, status; `#article` brings DOI, journal; `#lecture` brings speaker, date. Supertags are
   defined by the owner in the brain itself, not hard-coded (the built-in source supertags ship as editable defaults).
3. **Saved searches.** A node can be a live query (by tag, field, text, link), shown as a table or list.
   The Library view is a saved search over source-type nodes.
4. **Library.** A **work** is a brain node with a source supertag. It holds:
   - **Formats:** EPUB, PDF and audio files of the same work.
   - **Highlights and notes,** each tied to its exact place in a file.
   - **Links:** the notes that mention it and the projects that cite it.

   Search, filter and sort by author, type, tag, status, date added, project tag. No fixed folder tree.
5. **Adding sources.** Look up a DOI (Crossref) or ISBN (Open Library) to fill fields. Manual
   entry works offline. Import `.bib` and CSL-JSON, sharing rc24's parser.
6. **`@` everywhere.** One picker searches the library and brain from any typing surface:
   - **In a note:** `@Discipline and Punish` inserts a live link to the work. It can point deeper, to a page, a
     highlight or an audio moment.
   - **In a manuscript:** it inserts rc24's citation, e.g. `[@foucault1975, p. 200]`, and copies the work's
     CSL-JSON record into that project's `sources.json`. Projects stay self-contained for export, Drive
     and backups.
   - **Picking a highlight:** brings in the quoted text with its citation.
7. **Reader.**
   - **EPUB:** reflowed text with highlights and notes.
   - **PDF:** selectable text first, then real page images in the native graphical session.
   - **Audio:** playback with chapters, bookmarks and notes on a moment.

   Notes go in the side pane, attached to the spot.
8. **Review (spaced repetition).**
   - **Card types,** written in the outline (RemNote's core set): `front :: back`, reversible cards,
     cloze `{{hidden}}`, multi-line answers and list answers.
   - **Cards from highlights,** linking back to their spot in the book.
   - **Grades** run 0–5 (0 blank, 5 instant). FSRS schedules; WrOS stores the exact grade and maps it to
     FSRS's four ratings (mapping tunable later, never losing the stored grade).
   - **Filtered review:** a session can cover one tag.
9. **Sync through Google Drive**, offline-first, merging edits from several devices with nothing lost.
   It uses WrOS's existing Drive sign-in, with no server and no subscription.
10. **Readable copy.** Every page is also written out as Markdown, so notes stay readable even if the app
    broke.

## Non-goals (later, not now)

- Phone app (its own project, on this sync; WrOS has an Android Tauri prototype to start from).
- Hanzi stroke-order cards (animation on WrOS; writing practice probably on the phone).
- Incremental reading and incremental writing.
- Calendar and tasks. The owner said every tool does these badly and isn't sure yet what they want.
- Making study material from PDFs automatically.
- Video. Transcription.
- Zotero in any form: no login, no client, no import, for now.
- Changing writing projects. Their scheme stays as it is.
- Collaboration with other people.

## Anti-goals (failure even if it "works")

- Losing or duplicating a note, highlight or review after two devices merge.
- A required folder hierarchy, or a project tag that hides material from the rest of the brain.
- Notes trapped in a format nobody else can read: the Markdown copy must always exist.
- Anything that needs a server the owner runs or pays for.
- An AGPL or GPL dependency linked into WrOS (WrOS is MIT): no MuPDF, no poppler linked in.
- A view that only works with a mouse.
- A weak calendar or task feature added "for completeness".

## Constraints

- **Hardware:** HP Stream 11: 4 GB RAM, eMMC storage. Native session is 170×48 cells of
  Unifont 8×16 at 1366×768. Everything must stay responsive on it.
- **Input:** keyboard-only and usable with Colemak or QWERTY. The owner's standing key decisions hold:
  Esc opens menus, F10 is the menu bar, and a Super *tap* is Find anything. See
  [interface decisions in rc13 design](2026-11-12-rc13-interface-design.md).
- **Graphics:** the native DRM session draws its own pixels (`crates/wros-graphics`), so page images are
  possible there. The console session stays text-only. The Cage/Foot and wgpu backends must at least
  fall back to text.
- **Offline-first:** everything but lookups and sync works with no network.
- **Network:** each new network use is documented in `docs/API_AND_NETWORK_USE.md`, and no personal data goes
  in requests.
- **Packaging:** through Nix, like the rest of WrOS. Pin versions and upgrade on purpose.
- **Interface:** new features live in menus and panes, not the dashboard (being retired).

## Approaches

The main choice was what notes are stored as.

| | Approach | Trade-off |
|---|---|---|
| A | Markdown file per page + rebuilt index (Logseq's old file graphs) | Reuses Drive sync and Git as they are, but block links, supertag fields and moves are fragile in text, and conflicts are per file. Logseq abandoned it for these reasons. |
| B | SQLite + a hand-written change log synced through Drive | Fast and fully ours, but merging concurrent moves and edits by hand is where sync bugs come from. |
| **C** | **Loro (MIT, Rust CRDT) as truth + SQLite index + Markdown copy** | **Chosen by the owner.** Merges offline edits from several devices correctly, including outliner moves (Loro's native movable tree). It's a younger library; the owner asked about more established ones. |

**More established libraries, rejected for now:** Yjs/Yrs and Automerge. Neither has a native tree move.
Built on parent pointers, both lost nodes to cycles in a public concurrent-merge test where Loro
lost none ([tuna-os/gtk-office-suite#977](https://github.com/tuna-os/gtk-office-suite/pull/977)).
Automerge is the fallback.

**Guardrails the owner accepted:**
1. Stage 0 stress-tests Loro on our workload and on the HP Stream before real notes go in.
2. One pinned Loro version, upgraded on purpose.
3. The readable Markdown copy always exists.
4. Loro sits behind WrOS's own storage interface, so Automerge (with our own move handling) can
   replace it without touching the views.

Also rejected: running Nostos as a server with Grimoire as a client (hosting, weekly API churn,
unauthenticated REST), and Logseq DB as the brain (beta, sync alpha, no third-party sync access).

## Decided by the owner

- The four views and the name **views**. "Workspaces" already means Write's layout slots.
- **Switching:** Super+1 … Super+4 switch views. A Super tap stays Find anything, which also searches notes and the
  library. Super chords are unused today (`src/main.rs` drops them), so they are free.
- **Brain editor:** an outliner (every line a node), not pages of prose.
- **Library and brain:** the library is a saved search over supertagged nodes. A work holds formats, then
  highlights, then links.
- **`@`:** works in notes and manuscripts alike; in manuscripts it writes rc24's citation format.
- **Storage and sync:** Loro with the guardrails above. Each device keeps its own store; changes travel through Drive.
- **Cards and grades:** RemNote's core card types; 0–5 grades over FSRS.
- **Zotero:** none.
- **Stage order:** below.

After the review, 2026-11-22:

- **Brain opens to Today.** A fresh page for today, where quick notes land, like Logseq and Tana.
  - **Pages:** the list of named pages is one key away.
  - **What a page is:** a top-level node, either a named page or a day page.
  - **Not a calendar:** a day page is just a page, with no calendar features. It is also not the Writing journal
    (`docs/JOURNAL.md`), which stays as it is. Keep the names distinct on screen.
- **`@` in manuscripts opens the picker as you type,** but only when `@` starts a word.
  - **Literal `@`:** `name@email.com` never triggers it, and `@@` types a plain `@`.
  - **The picker:** has a page (locator) field and an "author named in my sentence" switch (rc24's `[-@key]`).
  - **One way to cite:** it replaces rc24's separate Insert citation dialog. rc24's insertion guards still apply: the
    right pane, revision checks, one Undo step.
- **Notes are encrypted on Drive.** Change files and snapshots are encrypted before upload.
  - **New devices:** each one (including a future phone) receives the key once, e.g. by scanning a QR code shown on
    a device that already has it.
  - **Losing the key:** losing every copy loses the synced copy, like WrOS's existing recovery key, and the screens
    say so.
  - **Library files:** they are the owner's own books, so encrypting them is the builder's call; say which in the
    stage 5 plan.
- **Cited sources stay linked to their work.** A project's `sources.json` record carries the work ID it came from.
  - **When a work changes:** if its details change in the Library, opening or exporting the project says "Details
    for this source changed. Update?" A finished paper's Works Cited never changes silently.
  - **Hand-entered rc24 sources:** they stay project-only until the owner chooses to add one to the Library.
  - **Same key or same source:** a project record that already matches the work's key or source is offered for
    linking, never overwritten.

## Design

### Data model (direction; details to the builder)

- **Node.**
  - **Contents:** a stable ID and a text body with inline formatting, links, tags and `@` references stored as ID tokens
    (shown as titles, so renaming a work never breaks a link).
  - **Place:** a parent and an order among siblings, in Loro's movable tree.
  - **Extras:** tag IDs, field values, and created and edited times.
- **Supertag.** A node that defines fields: name, type (text, number, date, node link, URL, choice,
  file), and whether the field is required. Built-in defaults:
  - **Source types:** `#book`, `#article`, `#chapter`, `#lecture`, `#audio`, `#web`. Their fields map
    one-to-one onto CSL-JSON, so a work becomes a `sources.json` record.
  - **Others:** `#concept`, `#person`, `#card` (if cards are tagged rather than marked by syntax; builder's
    call).
- **Format (file).** Belongs to a work:
  - **What it is:** type (EPUB, PDF or audio), SHA-256, size, label ("2nd ed. scan").
  - **Where it is:** a path under the library folder, and whether it's kept on this device.
- **Anchor.** Where a highlight, note or bookmark points:
  - **EPUB:** a CFI plus the quoted text with some text before and after, so the anchor can be found again.
  - **PDF:** page, character range, quads and quoted text.
  - **Audio:** a time and an optional end time.

  Anchors that no longer match are shown as orphaned, never dropped.
- **Highlight.** A node (so it can be linked, tagged and carded) with an anchor and a colour.
- **Card.** Derived from a node's syntax (`::`, `{{ }}`, list/multi-line markers).
- **Review log.** Append-only entries: card ID, time, grade 0–5, device. The schedule is
  computed by replaying the log through FSRS, so merged logs from two devices give the same
  schedule everywhere. (my call)
- **Project tags.** A project's slice is a tag. `project.toml` records which tag belongs to the project, so Write
  can offer "this project's notes" in its side pane. (my call)

### Where things live (my call)

| What | Where |
|---|---|
| Loro store (snapshot + updates) | `~/.local/share/grimoire/brain/` |
| SQLite index (FTS5) | Beside the store. Disposable: rebuilt from Loro whenever missing or stale. |
| Library files | `~/Documents/Library/<work-id>/`, so ordinary backups include them |
| Markdown copy | `~/Documents/Brain/`, one file per top-level page (named pages and day pages), plus one per work with its highlights. Written after saves, never read back as truth. |

The shelf must not mistake `~/Documents/Library` or `~/Documents/Brain` for a writing project.

### Sync (stage 5)

- **Encryption:** everything under `WrOS/Brain/` is encrypted before upload (see the owner's decisions).
- **Updates:** each device writes Loro updates, bounded change files, into `WrOS/Brain/changes/<device-id>/` on Drive
  through the existing Drive helper (`os/scripts/wros-sync`, rclone). Each device only ever writes its own folder,
  so Drive never has two writers on one file.
- **Merging:** every device imports the others' change files. Loro merges them.
- **Compaction:** occasionally a device writes a snapshot and the old change files are retired, without deleting
  another device's unread files.
- **Library files** go to `WrOS/Library/`. Large audio can stay on Drive only, marked
  "not on this device".
- **Same rules as WrOS's readable Drive sync:** sync never blocks typing, offline queues, and the screen
  says what it is doing.

### Reader (stage 4)

- **EPUB:** parsed in Rust and reflowed into cells. Images come later.
- **PDF:** pdfium (BSD/Apache) through `pdfium-render`, from the Nix package of pdfium binaries.
  - **Text first:** selection maps to character boxes, then to anchors.
  - **Page images:** come next, in the native DRM session. `wros-graphics` gains a way to draw an image into a pane's
    rectangle.
  - **Other sessions:** console and the Cage/Foot fallback stay in text mode.
- **Audio:** reuse WrOS's existing playback (rodio/symphonia, or mpv, both already shipped). MP3, M4A
  and M4B with chapters must play. Builder's choice which engine.
- **Notes and highlights:** select, highlight, then note in the side pane at that anchor.

### Review (stage 2)

- **Scheduling:** `fsrs-rs` with default parameters at first. Optimising parameters from the owner's own log comes
  later.
- **Screen:** shows the prompt, reveals the answer, and takes a grade 0–5 on number keys.
- **Choosing cards:** a session can be filtered by tag or saved search. Due counts appear in the view bar.
- **Naming:** Write already has "Review notes" (`src/app/review_notes.rs`), so menus say "Review cards"
  where the two could be confused. (my call)

## Stages

Each stage ships on its own and is usable when it lands.

| # | Stage | Done when |
|---|---|---|
| 0 | **Loro test** | A deterministic stress test merges thousands of random concurrent edits, moves, deletes and tag/field changes from 2–3 simulated devices, with no lost, duplicated or cyclic nodes. A 50,000-node store loads, searches and saves within budgets the builder sets and measures on the HP Stream (or a 4 GB VM if the laptop isn't available). If it fails, switch to Automerge before stage 1. |
| 1 | **Brain view** | Outliner editing, links, backlinks, tags, supertags with fields, saved searches, Markdown copy, the view bar and Super+1..4, Brain opening to Today, and the brain as a side pane in Write. Laptop only. The brain store and (from stage 3) `~/Documents/Library` join WrOS's encrypted backups (`os/scripts/wros-backup`, which today covers only projects and a small settings profile), with a tested restore; a restored store takes a new Loro device ID so two devices never share one. (my call: backups only covered projects, so notes would otherwise be unprotected until stage 5) |
| 2 | **Review view** | Card syntax, the review screen, 0–5 grades, FSRS from the replayed log, filtered review. |
| 3 | **Library and `@`** | Works with formats, the sortable/filterable library, DOI/ISBN lookup, `.bib`/CSL-JSON import, the `@` picker in notes and manuscripts writing rc24 citations and `sources.json` with linked work IDs. rc24 ships first (it's the next release) and builds the source store and `.bib`/CSL-JSON parser; this stage plugs into them. Stages 0–2 don't depend on rc24. |
| 4 | **Reader** | EPUB with highlights and notes, then PDF text, then PDF page images in the native session. Cards from highlights. |
| 5 | **Drive sync** | Two laptops editing offline converge after sync with nothing lost; library files sync. |
| 6 | **Audio** | Playback with chapters, bookmarks and notes on a moment. |

## Left to the builder

- **Crates and modules:** where the code lives, e.g. a new `grimoire-brain` crate under `crates/` used by
  `grimoire-app`.
- **Formats:** the exact Loro container layout, change-file format and compaction rules.
- **Outliner keys:** within the owner's standing key decisions. Follow Logseq/Tana conventions where they don't
  clash, and work on the Linux console, where Super may not arrive. Every view must also be reachable
  from the menus and Find anything.
- **Memory and speed budgets,** measured in stage 0.
- **Rendering:** EPUB parser and reflow details. How page images reach the screen in the native session.
- **Mapping 0–5 grades to FSRS ratings,** documented so it can be tuned.
- **Tests:** following the repo's patterns (`Desk` UI tests, PTY tests, VM tests).
