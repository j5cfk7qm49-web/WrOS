# Testing and verification

## Local writing-session network boundary

`test_local_session_privacy_pty.py` uses the real compiled app at 80x24,
100x30, 120x40 and 160x50, an isolated HOME, generated Markdown/YAML and a
disposable local bare Git remote. It proves startup, explicit local snapshot,
quit and restart leave that remote empty and never invoke Git push/fetch/pull
or transport commands. Local committed prose, history browsing and Undo are
verified. A second case adds the remote during an open session. The old binary
fails before the first edit because opening the book pushes its history.
No external server, credentials or real manuscript is involved. Captures under
`build/verification/local-session-privacy-screens` use
`WRITINGDECK_CAPTURE_PTY=1`. Native verified backup/restore remains separate.


## Shared story registry recovery boundary

`crates/grimoire-core/tests/story_identity.rs` checks lost/empty registries,
late orphan references before allocation, book-wide records and invalid bounded
dependencies. `test_story_identity_pty.py` uses the actual app at all four
sizes, retains the original registry as a fixture backup, proves wrong-target
restore is refused without changing source/sidecars, exercises normal writing
and Undo, then explicitly restores that registry and correctly restores scene B.
Captures use `WRITINGDECK_CAPTURE_PTY=1` under
`build/verification/story-identity-screens`. [Scope and limits](STORY_IDENTITIES.md)
distinguish this guard from automatic recovery or a cross-file transaction.


## Reviewed draft and passage restoration

`test_passage_restore_pty.py` runs the actual application at all four terminal
sizes for both milestones and variants. It creates two saved drafts, verifies
the second is the one reviewed/restored, edits two independent passages, cancels
the default confirmation, then restores only the second change. It checks
original YAML/private text, recovery variant and dated-history bytes, Undo,
offline help and a fresh process. Captures use `WRITINGDECK_CAPTURE_PTY=1` under
`build/verification/passage-restore-screens`. Core and controller regressions
separately cover UTF-8/empty/EOF/bounds, changed revisions, backing files and both
external/in-process metadata. [The scope](PASSAGE_RESTORE.md) retains limits.


## EPUB note validation

The Debian verifier additionally requires `epubcheck` and `default-jre-headless`.
`test_epub_footnotes.py` invokes the actual compiled CLI and the packaged JAR
through Java, requires a clean validator report without skipped coverage, and
checks same-file note/return links, private exclusions, scene-local definitions,
multiple paragraphs and unchanged source. Its compiled TUI workflow covers
80x24, 100x30, 120x40 and 160x50, offline help, review/export, Undo and restart.
Set `WRITINGDECK_CAPTURE_PTY=1` to retain packages/validator receipts in
`build/verification/epub-footnotes` and four frames in `epub-footnote-screens`.
These are build/test components; no OS image or all-reader/accessibility
qualification is implied. [Scope and limits](EPUB_FOOTNOTES.md) stay explicit.
The older manuscript PTY refusal now explicitly selects unsupported Paperback;
it retains Word-only no-credit and actual PDF completion checks for lesson 34.


## Manuscript page-footnote rendering

The Debian verifier requires `libreoffice-writer-nogui`, `poppler-utils` and
`fonts-liberation` for actual output checks. These are test/build dependencies;
adding them to CI does not refresh the minimal OS image. The tests refuse
missing renderers rather than skipping. `test_docx_footnotes_render.py` checks
real Word relationships, physical PDF pages and long-note continuation.
`test_manuscript_footnote_pty.py` checks genuine async export, lesson 34,
original editing/Undo and fresh-process progress at all four terminal sizes.
Set `WRITINGDECK_CAPTURE_PTY=1` to retain disposable render artifacts under
`build/verification/manuscript-footnotes` and terminal frames under
`build/verification/manuscript-footnote-screens`. See
[scope and limits](MANUSCRIPT_FOOTNOTES.md). The
[independent full Debian run](evidence/manuscript-footnotes-local.json) passed
1060 Rust tests per configuration, 180 Python tests with zero skips, both Clippys and 21 gates.

## Warm-screen qualification

[Warm-screen controls and boundaries](WARM_SCREEN.md) distinguish the rootless
actual-C fake DRM driver/real-state fault tests from real VM driver refusal and
physical acceptance. `python3 -m unittest discover -s os/tests -p 'test_display_*.py' -v`
requires a normal user, a C compiler, pkg-config and libdrm-dev. The ordinary full
verifier includes these tests and the actual four-size Settings PTY. Never pass
the fake DRM library to a root process or stage it into an image.

`GRIMOIRE_BINARY=... python3 os/tests/display_vm.py --artifacts-directory NEW_NATIVE_DIRECTORY`
uses a new no-NIC QEMU layer over the regular VM demo only. It retains source,
binary, bridge and base-disk hashes, real polkit/driver status, page barriers and
an actual screenshot. Unsupported-driver refusal is separate from successful
gamma/readback/timer/Off application; neither establishes physical qualification.

## Current M7b qualification

Use `bash tools/test-m7b.sh` after provisioning the exact runtime described in
[M7B](M7B.md). The unified verifier records source/tool hashes, the tested binary
and runtime marker, exact overlapping Rust counts, ignored cases and real Python
coverage; it refuses skipped Python methods or changed source. Custom output
paths use `GRIMOIRE_BINARY`, `CARGO_TARGET_DIR` and `WRITINGDECK_TOOLS_PYTHON`.
Current software totals and their source identities are recorded in
[STATUS](STATUS.md); historical milestone counts do not qualify later changes. The independent image audit verifies Wi-Fi payloads.
The 512-MiB zero-NIC TCG VM records successful native/proselint/WordNet/Harper
checks and a real Nabokov kernel CPU-budget denial; positive trained analysis
is required and passes in native offline UI/PTY tests. See [STATUS](STATUS.md)
for source identity, measured costs and remaining release gates.

## Evidence levels

- **Unit/integration:** deterministic temporary files, fake clocks/runners and Ratatui TestBackend.
- **Simulated services:** explicitly labeled state; no hardware or cloud operation.
- **VM:** complete UEFI disk boot under QEMU/OVMF, actual Linux console/systemd/polkit, no network device.
- **Hardware verified:** requires an identified physical laptop and recorded firmware/keyboard/power results. **None yet.**
- **Credential/provider verified:** requires explicit enrollment and real restore/round-trip tests. **None yet.**

[STATUS](STATUS.md) and `docs/evidence/` record latest completed runs. A source-level test or successful VM poweroff is not a physical battery/suspend/Secure Boot result.

## Commands

Run as a normal user except staging/constructing the disposable rootfs/image:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked --offline -j 2
cargo test --workspace --no-default-features --locked --offline -j 2
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
python3 -m unittest discover -s os/tests -p 'test_*.py' -v
shellcheck os/scripts/*.sh os/scripts/writingdeck-session
python3 tools/check-writingdeck.py
```

First fetch/build needs installed dependencies/cached crates; `--offline` documents that running the cached tests itself needs no network. The default audio build needs ALSA headers. Build `cargo build --locked --offline --no-default-features -j 2` before Python tests to exercise the actual headless CLI; CLI and PTY tests explicitly skip if no binary exists. Upstream tests requiring particular external engines/export environments remain ignored unless those prerequisites are installed and explicitly exercised; report those, do not count them as passed.

Image/boot commands are in [INSTALLATION](INSTALLATION.md). The VM test is one-shot, isolated, networkless and runs on a disposable overlay. Its boot probe is installed only in the explicit VM-demo profile and powers that VM off. Do not run a real host suspend/shutdown or format a device to exercise an interface.

## Current coverage

- Existing Markdown/YAML, note counts, native export, history, guarded mobile save, sync/unavailable/deleted/conflict behavior, recovery, atomic rename/backoff and UI/help/menu/key invariants.
- Schema-v3 dual-editor positions, directed selections, free scroll, active role/visibility/ratio, per-workspace card IDs; v1/v2 read-only migration; invalid/traversal/unknown/corrupt/future/conflicting state preservation.
- Tutorial catalog, unavailable rejection, replay/skip, actual demo edit completion and persisted progress; existing directories refused.
- Four required TUI sizes: 80×24, 100×30, 120×40, 160×50; dashboard scroll/hints, outline, strict focus with a retained reference, both editors' cursor/selection/scroll/size restoration and additive logical aliases.
- Background system status and explicit simulator label; missing hardware/services; bounded typed command construction; preview timer model.
- Real non-root compiled-binary PTY at all four sizes: F11 escape input, bracketed paste, separate disk saves, isolated undo/redo, hide/show, normal quit and schema-v3 role/path assertions. Isolated HOME/new demo fixtures only; the shim answers the initial cursor query, not a physical keyboard certification.
- 28 dedicated secondary UI cases: aliases changing externally, replacement refusal on failure, concurrent external text, clean/dirty/ambiguous rename, read-only UTF-8 refusal, hidden-buffer autosave, dual crash recovery, bulk replace/undo, trash/conflict closure and lesson 13.
- M4a pins: 14 core model/store/anchor/containment tests, 31 UI tests and one optional-alias case, plus two actual-binary PTY tests (four-size controls/restart and corrupt-metadata refusal). No render-time file I/O, copied body, implicit writer or fake tutorial completion. Category paste/cut/undo/drag safety and ambiguous retained source ownership are covered. Details: [PINS](PINS.md).
- M4b journal: 10 core + 23 UI + one alias case, plus actual compiled-app four-size source/save/restart and future-log/explicit-force PTYs. Two small ANSI tracker tests reconstruct differential screen runs; no complete terminal or physical-keyboard emulation claim. Cold-start unknown totals and later repair retain unavailable baselines instead of inventing net metrics. See [JOURNAL](JOURNAL.md).
- Actual headless CLI: simulated status label, help/version/errors, exclusive demo creation, read-only layout/tutorial/pin/known-date journal metadata validation and unknown/future-schema preservation.
- Rootless helper tests: timer-before-load, cancellation/expiry, valid/invalid/reused tokens, unresolved-preview refusal, committed-before-crash behavior, bad config/maps and service failure.
- Read-only installer planner: clean candidate is not apply permission, mounted/read-only/small/unknown targets rejected, invalid account/timezone/layout, no `--apply` mode.
- Whole-disk UEFI VM: actual tty1 dashboard, unprivileged Grimoire process, active-local polkit eligibility, physical-key-code mapping via dumpkeys, independent timed revert, durable commit + reapplication. Actual screenshot and serial markers are retained.

## Missing acceptance tests and release gates

Clean **physical** install/LUKS/authentication/recovery; full modifier/ANSI/ISO/custom-layout matrices; real Wi-Fi/brightness/charging/discharge; lock-before-suspend/resume; update interruption and rollback; full-kernel power-cut testing of both editors beyond fixture rescue; large-note/card/log CPU/input/flash-write budgets; long-load/power-cut accounting and civil-date qualification; independent human editorial/rights qualification of the original corpus; deeper cross-engine Nabokov/proselint Unicode/dialogue source maps (M7a Harper/common mapping now tested); WordNet senses; continuity/annotation/variant safety; genuine DOCX/PDF page-footnotes; verified off-device restore and sync conflicts; Google Docs round trip; all 39 original + supplemental 40 complete tutorial exercises; Python/spaCy/battery resource benchmarks.

Safely simulate power-loss/save failures with injected rename/transport/filesystem behavior or disposable VM images. Never corrupt the user's only manuscript. For a crash process test use a new fixture book, verify recovery copies, then explicitly restore into a separate fixture. VM timer/commit reapplication is not yet a full reboot-persistence/cold-resume hardware test.

## Performance interpretation

The current VM records process RSS from `/proc/<pid>/status` for a stripped debug/no-audio build on 512 MiB virtual RAM. QEMU wall time includes emulation, firmware, deliberate timer waiting and shutdown; it is not a laptop boot benchmark. Record future release builds, input latency, idle CPU and whole-system memory separately. The optional Python analyzers/models are not running in this measurement.

## M3b evidence commands

`cargo test -p grimoire-tui writingdeck_secondary --locked --offline` selects the 28 new behavior cases. `python3 -m unittest discover -s os/tests -p 'test_secondary_pty.py' -v` selects the actual application/PTY test, containing four terminal-size subcases. The complete Python discovery includes it; root or missing-binary skips must be reported, not counted as passed. Current full-suite totals and source identity are in [test-results.json](evidence/test-results.json). Run `python3 tools/check-writingdeck.py --require-current-evidence` after refreshing both software and VM records to reject stale-source evidence.


## M4c workflow/preset acceptance

Run `cargo test --no-default-features writingdeck_workflow --locked --offline -j2` for 24 dedicated workflow UI cases and `PYTHONPATH=os/tests python3 -m unittest test_workflow_pty test_cli -v` for the compiled workflow/CLI methods. Full discovery includes those; skips are not passes. M4c adds 45 Rust cases (16 workflow core, three workspace core, one pin-preview core, 24 UI, one alias), three actual compiled-app PTY methods and one read-only CLI method beyond M4b. [WORKFLOW](WORKFLOW.md) details the source/receipt/size/role/recovery contracts.

The actual four-size PTY flow saves and links an original scratch note, promotes once, verifies source/linked-manuscript bytes and metadata-only receipts, arms ordinary handoff prose, quits, launches a **separate process**, reads the saved intention, resumes actual manuscript context, edits and independently undoes the loaded original intention. Another four-size flow applies Research, edits its real Secondary, saves an empty workspace pin set and refuses a starter after external future-layout replacement. A future-workflow case preserves all index bytes while still allowing explicit original-handoff editing, with no false arm.

Prepared creation/retry and exact-target completion-write-failure tests synthesize durable phase/I/O conditions and restore known metadata before retry. They verify retained receipt/original/snapshot bytes, not real kernel power cuts, locks or cross-file atomicity. Rendering remains file/command-free. The separate VM boot probe does not perform these workflow interactions. Curated real 80×24 terminal frames are in [workflow-pty-screen.txt](evidence/workflow-pty-screen.txt); the small cursor/erase/UTF-8 tracker is not a full terminal emulator.

To recapture the curated demo-only terminal frames, first build the no-audio binary, then run `python3 tools/capture-writingdeck-screens.py` as a normal user. It uses new temporary projects/HOME, performs real saves/promotion/process restart and replaces only the two curated evidence text files; no user novel or host console mapping is opened. Review the resulting frames before recording their hashes.

## Initial M5 verification

The actual source-range/classification Editor/ledger tests use injected monotonic/civil clocks, IANA DST/travel/reset cases, actual paste/replay/batch receipts, unavailable word/time gaps, multipart partial-failure retention and deduplicated retry. Goal/calendar/sprint/challenge/collection domain tests supplement **30 real controller/UI tests**, including saved author status references and tutorial 18/20/21 completion/noncompletion. Existing regression assertions are retained; the new short nested Productivity menu is included in all menu/help/palette coverage.

`os/tests/test_productivity_pty.py` executes the compiled non-root app at four sizes, with actual keyboard and bracketed-paste origins, undo/redo, calendar/target creation, measured sprint/challenge completion, normal quit, separate-process loaded-source editing/undo and future-ledger refusal/retry. `test_cli.py` additionally audits M5 metadata without any file changes. Capture optional current terminal frames with `WRITINGDECK_CAPTURE_PTY=1 python3 -m unittest discover -s os/tests -p test_productivity_pty.py -v`.

The opt-in release-mode synthetic probe is `cargo run -p grimoire-core --no-default-features --example accounting_probe --release --locked --offline -- build/NEW-PROBE`. It refuses an existing target and removes only its newly created fixture. It measures source-range calls, actual Editor insertions and 1,024 synthetic ledger groups, not human typing, full UI latency, battery or physical flash endurance. See [accounting-benchmark.json](evidence/accounting-benchmark.json) and [DAILY_CHALLENGES](DAILY_CHALLENGES.md). Checkpoint crash tails, long-history archival, full loaded-card/log/analyzer and power-cut qualification remain explicit gates.

## M6 content and actual Spark integration

`python3 tools/build-spark-corpus.py --check` verifies immutable-ID derived artifacts and exact source/text hashes; it does not certify semantics or legal rights. `cargo test -p grimoire-core spark:: --locked --offline -j2` has 28 dedicated cases; three workflow adoption cases retain exact first-fingerprint/ID/foreign-state guards. `cargo test -p grimoire-tui --no-default-features spark_ui_tests --locked --offline -j2` has 28 actual controller/UI cases. `PYTHONPATH=os/tests python3 -m unittest test_spark_corpus test_spark_pty -v` adds six artifact/provenance contracts and four compiled non-root PTY methods, including four-size original editing/undo after actual process restart, fixed daily/favorite recall, ignored pasted commands, corrupt/future state, prepared source retry, initially empty independent scene and genuine active-sprint checkpoint refusal/normal quit/restart. Completed-frame captures wait for the exact newly selected IDs and final frame output, not just a file receipt or stale differential title.

The complete verifier runs both overlapping Rust configurations, both warning-denying all-target Clippys, all Python methods without skipped PTYs, corpus drift, inventories, shell/syntax/docs/reference/diff gates. Fresh VM/source fingerprints and physical/release gates remain separate. Agent review/duplicate checks are not independent human/legal qualification. Future gates still include M7–M10 and the production M2b installer/lock/update/hardware matrix.

## M7a common diagnostics / real optional Harper

[Writing checks](WRITING_CHECKS.md) separates core controlled offset/fault/queue fixtures from actual pinned engine runs; 15 targeted UI cases include a genuine worker at all four sizes and genuine demo grammar replacement, plus location-bound consent/revocation/idle cleanup. Five `test_checks_pty.py` methods use the actual no-audio binary, disposable demo/HOME and real models: Unicode apply/undo/restart, once/novel/personal mutes, copied-novel consent, future/external refusal, edits during an in-flight check and supplementary lesson-40 proof retaining original 26. Ledger assertions wait for durable origin checkpoints, not merely source writes; analysis/debounce does not grant authorship credit.

`tools/audit-writing-checks-assets.py` is a full-verifier gate for nine pinned packaged assets and seven retained upstream notices. Independent training/dictionary/legal/corresponding-source review remains open. [Debug resource measurements](evidence/writing-checks-resources.json) distinguish small Unicode vs specified repetitive 2,340-word fixture, cold/hot/CPU and baseline/kernel-peak RSS. Native cancel does not preempt a running parser or prove cache unloading. No physical battery/production Python claim. The VM-only probe now also requires a non-root actual model/apply/undo result under the installed stripped binary with zero NICs; it records loaded grammar separately from dashboard RSS. See [STATUS](STATUS.md) for whether full software and VM qualification have actually run on the current source.

## Writing workflow increment (2026-10-29)

[The independent Debian receipt](evidence/development-writing-local.json)
records 1037 Rust tests in each overlapping configuration, both Clippys,
173 Python tests with zero skips and 21 gates. Fresh demo/HOME fixtures exercise
annotation creation, normal original-source editing, explicit re-anchor,
revision-bound resolution, retained history, Undo and process restart at four
terminal sizes. Footnote insertion and reference/definition navigation retain
exact YAML/source bytes through Undo; modal/help input cannot mutate hidden prose.
Model/UI tests cover unknown, corrupt, future, oversized, stale and aliased
metadata and interrupted owned moves. The [initial documentation failure](evidence/development-writing-documentation-failure.json)
is preserved separately. These tests do not establish crash/power-loss,
large-load, image, full encrypted installation, physical or release qualification.

## Contextual footnote reader (2026-10-29)

The [independent Debian receipt](evidence/footnote-context-local.json) records
1044 Rust tests per configuration, 174 Python tests with zero skips, both Clippys and 21 gates. Fresh demo/HOME fixtures exercise the
docked reader, private-note masking, repeated-reference selection, original-source
editing, Undo and restart at four terminal sizes. The initial local run failed an
installer prompt synchronization test; its [partial receipt](evidence/footnote-context-local-initial-failure.json)
is retained and the corrected complete suite passed. These results do not qualify
page-bottom export, a full encrypted install, physical hardware or release.

## Export selection review (2026-10-29)

The [independent Debian receipt](evidence/export-preflight-local.json) records
1047 Rust tests per configuration, 175 Python tests with zero skips, both Clippys and 21 gates. The compiled-app workflow uses fresh
demo/HOME fixtures at four terminal sizes and exercises review, help, modal
input, original-source save and Undo. Scope tests cover selected parts, samples,
editions, private material, the rich-footnote guard and bounded refusal. This
is software evidence, not rendered-footnote, installer or hardware qualification.

## Reviewed Argon writing integration (2026-10-30)

The [independent Debian receipt](evidence/argon-writing-local.json) records
1057 Rust tests per configuration, 177 Python tests with zero skips, both Clippys and 21 gates. Fixture UI/PTY checks cover durable identity after
inserted tree nodes, milestones, scene variants, comparison, source/metadata
refusal, restore/Undo, continuity facts/events/threads, help and actual restart
at four terminal sizes. Silent-engine tests verify chapter scope and guarded
revision marks; they do not qualify installed audio hardware. CLI tests retain
old aliases and compare local source bytes without transmission. The
[contribution review](ARGON_REVIEW_2026-10-30.md) explains excluded prototypes
and the remaining full milestone, provider, system and release work.
