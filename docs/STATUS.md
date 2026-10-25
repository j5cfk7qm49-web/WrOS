# Status — WritingDeck OS 0.1.0-dev, through M4a pins

**As of 2026-10-24: tested appliance foundation, independent editors and single-source pins. The full requested appliance is not complete or safe for production installation.** No physical laptop, cloud account or user manuscript was used.

## Completed software evidence

- Full offline Rust suites: **578 passed, 5 ignored** with default audio features; **578 passed, 4 ignored** without audio. These suites overlap, not 1,156 distinct tests. Core 262, TUI 261, system 11 and retained app/integration tests pass. Formatting and warning-denying workspace/all-target Clippy pass.
- **46 new Rust tests beyond M3b:** 31 pin UI cases, 14 portable pin model/store/anchor cases and one optional-shortcut case. The prior 42 M3b additions remain enabled. Required layouts: 80×24, 100×30, 120×40 and 160×50, plus tiny-viewport fallback.
- **25 Python tests passed, zero skipped:** 13 rootless keyboard-helper, four read-only installer-plan, five actual headless CLI and three **unprivileged compiled-app PTY** tests. The retained Secondary test exercises F11, paste, separate save/undo/redo, hide/show and roles. New pin coverage exercises real palette/source selection, heading/PgDown/Home, pasted-command suppression, source-only saves/YAML, duplicate-card scroll, archive/Trash/restore, quit and restart loaded-state proof at all four sizes; a separate corrupt-metadata scenario refuses overwrite. HOME/projects are new fixtures. The startup cursor-query shim is not physical-keyboard or full terminal-emulator certification.
- Shellcheck, Python syntax, docs/manifests/matrix parity, original tutorial/dictionary hashes, self-contained printable reference and diff checks pass. Ignored screenshot/performance/real-Piper-model tests are not passed.

Machine-readable results and source identity: [test-results.json](evidence/test-results.json). Logs under ignored `build/` are transient; small evidence under `docs/evidence/` is retained. The strict evidence checker rejects stale-source software/VM records.

## VM evidence

A freshly assembled **regular-file UEFI image actually booted and passed** on current M4a source. Probe wall time was **60.64 seconds**, including scripted waits; app UID **1000**, ordinary dashboard RSS **20,392 KiB**, all five acceptance markers present. This is not a laptop boot-speed or loaded-pin memory result. The readable **160×50** dashboard screenshot was inspected; the new pin widget/footer is present, but no pin interaction or card stress is shown.

[VM record and source/image/binary hashes](evidence/vm-boot.json) · [serial log](evidence/vm-serial.log) · [dashboard screenshot](evidence/vm-dashboard.png). Source fingerprint: `9256f2b1a75f29de7aee18f02a31440c80ea82c0787b1f8848a018eb04e758e8` (189 files).

Before rebuilding, the strict checker was deliberately run with the current software record and historical M3b VM record: it **failed for stale VM source** as required. After the fresh boot, strict software/VM/current-source verification passed. Historical foundation/M3b records remain in their commits rather than being relabeled as pin evidence.

The VM profile is QEMU TCG/q35, OVMF UEFI with Secure Boot disabled, 512 MiB, two vCPUs, no NIC. Its probe checks actual tty1 dashboard/non-root UID, active-local narrow polkit eligibility, Colemak-DH physical E → logical F, independent systemd timeout rollback, committed configuration and boot-command reapplication. It does **not** send F11 or pin controls; those have separate Rust UI and compiled-app PTY coverage. It is not physical boot-speed, battery, pin-stress, full cold-reboot or suspend qualification.

## Implemented and tested scope

- Preserved upstream Grimoire v0.7.5 base `f3ca1e8f1cb09b714e71046e8ed991051793d33a`, ordinary Markdown/raw YAML, autosave/history/conflict/recovery and existing shortcuts.
- Debian 13.7 amd64/kernel `6.12.113+deb13-amd64` staged VM recipe; non-root console start, no custom kernel/heavy desktop/physical-disk manipulation.
- Typed Linux/simulated services, bounded fixed commands and cached off-thread system state. Four registered QWERTY/Colemak/DH ANSI/ISO console maps, narrow preview/commit helper and independent rollback.
- Configurable existing-data F12 home, F10 outline, strict Ctrl-D focus; additive aliases for 12 actions, retaining established defaults.
- **F11 independent editor:** separate complete Editor contexts/cursor/selection/free scroll/undo, canonical/dev-inode ownership, shared original guarded Project saves and both-buffer recovery. Wide split, narrow stacked and tiny/strict hidden-view behavior remain tested.
- **Schema-v2 dual workspaces:** both document/position/selection/scroll states, active editor, visibility/ratio, outline/custom slots, read-only v1 migration, newer Primary resume precedence and unknown/corrupt/future/conflicting-state preservation.
- **M4a source-linked pins:** Pinboard, responsive sidebar and dashboard-scoped cached readers; no copied body becomes another writer. Independent card/list scroll, unique heading/quote anchors with explicit missing/ambiguous states, scopes tied to Primary manuscript context, category/color/order/collapse/size and recoverable card archive/Trash. Explicit source edits reuse ownership, guarded saves and recovery; ambiguous retained editors are not guessed. Failed pin/relink operations stay in the manager rather than unexpectedly returning typing to the manuscript. Read-only CLI metadata validation includes pins. See [PINS](PINS.md) for exact controls, containment checks and limitations.
- Original 39-lesson catalog: **18 event-capable**, including successful demo pin creation and actual anchored scroll-and-return; **21 unavailable** lessons remain disabled. Real-novel exercises are blocked. Saved replay/skip progress and searchable offline help remain available.
- Read-only installation planner with **no apply mode**; all 17 required docs plus pin documentation, inventories, retained notices and printable logical shortcuts. Inventory is not legal distribution clearance; 31 all-platform notice gaps remain, none selected by this Linux no-audio binary.

## Critical outstanding gates

Production installer/LUKS/first-run/authentication/lock-before-suspend; custom layout registration/onboarding and remaining sound/clock/timezone controls; signed boot/update/recovery/rollback and authenticated maintenance; full workspace content presets and per-workspace pin sets; journal automatic facts versus reflections, scratchpad promotion and handoff; detailed gross/net/deleted/time/footnote accounting; November calendar/advanced goals/challenges/achievements; actual 1,000-option licensed Spark corpus; Harper/Nabokov/proselint/WordNet; continuity/annotations/variants; genuine bottom-page DOCX/PDF footnotes; verified off-device/Drive restore/mobile/Docs workflows; all 39 complete exercises; physical keyboard/Wi-Fi/suspend/battery, kernel power-cut and large-note/card/analyzer performance qualification. Wi-Fi scan truncation/enrollment is not fixed or qualified by M4a.

The document registry/metadata guards are **not an OS lock, cross-process CAS or hostile-filesystem-race defense**. Card metadata saves are currently synchronous; input/idle CPU and flash-write budgets need benchmarks. Local saving/history/pins are not backup.

**VM-only security warning:** tty1 autologin is passwordless, passwords locked, bootloader unsigned and acceptance probe powers the guest off. Never install this demo onto a real laptop or treat it as recovery-capable production software.

## Next dependency order

1. Preserve M4a as a tested local milestone; no push or physical installation is needed.
2. Continue M4 with a portable journal facts/reflections model and actual UI. Report only existing net/session facts until a real edit/idle ledger exists. Then add scratchpad promotion and handoff with original-preserving operations, tutorial events and failure/restart tests.
3. Benchmark large notes/card counts and metadata persistence before low-power/performance claims; extend presets and pin organization without introducing a second source text store.
4. Continue corpus/productivity/analyzers/long-form/export/backup and authenticated setup/lock/update/recovery in dependency order. Hardware and credential acceptance need separate explicit authorization when a safe testable workflow exists.

See [feature matrix](FEATURE_MATRIX.md), [pins](PINS.md), [workspaces](WORKSPACES.md), [roadmap](ROADMAP.md), [testing](TESTING.md) and [risks](RISKS.md). No Hardware verified claim exists.
