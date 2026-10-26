# Testing and verification

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

Clean **physical** install/LUKS/authentication/recovery; full modifier/ANSI/ISO/custom-layout matrices; real Wi-Fi/brightness/charging/discharge; lock-before-suspend/resume; update interruption and rollback; full-kernel power-cut testing of both editors beyond fixture rescue; large-note/card/log CPU/input/flash-write budgets; idle/gross/net/revision/footnote metrics; civil-date/DST challenges; actual 1,000-record corpus and rerolls; Harper/Nabokov/proselint Unicode/dialogue source maps; WordNet senses; continuity/annotation/variant safety; genuine DOCX/PDF page-footnotes; verified off-device restore and sync conflicts; Google Docs round trip; all 39 complete tutorial exercises; Python/spaCy/battery resource benchmarks.

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
