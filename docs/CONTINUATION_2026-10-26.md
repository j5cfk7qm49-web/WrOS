# WrOS continuation — 2026-10-26

## Active continuation — integrated native image, 2026-10-27

`codex/integrated-installer`, implementation `9e7d99c`, draft #40 stacked on
input #39. Includes the separate installer keyboard #25 commits and native login
by default. Automatic lid/idle/power/sleep-key actions are ignored; all sleep
modes disabled/targets masked until secure locking is qualified. No host changes.
Clean native source/build and Debian CI 40611457588 passed: 959 Rust/configuration,
108 Python/zero skips, both Clippys and 17 gates. Source `97e9afe2...1d2741` / 309;
tested merge `4b821f15b9e8416a8c6c4326a1d113ac1d2225fb`.
Safe raw build/CI reports are `integrated-installer-{build,ci}.json`.

A fresh no-NIC install/UEFI/native PAM/editor/input/sleep-guard/logout run is
in progress in the clean native snapshot. Record its actual result separately;
do not substitute older image, interrupted-install or overlay evidence. Current
ISO `cdb2be91...665d8ff` is 1,219,952,640 bytes. It predates the separately tested
native timezone controls, draft #41, implementation `c88c66c`, whose focused and
real guest records are on that branch and whose Debian CI is running.
Secure locking/suspend, LUKS/recovery, updates, remaining management/writing tools,
provider credentials, rights and physical qualification remain open.

## Active continuation — editor input ownership, 2026-10-27

Current `codex/input-ownership`, implementation `930ee9e`, draft PR #39, stacked
on keyboard #38 (`fd1864e`). CI remains WrOS Debian only. The MIT `wros-input`
crate and early same-binary re-exec replace Linux's blocking input thread with
an unprivileged reader process, without a new installed executable. Pause
retires/reaps it, discards its complete/partial parser and queued input, flushes
the kernel and returns an opaque single-use proof; explicit resume flushes again
and starts a fresh acknowledged parser. Parent death kills the reader; controller
thread affinity is enforced. Failure/proof drop never resumes input.

Focused record `input-ownership-focused.json` passed 11 checks: four unit tests,
one compile-fail and four actual private-PTY scenarios in both configurations,
both workspace Clippys/build, three real editor fault/context tests, retained
four-size Settings/cursor/undo/restart PTY and static gates. Source
`0f668f5d...17aba` / 304 files; editor `250ea0fb...63601`, greeter `bf193c66...4e7c`.
The only Cargo.lock changes add the internal package/edges and crossterm serde;
no dependency versions changed. Existing notices preserved; new internal MIT
inventory/notice retained.

Debian CI 40601108193 PASSED: 959 Rust/configuration (5 default / 4 no-audio
ignored), both Clippys, 99 Python/zero skips and 17 gates. Four standalone real-PTY
ownership scenarios ran in each configuration. Tested merge
25c2b9fd1b7b132f5f966c94c47fe9e6a0b30f95; source matches focused/guest records.
The no-NIC native-login guest run-k45jefgu PASSED 376.22s: reader UID/parent, writer
SIGKILL and ordinary logout without orphan readers, PAM/keyboard/account switching
and secret scan. Base unchanged; exact local editor/greeter match focused and
archived fixture. Safe raw reports input-ownership-{ci,vm}.json are retained.
Do not relabel old keyboard/login evidence as current. This is not VT-lock proof.

Next #4 requires the independent maintained locker, protected seat/VT ownership,
stopping editor rendering and ALL terminal queries during handoff, verified
unlock and authoritative lock-ready before sleep. This reader API alone cannot
authorize a credential prompt or suspend. Suspend remains unavailable. No live
host terminal/disk/account/credentials/lock/suspend operations were performed.

## Prior checkpoint — WrOS CI and greeter keyboards, 2026-10-27

Current branch `codex/greeter-keyboard`, implementation `4381d58`, draft PR #38,
stacked on CI PR #37 (`75a2a3a`) and native login #36. WrOS is the platform;
Windows/macOS jobs are removed and upstream application publishing workflows
are preserved outside active Actions. The sole Debian job for #37 passed run
40580883303. Do not restore cross-platform CI.

Native pre-login F2 recovery uses only the narrow keyboard helper and existing
independent rollback. Focused 22 greeter tests/configuration, both workspace
Clippys/build, 13 helper tests and static gates pass. Real guest passed 302.81s:
four console maps/cancel, expiry, crash rollback, durable confirmation/QWERTY
recovery, real PAM/hidden input, two-account writing/switch/logout, secret scan.
Base unchanged. Focused/package/guest source `18bc11d9...b445` / 299 files;
package greeter `ab85be8e...2a9e` matches the guest fixture, while the workspace
feature-unified binary is independently recorded. See [keyboard recovery](GREETER_KEYBOARD.md).
Separate cold boot passed 65.93s: automatic service start, complete login/keyboard
text stable three seconds, six identical full keyboard captures, agent visual
inspection at original resolution, same guest binary, fixture unchanged. Safe
report, images and exact observer harness retained under greeter-keyboard-*.
Final-source Debian CI 40582861536 PASSED: 954 Rust/configuration (5 default /
4 no-audio ignored), both Clippys, 96 Python/zero skips and 17 checks. Unmodified
report greeter-keyboard-ci.json matches focused/guest source18bc. Tested merge
f1cf70511be97ab3158157541d0c4fc6118c2ae6, implementation4381d58. Do not relabel
older native-login evidence as current.

Next security priority is still the independent #4 locker and the editor input
pause/acknowledgment/drain/VT handoff. Suspend remains unavailable. No live host
accounts, displays, disks, credentials or power operations were changed. Keep
#27/#4 open for remaining acceptance and integration.

## Prior checkpoint — native greeter, 2026-10-27

Work continues on `codex/native-greeter` from power checkpoint `88c2903`.
The isolated greeter shares presentation only, uses maintained greetd/PAM and an
independent bounded JSON codec, and never links the editor or GPL Rust IPC crate.
Full local baseline: 947 Rust/configuration, both Clippys, 96 Python/zero skips,
18 checks. Final cache-only display refinement: 15 greeter tests per overlapping
configuration, both workspace Clippys and build/static gates; separate exact
source record, not a relabeled full baseline. Final real guest passed in
147.06 seconds, with actual PAM rejection/success, hidden input, crash recovery,
UID-1000/1001 writing, real account picker, logout, secret scan and tty1/tty2.
Cold boot also passed stable visible-text and six complete identical framebuffer
captures, inspected by the Codex agent. Source ffd48f61…074c1 / 297 files and exact
binaries match all final focused/guest/reboot records under `docs/evidence/`.

The profile stays opt-in; no new installer or live-host configuration changed.
Debian/macOS CI 40568739789 passed implementation `e758294`, with final-source
947 Rust/configuration, 96 Python/zero skips, both Clippys and 17 CI checks.
The unmodified report is `docs/evidence/native-greeter-ci.json`; source matches
the final guest. Separate Windows 40568739805 was still running when recorded.
Next: pre-login keyboard/further
crash acceptance and independent secure locker/input pause, acknowledgment/drain
and VT/sleep handoff. Suspend stays unavailable. Read [NATIVE_GREETER](NATIVE_GREETER.md)
and current status before relying on older branch/evidence claims below.

## Latest architectural directive and working branch

Current branch is `codex/suspend-lock-guard`, based on warm-screen head `8e2aeeb`
(draft PR #33, stacked on #31). Warm-screen's real Debian/macOS CI 40466790817
passed on that exact head. New power guards address #32 and the controller-test
reproduction #34: no Suspend without a qualified locker; no shutdown/restart
with dirty buffers, stale source tree or unresolved conflicts. See
[POWER_SAFETY](POWER_SAFETY.md). Targeted offline qualification passed 17 backend,
11 Settings/controller, one retained asynchronous System test, both Clippys and
the actual four-size Settings PTY; source fingerprint 56e59e1b…8410 / 287 files.
`power-guard-targeted.json` keeps exact scope, current binary and log hashes.
Power code head `15512d4cae28dc6d160124909df71ecac72b68ab` is pushed in draft
PR #35, stacked on #33. Fresh Debian/macOS CI **40468810112 passed** with
**932 Rust/configuration** (5 default / 4 no-audio ignored), both Clippys,
**96 Python / zero skips** and **17 CI gates**. `power-guard-ci.json` is the
unmodified actual report and matches source fingerprint 56e59e1b…8410. Its tested
merge revision is e656c3b0cdfccd282f5ea536bafa45b730da1634. The separate Windows
run40468809931 was still running when recorded. Issues #32/#34 remain open for
integration; #4/#27 locking/authentication remain unimplemented.

Next: integrate the qualified safety increments through the experimental branch
stack, then implement #27's isolated native greetd/PAM greeter and #4's separate
locker/input handoff. Keep current authenticated maintenance and installer
profiles; test only disposable guest accounts until authorized live acceptance.
Do not lose #25's separately qualified installer-keyboard correction when making
a new ISO. Required full installation/encryption/recovery/physical/provider and
later writing-feature gates still prevent release.
do not label this newer source as covered by the prior warm-screen VM record.

The user supplied the Unified Operating System Shell Architectural Directive:
Grimoire evolves into the whole native WrOS interface, with modular internals.
Ordinary settings/network/power/accounts/Drive/help must stay in WrOS panes.
The greeter is separate and shares presentation only; PAM/greetd authenticate,
not the editor. A secure locker and input handoff remain separate. Active
manuscripts stay local; OAuth, mounted Drive browsing/cache, conflict-safe sync
and verified backups are distinct services. Emergency maintenance and all
existing security/data/release/hardware gates remain. Do not resume unrelated
M8 work before this new architecture is established.

The user additionally requested a blue-light filter with really warm screens.
Issue #30 now has a `codex/warm-screen` implementation based on `e31d12b`, with
1000–6500K / initial 2200K, strength/Off, original calibration, root-only libdrm
bridge, narrow polkit helper, durable confirmed preferences and independent
15-second systemd preview rollback. See [service and limits](WARM_SCREEN.md).
Twenty Python display fault tests, 16 backend tests and eight native Settings UI
tests have passed. Fresh complete qualification passed **928 Rust/configuration**
(5 default / 4 no-audio ignored), both Clippys, **96 Python / zero skips** and
**18 gates**, with unchanged source/verifier inputs. The final current-code
VM run passed in 88.51 s with source fingerprint 30691d88…c863 / 287 files and
binary SHA 911c8675…13c7c, base disk unchanged. Real virtio gamma is unsupported,
so refusal/polkit/native UI passed; no positive hardware gamma claim. Its actual
screen and report are retained under `docs/evidence/warm-screen-vm.*`.
Do not claim a theme is a screen filter or unsupported outputs are successful.

Unified-shell head `e31d12b9d30d205a6796b34c6ac848530b322ba9` has now also
passed real Debian and macOS CI run 40459742814. PR #31 remains draft; the warm
increment's qualified source is commit `22f6fcf` on `codex/warm-screen`, with its
own stacked PR/evidence. Three secret-scan findings were manually verified as
deterministic noncredential fixtures and narrowly fingerprinted in `.gitleaksignore`.
No host display/account/disk
has been accessed. Original import/main/foundation history remains preserved.

Next immediate safety fix: issue #32 reproduces the pre-existing Linux adapter
constructing an unsecured `systemctl suspend` invocation without a qualified
locker. The private rootless reproduction constructed arguments only; it never
ran a power command. Block this capability in both backend and native/legacy
Power pages, retain shutdown/restart confirmation/save behavior, add no-process
and retained-editor regressions, then resume #27/#4 greeter/locking work. Do not
mistake that guard for complete session/lid/suspend security.

Prior branch `codex/unified-shell` starts at installer checkpoint `c9e513f`.
It added the editor-independent `crates/wros-shell` library and `src/app/shell.rs`
adapter: bounded navigation/selection history, clipped shared Ratatui menus,
bounded notices, guarded version-1 per-user UI preferences and native categories.
Existing writing/interface actions and bounded system worker are reused. Return
from child panels retains the exact document/pane/cursor/selection/free-scroll/
workspace/undo objects. Unimplemented controls remain visibly unavailable.

Fresh full qualification passed: **922 Rust tests/configuration** (default 5
ignored/no-audio 4 ignored), both Clippys, **76 Python methods/zero skips** and
**18 gates**. `docs/evidence/unified-shell-software.json` has exact source/tool/
binary/runtime identities; source fingerprint
`22e7e0e4383a1d4c6842d7ee35f085d5d644c247b7f7b0c1e9491fdda8a65e9a`
contains 274 files and stayed unchanged across verification. Previous M7b
`test-results.json` was preserved; this new source is not covered by older VM/ISO
records. Tests initially caught hidden cut/undo reaching the editor under Deck
pages; all Deck controls are now guarded, with keyboard-test-field paste explicit.
The real PTY case checks caret/undo/redo/save/quit and persisted Settings selections
at four sizes; a stale parent-title barrier was corrected, not weakened.

Issues #26 shell/settings, #27 native greeter, #28 Google enrollment, #29 Drive
browsing and #30 warm screen now track the directive. Existing #4 and #10 were
updated with isolation and service distinctions. `docs/UNIFIED_SHELL.md`, roadmap,
76-row feature matrix and offline help describe implemented vs remaining work.
Final keyboard PR #25 head `b5d486b` separately passed Debian/macOS CI
`40457429492`; that code is still on `codex/installer-keyboard`, not this branch.

Next: publish/review this shell increment, qualify the combined branches through
normal merges, implement the actual warm-display service/control, then native
network enrollment and isolated greeter/session/lock work. Provider registration,
live accounts and physical disks require authorization only when actually reached.


The original goal remains a complete Writer's OS. The user resumed after a model
switch and asked to continue toward completion. Common laptop Wi-Fi support must
be bundled on installation media and in the installed system, without requiring
an initial firmware download. No physical disk operations or real account access
are authorized by this development task.

## Repository

- Repository: https://github.com/j5cfk7qm49-web/WrOS
- M7b merged through PR #15 at `268fa6310c8866178ba00a905221b13f284241a0`.
  Default development branch is now `writingdeck/m7b-wip`; foundation is retained.
- PR #20 corrected canonical/lexical source paths exposed by actual macOS CI,
  merged at `4eb1533a7668e3a1a551fb8cc0bb081a4eb24c64` after actual Debian/macOS
  run `40409883083` passed (911 Rust/configuration and 70 Python on Debian).
  Its focused 24 journal tests and Clippy pass locally; #19 is closed. The next
  increment is on `codex/installer-media`.
- PR #23 fixed a deterministic session-preview Git index timestamp race (#22),
  merged at `c67a5b167996b67f932bb68b6d4a09f92c70be36` after Debian/macOS CI
  `40445356850` passed (912 Rust/configuration, 70 Python, all 17 software gates).
  The test failed before the production fix and passes afterward; all 23 focused
  Git tests passed with external networking disabled. #22 is closed.
- Historical foundation remains `3360ec511caa9004547a9c959ac09ea2cc90ff9d` (191 commits).
- Preserved M7b handoff head is `461bb64434e524fd3a8dba8a42568b3e86839d21` (193 commits).
- Placeholder `main` remains unchanged. Import #1 was closed after independent
  remote-clone/history and source visibility verification, not just extraction.
- Continue the existing source. Do not reset historical branches or force-push.

## Work completed since recovery

- Fixed long-menu explanation clipping and obsolete tutorial assertions.
- Fixed exact-runtime materialization, reproducibility, tampering detection,
  custom output caches and verification before publication.
- Fixed Git timeout cleanup on minimal Debian using native Unix process-group
  signalling instead of depending on an external `kill` executable.
- Added verified runtime/image staging, retained Python notices and real guest
  probes for native analysis, proselint, Nabokov and WordNet.
- Added common Intel, Realtek, Qualcomm/Atheros, Broadcom/Cypress and MediaTek
  firmware, regulatory/connection tools and an actual installed-payload audit.
- Added Debian CI and extended the existing verifier to retain source, tool/CI,
  binary and runtime identities. Remote Actions has not yet run successfully.
- Added prose-safe child exit diagnostics and actual kernel CPU-budget reporting.
  Raw third-party stderr remains suppressed because it may contain prose.

Relevant commits include `0386b7f`, `28fbf7a`, `8d9aa35`, `5f889be`, `2e10fcf`,
`b632f8d`, `9cba25f`, `5999008`, `4c27f5f` and `2fa2194`. Inherited upstream
publishing and release-install checks are restricted to the upstream repository;
WrOS's own signed release pipeline is still unfinished.

## Qualification

The final completed full run passed 17 software gates, including 910 Rust tests per
configuration (overlapping, default 5 ignored / no-audio 4 ignored), both
warning-denying Clippys, and 70 Python methods with zero skips. It also passed
real model/data/notice audits, source-safe UI/PTY tests, runtime staging/tampering,
corpus, documentation, syntax and ShellCheck. The actual staged-image Wi-Fi
package/firmware/notice/module audit passed independently. Software, VM and
benchmark evidence share source fingerprint
`b23ed164a65904fe2aaf85a2534bac08c2e1bc5bffa21972690c7021f98775f5`
(263 files). Three subsequent workflow publishing guards were statically parsed
and checked separately; this is not evidence of an actual remote CI run.

The trained Python runtime is pinned to Linux amd64 / CPython 3.13 with 49 exact
packages and 4,410 payload files. NumPy declares an x86-64-v2 baseline. QEMU uses
Nehalem without AVX. Native editing/spelling do not depend on that optional model.

VM runs passed boot, non-root UI, polkit, keyboard rollback/commit and real Harper
apply/undo. Nabokov then hit its real 30-second kernel CPU limit under TCG; it
completes in roughly 2–3 seconds in native host tests. The VM probe now records
that specific denial with empty findings and unchanged source as a bounded-failure
contract, **not a clean or completed analysis**. Other errors still fail the probe.
The final VM run passed these contracts in 223.45 seconds with 512 MiB, two virtual
CPUs and no NIC. Real Harper apply/undo took 93.1 seconds under emulation; its
reported peak was 187,144 KiB. Actual positive Nabokov runs passed separately in
the native software/UI/PTY suite. No application resource limit was increased.

`docs/evidence/m7b-prose-resources.json` contains actual synthetic measurements:
4,200 words take about 40 ms native, 0.2 s proselint (~29 MiB worker), and 2.6 s
Nabokov (~224–225 MiB worker). The repeated fixture exceeds the bounded finding
queue and reports omissions. These are development/WSL results, not physical
battery, literary-corpus or production-performance qualification.

## Resume commands

Use Debian 13 amd64 with CPython 3.13 as a normal user:

```sh
cargo fetch --locked
python3 tools/prepare-prose-tools.py --include-sources
# Once the reviewed public build inputs are cached:
python3 tools/prepare-prose-tools.py --offline
bash tools/test-m7b.sh
```

Custom builds may set `CARGO_TARGET_DIR`, `GRIMOIRE_BINARY` and
`WRITINGDECK_TOOLS_PYTHON`. The runtime uses Debian's interpreter and cannot be
executed against an older host Python. Test logs are ignored under
`build/verification/`. Do not edit source/tools/CI during a verifier run; it
rejects changed inputs. VM image assembly needs the verified runtime path in
`WRITINGDECK_PROSE_RUNTIME`. VM commands and safety constraints are in
[INSTALLATION](INSTALLATION.md); [M7B](M7B.md) records tool limits.

## Installer and CI work in progress

The experimental `os/scripts/build-installer.sh` completed a Debian live-build
ISO with offline writing/runtime/WordNet and Wi-Fi firmware. Actual media/rootfs
audits and the Debian signature chain for the cached installer kernel/initrd
passed. Five new non-root media/authentication/hash-chain regressions pass.
See [INSTALLER_MEDIA](INSTALLER_MEDIA.md) for exact boundaries; #3 remains open.
The first actual no-NIC QEMU installation powered off normally after 1,016.21 s
and its installed disk reached a real tty1 login prompt. The acceptance record
remains failed because `in-target` hid earlier payload markers in syslog and
the first serial fixture did not propagate console settings after `---`.
The corrected full install/PAM/tty1 probe used a new qcow2. Its generated
test initrd contains synthetic account/disk answers; the shipping ISO does not.
Do not commit private generated acceptance artifacts.

The second install verified real payload/version markers and rebooted into PAM.
Its wrong-password rejection, UID-1000 login and refusal of passwordless sudo
were observed. The overall probe still failed safely at the sudo prompt parser:
Bash emitted a lone CR after disabling bracketed paste. The revised third run
accepts either real line-start prompt, checks tty1's actual Password prompt and
disabled echo before keyboard input, and checks the real dashboard plus installed
Wi-Fi packages. Generated fixture credentials remain private (0600 in a 0700
ignored test directory), enabling auth-only retries; successful acceptance removes
that generated secret. No real account or manuscript is used.

The third fresh install completed, then its auth-only retry passed real UEFI/PAM,
UID-1000 maintenance login, password-required sudo, actual non-root tty1 dashboard
rendering, and all six firmware packages plus nmtui/kernel modules in the guest.
See `docs/evidence/installer-vm.json` and `installed-writing-session.png`.
The 1,044.5-second installation duration excludes the subsequent auth retry.
Its private synthetic secret was removed on success. This is the original
experimental ISO/older M7b binary. Earlier failed probe records remain separate.
The revised recipe has now passed a clean native Debian build at `59cc34f`,
with current source-path/Git fixes, unchanged measured inputs and actual
signature/media/firmware audits. Exact identities are in
`docs/evidence/installer-clean-build.json`. The newer ISO's full install probe
reached payload/late-command markers but timed out at 1,200 seconds during finalization.
The original failed record is retained. Its independent post-interruption UEFI/PAM/
non-root-dashboard/Wi-Fi probe passed; see `installer-current-interrupted.json`.
This is separate boot evidence, not a successful normal installation. Actual
current-image no-disk refusal (463.62 s) and interactive cancellation (467.95 s)
also passed; `installer-safety.json` retains exact ISO/harness/disk hashes.

PR #21 is the separate installer draft. Its earlier CI `40443525255` passed macOS
but exposed #22 on Debian; the qualified correction is now merged into the draft.
Updated draft CI `40448965974` passed Debian/macOS on `bf22273`:
912 Rust/configuration, 75 Python with zero skips and all 17 software gates.
`docs/evidence/ci-40448965974.json` retains the actual artifact. The clean build
has the same source-content fingerprint; commit `59cc34f` corrects intended
OS entry-point executable modes. Current-image VM acceptance remains required.

Cancellation and no-disk probes now pass on the current ISO. The first
cancellation probe incorrectly inferred a returned menu from stale ANSI cells;
its reported pass is rejected. The final probe chooses Finish interactively,
answers choice 2, and requires newly received menu text plus unchanged whole-disk
SHA-256. Another test-only automatic-Finish loop was removed. No-disk acceptance
actually attempts finishing the empty plan and observes the installer's refusal.

Inspection also found that the installed default WrOS QWERTY configuration can
override a different keyboard selected in Debian Installer. First-install
adoption is isolated in PR #25 (`codex/installer-keyboard`, `b5d486b`). Its helper
passed 19 safety tests, all 81 Python methods offline, and real guest boot/keymap/
systemd rollback in 84.71 seconds; actual d-i selection/corrected ISO acceptance
is still pending. #24 and #3 remain open. Source schema/legacy IDs are preserved.

The user approved retiring only the completed generated test disks and temporary
inputs in two named original-ISO fixtures; that exact cleanup completed while
retaining reports/logs/screenshots. The user independently removed Docker's
separate docker_data.vhdx, freeing roughly 93 GB. WrOS uses Ubuntu WSL + isolated
Debian + QEMU, not Docker; the Ubuntu disk is intact.

The new unified-shell directive now takes priority over unrelated features:
audit the existing Grimoire navigation/services, document pre-login/authenticated/
privileged/shared-UI boundaries, update roadmap/matrix/issues, then implement a
reusable native Settings shell with a mockable backend. Separate greeter input,
credentials, cloud mounting, conflict-safe sync and verified backup. Ordinary OS
tasks must stay in WrOS panes. Existing safety and release gates remain in force.

The first real workflow run `40407371491` passed Linux and failed two macOS
journal assertions. PR #20 fixed both application behaviors; the next run
`40408744544` reached the retained recovery assertions and exposed a test-only
path-prefix assumption. Commit `881bd47` preserves those content assertions with
the loaded path spelling. Run `40409883083` passed that final correction on both
platforms.

## Next steps

1. Keep final source-aligned software, benchmark and VM evidence with the changes.
2. Finish documentation/feature inventory review and push the qualified changes.
3. Retain exact CI results for merged PR #20; do not describe the installer
   increment as covered by those earlier source hashes.
4. Finish installer VM install/first-boot/cancel/interruption/encryption evidence
   and submit the separate installer increment as a reviewed PR. Preserve full
   ancestry. Integrate placeholder main only through a reviewed merge.
5. Prioritize production installer/authentication (#3), input-safe lock/suspend
   (#4), signed updates/rollback/recovery (#5), then remaining M8/M9 and tutorial,
   branding, corpus and hardware qualification (#6–#14).

The OS remains experimental. The VM profile is intentionally passwordless and
must not be deployed to hardware. Physical Wi-Fi, keyboard, suspend/battery,
independent rights review, real backup restore and signed release acceptance
remain open. Neither passing M7b software nor a VM proves production readiness.
