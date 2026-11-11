# rc11 issue sweep — 2026-11-10

This records the current local work and GitHub status. Local rc11 changes span
an existing dirty checkout; this report does not qualify that entire checkout
as a published release.

## Published fixes

[PR 112](https://github.com/j5cfk7qm49-web/WrOS/pull/112) passed CI run
42944429988 and merged as `c74e83c01d0572838493b7348502547bd18f0837`.
It closed the documentation checker bug (#95) and missing drm-fourcc notice
(#97).

[PR 113](https://github.com/j5cfk7qm49-web/WrOS/pull/113) contains only five
import/tutorial files in commits `4599884` and `330f694`. Its exact source
passed formatting, seven import tests and all 50 tutorial launch routes at
80×24 and 136×42. CI run 42953678067 passed; it merged as
`a11e860bef1a3d6a0f9ac8dd107642f4f5cf23e2`.

[PR 114](https://github.com/j5cfk7qm49-web/WrOS/pull/114) contains the isolated
media/VM corrections (`af928e7`), policy-comment clarification (`c7081e6`) and
bounded serial-login readiness fix (`6c6c80a`). Its 65 image, media, package-pool
and harness tests initially passed, as did documentation
and shell syntax checks. The comment clarification leaves all preseed answers
unchanged. The owner authorized publication and merging after GitHub checks
and the running VM qualification pass.

The follow-up `cda9257` bounds cold VM startup and permits the exact compatibility
marker in the naming guard; `00d45ec` makes unexpected extent-query errors fail
disk verification. All 16 focused harness/naming tests and the complete
tracked-source naming scan passed. The 52 image/media/pool regressions passed
separately. The final follow-up `983e8db` waits for a ready greeter form after
authentication rejection instead of sending keys during PAM cancellation.
Both reset paths and the other focused harness/naming tests passed (17 tests).
CI run 42971231024 and installed boot passed; PR 114 merged as
`2bf44ca002d9d543079f3745f92b4d9f1294db20`. Issue #70 is closed under the
owner's revised network policy.

## Local verification

- The current local app suite passed 572 tests, with three intentionally ignored.
- The core suite passed 537 active tests, with one intentionally ignored.
- The Word CLI imported a synthetic DOCX resaved by real LibreOffice without
  losing its Unicode, inline note reference or either note paragraph.
- The documentation gate passed 17 required documents, 89 capability rows and
  the 50-lesson catalog. The Linux no-audio missing-notice count is zero.
- Native image, media and package-pool regressions passed 52 tests. The final
  installer-media changes subsequently passed all 24 media tests.
- The final 16 VM harness tests and the naming regression passed, including
  network policy, bounded login/startup readiness, greeter reset synchronization
  and extent-error rejection.

## Installer evidence

The local interim ISO is `wros-0.1.0-rc10-interim-amd64.iso`, 1,560,281,088
bytes, SHA-256
`a779222848614b282f6721ecef4e8a82224d20c65c4cb22d0b3fced5c67665e0`.
Its base commit is `6963be208931a044a52f3884b3cd89496ed10c38`, with dirty
source and explicitly accepted audit-tool drift. It predates the new Word
import and tutorial fixes. Its static media audit and both UEFI CD/USB smoke
boots passed; these are separate from installation and physical testing.

The live/cancellation run `live-20261111T031754Z` passed. It verified writing
and reopening, unchanged target bytes, the default Cancel choice, cancellation
inside the installer, retired/recreated input, storage cleanup and clean
shutdown. Earlier failed runs remain retained, including harness mistakes in
menu timing, dashboard selection and shutdown-marker handling.

An early encrypted-install attempt read a provisional outcome before final
service cleanup and remains failed. The later `install-20261111T032931Z`
passed target verification, final cleanup, credential checks and shutdown;
its sole failed assertion was the superseded ban on non-DHCP traffic. Its
original result remains unchanged. An independent installed-boot trial from
that verified target exposed the short shell-ready timeout after successful
PAM authentication; that failed result is retained. The fresh
revised-policy installation `install-20261111T040123Z` subsequently passed in
full, including final artifact verification. Its first installed-boot attempt
exposed a harness race while PAM cancelled a rejected login. After the ready-form
fix, `installed-boot-20261111T061518Z` passed all 27 assertions: wrong/correct
graphical unlock and login, Colemak persistence, receipt binary hashes,
manuscript save/reopen, maintenance/sudo, secret checks and clean shutdown.

The owner clarified #70 during this sweep: use the network when available,
including for missing drivers or packages, and fall back to the media without
Internet. Automatic NTP and update lookups are acceptable defaults. They have
not been disabled. The updated harness records attempted traffic and requires
installation to finish in a fresh network namespace without Internet links.
The supported offline base still has its package pool, matching kernel modules
and bundled firmware checked. Additional hardware may use online sources.

## Cleanup and retained evidence

Completed build trees, isolated Linux test copies, disposable VM disks,
generated test credentials and source-transfer archives were removed after
qualification. The cleanup journals account for 87,786,467,328 bytes (81.76 GiB)
of scratch allocations and archives removed, before the final small Windows
checkout cleanup. No original workspace files were reset or deleted.

One interim ISO, its receipt/checksum, build manifests/logs, smoke results and
the exact VM driver scripts remain in the Debian builder at
`/home/builder/rc11-qualified-evidence.20261111`. Full VM reports, screenshots
and packet capture remain in their run directories. The build source snapshot
and reusable toolchain/dependency caches remain. The original result records
an unknown harness commit; a sidecar retains the exact driver hashes, and all
seven retained files were verified against the merged PR source. The summary
is in [the installer qualification record](rc11-installer-vm-qualification.md).

## Remaining issues

Each issue has current GitHub notes. Open status still matters: an implemented
local change, a passing software test and acceptance on the owner's hardware
or account are separate results.

| Issue | Work or finding | Remaining acceptance |
| --- | --- | --- |
| #3 | Interim image audit, UEFI CD/USB, live writing/cancel, offline install and installed boot passed; fixes merged in PR 114. | Remaining wrong-target/interruption matrix and final current-source ISO publication. |
| #4 | Input retirement, lock/suspend and power-operation regressions verified in software/VM work. | HP Stream lid/Wi-Fi/power checks over a day. |
| #5 | Recovery/rollback VM evidence and release/signing checklist retained; weekly signing secret absent. | Owner's offline signing subkey setup and HP recovery-menu check. |
| #7 | Anchored notes, named variants, guarded restores and wide comparison work checked locally. | Integrate the remaining local changes into a reviewed release. |
| #10 | Word note-loss bug fixed; LibreOffice round trip and importer tests passed. | Live iPhone/Google Docs review-book round trip and backup-verification timing. |
| #11 | All 50 catalog lessons launch; Spark, spelling and history route fixes merged in PR 113. | Complete each lesson through real actions in an isolated demo book and verify replay. |
| #12 | Concrete plot/exercise sample and 32 closest lexical pairs screened in the local editorial report. | Independent human editorial/rights review and flagged-pair decisions. |
| #13 | Final PR 114 CI and interim-image qualification passed; receipt/audit and VM-driver corrections merged. | Complete current-source release matrix and measured physical hardware results. |
| #28 | QR/cancel VM and credential-safety work retained; phone acceptance procedure documented. | Android, Google project publishing status and a saved link after eight days. |
| #29 | User/grant-scoped cache, missing-file and safe local-copy work verified with synthetic tests. | Live whole-Drive browsing, disconnect/reconnect and offline-copy acceptance. |
| #80 | Bounded media-key event survey added to the hardware checklist. | HP key events and firmware behavior before choosing bindings. |
| #92 | Foot/fallback/XKB/lock work retained; root-only preferences repaired locally; stock Cage refusal is explicit. | A compositor implementation that applies warmth, plus HP startup/typing/power checks. |
| #107 | First-lock/warmth/wake observations specified; emulated lock evidence retained. | Identify the blackout stage and verify on the HP's Intel display. |
| #108 | Lock-stage diagnostics and delayed-preparation regression added locally. | Capture the exact refused precondition if the intermittent suspend failure recurs. |

The related local procedures are in [the hardware checklist](../HARDWARE_CHECKLIST.md),
[the release checklist](../RELEASE_CHECKLIST.md),
[sync and backup](../SYNC_AND_BACKUP.md) and
[the Spark screening](rc11-spark-screening.md).
