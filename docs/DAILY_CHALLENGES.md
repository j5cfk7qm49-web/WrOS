# Daily challenges, goals and recorded productivity

**Initial M5 is implemented with core, real-controller and compiled-app PTY tests.** This is not production, physical-device or complete all-day activity certification. Existing Grimoire daily/book/scene targets, sampled net-growth journal, Pomodoro and basic sprints remain separate and unchanged. The new system does not relabel their baselines as gross input or typing time.

## Finding the features

Ctrl-K offers **Productivity ledger**, **Advanced goals**, **Calendar and rest days**, **Advanced sprints**, **Milestone collection**, and **Daily challenges**. Esc → Writing tools → Productivity tools provides the same six actions without enlarging the first menu. Six optional dashboard widgets and six additive logical aliases expose them too; existing default chords and old widget order/visibility survive.

The feature reader has six numbered tabs: 1 edits, 2 goals, 3 calendar, 4 sprints, 5 collection, 6 challenges. Tab cycles pages. Arrows choose rows; PgUp/PgDn scroll; Esc returns; F1 explains policy. Readers do not accept pasted text as commands or modify an underlying editor selection. `r` explicitly reloads/retries metadata; this is not a prose reload or automatic conflict merge.

## What the measurements actually mean

Both exclusive original editors emit metadata-only receipts from **actual changed source ranges**, not Git, a before/after document word-count difference, file modification times, or an open window. Keyboard, deliberate paste, replace/format, undo, redo, history restore and programmatic adoption remain distinguishable. Batch replacement counts only actual hits and preserves its reversible receipt. Loading an existing document is not input. An unmeasured history step is an explicit gap, never inferred gross input.

**Source word-unit policy v1:** whitespace-delimited units containing at least one Unicode alphanumeric character; surviving source characters establish one-to-one word identity across a range mutation, including splits and merges. This is bounded source-token measurement, **not Grimoire's native word count, linguistic segmentation, or fully rendered Markdown**. Code and link destinations can count. Private inline notes/TKs, footnote reference labels and definitions are classified separately. Revealing/hiding existing excluded text or moving existing units into/out of footnotes is a recorded transfer, not new authored gross words. No excluded prose is serialized.

A measurement is bounded to **262,144 bytes** of before/after source. Larger operations retain real character/operation facts with a word gap. Word totals/goals that depend on that gap are unavailable rather than apparent zeroes. A measured word sprint cannot start from unavailable counters. Native existing-source novel/scene goals explicitly retain the original native/private-note policy; toggling ledger footnotes does not rewrite native totals.

Manuscript, Notes, Research and other source roles are separate; opening research in Secondary does not make it novel text. The existing canonical/dev-inode ownership registry prevents duplicate editable owners. Metrics contain relative source references, counts, origin/mode, clock information and receipts, **never a second manuscript body**.

### Time is observed, not exclusive-typing proof

An injected clock supplies monotonic milliseconds, an offset-bearing RFC3339 wall instant and, when available, a validated IANA timezone. Activity is the interval **between actual eligible edits on the same source in the same mode**, only when at/below the configured idle cutoff (default 60 seconds, configurable 1–900). Switching mode/source, focus loss or explicit pause breaks the pair. No startup, idle tail, absent app or offline interval is credited.

Keyboard-to-keyboard intervals are labeled **keyboard-pair time, not proven exclusive human typing**. Observed inter-edit time can include thinking or other activity between the two edits; it is not CPU time or independent proof of attention. Time beyond the idle cutoff is excluded, not shortened into invented work. Drafting, Revision and Read-through modes are explicit. Reading without edits is not inferred from a running timer.

Unambiguous civil-date crossings are split using the recorded timezone/offset and monotonic interval. DST/travel, inconsistent clocks and monotonic reset have deterministic tests; ambiguous duration remains unassigned and affected time goals become unavailable. Local civil dates are not assumed to be fixed 24-hour windows. IANA identity is recorded when discoverable; fixed-offset fallback is identified, not fabricated as a named zone.

## Persistence, failure and limits

- Operation parts: `.grimoire/writingdeck/ledger/YYYY-MM-DD/{session}-{part:06}.json`.
- Settings, goals, schedules, results and optional collection: `.grimoire/writingdeck/productivity.json` (schema 1).
- Frozen assignments: `.grimoire/writingdeck/challenges/YYYY-MM-DD.json` (schema 1).

JSON is bounded to 1 MiB per file. Parts contain at most 128 groups, with earlier byte-size rollover if necessary. A same-date pending prefix is published in one guarded operation, not a full-file rewrite/fsync per group. A partial multipart failure retains every unpublished group; exact retry/reload does not duplicate already published IDs. Pending groups are bounded to 4,096; reload audits at most 16,384 ledger files / 64 MiB. These are bounded initial read/checkpoint facilities, **not an indefinite history/archival service**. Source editing/saving is retained when a metric cannot be recorded; unknown facts are not reconstructed from prose.

The UI coalesces actual operations and uses an intended 30-second dirty checkpoint plus explicit save/normal-quit checkpoints. Sprint polling is monotonic and injected, approximately once a second. A due timer checkpoints actual pending receipts before final results. Draw consumes cached rows and performs no file I/O or event generation. Date-sensitive cache labels refresh across midnight.

**A crash can lose the uncheckpointed tail.** No durable per-keystroke/session-coverage journal is claimed; recorded sums are not a complete all-day history. These are not OS locks, hostile-race CAS, cross-file transactions, power-cut certification, backup or cross-process attestation. Unknown/corrupt/future/externally conflicting files and original Markdown are preserved. Pending ledger/runtime failures block normal quit until repaired/retried or the existing explicitly warned force quit is chosen. A badge-only failure is optional: it preserves prose and recorded ledger and does not block normal quit. A changed sprint checkpoint is not treated as an optional badge write.

`grimoire writingdeck check-config BOOK` audits settings, recognized ledger parts and existing challenge date files without creating a date or rewriting metadata. Unrecognized challenge filenames are left uninterpreted. Live metrics are local, not tamper-proof certificates; external editing/history replacement and cross-process merges are not authenticated measurement.

## Goals and calendar

Create an integer target for daily input additions, weekly keyboard additions, monthly recorded net, native current novel/scene words, daily observed inter-edit minutes, daily revision operations, daily footnote additions, or a configured calendar deadline's recorded net. Existing-source targets are explicitly native totals; they do not create historical input/time. Weekly/monthly/deadline windows use civil dates. Goal metadata can be disabled/deleted without deleting prose or earlier earned collection entries.

Calendar `n` explicitly creates the offline **50,000 recorded-net-word November preset**; Space toggles a chosen rest day, `b/e/g` edit start/end/target. Invalid dates or targets preserve the field and original settings. Replacing a schedule retains the previous schedule/rest choices in bounded history. Required remaining-workday pace and ahead/behind use an identified end-of-day plan. Forecast requires at least three of up to seven **recorded work dates**, using their stated mean; absent dates are not silently invented zero-work days. Rest days are configurable, not punitive. No external account or endorsement is implied.

## Sprints and optional collection

Timed, recorded-net-word, revision-operation, no-editing experiment and accepted measured-challenge variants provide pause/resume, explicit stop, optional retained breaks and bounded results/history. Word sprints allow paste and include recorded replay in net change; revision sprints use Revision mode operations. Paused work is excluded. A no-editing experiment **does not block, discard or rewrite deletions**; it reports observed deletion, keeps undo working and withholds that experiment's completion badge rather than punishing the writer.

Challenge-linked sprints use the accepted task's frozen mode/footnote policy and **remaining measured target**, not a new arbitrary target. A subjective task cannot acquire an invented measured sprint. Restarted timers require explicit resume and add no offline wall time. A completed/skipped break remains in the record and does not force work to resume. Timed results are actual observed elapsed segments at recognition/checkpoint, not exclusive writing duration or metrologically exact alarm scheduling.

Milestones are disableable, deduplicated, quiet and non-punitive: real keyboard/revision counters, reached native/measured targets, recorded sprint/challenge results, actually observed learning events, and clean successfully saved finished-scene/chapter statuses. Finished status is **the author's classification, not measured literary quality**; metadata holds a manuscript-relative reference, not prose. Chapter status awards require its manuscript scenes to be available, clean and marked finished. Lifetime goals do not earn another copy merely because the date changes. The collection retains at most 512 awards; exhaustion declines a new optional award without blocking metrics or source saving. Turning collection/notices/challenges/tracking off preserves originals and prior records, with no forced streak or notification.

## Daily assignments

The bundled M5 bank contains **30 original MIT exercises, three in each of ten categories**: productivity, creative writing, character, plot, dialogue, description, point of view, revision, read-through and Spark combinations. This is **not M6's required 1,000-option plot corpus** or the Story Spark generator.

The local civil-date file freezes creation instant, IANA/offset context, corpus/source stamps, difficulty, maximum duration, drafting/revision/read-through/mixed filters, assignment IDs and acceptance/completion evidence. Reopening/reboot/repeated travel date retrieves that file without regeneration. An absent past date is not manufactured. Today can explicitly recover an empty selection after widening filters. Filters save for future assignments and explicit rerolls; changing them never silently replaces today's tasks.

Assignments are optional. `a` accepts and checkpoints its baseline; `c` checks subsequent recorded work against a measured criterion; `x` explicitly self-reports a subjective task; `R` deliberately rerolls. Rerolls retain the full prior assignment/profile/acceptance and source reference. Recent-repeat avoidance is deterministic with a bounded fallback when the eligible pool is exhausted; an exhausted pool does not imply infinitely unique exercises. Quantity is not quality; merely opening a page, elapsed timer time, or self-report is not upgraded to measured writing.

## Tests, tutorials and remaining gates

Core tests inject clocks, dates and seeds for DST/travel/repeated dates, classification/word gaps, paste/replay/restore, idle cutoff, goals/calendar/forecast, sprint pause/restart/breaks, filters/rerolls/history, future state and publication failure. **30 M5 controller/UI tests** exercise both source roles, actual commands, four terminal sizes, valid/invalid inputs, optional failure versus critical runtime, midnight, accepted remaining-target policy, genuine saved author statuses and tutorial completion/noncompletion. The compiled non-root app has four-size PTY edit/save/undo/redo/goals/calendar/sprint/challenge/separate-process-restart plus corrupt-ledger retry evidence. Existing app/editor/pin/journal/workflow tests stay enabled.

Lessons 17/19 retain their real baseline goals/sprint exercises. **18 requires a saved calendar plus an actual rest-day choice; 20 reviews a real nonempty collection; 21 requires accepted, persisted measured daily completion.** Opening a page, an empty collection, arbitrary Next presses, or subjective completion cannot satisfy these exercises. Writing exercises remain gated to an explicitly created safe demo, never an ordinary novel.

See [TESTING](TESTING.md), [STATUS](STATUS.md) and [ROADMAP](ROADMAP.md) for current full-suite/VM evidence and remaining gates. The synthetic release-mode accounting probe is an executed shared-sandbox microbenchmark, not physical low-power latency, flash endurance, all-day coverage or battery qualification. Loaded-log/card/analyzer stress, interrupted power, signed release and hardware acceptance remain M10 work.
