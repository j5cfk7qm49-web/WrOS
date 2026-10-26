# M4b: saved observations and original reflections

**Tested initial journal, not a complete productivity/edit ledger.** Original scratchpad promotion, handoff and explicit workspace starters/pin sets are now separately tested in [M4c](WORKFLOW.md). No hardware, power-loss, legal or production-install qualification is implied.

## Two kinds of material, not two text writers

Open **Writing journal** in Ctrl-K or Writing tools, use its home widget, or press dashboard `j`. Left/Right select the previous/next local date; `t` returns to today. Tab switches between read-only recorded facts and a cached reflection preview. Up/Down/PgUp/PgDn/Home/End scroll only this reader. Escape leaves it.

**Facts** are schema-v1 JSON observations, logically append-only through the core API. There is no UI for manually changing a fact or replacing it with an invented achievement. An atomic guarded transaction rewrites the daily JSON representation, but never edits/removes an existing fact. This is not OS-enforced append-only storage, attestation or a file lock.

**Reflections** are ordinary project Markdown at `notes/journal/YYYY-MM-DD.md`. They start empty, with a date title in YAML, only when explicitly requested. `Enter` creates/opens the original in Secondary; `e` uses Primary. Existing canonical/device-inode ownership focuses the original context rather than making another writer. Cursor/selection, undo/redo, raw unknown YAML, guarded save, history, unavailable/conflict handling and recovery remain the existing Project/editor machinery. Strict focus cannot focus a hidden Secondary; Escape then Ctrl-D restores the normal layout.

Mapped single letters are commands, not a prose input field. Bracketed/clipboard paste in the journal is ignored instead of executing those commands; cut/undo/drag cannot mutate a hidden editor. Only the explicit source editor accepts prose. Facts never contain a copied reflection body or automatically rewrite its source. Creating a reflection refuses outside-project/internal metadata sources and notes-directory symlinks into manuscript/research roles. A known manuscript alias, non-UTF-8/unavailable source or ambiguous retained pair of editor contexts is not guessed or replaced.

## What is actually measured

A completely clean, successful **manuscript** save records the first Project-count checkpoint in that app/book context. Later automatic checkpoints require another manuscript save and at least ten monotonic minutes. A normal quit/book-change request records a final saved snapshot if manuscript work was saved in that context. `c` explicitly captures current clean counts sooner; it requires a real prior manuscript save, refuses dirty buffers, and does not duplicate unchanged measured values. It always observes **now**, never backdates facts into the selected historical view.

Opening/reloading/navigating the reader, moving a cursor, creating a reflection or saving notes does **not** manufacture a manuscript-writing event. A snapshot is not proof of exclusively typed words, every autosave/edit, or an actual process exit. A failed quit/book switch can be followed by more work, so the boundary is correctly named **Exit/book-change requested**.

Each observation keeps:

- An RFC3339 observation instant and local offset; its filename/day is that observed local date, not the UTC or app-open date.
- The app/book-context opening instant, not an active-duration measurement.
- The current Project manuscript word count at a fully clean save boundary. This is a cache/model snapshot, not an atomic filesystem-wide view of concurrent external writers.
- An optional known initial word total and derived signed delta since app open. External adoption/import/structure/recovery can affect this delta; it is not exclusively typing.
- The existing daily net tracker only when its date and initial completeness are known. Negative change remains negative, not zeroed or relabeled gross additions/deletions.

Known non-UTF-8/unreadable/unsettled manuscript sources inhibit complete-total observations rather than inventing zeroes. If the initial book total was unknown, since-open delta and daily net remain unavailable for this context even after repair; current complete totals can then be recorded. Clock adjustments may put observation wall time before app-open wall time; both are retained without inventing elapsed time.

There is **no gross-added/deleted count, active/idle clock, drafting/revision classification, detailed footnote ledger, creative achievement or new historical backfill**. Those require real M5 edit/idle instrumentation. An existing daily net number is not a substitute for them.

## Persistence and failure behavior

Portable model/store: `crates/grimoire-core/src/journal.rs`. TUI cache/input/adapter: `src/app/journal.rs`.

Facts use `.grimoire/writingdeck/journal/YYYY-MM-DD.json`, at most **1024 facts / 1 MiB per day**, stable increasing IDs per date, schema version, local date and next ID. Only one date is loaded for the view/writer at a time. Different local dates rotate to separate files. No archive/prune/purge UI exists yet; limits fail visibly without deleting old facts.

Loading/validation/navigation is read-only, even for absent directories. Unknown/nested/future/corrupt/date-mismatched data, oversized files, external changes and failed writes preserve original bytes. Existing ancestors are checked before directory creation, including early/late outside-project symlinks. Append transactions clone/validate, compare the last-read fingerprint and advance memory only after success. There is no silent normalization, automatic external-log adoption before retry, OS lock or cross-process compare-and-swap. Hostile filesystem races remain outside the guarantee.

Failed observations stay pending in memory, not falsely counted as recorded. `r` deliberately reloads and retries, merging an exact persisted retry idempotently rather than duplicating or replacing other facts. Resuming writing cancels an unrecorded exit request. Pending state is bounded to an outstanding checkpoint and exit request, not an unbounded hidden event queue. Manuscript saving itself still succeeds independently of log failure. A normal quit refuses an unrecorded observation; the existing explicit second Ctrl-Q can force exit after a visible warning, losing pending automatic facts. A crash/forced exit can also miss observations after the last sampled checkpoint. This is not a durable per-edit ledger or off-device backup.

Reflections use the documented date-path convention, not a separate association index. Moving one elsewhere keeps its source/undo/history, but the date reader will not find it until its path is restored. Existing bytes are never replaced by an automatic prose template. Per-frame rendering reads only cached facts/Project nodes; no file access, system command or metadata save occurs in draw. Identity/source checks run on input/reload and the existing disk tick; external text retains the upstream settling/conflict rules.

Read-only `grimoire writingdeck check-config DIR` audits known date JSON files without creating directories, rewriting facts or interpreting unknown filenames. It does not claim to validate an unknown future index format.

## Verification

- **10 portable core tests:** side-effect-free reads; exact round-trip/retry; signed/unknown counts and offset/clock behavior; local-date rotation; corrupt/future/nested-unknown/date mismatch preservation; conflict/I/O rollback; schema/size/fact/ID limits; original reflection bytes; early/late metadata/reflection containment and internal notes-to-manuscript role escape refusal.
- **23 `writingdeck_journal_*` UI tests:** actual four-size flow/restart; no phantom note-only/view facts; negative net; monotonic throttle and request-boundary idempotence; explicit clean snapshot; raw YAML/undo/owner preservation; both editor positions/reader scroll; cached render without I/O; readonly input/drag guard; future/unknown/corrupt/conflicting/failing-I/O logs; pending reload; waiting source save/recovery/quit guard; outside/manuscript/non-UTF-8/ambiguous sources; strict focus; correct date; cross-book separation; cold-start unknown totals and repair without invented net; additive widget/alias/help; real event-gated safe-demo lesson and real-novel refusal.
- **One new optional-alias compatibility test:** existing F10/F11/F12 defaults remain unchanged. These **34 new Rust tests** supplement M4a, not a second count of the full suite.
- **Two compiled-app PTYs:** real palette/CPR/input, saved manuscript observation, explicit original reflection save, bracketed-paste command suppression, isolated undo/redo, normal quit and actual loaded-source restart at **80×24, 100×30, 120×40, 160×50**; separate future-log and unavailable-total scenarios preserve bytes, refuse the first quit and only exit on explicit force after the displayed warning. The latter saves while the reader is open: a count warning with no constructible pending fact cannot hide the force instruction.
- **One new read-only headless CLI test**, plus two basic ANSI screen-tracker tests. The tracker reconstructs differential cursor/erase/UTF-8 runs for assertions; it is not a complete terminal emulator or physical-keyboard certificate. The retained pin and dual-editor PTYs remain enabled.

[STATUS](STATUS.md), [software evidence](evidence/test-results.json) and the [actual compiled-app journal screens](evidence/journal-pty-screen.txt) report current totals, source identity and the separate facts/reflection views. The reader viewport is ephemeral; resize-driven scroll clamping is not implemented/qualified, so Home resets an out-of-range reader after reflow.

The independent whole-VM probe checks boot/system/console integration, **not journal controls**. Large reflection/log performance, flash-write budget and full power-cut behavior remain unqualified.

## Separate M5 operation ledger

M5 now supplies real source-range operation receipts and conservative observed inter-edit accounting through separate readers/widgets and metadata parts. It does not retroactively upgrade this sampled journal, its historical baselines or manual reflections into gross/active/exclusive-typing facts. See [DAILY_CHALLENGES](DAILY_CHALLENGES.md) for origins, word-unit policy, unavailable gaps, clock/civil-date behavior, checkpoint tail and limits.

## M6 source captures are not writing-time evidence

Spark save/pin/scratch/outline/task/independent-scene actions are explicit original-source operations, not observed human typing or automatic qualitative-completion proof. A new experiment is initially empty, compile-excluded and references a separate prompt note; opening/generating it cannot manufacture manuscript prose or exclusive typing time in the M5 ledger. Existing sampled journal/hand-authored reflection behavior is retained. See [STORY_SPARK](STORY_SPARK.md).
