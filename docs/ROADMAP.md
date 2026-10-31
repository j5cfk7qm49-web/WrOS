# WrOS (Writer’s OS) implementation roadmap

Historical M7a verified branch: `writingdeck/foundation`; the original unqualified M7b handoff remains in the ancestry of `writingdeck/m7b-wip`. Upstream base: Grimoire v0.7.5 / `f3ca1e8`. Current development scope is **0.1.0-dev through M7b software**, including its documented emulated-CPU limits. Production installation, authentication/lock, updates/recovery and later milestones remain open. Read [STATUS](STATUS.md) for fresh evidence and exact remaining work.

## Current architectural priority

The reviewed [Argon contribution](ARGON_REVIEW_2026-10-30.md) advances explicit
continuity, durable draft comparisons and chapter read-through. Full M8 knowledge, M9 paperback-footnote output and wider preflight, maintained backup/sync
and privileged system workflows remain open. Submission claims are not gates.

The user's 2026-10-28 sequencing decision puts the installed writing environment
ahead of further full installer qualification. Preserve the current installer
build checkpoint, then finish the everyday native environment and remaining
writing workflows before repeatedly rebuilding the final installation image.
Packages, services, defaults and first-run behavior should settle together.
Keep basic boot/install smoke checks as prerequisites change; a finished installer
and release qualification still remain required before shipping.

The next environment work should close visible native gaps such as Wi-Fi
enrollment, account management, backup/restore and update/recovery flows through
maintained Debian services and the shared Grimoire interface. Retain existing
writing behavior and complete the remaining story/publishing work in dependency
order. Installer polish must not displace that environment work.

**Reuse first:** follow the [Debian/FOSS component decisions](FOSS_REUSE.md).
The user authorized maintained Debian packages and suitable upstream builds for
remaining work and strengthening existing parts. Native WrOS UI and necessary
integration stay local; do not create replacement OS engines where maintained
components fit. Preserve existing authoring and safety contracts during adoption.

The user's unified-shell directive now precedes unrelated feature expansion.
[Architecture audit and decisions](UNIFIED_SHELL.md) preserve all existing work.
Execute #26 shared UI/native categorized Settings and exact editor restoration;
#30 native very warm display controls; #27 isolated greetd/PAM greeter and #4
secure input/lock/suspend; #28 optional protected Google enrollment; #29 native
Drive browsing/materialization, then #10 independent verified backup and
conflict-safe sync. Ordinary management stays inside WrOS. Emergency maintenance
remains documented. No root editor, desktop-environment replacement, credential
shortcuts, mounted active manuscripts or fake tutorial completion.

The initial `wros-shell` library and categorized Settings adapter passed local and
Debian/macOS CI qualification. Existing menu/settings actions remain available;
unsupported account/update/recovery operations remain explicitly unavailable.
The [warm-screen service](WARM_SCREEN.md) now implements 1000–6500K, initial
2200K, strength/Off and independent preview rollback; fresh full software and
unsupported-driver VM qualification passed. Physical and transition gates stay
open. Issue #32 immediately blocks unsecured suspend before #27/#4 enable it.
Installer/encryption, signed updates/recovery, notices/source and identified
physical hardware remain release gates throughout this architectural change.

The separate [native greeter](NATIVE_GREETER.md) now implements the greetd/PAM
conversation, local account selection and fixed unprivileged session startup.
Its software and guest qualification are tracked in current status. [Native
keyboard recovery](GREETER_KEYBOARD.md) now has software and real guest coverage
for all four layouts, timed/crash rollback and explicit confirmation. The
independent secure locker/input handoff, remaining transition/failure cases and
physical suspend/resume still prevent completion of the authentication/session
work. [CI](CI.md) targets only the WrOS Debian runtime.

The [editor input-ownership prerequisite](INPUT_OWNERSHIP.md) now replaces its
Linux reader thread with an isolated parser process, acknowledged retirement
and explicit fresh-parser resume. Focused fault/private-PTY checks pass; full
software and guest records remain separately tracked. This is not the protected
locker/VT or lock-before-suspend integration required by #4.

## Working rules

Keep changes vertical, safe and tested. Preserve manuscript formats and all existing UI guard tests. Add tutorial events, offline help, recovery behavior and tests when a feature lands, not at the end. Unknown configuration must not be rewritten. Never open a physical disk merely to demonstrate an installer. No root Grimoire, telemetry, mandatory AI or automatic manuscript transmission.

Status vocabulary: **Existing** (audited upstream), **Planned**, **In progress**, **Implemented** (code, not yet verified), **Tested** (named automated/VM evidence), **Hardware verified** (actual identified device only), **Blocked** (specific credential/hardware/service or rights gate). See the per-capability matrix; completing a foundation does not complete every phase.

## Milestones and acceptance gates

| Milestone | Dependencies | Independently testable deliverable / exit criteria | Current state |
|---|---|---|---|
| M1: Audited appliance foundation | Baseline checkout/toolchain | Audit, base comparison, architecture, manifests, all required doc entry points; Debian rootfs and GPT/UEFI image workflow; real non-root tty1 Grimoire boot without a desktop; no host disk writes | **Tested** foundation; production installer intentionally not included |
| M2a: Narrow system + console keyboards | M1 | Linux/simulated backend; fixed commands; cached off-thread status; pre-login QWERTY/Colemak/DH ANSI/ISO; token-bound preview; independent 15s rollback even if app disappears; documented maintenance tty | **Tested** interfaces/VM subset; no physical laptop verified |
| M2b: Secure laptop setup and maintenance | M2a | Debian installer integration or guarded installer with compatibility/disk/LUKS/account/timezone/keyboard/Wi-Fi review; authenticated first boot; working console lock before suspend; signed updates and verified recovery/rollback | **In progress** experimental installer and isolated native login; production installation, secure lock and updater remain open |
| M3a: Home and primary workspace state | M2a (does not need disk installer) | 80×24+ navigation; real project/goal/progress/sprint/system widgets; hide/reorder; optional direct restore; outline toggle/width; primary cursor and custom layout slots; strict focus; safe alias UI | **Tested** initial slice; advanced widgets/pane presets not complete |
| M3b: Multi-document pane registry | M3a | Independent editable secondary buffer/cursor/selection/undo/autosave; no two competing saves for same canonical file; full focus cycling/resizing/external-change/recovery tests; only then assign F11 | **Tested**: 28 dedicated UI tests, registry/schema tests and real-binary PTY at four sizes; F11 enabled |
| M4: Pins, journal, scratchpad, handoff | M3b | Single-source Markdown pin views, scopes/trash/categories; free-scroll and semantic anchors; card/sidebar restoration; quick edits; journal event/manual separation; scene-linked scratchpad and next-boot intention | **Tested initial M4**: M4a pins, M4b sampled journal/original reflections, M4c original-preserving multi-note promotion, next-open handoff, explicit starters/pin sets; real UI/tutorial/failure/process-restart evidence. Gross/time ledger and large-load/power-loss qualification remain M5/M10 |
| M5: Goals, accounting, sprints, achievements, daily challenges | M4 and edit ledger | Monotonic idle-aware accounting with gross/net/deleted words, revision/footnote policy; calendars/rest days/50k preset; sprint variants; disableable non-punitive achievements; persisted local-date challenges across ten categories | **Tested initial software**: actual-range source-unit ledger/injected civil clocks, goals/calendar/rest days, sprint variants/retained breaks, optional collection and 30 original exercises; 30 controller cases + compiled four-size restart/retry PTY. Checkpoint crash-tail completeness, indefinite archival and physical/loaded/power qualification are not claimed |
| M6a: Licensed corpus | Rights review and M5 data format | Per-item source/edition/hash/attribution; 1,000+ substantively distinct original situations plus usable exercises; no adjective/Cartesian inflation | **Tested content integrity / agent-reviewed originals**: 1047 accepted plots from 1051 immutable drafts (four overlap holds), 84 exercises, source/edition/line/text/file hashes, compiler/duplicate/fixed-sample checks. AI-assisted drafting and agent review; independent human/editorial/legal release qualification is not claimed |
| M6b: Story Spark | M6a, M4-M5 | Four modes; compatibility and independent/joint rerolls; frozen local daily; bounded references/filter/recent fallback; static/optional presentation; actual source actions | **Tested software**: 28 core + 28 UI + guarded adoption and four compiled PTY methods, including four-size loaded original/restart, metadata/export failures and active-sprint normal quit. Real save/pin/scratch/Markdown task+outline/initially empty independent scene/timed sprint/optional SELF-REPORT assignment. M8 structured services and physical/release gates not implied |
| M7a: Common diagnostics and Harper | M3b | Byte-accurate source maps, revision-tagged jobs, stale-result discard; live offline grammar alongside current spelling; novel/global/once ignores; dialogue policy; no automatic replacements | **Tested software**: explicit EditorBody/MarkdownFile maps, optional pinned model-based Harper/Brill/POS 2.13, bounded worker/current-source guards, live/once + personal location consent, three ignore scopes; 15 UI and five actual-binary methods, genuine additional lesson 40. Physical/legal/Python gates separate |
| M7b: Prose/lexicon/voice/revision dashboard | M7a | Versioned Nabokov/proselint adapters; fiction policies, dedup/suppression, performance budgets; WordNet senses/antonyms; sufficient-material voice profiles and ranked review queue, no quality score | **Tested software**: 910 Rust tests/config, 70 Python methods, real host model/UI/PTY, verified runtime/image staging, explicit VM trained-worker CPU denial and measured budgets. See M7B.md and current evidence |
| M8: Long-form story management | M4, M7 | Structured explicit facts/timeline/knowledge; definite vs suggested contradictions; quote-context annotation anchors; named checkpoints/variants/diff/restore; multi-scene local TTS queue | **In progress**: durable notes/facts/events, scene snapshots/variant restore and chapter speech queue qualified; selective passage restore added with full qualification in progress; complete knowledge remains open |
| M9a: Publishing correctness | M7-M8 | Unified preflight; genuine OOXML and PDF page-footnotes with package and pagination fixtures; private material excluded; native export regression suite retained | **In progress**: selected-source review and manuscript Word/PDF page notes; independent full Debian software qualification passed; native EPUB notes independently qualified; paperback and broader preflight remain open |
| M9b: Backup, sync and mobile workflow | M9a, credentials optional | Explicit OAuth enrollment; private credentials; offline queue; versioned SHA-256 verified backup + real restore; interrupted/conflict-safe sync; tested or explicitly rejected Google Docs round trips | **Planned**; live Google authorization/round-trip tests **Blocked** on user credentials when reached |
| M10: Production qualification | All above | All 39 original lessons plus supplemental 40 exercised; install/update/rollback/power-loss test matrix; low-power benchmarks including Python; signed release + complete notices/source obligations; real laptop Wi-Fi/keyboard/lock/suspend/battery qualification | **Planned**; physical qualification **Blocked** on available hardware |

## Initial acceptance evidence

The checked-in small VM evidence records an OVMF **UEFI whole-disk boot**, QEMU TCG, 2 vCPUs, 512 MiB guest RAM and zero network devices. It asserts the actual tty1 dashboard, non-root UID, narrow polkit eligibility, real DH console mapping, systemd rollback and committed-map reapplication. It is not a boot-speed or battery claim for physical hardware, not a complete installation test, and not Secure Boot certification.

The application tests retain the upstream Every key/menu/palette/help contracts and add the four requested representative terminal sizes, strict focus with a retained reference, widget persistence, both workspace cursors/selections/free scroll, alias ownership and capture, demo-only verified tutorial editing, invalid state preservation and asynchronous system status.

## Next executable work

1. Preserve original-source M4, actual-operation/civil-date M5 and original offline M6 behavior and all failure/restart/tutorial tests. Preserve tested M7a common byte-accurate source maps/revision-tagged jobs/opt-in model Harper, then continue M7b prose/lexicon/voice/ranked dashboard before M8–M9. See [M7a scope/limits](WRITING_CHECKS.md). Original agent-reviewed corpus integrity is not independent human/legal attestation; observed keyboard-pair time is not exclusive-human typing. Archival/large-load/power/release gates remain explicit.
2. Preserve the qualified manual vlock/PAM/systemd lock and reader handoff. Continue lock-before-sleep/resume qualification while all sleep requests stay disabled.
3. Integrate safe authenticated first-run provisioning with an upstream Debian installation path; review without selecting or erasing a real disk. Add staged tests for current account/keyboard/timezone choices.
4. Pin image/toolchain inputs for reproducible releases; build `.deb` integration packaging, security update/rollback fixtures and required corresponding-source/notices artifacts.
5. Continue M7–M9 in dependency order after their actual prerequisites. Avoid shallow stubs for every later feature; real M6 source/reference actions are not an implemented M8 story database or complete OS.

## Release blockers

A production release cannot ship the current passwordless VM profile, an unsigned unreviewed boot chain, untested lock/suspend behavior, a claimed verified backup without a restore, unlicensed corpus text, or untested physical keyboard/power support. Development mode and the networkless VM remain useful and testable while those gates are open.
