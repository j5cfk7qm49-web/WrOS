# WritingDeck OS architecture

## Scope and non-negotiable boundaries

WritingDeck is a Debian-based writing appliance, not a custom kernel. The principal UI remains Rust/Ratatui Grimoire. Ordinary novel writing must remain offline, unprivileged and usable without AI, a GPU, cloud authentication or a desktop. The project is independent and has no implied NaNoWriMo affiliation.

Preserve upstream Markdown/YAML, atomic saves, conflict checks, history, resume and recovery. Add versioned state alongside them; never migrate manuscript text merely to implement a UI widget. [AUDIT](AUDIT.md) is the baseline.

## Layers

```text
UEFI / Debian GRUB -> maintained Debian kernel + initramfs -> systemd
  -> console-setup -> WritingDeck committed keymap -> tty1 PAM login
  -> non-root writingdeck-session -> grimoire
       |
       + Ratatui + App (existing editor/autosave/history/conflicts)
       + Deck (home, dual-editor workspaces, tutorial, cached settings)
       + Pins (metadata-only references, cached Project readers, explicit source editing)
       + Journal (append-only observation API, separate original reflection sources)
       |   + bounded request queue -> system worker -> writingdeck-sys::Backend
       |       + Linux: fixed nmcli/brightnessctl/systemctl/pkexec arguments
       |       + Simulated: explicit label, no hardware/network writes
       + grimoire-core (ordinary files and new portable domain services)
       + grimoire-app (existing portable mobile/front-end service API)

privileged keyboard helper: fixed layouts only, root-owned files only
  -> independent 15-second systemd timer -> restore committed layout
```

`crates/writingdeck-sys` knows OS capabilities but not manuscript contents. It never receives a book path or text. `grimoire-core` and `grimoire-app` remain free of Linux privilege/service dependencies. The TUI uses both layers. External Python analyzers and cloud adapters are future workers, never synchronous input-loop work.

## Implemented vertical slice

- Debian 13 (trixie) rootfs recipe and regular-file UEFI GPT disk assembler; no custom kernel, desktop, block-device formatter or host mount operation.
- tty1 session wrapper refuses root; PAM remains the authentication boundary. VM-only autologin is conspicuously marked and networkless acceptance tests use a disposable COW layer.
- Fixed system request types and backend trait; bounded background command queue, bounded captured command output, subprocess deadline, explicit unsupported/error state.
- Root-owned console keymap application before tty1 login and an independent systemd preview/revert transaction.
- Home widgets derived from existing project/session data, hide/reorder configuration, dual-editor workspace slots, cursor/selection/free scroll, outline width and secondary proportion, additive safe shortcut aliases.
- Strict focus hides retained references and removes them from focus traversal.
- All 39 tutorial lessons cataloged, 27 currently wired to real application events; unavailable lessons are not fake-completable. Writing exercises require a separately created demo project.

This slice is not the full requested OS. See [FEATURE_MATRIX](FEATURE_MATRIX.md).

## Persistence and concurrency

| State | Location | Ownership / behavior |
|---|---|---|
| Novel and notes | Existing Markdown + `novel.toml` | Existing core is the source of truth; YAML frontmatter remains raw |
| Autosave/history/recovery | Existing `.grimoire/` facilities | Unchanged save guards and recoverable copies |
| Selected workspace, both document/selection/cursor/scroll states, pane sizes, card-ID sets, 21 widgets, launch/low-power preferences | `.grimoire/writingdeck/layout.json` | Schema v3; v1/v2 migrate in memory; bounded guarded save; unknown/unreadable/future schema preserved |
| Scratch references, promotion receipts, handoff provenance/settings | `.grimoire/writingdeck/workflow.json` | Schema v1; no prose; bounded clone/validate/guarded-save; Prepared before exclusive scene creation |
| Scratch and handoff originals | `notes/scratchpad/NNNNNN.md`, `notes/handoff.md` | Ordinary Markdown/YAML; explicit owner-aware Project editing; no preview writer |
| Pin references/presentation | `.grimoire/writingdeck/pins.json` | Schema v1; no copied bodies, bounded quote anchors; guarded clone/validate/save transactions; containment/alias checks on input/tick, no render I/O |
| Journal facts / reflections | `.grimoire/writingdeck/journal/YYYY-MM-DD.json` / `notes/journal/YYYY-MM-DD.md` | Bounded schema-v1 guarded per-date append; sampled saved Project counts, explicit unavailable metrics; manual original source through existing owners; no draw I/O |
| Operation counts, origins, source references and observed clock spans | `.grimoire/writingdeck/ledger/YYYY-MM-DD/{session}-{part}.json` | Schema v1; 128-group/1 MiB parts; guarded prefix publication/retry; no manuscript body or crash-tail completeness |
| Advanced settings/goals/calendar/sprint/results/optional awards | `.grimoire/writingdeck/productivity.json` | Schema v1; bounded validated original-preserving metadata; timer anchors deliberately not serialized |
| Frozen daily assignments/acceptance/completion/rerolls | `.grimoire/writingdeck/challenges/YYYY-MM-DD.json` | Schema v1; civil date + zone/offset/profile/source stamps; explicit measured/self-report evidence |
| Spark corpus | Compiled original `spark-corpus.json`, immutable identified TSV sources/decision manifest | Original AI-assisted wording; version/text/file hashes, license/attribution; no human/legal attestation |
| Spark reference history/settings/exports | `.grimoire/writingdeck/spark/state.json` | Schema 1; guarded 1 MiB; history 128/favorites 64/dismissals 128/recent 12/receipts 256; no prompt or manuscript body |
| Fixed Spark daily / optional assignment | `spark/daily/YYYY-MM-DD.json` / `spark/assignments/YYYY-MM-DD.json` beneath WritingDeck metadata | Original IANA date/zone/version frozen; optional explicit assign/accept/self-report, not measured M5 IDs |
| Spark original outputs | `notes/sparks/*.md`, workflow scratch source, `manuscript/98-Spark-*-experiment.md` | Prepared → exclusive create → ready; edited originals retained; independent scene initially empty and compile-excluded; no cross-file power-cut transaction |
| Tutorial progress | `.grimoire/writingdeck/tutorial.json` | Schema v1; verified event, skip and replay state; guarded save |
| Shortcut aliases | `paths::data_dir()/writingdeck/shortcuts.json` | Per-machine v1; conflicts/unreadable files are not overwritten |
| Committed console layout | `/etc/writingdeck/keyboard.json` | Root-owned; application changes only through helper/polkit |
| Preview token + monotonic deadline | `/run/writingdeck/keyboard-preview.json` | Root-only, boot-ephemeral; never a manuscript setting |
| Package/image identity | `/etc/writingdeck/release`, `build/appliance/packages.lock.tsv` | Recipe/profile/base and actual resolved package inventory |

`sync::save_guarded` detects ordinary external state changes. It is not a lock against an unrelated writer racing between check and rename. The M3b registry enforces exclusive ownership of the open views by project-contained canonical path and Unix device/inode, retaining missing-file leases and refreshing identity after atomic replacement. It is not an OS lock or cross-process compare-and-swap. Unknown schema versions must be rejected rather than normalized into older state.

Workspace restoration deliberately gives the existing newer resume record precedence on startup. Explicitly choosing a workspace restores both positions/selections/free scroll, active role, visibility and ratio. Invalid UI preferences must never prevent manuscript autosave.

## Document engine and next domain services

1. **Document/pane registry — Tested M3b:** `crates/grimoire-core/src/documents.rs` owns file identity; `src/app/documents.rs` moves/swaps complete Editor contexts (never cloned on each frame). `App.editor/open` is the active context; the other keeps geometry, caret/manual-scroll and click state. Pure render/position/save-resume operations borrow a role via a temporary context swap; input activation additionally flushes both and changes focus. Both contexts flush to their original Project nodes, including inactive bulk edits and recovered words. The registry is validated before saves, external adoption and editor replacement; unsafe alias changes keep both buffers and attempt recovery. A duplicate-file selection focuses its current owner. Core source fingerprints remain the existing save guard, not a new unsafe write path. See [WORKSPACES](WORKSPACES.md) for threshold/layout/rename behavior and boundaries.
2. **Pins — Tested M4a:** portable `pins.rs` metadata store, case-sensitive semantic heading/quote resolution and bounded retained preview rows; TUI cache maps canonical identities to authoritative Project nodes. Card/sidebar/dashboard reads never create writers. Explicit source editing reuses document ownership/save/recovery, including refusal to guess between two externally aliased retained editors. Card lifecycle is metadata-only recoverable archive/Trash, never source deletion. See [PINS](PINS.md). **Journal — Tested initial M4b**: portable daily append API records actual sampled saved Project counts, offsets/request boundaries and unknown metrics; reflections are separate ordinary Notes Markdown. Cache/input adapter reuses exclusive editor ownership/save/recovery and read-only modal/paste guards. See [JOURNAL](JOURNAL.md). **Scratchpad/handoff — Tested M4c:** `workflow.rs` stores references/Prepared→Ready receipts, never prose; exclusive independent scene creation preserves the original. Handoff is user-authored ordinary Notes shown on next app/book open after recovery priority, not an inferred intention or reboot/time fact. Source editing reuses the same registry/save/recovery path. Workspaces add explicit empty original starters and global-card ID sets, not copied bodies. See [WORKFLOW](WORKFLOW.md) and [WORKSPACES](WORKSPACES.md). The sampled journal remains separate from M5 recorded operations and does not infer unobserved gross/time.
3. **Productivity — Tested initial M5:** `measure.rs` / opt-in Editor range receipts preserve word identity/classification; `accounting.rs` records bounded metadata-only parts and conservative observed inter-edit spans using injected clocks; `productivity.rs` and `challenges.rs` provide goals/calendars/sprints/optional awards/frozen civil-date tasks. `src/app/productivity.rs` owns captured original-editor operations, save/checkpoint barriers and cached reader/widget rows. Unassigned time/word gaps stay unavailable. Native source counts remain separate; no exclusive typing, all-day crash coverage or indefinite archival claim. See [DAILY_CHALLENGES](DAILY_CHALLENGES.md).
4. **Implemented M6 Story Spark:** immutable identified original source drafts compile to 1,047 accepted plots and 84 exercises with complete text/source hashes, attribution and explicit agent-review limits. The cached reader uses persisted reference-only selection/history/favorites/filter/receipt state, fixed IANA civil-date daily results and actual M4/M5 source-safe actions. Combined selection uses drafting exercises and explicit compatibility; no Cartesian plot-count inflation or runtime model/network generator. See [STORY_SPARK](STORY_SPARK.md).
5. **Diagnostics:** revision-tagged jobs, source maps and common UTF-8 byte ranges; Harper in process, optional Nabokov/proselint workers, cancellation/stale-result discard, deduplication and explicit user-only application of suggestions. Dialogue and narration policies differ.
6. **Continuity/revision:** explicit facts with scene references, quote/context text anchors, named immutable checkpoints and reversible variants. Inferred issues remain suggestions, not definite contradictions.
7. **Publishing/backup:** one export validation model; real footnote parts/pagination tests; opt-in versioned, cryptographically verified off-device backups; a separate conflict-aware sync protocol. Never confuse local Git history, rclone copy and Google Docs round trips.

## Security and services

Use NetworkManager/logind/UPower/brightnessctl and maintained Debian services; do not create a root application server accepting arbitrary commands. The first helper accepts only four registered layout IDs and token-bound preview/confirm/cancel. The policy grants only that exact executable to active local members of `writingdeck`; there is no passwordless general sudo rule.

Power requests are user-confirmed and require successful manuscript saves. Normal development does not install appliance login/polkit policies. Timezone, sound, updates, first-run account provisioning and screen-lock integration remain planned. An upstream authenticated tty2 is the maintenance path; a passwordless VM demo cannot provide production authentication or recovery guarantees.

Production release gates: encrypted Debian installation, authenticated login and lock/suspend interaction, recovery secrets, firmware/Secure Boot policy, signed update source and tested rollback. The present GRUB fallback image is unsigned. Never deploy its passwordless profile to a physical laptop.

## Performance design

No system command runs from drawing or key handling. OS status is on demand and cached; the backend has no manuscript data. Low-power UI mode reduces redraws to one second and disables the spectrum; input events still wake immediately. Python/spaCy whole-book jobs must be opt-in/debounced and benchmarked before shipping. Their budgets are not inferred from the current Grimoire VM RSS.

Pin scroll/presentation currently commits synchronous guarded metadata transactions. Large-card/document CPU, metadata latency and flash-write budgets remain acceptance work; rendering is file/command-free but is not claimed constant-time in source length.

## Verification

Unit tests exercise persistence, invalid versions, conflicts, safe alias validation and monotonic preview state. Rootless Python tests inject the helper's runner/clock. Real QEMU UEFI tests check the actual tty1 screen, non-root process, polkit eligibility, real console mapping and systemd-timed revert. [TESTING](TESTING.md) separates those from all remaining hardware/credential tests.
