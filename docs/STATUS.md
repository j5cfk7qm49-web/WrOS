# Status — WritingDeck OS 0.1.0-dev

**As of 2026-10-24: functional, tested appliance foundation. The full requested appliance is not complete or safe for production installation.** All subsequent claims are scoped by the named evidence. No physical machine, Google account or user manuscript has been used for qualification.

## Latest completed evidence

- Fresh full offline Rust suites: **490 passed, 5 ignored** with default audio features; **490 passed, 4 ignored** without audio. Includes the 12 WritingDeck UI tests at 80×24, 100×30, 120×40 and 160×50. These are overlapping suites, not 980 distinct tests. Formatting, warning-denying Clippy and shellcheck pass.
- Fresh Python run: **21 passed** (13 rootless keyboard-helper, four read-only planner, four actual headless CLI tests). Docs/manifests/offline printable-reference checker passes. CLI tests use new temporary fixture projects; no user project is opened.
- Actual whole-disk boot: [vm-boot.json](evidence/vm-boot.json), [serial assertions](evidence/vm-serial.log), [console screenshot](evidence/vm-dashboard.png). QEMU TCG/q35, OVMF UEFI without Secure Boot, 512 MiB, 2 vCPUs, **no NIC**. Real tty1 application UID 1000, RSS **20,108 KiB**, wall **69.62 s** including timer and shutdown. Rebuilt against the current foundation source, with image/binary/source hashes in the VM record; the screenshot confirms the initial console-clear fix.
- Guest verified actual dashboard, active-local polkit authorization, physical E key mapping to F under Colemak-DH, independent timeout rollback, committed config and boot-command reapplication. This is not physical hardware or a full cold-reboot/resume guarantee.
- All four registered keymaps generated from maintained Debian XKB and validated as Unicode binary maps without changing the host layout.

Machine-readable software results: [test-results.json](evidence/test-results.json). This file is updated only after new commands actually complete. Build logs under ignored `build/` are transient; small verification records under `docs/evidence/` are retained.

## Implemented scope

- Preserved Grimoire v0.7.5 base `f3ca1e8f1cb09b714e71046e8ed991051793d33a` and its Markdown/YAML/autosave/history/conflict/recovery conventions.
- Debian 13.7 amd64/kernel `6.12.113+deb13-amd64` staged VM image, direct non-root console start; no custom kernel, heavy DE or physical disk manipulation.
- Typed Linux/simulated system interfaces, bounded fixed commands and off-thread/cached UI state; no app as root.
- QWERTY, Colemak, Colemak-DH ANSI/ISO maps, helper-only privileged preview/commit and independent systemd rollback.
- F12 dashboard with configurable existing-data widgets; F10 outline, strict Ctrl-D focus; persisted primary positions/widths/five named slots/custom clones; additive safe shortcut aliases.
- Original 39-lesson catalog, **15 event-capable** baseline/foundation lessons, unavailable lessons explicitly disabled; exclusive safe demo, saved progress, replay/skip and offline help.
- Read-only disk/account/timezone/encryption/layout planner with **no apply mode**.
- Required documentation entry points, feature matrix, source/license manifests and self-contained printable reference. Machine inventory is not legal release approval.

## Critical outstanding gates

Production installer/LUKS/first-run/authentication/lock-before-suspend; signed boot/update/recovery and rollback; usable authenticated maintenance in a non-demo profile; true independent secondary editor; full pane presets/pins/journal/scratchpad/handoff; detailed edit/time/footnote accounting; November/goals/challenges/achievements; actual 1,000-option licensed Spark corpus; Harper/Nabokov/proselint/WordNet; continuity/annotations/variants; genuine bottom-page DOCX/PDF footnotes; verified off-device backup/Drive restore/mobile/Docs workflows; all 39 feature exercises; physical keyboard/Wi-Fi/suspend/battery and analyzer benchmarks.

**VM-only security warning:** tty1 autologin is passwordless, passwords locked, bootloader unsigned and acceptance probe automatically powers the guest off. Never install this demo onto a real laptop or treat it as a recovery-capable production image.

## Next dependency order

1. Preserve this passing foundation milestone and continue only in tested increments.
2. Introduce a shared document registry and genuinely independent editable secondary buffer before pins/pane persistence.
3. Build project-local pin/journal/edit-ledger services with continuous tutorial and safety tests.
4. Populate licensed, checked corpus; implement local generators/calendar productivity; integrate analyzers; then long-form/export/sync.
5. Advance production installer/security/update/hardware qualification in parallel without enabling unsafe defaults.

See [feature matrix](FEATURE_MATRIX.md), [roadmap](ROADMAP.md), [testing](TESTING.md) and [risks](RISKS.md). Statuses mean Existing / Planned / In progress / Implemented / Tested / Hardware verified / Blocked; no Hardware verified claim exists.
