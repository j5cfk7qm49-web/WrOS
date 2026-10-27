# WrOS continuation — 2026-10-26

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
`docs/evidence/installer-clean-build.json`. That newer ISO's fresh no-NIC install
is still running; do not attribute the older passed VM record to it.

PR #21 is the separate installer draft. Its earlier CI `40443525255` passed macOS
but exposed #22 on Debian; the qualified correction is now merged into the draft.
Updated draft CI `40448965974` passed Debian/macOS on `bf22273`:
912 Rust/configuration, 75 Python with zero skips and all 17 software gates.
`docs/evidence/ci-40448965974.json` retains the actual artifact. The clean build
has the same source-content fingerprint; commit `59cc34f` corrects intended
OS entry-point executable modes. Current-image VM acceptance remains required.

Cancellation and no-disk probes are under development. The first cancellation
probe incorrectly inferred a returned menu from stale ANSI cells; actual serial
output showed that the text frontend requires numbered answers. The probe is
being corrected to answer choice 2 and require newly received partition-menu
text, as well as an unchanged whole-target SHA-256. Its earlier reported pass
is invalid. No-disk acceptance must attempt finishing the empty plan and observe
an actual installer refusal. Neither safety case is qualified yet.

Inspection also found that the installed default WrOS QWERTY configuration can
override a different keyboard selected in Debian Installer. First-install
adoption needs an explicit, tested integration that preserves committed layouts
on upgrades and never silently substitutes QWERTY for an unsupported layout.
Do not close #3 before this and remaining safety/recovery gates pass.

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
