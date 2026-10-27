# Status — WrOS (Writer’s OS) 0.1.0-dev, through M7b software

## Native login — focused software and real guest qualification passed

`codex/native-greeter` adds an isolated Rust/Ratatui greetd client, bounded IPC,
non-echoing secret entry, local account selection, fixed non-root writing
sessions and an explicit staging profile. Read [native login boundaries](NATIVE_GREETER.md).

The [complete local software baseline](evidence/native-greeter-software.json)
passed **947 Rust tests per overlapping configuration** (5 default / 4 no-audio
ignored), both Clippys, **96 Python methods / zero skips** and **18 checks**.
A subsequent cache-only display refinement has its own [focused final record](evidence/native-greeter-focused.json):
**15 greeter tests in both configurations**, both workspace Clippys, full build,
syntax, staging ShellCheck and documentation checks. The baseline retains its
own prior source identity; it is not relabeled as a full rerun of the refinement.

The [final unattended real guest run](evidence/native-greeter-vm.json) passed in
**147.06 seconds** with 512 MiB, two CPUs and no NIC: actual PAM rejection/success,
hidden input, crash recovery, UID-1000/1001 editor sessions, actual picker-based
account switching, logout, journal/user-file secret scan and tty1/tty2 boundaries.
The base image stayed unchanged. The [cold-boot record](evidence/native-greeter-reboot.json)
and [actual screen](evidence/native-greeter-reboot.png) verify native service auto-start,
three seconds of complete visible text and six identical complete framebuffer
captures. The screenshot was inspected by the Codex agent. No password was
entered during this separate reboot test; the completed fixture stayed unchanged.

Final focused, authentication and reboot records share source
`ffd48f613a3387a9f8c579200424b7313cfb7093a8b7dcfd11c1ac0d375074c1` / 297 files and the exact binaries.
Idle console damage is repaired by cache invalidation without physically clearing
the display or adding cursor queries to authentication input. Full remote CI is
required before integration. This remains an opt-in experimental image profile;
pre-login keyboard recovery, further crash/layout cases and secure locking keep
#27/#4 open. Installer, encryption, update/recovery and hardware gates remain.

## Power safety follow-up — qualified parent increment

`codex/suspend-lock-guard` follows warm-screen PR #33. [Power guards](POWER_SAFETY.md)
address reproduced issues #32/#34: Suspend is unavailable until secure locking is
qualified, and shutdown/restart refuse pending writes, stale source trees and
unresolved conflicts after saves. [Focused offline qualification](evidence/power-guard-targeted.json)
passed 17 backend tests, 11 Settings/controller tests, the retained asynchronous
System test, both Clippys and an actual four-size Settings PTY. Fresh Debian/macOS
CI [40468810112](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40468810112)
passed on code head `15512d4`: **932 Rust/configuration, 96 Python/zero skips**,
both Clippys and **17 CI gates**. [Its unmodified report](evidence/power-guard-ci.json)
matches the targeted source fingerprint. The separate Windows check was still
running when recorded. The earlier warm-screen/VM records below apply to their
own source, not automatically to this newer increment. Full locking remains #4.

## Manual warm-screen increment — software qualified, hardware gates open

`codex/warm-screen` builds on unified-shell `e31d12b` (Debian/macOS CI
[40459742814](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40459742814) passed).
[Warm-screen controls and the narrow display service](WARM_SCREEN.md) now implement
1000–6500 K, a very warm 2200 K initial choice, strength, Off, confirmation and
independent rollback while retaining original calibration. Twenty display fault
tests, 16 backend tests and eight shared Settings UI cases passed locally.
The [fresh complete software run](evidence/warm-screen-software.json) passed
**928 Rust tests per overlapping configuration** (5 default / 4 no-audio ignored),
both Clippys, **96 Python methods with zero skips** and all **18 gates**. Source
and verifier inputs stayed unchanged; the tested binary matches the VM exactly.
The [current-code guest
probe](evidence/warm-screen-vm.json) passed in 88.51 s: the actual virtual GPU was
correctly refused, policies isolated users and native Settings rendered. No
positive gamma, physical display or current installer claim is made; #30 stays open.

Read-only audit also reproduced an existing unguarded suspend invocation despite
the absent secure locker. **Issue #32** tracks immediate fail-closed mitigation;
the broader lock/input ownership, lid/resume and hardware work stays under #4.
The follow-up implementation is above. Warm-screen's published head `8e2aeeb`
also passed Debian/macOS CI
[40466790817](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40466790817).

## Initial unified WrOS shell — current architectural direction

The user's unified-shell directive supersedes the standalone appliance-launcher
assumption. [The architecture audit and proposal](UNIFIED_SHELL.md) preserves
Grimoire as the foundation of one WrOS interface with isolated authentication
and service processes. The first `codex/unified-shell` increment adds shared
navigation/menu/notification/preferences components and native Writing,
Interface, System and Accounts Settings around retained editors and the existing
mockable worker. Existing settings remain available during migration.

[Its own fresh software run](evidence/unified-shell-software.json) passed **922
Rust tests/configuration**, both Clippys, **76 Python methods with zero skips**
and all **18 gates**, with exact unchanged inputs. This includes two-pane
cursor/selection/free-scroll/focus/undo, native error/confirmation and actual
four-size PTY/restart tests. No new OS/physical/credential acceptance is inferred.
Native greeter/lock, Google linking and Drive browsing remain unfinished
(#26–#29). Warm-screen implementation and its remaining qualification are above.


## Qualified M7b software — 2026-10-26

Source import #1 is complete. The historical foundation remains at `3360ec5`;
M7b was merged through PR #15 at `268fa63`; #2 and reproduced defects #16–#18
are closed. The default development branch is `writingdeck/m7b-wip`; placeholder
`main` is unchanged. A subsequent [experimental installer-media increment](INSTALLER_MEDIA.md)
is being tested. Real Debian and macOS CI passed on `881bd47` in run
[`40409883083`](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40409883083),
including 911 Rust tests per overlapping Debian configuration and 70 Python
methods. The source-path correction is merged through PR #20 at `4eb1533`;
#19 is closed. These later changes are not covered by
the earlier M7b source fingerprint below. This is an experimental
writing OS, with production installation, authentication/locking, signed updates,
recovery and hardware qualification still open. See [M7b scope](M7B.md),
[continuation](CONTINUATION_2026-10-26.md) and [import evidence](evidence/recovery-import-2026-10-26.md).

PR #23 subsequently fixed a deterministically reproduced Git index timestamp
race and is merged at `c67a5b1`; #22 is closed. Its actual Debian/macOS run
[`40445356850`](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40445356850)
passed **912 Rust tests per overlapping Debian configuration**, both Clippys,
**70 Python methods with zero skips**, and all 17 software gates. The separate
installer draft is PR #21. Its own Debian/macOS CI
[`40448965974`](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40448965974)
passed 912 Rust/configuration, 75 Python with zero skips and all 17 gates.
The revised recipe also passed a [clean current-source ISO build](evidence/installer-clean-build.json).
Its current-image full install hit the 1,200-second test deadline during
finalization and remains failed. A separate [post-interruption UEFI/PAM/UI/Wi-Fi
probe](evidence/installer-current-interrupted.json) passed, as did actual
[no-disk refusal and unchanged-target cancellation](evidence/installer-safety.json).
The older ISO's passed normal installation remains a separate result.

- **910 Rust tests passed per overlapping configuration**, default **5 ignored**,
  no-audio **4 ignored**; ignored cases are not passed. Both warning-denying
  workspace/all-target Clippys passed. **70 Python methods passed, zero skipped**,
  including actual Unicode/UI/PTY/permission/restart/undo, kernel denials,
  parent-death cleanup, exact source repacks, runtime materialization/tampering,
  staged-runtime safety and Wi-Fi packaging tests.
- [Software evidence](evidence/test-results.json) passed **17 software gates**.
  The independent [actual Wi-Fi image payload audit](evidence/wifi-image-payload.json)
  passed too. Source identity is
  `b23ed164a65904fe2aaf85a2534bac08c2e1bc5bffa21972690c7021f98775f5`
  across **263 source files**; verifier/tool hashes and binary/runtime identities
  are retained. The extra legacy publishing-workflow guards are independently
  recorded in [static workflow review](evidence/workflow-policy.json); actual
  GitHub CI is tracked separately from these local records.
- [Fresh VM evidence](evidence/vm-boot.json): **512 MiB / 2 vCPUs / zero NICs**,
  TCG Nehalem (x86-64-v2, no AVX), **223.45 s** including firmware, deliberate
  keyboard waits and shutdown. Actual non-root UI, polkit, keyboard rollback and
  commit, Harper original-source apply/undo, native prose, proselint and WordNet
  passed. Harper's tiny fixture took **93.1 s** / **187,144 KiB** peak under
  emulation. These are debug/emulator costs, not laptop latency or battery claims.
- **Nabokov did not complete inside that VM**: its existing 30-second kernel CPU
  guard returned an explicit `timed-out` result after **31.109 s**, zero findings,
  unchanged original bytes and no automatic retry. The VM verifies that bounded
  denial, not a clean analysis. Actual positive trained analysis passes in the
  native offline Rust/UI/PTY suite. CPU/wall limits were not raised to obtain a
  successful model result.
- [Synthetic host measurements](evidence/m7b-prose-resources.json) separate native,
  proselint and trained-worker costs. The 4,200-word repeated fixture is about
  **40 ms / 0.2 s / 2.6 s**, respectively; Python worker peaks about **29 MiB /
  224–225 MiB**. Its bounded Nabokov queue explicitly reports 144 omitted findings.
  Two fresh processes per engine are not persistent warm-model measurements.
- The image contains verified **49-package / 4,410-file** Python payloads, 94
  retained Python notices, WordNet data and common laptop Wi-Fi firmware with
  NetworkManager/nmtui. Physical adapter tests and production installer-media
  acceptance are pending. The inventory records **685 Rust / 212 staged Debian
  packages**, with **71 all-platform root-notice omissions / zero selected
  Linux-no-audio omissions**; release/corresponding-source/independent rights
  review remains open.

The following M7a and earlier evidence is historical. It does not certify the
remaining installer, lock, update/recovery, M8/M9, tutorial or hardware work.

**As of 2026-10-25: initial M5 recorded productivity, M6 original offline Story Spark and M7a opt-in source-safe grammar are implemented, preserving the original-source M4 workflows. The full requested OS is not complete or safe for production installation.** No physical laptop/disk installation, user manuscript, credential enrollment or Git push. M6 content is AI-assisted original drafting and agent authoring/editorial review, not independent human/legal qualification.

## M7b work in progress

After committed M7a `3360ec5`, prose/lexicon/voice/revision work is now in progress. The executed records below qualify **M7a**, not new unqualified M7b changes. Preserved records: [M7a software](evidence/history/m7a-test-results.json), [M7a VM](evidence/history/m7a-vm-boot.json), [M7a resources](evidence/history/m7a-writing-checks-resources.json). No full M7b verification or release qualification has run. Native/offline optional-tool boundaries and current spelling remain preserved.

## Executed M7a software and networkless VM evidence

The common source-map/worker and optional disclosed **Harper/Brill/POS 2.13.0** slice is now tested. See [Writing checks](WRITING_CHECKS.md) for exact source spaces, consent/location binding, three ignore scopes, dialogue policy, explicit native replacement/undo and failure/cancellation bounds. It includes neural inference and is **not described as non-AI**. Native spelling remains independent; nothing starts merely by opening the reader. Additional demo-only lesson 40 requires a real current grammar replacement; original spelling lesson 26 and all other original IDs retain their meanings.

- **859 Rust tests passed per overlapping configuration**, not 1,718 distinct tests: core **421**, TUI **381**, system **13**, other retained app/integration **44**. Default **5 ignored**, no-audio **4 ignored**; ignored cases are not passed. Tests took **54.170 s / 70.896 s**, including incremental compilation. Both warning-denying workspace/all-target Clippys passed (**28.451 s / 0.697 s**).
- **32 added Rust cases beyond M6:** 16 diagnostics engine/source/queue/preferences cases, one tutorial-history contract and 15 controller/UI cases. Real model runs are separated from named injected contract reports. Genuine worker/highlights/apply/undo run at all four terminal sizes; idle cleanup, copied-novel location consent, explicit revocation and durable no-credit checks are included.
- **52 Python methods passed, zero skipped, 75.610 s**. Five added actual-binary grammar methods cover four sizes/Unicode/apply/undo/restart, once/novel/personal scopes and location consent, future/external refusal, an edit during genuine work and real supplementary grammar completion retaining historical spelling. Source writes alone are not mistaken for already-flushed accounting records.
- All **16 verifier gates** passed: both configurations/Clippys plus formatting, corpus drift, locked model/notice audit, no-audio build, Python, terminal capture, Shellcheck, syntax, inventories, printable reference, documentation and diff checks. The earlier help-label/title contract failures were fixed without weakening tests; only this successful run is published.
- [Current software record](evidence/test-results.json): source **`25aaf5a6c43f6b67e81d8d774c28e224ffd650465b8da64a80641c117e3fb41f`**, **244 files**; no-audio debug binary **`a8061d3feb725ca833f069da5429632411549f080b66a9bc9c47286f79f4a3dd`**. Fingerprints are alignment/integrity records, not signed attestation or reproducible-release proof.
- [Fresh VM record](evidence/vm-boot.json): QEMU TCG/q35/OVMF, Secure Boot off, **512 MiB, two virtual CPUs, zero NICs**, actual non-root dashboard/polkit/keyboard rollback/commit plus a separate **non-root real grammar/apply/undo** fixture using the installed stripped binary. All required pass markers succeeded in **163.52 s**, including the new grammar workload. Dashboard RSS **35,552 KiB** is **not** loaded grammar memory.
- The guest's 86-byte Unicode grammar fixture had **189,448 KiB** kernel-peak RSS and **88.252 s cold wall time under TCG/debug**. That slow virtual-CPU result is retained, not presented as acceptable laptop latency. [Actual guest reader text](evidence/vm-grammar.txt). Physical/optimized/loaded-input/energy qualification remains open.
- [Separately measured sandbox debug resources](evidence/writing-checks-resources.json) are source/binary-aligned: small 86-byte Unicode versus 12,740-byte repetitive 2,340-word fixture; cold end-to-end **4.884 s / 5.331 s**, warm engine **8 ms / 593 ms**, kernel peaks roughly **182–187 MiB**. No Python/spaCy or physical battery inference. Pause does not promise cache unloading or preempt an active native parser.
- Inventory: **685 resolved all-platform Rust packages**, **197 staged Debian packages**, **71 unresolved all-platform root-notice omissions / zero selected Linux-no-audio omissions** after retaining seven exact upstream notices for eight selected archives. Nine packaged data/model files have locked archive/file/VCS identities. [Model/data manifest](model-licenses.json) and [notice sources](upstream-notice-sources.json) explicitly withhold independent training-data/dictionary/legal/corresponding-source/release clearance.

Strict software/VM source-alignment, 18 required docs plus STATUS, 69 capability rows, 40 catalog entries, local links, content/dictionary hashes and offline printable checks pass. No physical laptop, host-disk installation, credential enrollment, root Grimoire, manuscript upload or Git push has been performed. M7a software is **not** the whole requested OS.

Preserved previous qualification: [M6 software](evidence/history/m6-test-results.json) and [M6 VM](evidence/history/m6-vm-boot.json); the following historical counts are not the new M7a scope.

## Archived M6 software evidence

- **827 Rust tests passed in each overlapping configuration**, not 1,654 distinct tests. Default audio **5 ignored**, no-audio **4 ignored**; ignored external/model/performance cases are not passed. The full commands took **83.881 s / 60.885 s**, including this restored workspace's compilation. Core **404**, TUI **366**, system **13**, retained app/integration **44**. Both warning-denying workspace/all-target Clippys and formatting passed.
- **59 new Rust cases beyond committed M5:** 28 Spark core, three workflow adoption and 28 actual controller/UI. The established upstream, source-owner, pin, journal, scratchpad/promotion/handoff, starter/pin-set and M5 accounting/date/sprint/challenge guards remain.
- **47 Python methods passed, zero skipped, 21.718 s**: prior helper/planner/CLI/ANSI tests plus six new corpus contracts and four real non-root Spark PTY methods. These include four-size mode/reroll/paste/daily/favorite and **separate-process loaded-source edit/undo**, visible corrupt/future state, prepared source retry, initially empty independent scene editing, and real active-sprint metadata failure/refused normal quit/normal checkpoint/restart without offline credit.
- Corpus drift, terminal capture, Shellcheck, Python syntax, content/software/dictionary inventories, offline printable keys, documentation/local links and diff checks passed. The verifier rejects skipped Python coverage or source changes during a run; raw logs are ignored `build/verification/` artifacts and may disappear on workspace restoration.

Archived M6 software source SHA-256 **`e017f2cc3070fac0c56d0d74defcbba9be2c3834b478115f586069f0da258732`**, **234 files**. Tested no-audio binary **`e95e2e6b8a78105e0c9770d2586406eb796227a164bfaeda067ecbf9dad4095f`**. This is source alignment, not signed attestation or bit-identical release reproducibility. [Archived executed record](evidence/history/m6-test-results.json). Previous M5 evidence remains in [M5 software](evidence/history/m5-test-results.json) and [M5 VM](evidence/history/m5-vm-boot.json), not as current M6 evidence.

## Archived M6 VM evidence

A newly staged regular-file M6 image booted and passed **all five actual markers**: tty1/non-root UI, narrow active-local polkit eligibility, virtual console keyboard mapping/timed rollback, commit/reapplication and overall pass. Wall time **72.16 seconds including waits**; app UID **1000**; ordinary-dashboard RSS **28,688 KiB**. The real **1280×800 / 160×50** dashboard was inspected: appended Story Spark shows **1047 original plots / 84 exercises**, prior widget order remains, and neither Spark nor daily challenges is generated/assigned automatically.

[VM record](evidence/vm-boot.json) · [serial](evidence/vm-serial.log) · [actual dashboard](evidence/vm-dashboard.png). Source fingerprint matches the software run (**`e017f2cc3070fac0c56d0d74defcbba9be2c3834b478115f586069f0da258732`**); the strict current-evidence check passed at the M6 checkpoint, not on this M7 working tree. Image SHA-256 **`872b3f1e24363e94621c28b41b38121326dde0d6fe2c5e4638c3ec7e53de5fcc`**; installed stripped binary **`cce9b4c5d46d316e7e463af897af1b849edbe45b2143e59cb5838afdc8fd3143`**. Guest UTC can differ from the user's local civil date; its shown date is recorded, not guessed as the user's timezone.

QEMU TCG/q35/OVMF, Secure Boot off, 512 MiB, two vCPUs, **no NIC**. This VM is a boot/system/dashboard/console-map check, **not Spark control coverage**; those have separate genuine compiled-PTY tests. Wall time/RSS are not physical boot/battery, loaded-analyzer or long-history certification. Host Debian's maintained console compiler was available for this refreshed build; `loadkeys --bkeymap` parses generated maps without applying a host console map. The VM-only passwordless/locked-account profile remains unsuitable for production. No physical qualification is implied.

## Implemented scope and boundaries

- Audited Grimoire v0.7.5 base `f3ca1e8f1cb09b714e71046e8ed991051793d33a`: authoritative Markdown/raw YAML, native export, autosave/history/conflict/recovery and established default shortcuts.
- Debian regular-file GPT/UEFI networkless VM recipe, non-root console app, typed Linux/simulated services, cached asynchronous status and four narrow token-bound pre-login console keyboard layouts. No custom kernel, required desktop, root Grimoire or host disk installation.
- **21 optional widget kinds / 22 additive alias actions**, retaining F10/F11/F12 defaults, saved order/visibility, schema-v3 original editor/workspace contexts, canonical/dev-inode ownership and strict hidden-pane protections. See [WORKSPACES](WORKSPACES.md).
- M4 reference-only pins/anchors, separate sampled journal/original reflections, source-preserving multi-note scratchpad promotion, user-authored next-open handoff, explicit nonoverwriting starters and global-card-ID workspace sets. No competing body in JSON, live-sync resource rebase or fabricated restart intention/attestation.
- Initial M5 actual-range origin/classification/role/mode receipts, bounded source word units, conservative observed inter-edit/keyboard-pair spans, injected monotonic/civil/IANA clocks; nine goal families/calendar/rest/50k preset, five sprint variants with retained breaks/results and explicit restart, optional non-punitive collection, thirty original ten-category measured/self-reported daily exercises. See [DAILY_CHALLENGES](DAILY_CHALLENGES.md).
- **M6 original content:** 1,047 accepted independently worded plot situations from 1,051 literal drafts (four documented substantive-overlap holds), plus 84 new exercises. Full edition/section/file/line/text/file hashes, MIT to extent rights subsist, AI-assisted attribution; compiler normalized duplicates and published fixed editorial/lexical samples. Integrity/agent review is not independent human/editorial/legal or semantic certification. Historical OCR/modern cleaned prompts are not bundled/counts. See [CHALLENGE_LIBRARY](CHALLENGE_LIBRARY.md).
- **M6 real Story Spark:** four modes; compatible independent/joint rerolls; hard filters/dismissals and explicit recent-only fallback; frozen local-IANA-date daily; bounded reference-only history/favorites/settings/receipts; static and optional presentation-only roulette; genuine save/pin/scratch/Markdown outline/task/initially empty independent scene/timed sprint and separately accepted **self-reported** assignment. No fake M8 task database or timer-as-creative-completion claim. Source export uses guarded prepared → exclusive creation → ready, retains edited/missing/conflicting originals and reports failure. See [STORY_SPARK](STORY_SPARK.md).
- **39 catalog lessons: 27 event-capable / 12 unavailable**, not 39 final complete exercises. M6 22 requires actual new plot-note publication, 23 an exercise-only favorite, 24 a persisted reroll/favorite chain and new independent scene. No-op ready-source retries, navigation and ordinary novels cannot manufacture completion. Context events are not hardware/process/creative-work attestation. See [TUTORIAL](TUTORIAL.md).
- Read-only installer planner has **no apply mode**. CLI configuration audit includes layout/pins/journal/workflow/M5 ledger/settings/challenges and M6 reference/daily/assignment metadata without mutation. Capability/license inventories and notices aid review; `distribution_ready` and legal clearance remain false.

## Evidence and measurement limits

[Executed software record](evidence/test-results.json) · [80×24 Spark frame](evidence/spark-pty-80x24.txt) · [100×30](evidence/spark-pty-100x30.txt) · [120×40](evidence/spark-pty-120x40.txt) · [160×50](evidence/spark-pty-160x50.txt) · [daily-challenge frame](evidence/productivity-pty-80x24.txt) · [original-workflow frames](evidence/workflow-pty-screen.txt) · [journal frames](evidence/journal-pty-screen.txt) · [VM record](evidence/vm-boot.json).

The compiled PTY shim tracks differential cells and completed exact-ID frames, not an entire terminal emulator/physical keyboard. Ordinary VM dashboard RSS and QEMU wall time are not loaded-analyzer, real boot-speed or battery certification. VM profile: QEMU TCG/q35/OVMF, Secure Boot off, 512 MiB, two vCPUs, zero NIC; passwordless VM-only autologin, locked accounts and unsigned self-poweroff probe are not production authentication/recovery/update.

The historical [synthetic accounting probe](evidence/accounting-benchmark.json) is a shared-sandbox release-mode microbenchmark, not full UI/physical low-power certification. Source word units are not full native/rendered counts, keyboard-pair intervals are not exclusive human typing, and a 30-second uncheckpointed crash tail can be lost. Ledger 1 MiB/128-group parts, 4,096 pending groups and 16,384-file/64 MiB reload bounds are not indefinite archival. Optional awards cap at 512. Guarded stores are not locks/CAS or cross-file power-cut transactions; source hashes do not authenticate imported metadata.

## Remaining dependency/release gates

Preserve tested M7a common maps/worker/optional grammar and continue M7b versioned prose adapters/lexicon/voice/ranked dashboard, then M8 explicit continuity/annotations/variants/local read-through queue and M9 genuine footnote publishing/verified backup/restore/conflict-safe sync/mobile workflows, preserving M4–M6 behavior. M6 Markdown captures are not those later services.

M2b production installer/LUKS/authenticated first-run, input-reader-safe lock-before-suspend, signed update/recovery/rollback, complete corresponding source/notices, all 39 original plus supplemental 40 full exercises, long-load/archival/power-loss tests and independent content/release review remain open. Live provider acceptance requires explicit credentials when reached; physical Wi-Fi/keyboard/lock/suspend/battery/install/update qualification requires available authorized hardware. No software/VM substitute is labeled Hardware verified.

See [ROADMAP](ROADMAP.md), [FEATURE_MATRIX](FEATURE_MATRIX.md), [TESTING](TESTING.md) and [RISKS](RISKS.md). This is not completion of the full OS request.
