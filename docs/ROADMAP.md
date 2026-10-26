# WritingDeck OS implementation roadmap

Working branch: `writingdeck/foundation`. Upstream base: Grimoire v0.7.5 / `f3ca1e8`. Current release designation: **0.1.0-dev foundation + M3b + M4 + initial M5 (recorded operations, goals/calendar/sprints/challenges)**, not a complete production OS. Read [STATUS](STATUS.md) for the latest run evidence and exact remaining work.

## Working rules

Keep changes vertical, safe and tested. Preserve manuscript formats and all existing UI guard tests. Add tutorial events, offline help, recovery behavior and tests when a feature lands, not at the end. Unknown configuration must not be rewritten. Never open a physical disk merely to demonstrate an installer. No root Grimoire, telemetry, mandatory AI or automatic manuscript transmission.

Status vocabulary: **Existing** (audited upstream), **Planned**, **In progress**, **Implemented** (code, not yet verified), **Tested** (named automated/VM evidence), **Hardware verified** (actual identified device only), **Blocked** (specific credential/hardware/service or rights gate). See the per-capability matrix; completing a foundation does not complete every phase.

## Milestones and acceptance gates

| Milestone | Dependencies | Independently testable deliverable / exit criteria | Current state |
|---|---|---|---|
| M1: Audited appliance foundation | Baseline checkout/toolchain | Audit, base comparison, architecture, manifests, all required doc entry points; Debian rootfs and GPT/UEFI image workflow; real non-root tty1 Grimoire boot without a desktop; no host disk writes | **Tested** foundation; production installer intentionally not included |
| M2a: Narrow system + console keyboards | M1 | Linux/simulated backend; fixed commands; cached off-thread status; pre-login QWERTY/Colemak/DH ANSI/ISO; token-bound preview; independent 15s rollback even if app disappears; documented maintenance tty | **Tested** interfaces/VM subset; no physical laptop verified |
| M2b: Secure laptop setup and maintenance | M2a | Debian installer integration or guarded installer with compatibility/disk/LUKS/account/timezone/keyboard/Wi-Fi review; authenticated first boot; working console lock before suspend; signed updates and verified recovery/rollback | **In progress** design; production install, lock and updater code remain **Planned** |
| M3a: Home and primary workspace state | M2a (does not need disk installer) | 80×24+ navigation; real project/goal/progress/sprint/system widgets; hide/reorder; optional direct restore; outline toggle/width; primary cursor and custom layout slots; strict focus; safe alias UI | **Tested** initial slice; advanced widgets/pane presets not complete |
| M3b: Multi-document pane registry | M3a | Independent editable secondary buffer/cursor/selection/undo/autosave; no two competing saves for same canonical file; full focus cycling/resizing/external-change/recovery tests; only then assign F11 | **Tested**: 28 dedicated UI tests, registry/schema tests and real-binary PTY at four sizes; F11 enabled |
| M4: Pins, journal, scratchpad, handoff | M3b | Single-source Markdown pin views, scopes/trash/categories; free-scroll and semantic anchors; card/sidebar restoration; quick edits; journal event/manual separation; scene-linked scratchpad and next-boot intention | **Tested initial M4**: M4a pins, M4b sampled journal/original reflections, M4c original-preserving multi-note promotion, next-open handoff, explicit starters/pin sets; real UI/tutorial/failure/process-restart evidence. Gross/time ledger and large-load/power-loss qualification remain M5/M10 |
| M5: Goals, accounting, sprints, achievements, daily challenges | M4 and edit ledger | Monotonic idle-aware accounting with gross/net/deleted words, revision/footnote policy; calendars/rest days/50k preset; sprint variants; disableable non-punitive achievements; persisted local-date challenges across ten categories | **Tested initial software**: actual-range source-unit ledger/injected civil clocks, goals/calendar/rest days, sprint variants/retained breaks, optional collection and 30 original exercises; 30 controller cases + compiled four-size restart/retry PTY. Checkpoint crash-tail completeness, indefinite archival and physical/loaded/power qualification are not claimed |
| M6a: Licensed corpus | Rights review and M5 data format | Per-item source/edition/hash/attribution; CC ShareAlike separation; 1,000+ substantively distinct reviewed plot options plus substantial usable exercises; measurable unique-record validation, not adjective multiplication | **Planned**; source research complete enough to select a safe path, no such corpus bundled yet |
| M6b: Story Spark | M6a, M4-M5 | Plot/challenge/combined/daily modes; compatibility and independent/joint rerolls; local history/favorites/filtering/anti-repeat; static and optional roulette views; pin/scratchpad/scene/task/sprint actions | **Planned** |
| M7a: Common diagnostics and Harper | M3b | Byte-accurate source maps, revision-tagged jobs, stale-result discard; live offline grammar alongside current spelling; novel/global/once ignores; dialogue policy; no automatic replacements | **Planned**, researched harper-core 2.13.0 |
| M7b: Prose/lexicon/voice/revision dashboard | M7a | Versioned Nabokov/proselint adapters; fiction policies, dedup/suppression, performance budgets; WordNet senses/antonyms; sufficient-material voice profiles and ranked review queue, no quality score | **Planned** |
| M8: Long-form story management | M4, M7 | Structured explicit facts/timeline/knowledge; definite vs suggested contradictions; quote-context annotation anchors; named checkpoints/variants/diff/restore; multi-scene local TTS queue | **Planned** |
| M9a: Publishing correctness | M7-M8 | Unified preflight; genuine OOXML and PDF page-footnotes with package and pagination fixtures; private material excluded; native export regression suite retained | **Planned** enhancements; existing DOCX/EPUB/Markdown/PDF path retained |
| M9b: Backup, sync and mobile workflow | M9a, credentials optional | Explicit OAuth enrollment; private credentials; offline queue; versioned SHA-256 verified backup + real restore; interrupted/conflict-safe sync; tested or explicitly rejected Google Docs round trips | **Planned**; live Google authorization/round-trip tests **Blocked** on user credentials when reached |
| M10: Production qualification | All above | All 39 lessons exercised; install/update/rollback/power-loss test matrix; low-power benchmarks including Python; signed release + complete notices/source obligations; real laptop Wi-Fi/keyboard/lock/suspend/battery qualification | **Planned**; physical qualification **Blocked** on available hardware |

## Initial acceptance evidence

The checked-in small VM evidence records an OVMF **UEFI whole-disk boot**, QEMU TCG, 2 vCPUs, 512 MiB guest RAM and zero network devices. It asserts the actual tty1 dashboard, non-root UID, narrow polkit eligibility, real DH console mapping, systemd rollback and committed-map reapplication. It is not a boot-speed or battery claim for physical hardware, not a complete installation test, and not Secure Boot certification.

The application tests retain the upstream Every key/menu/palette/help contracts and add the four requested representative terminal sizes, strict focus with a retained reference, widget persistence, both workspace cursors/selections/free scroll, alias ownership and capture, demo-only verified tutorial editing, invalid state preservation and asynchronous system status.

## Next executable work

1. Preserve the software-tested initial M5 operation/civil-date subsystem and all original-source M4 workflows. Continue M6a with genuinely distinct reviewed/licensed plot records before Story Spark; the 30 M5 exercises do not meet its 1,000-option gate. Observed keyboard-pair time must never become an exclusive-human-typing claim; checkpoint/archival/large-load/power limits remain explicit.
2. Add screen-lock integration with an input-reader pause/ownership handshake. Do not launch a password reader while Grimoire's event thread can still consume password keystrokes.
3. Integrate safe authenticated first-run provisioning with an upstream Debian installation path; review without selecting or erasing a real disk. Add staged tests for current account/keyboard/timezone choices.
4. Pin image/toolchain inputs for reproducible releases; build `.deb` integration packaging, security update/rollback fixtures and required corresponding-source/notices artifacts.
5. Continue M6–M9 in dependency order after their actual prerequisites. Avoid shallow stubs for every later feature or claiming an original two-scene tutorial is a 1,000-item challenge corpus.

## Release blockers

A production release cannot ship the current passwordless VM profile, an unsigned unreviewed boot chain, untested lock/suspend behavior, a claimed verified backup without a restore, unlicensed corpus text, or untested physical keyboard/power support. Development mode and the networkless VM remain useful and testable while those gates are open.
