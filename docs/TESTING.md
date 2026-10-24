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

First fetch/build needs installed dependencies/cached crates; `--offline` documents that running the cached tests itself needs no network. The default audio build needs ALSA headers. Build `cargo build --locked --offline --no-default-features -j 2` before Python tests to exercise the actual headless CLI; those four tests explicitly skip if no binary exists. Upstream tests requiring particular external engines/export environments remain ignored unless those prerequisites are installed and explicitly exercised; report those, do not count them as passed.

Image/boot commands are in [INSTALLATION](INSTALLATION.md). The VM test is one-shot, isolated, networkless and runs on a disposable overlay. Its boot probe is installed only in the explicit VM-demo profile and powers that VM off. Do not run a real host suspend/shutdown or format a device to exercise an interface.

## Current coverage

- Existing Markdown/YAML, note counts, native export, history, guarded mobile save, sync/unavailable/deleted/conflict behavior, recovery, atomic rename/backoff and UI/help/menu/key invariants.
- Versioned workspace positions/widget order, invalid/traversal/corrupt/future/conflicting state preservation.
- Tutorial catalog, unavailable rejection, replay/skip, actual demo edit completion and persisted progress; existing directories refused.
- Four required TUI sizes: 80×24, 100×30, 120×40, 160×50; dashboard scroll/hints, outline, strict focus with a retained reference, primary cursor/width restoration and additive logical aliases.
- Background system status and explicit simulator label; missing hardware/services; bounded typed command construction; preview timer model.
- Actual headless CLI: simulated status label, help/version/errors, exclusive demo creation, read-only config validation and future-schema preservation.
- Rootless helper tests: timer-before-load, cancellation/expiry, valid/invalid/reused tokens, unresolved-preview refusal, committed-before-crash behavior, bad config/maps and service failure.
- Read-only installer planner: clean candidate is not apply permission, mounted/read-only/small/unknown targets rejected, invalid account/timezone/layout, no `--apply` mode.
- Whole-disk UEFI VM: actual tty1 dashboard, unprivileged Grimoire process, active-local polkit eligibility, physical-key-code mapping via dumpkeys, independent timed revert, durable commit + reapplication. Actual screenshot and serial markers are retained.

## Missing acceptance tests and release gates

Clean **physical** install/LUKS/authentication/recovery; full modifier/ANSI/ISO/custom-layout matrices; real Wi-Fi/brightness/charging/discharge; lock-before-suspend/resume; update interruption and rollback; second editable buffer conflict/crash/selection/undo; pin semantic scroll restoration; idle/gross/net/revision/footnote metrics; civil-date/DST challenges; actual 1,000-record corpus and rerolls; Harper/Nabokov/proselint Unicode/dialogue source maps; WordNet senses; continuity/annotation/variant safety; genuine DOCX/PDF page-footnotes; verified off-device restore and sync conflicts; Google Docs round trip; all 39 complete tutorial exercises; Python/spaCy/battery resource benchmarks.

Safely simulate power-loss/save failures with injected rename/transport/filesystem behavior or disposable VM images. Never corrupt the user's only manuscript. For a crash process test use a new fixture book, verify recovery copies, then explicitly restore into a separate fixture. VM timer/commit reapplication is not yet a full reboot-persistence/cold-resume hardware test.

## Performance interpretation

The current VM records process RSS from `/proc/<pid>/status` for a stripped debug/no-audio build on 512 MiB virtual RAM. QEMU wall time includes emulation, firmware, deliberate timer waiting and shutdown; it is not a laptop boot benchmark. Record future release builds, input latency, idle CPU and whole-system memory separately. The optional Python analyzers/models are not running in this measurement.
