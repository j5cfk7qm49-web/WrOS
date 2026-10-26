# WrOS continuation — 2026-10-26

The original goal remains a complete Writer's OS. The user resumed after a model
switch and asked to continue toward completion. Common laptop Wi-Fi support must
be bundled on installation media and in the installed system, without requiring
an initial firmware download. No physical disk operations or real account access
are authorized by this development task.

## Repository

- Repository: https://github.com/j5cfk7qm49-web/WrOS
- Development: `codex/m7b-qualification`; draft PR #15 targets `writingdeck/m7b-wip`.
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

## Next steps

1. Keep final source-aligned software, benchmark and VM evidence with the changes.
2. Finish documentation/feature inventory review and push the qualified changes.
3. Bootstrap actual GitHub Actions registration: files exist on the historical
   default branch, but workflow listings/dispatch currently return empty/404.
   Preserve the historical foundation and verify actual CI before claiming green.
4. Resolve #2 and discovered defects #16–#18 only after acceptance. Keep PR review
   and full ancestry. Integrate placeholder main only through a reviewed merge.
5. Prioritize production installer/authentication (#3), input-safe lock/suspend
   (#4), signed updates/rollback/recovery (#5), then remaining M8/M9 and tutorial,
   branding, corpus and hardware qualification (#6–#14).

The OS remains experimental. The VM profile is intentionally passwordless and
must not be deployed to hardware. Physical Wi-Fi, keyboard, suspend/battery,
independent rights review, real backup restore and signed release acceptance
remain open. Neither passing M7b software nor a VM proves production readiness.
