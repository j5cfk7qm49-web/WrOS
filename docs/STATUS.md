# Status — WritingDeck OS 0.1.0-dev, through M4b journal

**As of 2026-10-24: tested foundation, independent editors, single-source pins and an initial journal. The full requested appliance is not complete or safe for production installation.** No physical laptop, cloud account or user manuscript was used.

## Completed software evidence

- Full offline Rust suites: **612 passed, 5 ignored** with default audio; **612 passed, 4 ignored** without audio. The configurations overlap, not 1,224 distinct tests. Core 272, TUI 284, system 12 and retained app/integration tests pass. Formatting and warning-denying workspace/all-target Clippy pass.
- **34 new Rust tests beyond M4a:** 10 journal core, 23 journal UI and one optional-alias case. Prior M4a's 46 and M3b's 42 additions remain enabled. Required terminals: **80×24, 100×30, 120×40, 160×50**; existing tiny/strict fallback coverage remains enabled.
- **30 Python tests passed, zero skipped:** 13 rootless keyboard-helper, four read-only installer-plan, six actual headless CLI, five **unprivileged compiled-app PTY** and two small ANSI screen-tracker tests. The new journal PTYs exercise real saved observations, explicit original reflection editing, bracketed-paste command suppression, isolated source undo/redo, normal quit, actual loaded-source restart at all four sizes, and future-log/unavailable-total preservation, first-quit refusal and explicit forced exit (including a source save inside the reader). Retained independent-editor and pin PTYs still run. Fixtures use new projects/HOME only. Screen reconstruction follows cursor/erase/UTF-8 runs, not raw differential title assumptions; it is not a complete emulator or physical-keyboard certificate.
- Shellcheck, Python syntax, docs/matrix/manifests, original tutorial/dictionary hashes, self-contained printable logical-key reference and diff checks pass. Ignored screenshot/performance/real-Piper-model tests are **not** passed.

[Machine-readable software evidence](evidence/test-results.json) · [actual compiled-app journal screens](evidence/journal-pty-screen.txt). Device-local dates/offsets in these fixtures are not assumed to be the user's timezone. Transient logs are under ignored `build/`; curated small evidence is retained under `docs/evidence/`.

## VM evidence

**A fresh regular-file M4b image actually booted and passed all five markers.** Probe wall time was **61.09 s including waits**, app UID **1000**, observed dashboard RSS **20,968 KiB**. The actual **1280×800 / 160×50** screenshot was inspected: non-root dashboard with the separate Writing journal widget. No physical disk was touched.

[Boot record](evidence/vm-boot.json) · [serial log](evidence/vm-serial.log) · [actual dashboard](evidence/vm-dashboard.png). Source fingerprint **`68e1bc3e56dfbdc20dc656d3312c007fdbfd55fc80984d7bc58a4dfb3e798645`**, **195 files**, matches both software and fresh VM evidence. Stale-source VM evidence was deliberately rejected before rebuilding, and the strict current-evidence checker now passes. This is source alignment, not signed attestation or hardware clearance.

The independent VM profile is QEMU TCG/q35, OVMF UEFI with Secure Boot disabled, 512 MiB, two vCPUs, no NIC. The probe checks actual tty1 dashboard/non-root UID, active-local narrow polkit eligibility, real Colemak-DH physical E → logical F, independent systemd timeout rollback, committed keyboard configuration and boot-command reapplication. It does **not** send F11, pin or journal controls. Those have distinct Rust UI and compiled-app PTY evidence. Probe wall time/ordinary dashboard RSS are not physical boot-speed, loaded-journal/card stress, battery, full cold-reboot or suspend qualification.

## Implemented and tested scope

- Preserved Grimoire v0.7.5 base `f3ca1e8f1cb09b714e71046e8ed991051793d33a`, ordinary Markdown/raw YAML, autosave/history/conflict/recovery and established shortcut defaults.
- Debian 13.7 amd64/kernel `6.12.113+deb13-amd64` staged regular-file VM recipe; non-root console app, no custom kernel/heavy desktop/physical-disk changes. Typed Linux/simulated services, bounded commands, cached off-thread system state and four QWERTY/Colemak/DH console maps with narrow preview/commit/independent rollback remain tested.
- Configurable existing-data F12 home, F10 outline, Ctrl-D strict focus and **13** additive alias actions, retaining F10/F11/F12 defaults. Pins and Writing journal widgets migrate additively without replacing saved widget visibility/order/workspaces.
- **M3b independent editors/workspaces:** distinct complete contexts and undo/selection/scroll, canonical/dev-inode ownership, guarded shared original Project saves/recovery, wide split/narrow stack/strict-hidden safeguards; schema-v2 paths/positions/visibility/ratio, v1 read-only migration, resume precedence and unknown/corrupt/future/conflicting-state preservation remain tested.
- **M4a single-source pins:** Pinboard/sidebar/dashboard cached readers, independent free/heading/quote scroll, scopes/category/color/order/size/collapse, recoverable card archive/Trash and explicit owner-aware source editing. No copied body becomes another writer. Details: [PINS](PINS.md).
- **M4b initial journal:** read-only per-date sampled clean saved-Project observations, optional known initial/daily net values, signed delta and auditable timestamps/offsets/request boundaries; separate ordinary original Markdown reflections through the existing editors. No gross/deleted/time/exclusive-typing facts are invented. Guarded append/reload, pending failures, future/unknown/conflict/I/O preservation, date/source/role containment, paste/hidden-editor guards, no draw I/O and no phantom view/note-only writing events are tested. Known incomplete totals inhibit observations; after repair an unknown initial baseline stays unavailable. Details and limits: [JOURNAL](JOURNAL.md).
- Normal quit now also rejects a still-dirty Waiting source state and an unrecorded journal observation; the existing explicit second Ctrl-Q can force leaving after a warning, losing pending observations as documented. Manuscript saves themselves remain independent of journal-log failure.
- Original **39** lessons: **19 event-capable / 20 unavailable**. New lesson 14 requires recorded demo facts reviewed and a genuinely changed original reflection saved, not a Next button or opening/navigation. An ordinary novel cannot launch writing exercises. Safe shipped Pin Practice, replay/skip progress and searchable offline help remain available.
- Read-only installer planner with **no apply mode**; 17 required docs plus pin/journal documentation, 68 matching feature rows, inventories/notices and printable keys. CLI validation includes known-date journal records without rewriting or interpreting unknown filenames. **31 all-platform notice gaps remain; none selected by this Linux no-audio binary.** Inventory is not legal distribution clearance.

## Critical outstanding gates

Production installer/LUKS/first-run/authentication/lock-before-suspend; custom layout registration/onboarding and remaining sound/clock/timezone controls; signed boot/update/recovery/rollback and authenticated maintenance; full workspace content presets/per-workspace pin sets; multi-note scratchpad/source-preserving promotion and next-session handoff; real gross/net/deleted/active/idle/drafting/revision/footnote edit-ledger accounting and richer journal analyses; November calendar/advanced goals/challenges/achievements; actual 1,000-option licensed Spark corpus; Harper/Nabokov/proselint/WordNet; continuity/annotations/variants; genuine bottom-page DOCX/PDF footnotes; verified off-device/Drive restore/mobile/Docs workflows; all 39 complete exercises; physical keyboard/Wi-Fi/suspend/battery, kernel power-cut and large-note/card/log/analyzer performance qualification. Wi-Fi scan truncation/enrollment is not fixed or qualified by M4b.

Metadata/ownership guards are **not OS locks, cross-process CAS, hostile-filesystem-race defense, durable per-edit ledgers or backup**. Journal checkpoints are sampled and pending observations are memory-only; crash/explicit forced exit can lose them. Large-document/log latency, idle/input CPU and flash-write budgets remain unqualified.

**VM-only security warning:** passwordless tty1 autologin, locked passwords, unsigned bootloader and a probe that powers off the guest. Never install this demo onto a real laptop or treat it as recovery-capable production software.

## Next dependency order

1. Preserve M4b as a verified local milestone, with no push or physical installation.
2. Continue M4 with a portable multi-note scratchpad and original-preserving promotion through the same ownership/save/recovery path, then next-session handoff; add real UI/tutorial/failure/restart tests.
3. Build actual edit/idle instrumentation before richer journal/productivity/time/gross metrics. Extend presets and benchmark large notes/card/logs without creating a second source text store.
4. Continue corpus/analyzers/long-form/export/backup and authenticated setup/lock/update/recovery in dependency order. Hardware/credential acceptance needs separate authorization once a safe testable workflow exists.

See [feature matrix](FEATURE_MATRIX.md), [journal](JOURNAL.md), [pins](PINS.md), [workspaces](WORKSPACES.md), [roadmap](ROADMAP.md), [testing](TESTING.md) and [risks](RISKS.md). No Hardware verified claim exists.
