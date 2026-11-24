# Brain, Library and Review (design)

Status: direction approved by the owner, 2026-11-22. The owner selected paged Loro on 2026-11-23. The remaining proposals below are awaiting approval. Calls marked "(my call)" are mine and
need the owner's review. This design replaces the rc25 Zotero client
([rc25 design](2026-11-21-rc25-zotero-library-pdfs-design.md)) and removes Zotero from rc24
([rc24 design](2026-11-21-rc24-essays-mla-zotero-design.md)). rc24's essays, MLA 9 export,
per-project `sources.json` and `[@key, p. 12]` citations stay and are what this design's
citations write into.

## Open questions for the owner

1. **New-device key (proposed).** Use a roughly 12-word recovery phrase. Type it on the laptop
   or scan it as an offline QR code on the phone. Use a reviewed encoding with at least 128 bits
   of random recovery entropy and a checksum. Derive a key that wraps the random library key.
   Confirm the longer entry and the offline recovery workflow.
2. **Smaller stage 1 (proposed).** Ship saved searches as lists, with Brain beside Write as the
   only side pane in this release. Add editable tables and the other side-pane combinations
   when their views land.
3. **Cite an edition (proposed).** Make citations point at the edition quoted, with a stable work ID
   alongside it. A paperback page and a revised scan can then retain their own bibliographic details.
4. **Reader order.** Keep PDF extraction first as the foundation and the console fallback.
   Deliver page images with the full anchor schema in the first complete PDF milestone. Add full
   reflow after OCR, and decide on Foot sixel after measuring the fallback. This makes the original
   text-first plan precise enough to avoid saving early highlights in a temporary format.
5. **Measurements.** Use the proposed stage 0 budgets below and a corpus with English and Chinese
   PDFs. Confirm the HP Stream run and access to representative pages. Keep any private papers on
   the device and share hashes and timings.

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
The full design lets one view open another in a **side pane**: Brain or a book beside a manuscript,
or a book beside a note. Stage 1 starts with Brain beside Write (proposed).

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
3. **Saved searches.** A node can be a live query by tag, field, text or link. Stage 1 runs bounded predicates and displays lists (proposed).
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
   It uses WrOS's existing Drive sign-in, with no server and no subscription. Book files travel with the
   notes, so the whole library is portable, not just the notes. Sync should feel simple: one route, nothing to pair.
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

**Stage 0 decision, 2026-11-23.** The owner chose paged Loro for RC25
because it follows the original design. Loro keeps the authoritative operation
history and native movable tree. SQLite keeps the rebuildable current view and
search index. On the HP Stream, this design passed the six steady-state
benchmark limits and produced a 52 MiB checkpoint. The SQLite ledger also
passed and remains a comparison candidate with a 396 MiB checkpoint.
Stage 1 carries forward the measured layout and extends the tests to complex
editing, projection recovery and keyboard-to-display latency.
[Decision evidence](../../evidence/rc25-brain-alternatives.md#hp-stream-results)

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
1. Stage 0 stress-tests Loro on our workload and on the HP Stream before stage 1 ships.
2. One pinned Loro version, upgraded on purpose.
3. The readable Markdown copy always exists.
4. Loro sits behind WrOS's own storage interface, so Automerge (with our own move handling) can
   replace it without touching the views.

Also rejected: running Nostos as a server with Grimoire as a client (hosting, weekly API churn,
unauthenticated REST), and Logseq DB as the brain (beta, sync alpha, no third-party sync access).

## Decided by the owner

- The four views and the name **views**. "Workspaces" already means Write's layout slots.
- **Switching:** Super+1 … Super+4 switch views. A Super tap stays Find anything, which also searches notes and the
  library. `crates/wros-input/src/native.rs` and `src/main.rs` both drop Super chords today.
  Enable the four view chords in both places and preserve tap detection. Test held and repeated keys,
  release order, Colemak and QWERTY, and modal drafts. Menus and Find anything reach every view
  that is available, including on the console.
- **Brain editor:** an outliner (every line a node), not pages of prose.
- **Library and brain:** the library is a saved search over supertagged nodes. A work holds formats, then
  highlights, then links.
- **`@`:** works in notes and manuscripts alike; in manuscripts it writes rc24's citation format.
- **Storage and sync:** Loro with the guardrails above. Each device keeps its own store; changes travel through Drive.
- **Grades:** finer than four levels (the owner's ask). 0–5 over FSRS and RemNote's core card types are my call.
- **Zotero:** none.
- **Stage order:** below.

After the review, 2026-11-22:

- **Brain opens to Today.** A fresh page for today, where quick notes land, like Logseq and Tana.
  - **Pages:** the list of named pages is one key away.
  - **What a page is:** a top-level node, either a named page or a day page.
  - **Not a calendar:** a day page is just a page, with no calendar features. It is also not the Writing journal
    (`docs/JOURNAL.md`), which stays as it is. Keep the names distinct on screen. (my call)
- **`@` in manuscripts opens the picker as you type,** but only when `@` starts a word.
  - **Literal `@`:** `name@email.com` never triggers it, and `@@` types a plain `@`.
  - **The picker:** has a page (locator) field and an "author named in my sentence" switch (rc24's `[-@key]`).
  - **One way to cite:** rc24 builds this picker from the start, over the project's sources, instead of a separate Insert
    citation dialog, so nothing is thrown away; stage 3 widens its search to the library. rc24's insertion guards still
    apply: the right pane, revision checks, one Undo step. (my call)
- **Notes are encrypted on Drive.** Change files and snapshots are encrypted before upload.
  - **New devices:** the owner wants a QR code for a phone and a code typed in for laptop to laptop.
    The proposed recovery phrase above can serve both. A short code alone requires an authenticated,
    expiring key exchange.
  - **Losing the key:** losing every copy loses the synced copy, like WrOS's existing recovery key, and the screens
    say so.
  - **Library files:** encrypted on Drive like the notes, through the same mailbox. (my call)
- **Cited sources stay linked to their work.** A project's `sources.json` record carries its library
  provenance. See [Citations and library editions](#citations-and-library-editions).
  - **When a work changes:** if its details change in the Library, opening or exporting the project says "Details
    for this source changed. Update?" A finished paper's Works Cited never changes silently.
  - **Hand-entered rc24 sources:** they stay project-only until the owner chooses to add one to the Library. (my call)
  - **Same key or same source:** a project record that already matches the work's key or source is offered for
    linking, never overwritten. (my call)

## Design

### Data model (direction; details to the builder)

- **Node.**
  - **Contents:** a stable logical ID, schema version and text body. Inline links, tags and `@` references are
    typed tokens with stable target IDs and optional anchor IDs. A token edits and moves as a unit.
    Keep unresolved tokens recoverable and render the current target title when available.
  - **Today and built-ins:** freeze the local civil date when opening Today. Use one logical ID per date
    in the library, even when devices have different zones or independently open the same date.
    Use fixed IDs for built-in supertags and fields. Merge duplicate initializations without dropping
    edits, and preserve aliases created by recovery or migration.
  - **Place:** a parent and an order among siblings, in Loro's movable tree.
  - **Extras:** tag IDs, field values, and created and edited times.
- **Supertag.** A node that defines fields with immutable IDs and editable names, types and required status.
  Field types include text, number, date, node link, URL, choice and file. A type change preserves the
  previous values until a checked conversion succeeds or the writer repairs them. Keep CSL mappings
  keyed by field ID. Built-in defaults:
  - **Source types:** `#book`, `#article`, `#chapter`, `#lecture`, `#audio`, `#web`. Their fields map
    one-to-one onto CSL-JSON, so a work becomes a `sources.json` record.
  - **Others:** `#concept`, `#person`, `#card` (if cards are tagged rather than marked by syntax; builder's
    call).
- **Format (file).** Belongs to a work:
  - **What it is:** type (EPUB, PDF or audio), SHA-256, size, label ("2nd ed. scan"), original file name.
  - **Where it is:** stored once by fingerprint in `files/` (see below), never inside the Loro store. Notes point at
    the fingerprint, so files never break when moved and are never stored twice.
  - **Per device:** whether this device keeps a copy ("sync this file" or "this device only").

  The owner asked whether books could go *in* the Loro store, wanting the media portable. Not inside: Loro keeps all
  history in memory-loaded snapshots, so a removed 300 MB audiobook would stay forever and opening notes on a 4 GB
  laptop could mean loading gigabytes. Owner agreed; what they wanted was simple sync of books with notes.
- **Anchor.** Where a highlight, note or bookmark points:
  - **EPUB:** a CFI plus the quoted text with some text before and after, so the anchor can be found again.
  - **PDF:** a versioned page-space and quote anchor with layered document identity, defined under Reader.
  - **Audio:** a time and an optional end time.

  Anchors that no longer match are shown as orphaned, never dropped.
- **Highlight.** A node (so it can be linked, tagged and carded) with an anchor and a colour.
- **Card.** Derived from a node's syntax (`::`, `{{ }}`, list/multi-line markers).
- **Review log.** Append-only versioned events with stable event and card IDs, timestamp, device, local
  day/zone, original grade 0–5 and rating-map version. The Review section defines deterministic replay. (my call)
- **Project tags.** A project's slice is a tag. `project.toml` records which tag belongs to the project, so Write
  can offer "this project's notes" in its side pane. (my call)

### Where things live (my call)

Everything lives together in one folder, `~/Documents/Brain/`, so backups and moving to a new laptop are simple.
The owner said one folder isn't required; it's my call because it keeps sync and backup simple.

| What | Where |
|---|---|
| Loro store (snapshot + updates) | `Brain/store/` |
| Book files | `Brain/files/<sha-256>`, named by fingerprint, never changed once added. Backed up from stage 1 on (see Stages). |
| Markdown copy | `Brain/readable/`, one file per top-level page or work, with ID-based filenames, readable text, links and fields. Update affected files asynchronously, and publish an atomic generation watermark after the projection completes. Detect and rebuild stale copies after a restart. Drive only receives encrypted data. |
| SQLite index (FTS5) | `~/.cache/grimoire/brain/`, outside the folder. Stamp with index schema and indexed Loro version. Update affected pages in the background and rebuild from Loro whenever missing or stale. |

The shelf must not mistake `~/Documents/Brain` for a writing project.

### Local save and recovery (stage 1)

Take an exclusive Brain writer lock. Acknowledge a save after its checked update is durable.
Write a checksummed checkpoint to a new generation, synchronize it and its directory, and
atomically advance a manifest. Keep the previous generation until the new one verifies.
Replay complete checked updates after restart, quarantine truncated data and report what needs
attention. Retain original downloaded bytes while Loro is waiting for dependencies. A full
snapshot can omit unapplied updates. Retire covered bytes only after a verified durable checkpoint
includes their operations. [Loro persistence](https://www.loro.dev/docs/tutorial/persistence)

Add a Brain backup kind to `wros-backup` and schedule Brain-only edits independently of project saves.
Use a pinned committed metadata generation and stream/reuse checked immutable blobs so backup and
restore avoid extra library-sized copies on eMMC. Restore and verify to an isolated location before
activation. Give a restored or cloned store fresh Loro session peers and a fresh mailbox device
identity. An old backup opens in isolation. Return recovered content to the live library by explicit
reintroduction so existing synced deletions retain their meaning.
[Loro peer IDs](https://www.loro.dev/docs/concepts/peerid_management)

### Citations and library editions

Keep stable work and edition IDs separate. An edition supplies the bibliographic record and
owns the applicable files and locators (proposed). Identical files share one SHA-256 blob.
Use stable field IDs for CSL mappings even when a supertag's labels change.
The rc24 source store preserves versioned provenance with each record through editing, per-key
merge, backup and timeline restore. At stage 3 record the work ID, edition ID and a versioned
fingerprint of the accepted library bibliographic fields. Normalize only the fields included in
the fingerprint. Show a preview on open/export when library details change and preserve local
edits, the citation key and existing locators unless the writer explicitly accepts a change.
The accepted snapshot still supports export offline or if its Library target disappears.
Hand-entered rc24 sources stay local until explicitly added to the Library. Offer to link matching
keys or records without overwriting them.

### Sync (stage 5)

Use WrOS's existing Google Drive sign-in and transfer primitives. Implement a Brain mailbox
with its own protocol since the current helper accepts project roots. Keep all objects under
`WrOS/Brain/` encrypted and authenticated. An immutable envelope names the library, schema
and key version, object ID, author device and causal range. Use an established authenticated
encryption construction and correctly generated nonces, with keys stored in private app data.
Evaluate rclone crypt for authenticated contents, filename privacy and future phone
interoperability before choosing the wire format. Include the sync key in an explicit encrypted
offline recovery export. Keep credentials and keys out of generic settings backups.
[rclone crypt](https://rclone.org/crypt/)

Each device writes immutable bounded objects to its own `changes/<device-id>/` folder.
Deduplicate retries by authenticated object identity. Track received and durably applied
progress separately. Retry missing dependencies, and keep ambiguous duplicates until verified.
A missing listing conveys neither deletion nor delivery. Test interrupted uploads, duplicates,
out-of-order delivery and quota exhaustion. Sync runs in the background with offline status and
keeps lock and power paths independent. [Drive duplicate names](https://rclone.org/drive/#duplicated-files)

Start with full snapshots and retained CRDT history. Verify a durable snapshot's causal coverage
before retiring mailbox objects, with a tested path for returning devices to retrieve the coverage.
Wait for a causal checkpoint, explicit device-retirement rules and a recovery path before shallow
history truncation. Defer remote blob collection until device recovery is proven.
[Loro shallow snapshots](https://www.loro.dev/docs/concepts/shallow_snapshots)

Keep media encrypted in `files/` under protected fingerprint-derived names. Upload a verified
blob once and fetch it on open or through a device's download selection. Use quota feedback and
a size-based default. Project Drive behavior remains independent.

### Reader (stage 4)

- **EPUB:** parse in Rust and reflow into cells. Keep CFI plus quote/context for highlights.
- **PDF engine:** PDFium through pinned `pdfium-render` in a dedicated helper. Use the explicit
  Nix library path and matching bindings, with an installed render/extract smoke test. Bound its
  output, memory, per-page deadline and restarts. Run Poppler utilities or MuPDF only as separate
  benchmark/debug processes. Use no GPL or AGPL reader code or bindings.
- **Text:** extract a versioned canonical Unicode stream in PDF character order with a map to
  page boxes. Track character ranges independent of reading order. Group lines and blocks,
  use structure tags where usable, split columns on gutters, and handle margin text, footnotes
  and dehyphenation. The reflow projection maps back to canonical offsets and page boxes.
  Use links to the page for equations, figures and tables that need their original layout.
- **Native DRM:** compose the cached bitmap, highlights, overlay and cursor within the normal
  frame before presentation. Handle resize and VT recovery. Provide keyboard zoom and pan,
  bounded RAM and disk caches, nearby prefetch and viewport tiles at high zoom. Begin with
  page text on console, Cage/Foot and wgpu. Consider Foot sixel after measurement.
- **Anchors:** retain SHA-256, page index and printed label, unrotated PDF-space rectangles,
  page geometry, versioned normalized quote/context, page-relative character ranges, anchor
  version and text layer ID. Use optional PDF IDs and page geometry, image/text hashes and
  page counts for classification. Reuse geometry when only the OCR layer changes. Across
  revisions, require a unique confident match on quote, context and page alignment. Store
  a new revision when rematching succeeds and keep mismatches visible for reattachment.
  Scans support region marks before text becomes available.
- **OCR:** start with the pinned standalone Tesseract CLI, one background thread per job,
  one worker, with a second on idle AC only after measurement. Prioritize the open page and
  next three. Record completion atomically by page, resume missing pages, and support pause.
  Compare English, Simplified and Traditional Chinese models, including vertical layouts,
  at 200 and 300 dpi. Try PP-OCRv5 on the HP Stream only if ONNX Runtime fits its CPU and RAM.
  The Windows PC can produce compatible sidecars for offload.
- **Storage:** cache per-page hOCR/TSV with a PDF fingerprint, transform, page and model version.
  Keep cache data, tiles, OCR jobs and layout out of Loro. Store annotations, quote evidence
  and anchor revisions in Loro. Leave original PDFs intact.
- **Order:** extraction and basic console text, native page view and complete anchors, then
  resumable OCR, full reflow, and hardened version-to-version re-anchoring.
- **Audio (stage 6):** reuse a shipped playback engine. MP3, M4A and M4B with chapters must play.

**HP Stream measurements.** Stage 3 prepares the kit and records results here when available.
Record exact CPU and flags; pinned PDFium binding; cold helper and warm PDFium/hayro/Poppler
render time and RSS; extraction accuracy, time and RSS; DRM composition and page flip;
English and Chinese OCR time per page, RSS and character error at 200/300 dpi with one and two
workers; 30-minute thermal and input behavior; PP-OCRv5 instruction compatibility, speed and RSS;
ocrs English time and RSS; Unifont wide Hanzi; and Foot sixel. Use the owner's representative
PDFs and the full [measurement matrix](2026-11-22-pdf-reader-ocr-research.md#hp-stream-measurement-record).
Results pending.

### Review (stage 2)

- **Scheduling:** pin `fsrs-rs` and its parameter set. Version and document the mapping from original grades
  0–5 to ratings. Keep all original events and their mapping version. A card ID survives text edits and cloze
  reorder through persisted identities. Deduplicate events by ID, order by recorded effective time with stable
  ID tie breaking, immunize replay against the receiving machine's clock and time zone, and preserve the
  review's recorded local day and zone. Define and fixture-test clock-skew and day-boundary behavior before
  relying on merged reviews. Cache schedules and recompute only affected cards.
- **Screen:** shows the prompt, reveals the answer, and takes a grade 0–5 on number keys.
- **Choosing cards:** a session can be filtered by tag or saved search. Due counts appear in the view bar.
- **Naming:** Write already has "Review notes" (`src/app/review_notes.rs`), so menus say "Review cards"
  where the two could be confused. (my call)

RC26 pins [fsrs-rs 6.6.2](https://docs.rs/fsrs/6.6.2/fsrs/) with its FSRS-6
default parameters and a 90% retention target. Rating map 1 maps 0 and 1 to
Again, 2 to Hard, 3 and 4 to Good, and 5 to Easy. The grade, mapping version,
and recorded local time with its UTC offset stay in the event.

Each device clamps a backwards clock correction to its last effective time
for that card. Replay orders events by effective UTC time and then operation
ID. It uses the difference between recorded local dates, with a zero lower
bound and a ceiling of the elapsed UTC days plus one. The receiving
machine's clock and zone do not enter replay. The SQLite schedule cache
rebuilds from the journal, and a new event updates only its own card.

Edits save the active card IDs alongside the note. Cloze answers retain their
IDs when reordered. Matching uses the previous order for indistinguishable
answers and for remaining edits of the same card type. A removal keeps its
grade history. An answer with an empty `::` uses the note's children in
outline order. The parser reads the first 32 clozes of a note up to 16 KiB.

## Stage 0 correctness and performance gate

Use a deterministic seed and a reference model with expected visible and recoverable content.
Deletes hide the subtree even against a concurrent move or edit, with content retained for explicit
recovery. Resolve competing field edits by a stable tie-break and keep the losing value recoverable.
Undo reverses the local logical action and preserves concurrent work. Define split and join by stable
node identities and boundary tokens. Check independently initialized built-ins and concurrent Today
creation, uniqueness, acyclicity and convergence. Exercise two and three devices, churn, operation
reordering, truncated writes, pending dependencies across restart, and rejoining from an old backup.

Before running, fix a dated fixture and provisional thresholds. Start with 50,000 visible nodes
ranging from short notes to 2 KiB entries, 500,000 accumulated operations, 10,000 deleted nodes,
100,000 simulated review events and indexing/Markdown work active. Proposed targets on the
HP Stream: cold open p95 ≤ 3 s, keystroke-to-display p95 ≤ 50 ms, foreground FTS p95 ≤ 150 ms,
peak process-tree RSS during checkpoint ≤ 512 MiB, ordinary discrete edit ≤ 64 KiB written
(amortized; record checkpoint separately), and temporary staging ≤ one extra checkpoint plus
64 MiB. Record initial sizes, WAL and projection bytes, and actual temporary disk peak. A 4 GB VM
can exercise the memory cap; use the laptop for CPU, disk and interaction qualification. Keep
thresholds fixed for each reported run and identify any revisions. If Loro fails the correctness
or hardware gate, document the failure and repeat the same gate with Automerge behind the
storage interface before stage 1.

## Stages

Each stage ships on its own and is usable when it lands.

| # | Stage | Done when |
|---|---|---|
| 0 | **Loro test** | The correctness and performance gate passes, with reproducible results from the HP Stream. |
| 1 | **Brain view** | Outliner editing, links and backlinks, tags, supertags, list searches (proposed), Markdown copy, view bar, Super+1..4 and console alternatives. Brain opens to Today and can sit beside Write, using the project tag. The independent encrypted backup kind restores verified contents under a fresh device identity. |
| 2 | **Review view** | Stable cards, original grades, deterministic FSRS replay, filtered sessions and due counts. |
| 3 | **Library and @** | After rc24 ships: work/edition records, shared format blobs, sortable and filterable Library, DOI/ISBN lookup, shared import, a library-wide picker and accepted source provenance. Produce and run the PDF/OCR measurement kit on the HP Stream. |
| 4 | **Reader** | EPUB, then PDF extraction, native page view and durable markup, OCR, and full reflow with console support. Make cards from highlights. |
| 5 | **Drive sync** | Two devices editing offline converge with encrypted changes and media, recoverable history and per-file device choices. |
| 6 | **Audio** | Playback with chapters, bookmarks and notes on a moment. |

## Left to the builder

- **Crates and modules:** where the code lives, e.g. a new `grimoire-brain` crate under `crates/` used by
  `grimoire-app`.
- **Formats:** the exact Loro container layout and envelope serialization within the recovery and sync rules above.
- **Outliner keys:** within the owner's standing key decisions. Follow Logseq/Tana conventions where they don't
  clash, and work on the Linux console, where Super may not arrive. Every view must also be reachable
  from the menus and Find anything.
- **Memory and speed evidence:** measure the agreed budgets in stage 0 and record the candidate, corpus and hardware.
- **Rendering:** EPUB parser and reflow details. How page images reach the screen in the native session.
- **Mapping 0–5 grades to FSRS ratings,** documented so it can be tuned.
- **Tests:** following the repo's patterns (`Desk` UI tests, PTY tests, VM tests).
