# Status — WritingDeck OS 0.1.0-dev, through M3b

**As of 2026-10-24: tested appliance foundation and independent secondary editor. The full requested appliance is not complete or safe for production installation.** All verification is scoped below. No physical laptop, cloud account or user manuscript was used.

## Completed software evidence

- Full offline Rust suites: **532 passed, 5 ignored** with default audio features; **532 passed, 4 ignored** without audio. These suites overlap, not 1,064 distinct tests. Core 248, TUI 230, system 10 and retained app/integration tests pass. Formatting and warning-denying workspace/all-target Clippy pass.
- **42 new Rust tests since the foundation:** 28 dedicated second-editor UI cases, 12 core registry/workspace/editor cases and two shortcut-compatibility cases. Required layouts: 80×24, 100×30, 120×40 and 160×50, plus tiny-viewport fallback.
- **22 Python tests passed, zero skipped:** 13 rootless keyboard-helper, four read-only installer-plan, four actual headless CLI, and one real-binary **unprivileged PTY** test with all four terminal-size subcases. That last test sends actual F11 escape input, bracketed paste, save/undo/redo in both editors, hide/show and normal quit; it checks both disk files and persisted document roles. HOME/projects are new isolated fixtures. The PTY shim answers the startup cursor query; this is not a physical keyboard or full terminal-emulator certification.
- Shellcheck, Python syntax, docs/manifests/matrix parity, original tutorial/dictionary hashes, self-contained printable reference and diff checks pass. Ignored screenshot/performance/real-Piper-model tests are not passed.

Machine-readable results and source identity: [test-results.json](evidence/test-results.json). Logs under ignored `build/` are transient; small evidence under `docs/evidence/` is retained. `tools/check-writingdeck.py --require-current-evidence` rejects stale-source software/VM records.

## VM evidence

**M3b image rebuilt and actual whole-disk UEFI boot passed.** The new [VM record](evidence/vm-boot.json), [serial assertions](evidence/vm-serial.log) and [actual console screenshot](evidence/vm-dashboard.png) replace the historical foundation evidence. Runtime UID **1000**, process RSS **20,200 KiB**, wall **69.29 seconds** including the intentional rollback wait and shutdown. The screenshot was inspected: no stale login/splash text. Source fingerprint `a03f607b9ba589039a4de78584ec024945195c3d0a15175f982aa2c87fc6d5f5` (185 files) matches the software evidence and checked-out source; image and installed stripped-binary SHA-256 values are retained in the VM record. This is not a physical boot-speed, battery or memory benchmark.

The VM profile is QEMU TCG/q35, OVMF UEFI with Secure Boot disabled, 512 MiB, two vCPUs, no NIC. Its probe checks actual tty1 dashboard/non-root UID, active-local narrow polkit eligibility, Colemak-DH physical E → logical F, independent systemd timeout rollback, committed configuration and boot-command reapplication. It does not send F11 editor keystrokes; that behavior is covered by Rust UI and compiled-binary PTY tests. It is not a full cold-reboot/suspend/hardware qualification.

## Implemented and tested scope

- Preserved upstream Grimoire v0.7.5 base `f3ca1e8f1cb09b714e71046e8ed991051793d33a`, Markdown/YAML, autosave/history/conflict/recovery and existing shortcuts.
- Debian 13.7 amd64/kernel `6.12.113+deb13-amd64` staged VM image; direct non-root console start, no custom kernel/heavy desktop/physical-disk manipulation.
- Typed Linux/simulated services, bounded fixed commands and cached off-thread UI state. Four registered QWERTY/Colemak/DH ANSI/ISO console maps with narrow preview/commit helper and independent rollback.
- Existing-data configurable F12 home, F10 outline, strict Ctrl-D focus, additive aliases for nine actions.
- **F11 independent editor:** distinct complete Editor contexts, separate cursor/selection/free scroll/undo and geometry; canonical/dev-inode ownership; already-open aliases focus their owner; shared original Project guarded saves and both-buffer recovery. Wide layouts split, narrow layouts stack; tiny/strict-focus layouts cannot focus hidden text.
- **Schema-v2 dual workspaces:** both paths/positions/selections/scroll, active editor, visibility and ratio; custom clones, outline sizes; read-only v1 migration; unknown/corrupt/future/conflicting state preservation; newer upstream Primary resume precedence.
- Inactive-buffer updates for whole-book replacement/undo, name fixes, external adoption and recovery; clean/dirty/ambiguous external rename handling; closure of both affected views on trash; failed saves and external identity changes retain buffers rather than silently replacing them. Registry ownership is **not** an OS lock or cross-process CAS.
- Original 39-lesson catalog, **16 event-capable** lessons including actual demo secondary editing; 23 unavailable lessons remain disabled. Real-novel writing exercises are blocked; switching panes alone cannot satisfy an edit lesson. Saved replay/skip progress and searchable offline help are retained.
- Read-only installation planner with **no apply mode**; all 17 required docs, feature/content/software inventories, retained notices and printable logical-shortcut reference. Inventory is not legal distribution clearance.

## Critical outstanding gates

Production installer/LUKS/first-run/authentication/lock-before-suspend; custom layout registration/onboarding and remaining sound/clock/timezone controls; signed boot/update/recovery/rollback and authenticated maintenance; full content presets/sticky sidebar/pins/journal/scratchpad/handoff; detailed gross/net/deleted/time/footnote accounting; November calendar/advanced goals/challenges/achievements; actual 1,000-option licensed Spark corpus; Harper/Nabokov/proselint/WordNet; continuity/annotations/variants; genuine bottom-page DOCX/PDF footnotes; verified off-device/Drive restore/mobile/Docs workflows; all 39 complete feature exercises; physical keyboard/Wi-Fi/suspend/battery, full power-cut and analyzer performance qualification. Wi-Fi scan truncation/presentation and actual network enrollment are not fixed or qualified by M3b.

**VM-only security warning:** tty1 autologin is passwordless, passwords locked, bootloader unsigned and acceptance probe powers the guest off. Never install this demo onto a real laptop or treat it as a recovery-capable production image.

## Next dependency order

1. Preserve M3b as a tested local milestone; no push or physical install is needed.
2. Build M4 single-source Markdown pins and recoverable presentation/anchor state on this registry, then actual cards/sidebar, journal event/manual separation, scratchpad promotion and handoff. Add tutorial, restart, alias and failure tests with each real feature.
3. Continue corpus/productivity/analyzers/long-form/export/backup in dependency order; do not confuse safe interfaces or research with finished features.
4. Advance authenticated setup/lock/update/recovery and physical qualification in parallel, without enabling unsafe defaults or requesting credentials before a safe testable workflow exists.

See [feature matrix](FEATURE_MATRIX.md), [workspace behavior](WORKSPACES.md), [roadmap](ROADMAP.md), [testing](TESTING.md) and [risks](RISKS.md). Statuses mean Existing / Planned / In progress / Implemented / Tested / Hardware verified / Blocked; no Hardware verified claim exists.
