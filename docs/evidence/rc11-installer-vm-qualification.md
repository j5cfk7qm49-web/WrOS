# Interim installer qualification — 2026-11-11

[PR 114](https://github.com/j5cfk7qm49-web/WrOS/pull/114) merged as
`2bf44ca002d9d543079f3745f92b4d9f1294db20` after
[CI run 42971231024](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/42971231024)
passed on head `983e8dba8db59908c6c435f8133e573f5b7961ad`.
Local validation passed 52 image/media/package-pool tests and 17 focused
VM/naming tests, plus the source naming scan, documentation and shell checks.

The test artifact is `wros-0.1.0-rc10-interim-amd64.iso`, 1,560,281,088 bytes,
SHA-256 `a779222848614b282f6721ecef4e8a82224d20c65c4cb22d0b3fced5c67665e0`.
It uses a dirty snapshot based on rc10 (`6963be2`) with explicitly accepted
audit-tool drift. It predates the import/tutorial fixes in PR 113. These
results qualify this interim artifact; a final current-source rc11 image
and physical hardware acceptance remain outstanding.

| Check | Result |
| --- | --- |
| Actual shipped SquashFS and ISO-bound receipt audit | Passed |
| UEFI CD and USB firmware boots | Passed |
| Live writing, reopen, refusal/cancellation, input/storage cleanup | Passed (`live-20261111T031754Z`) |
| Encrypted NIC-attached install without Internet | Passed (`install-20261111T040123Z`) |
| Installed boot | Passed, 27 assertions (`installed-boot-20261111T061518Z`) |

Installed boot rejected wrong graphical unlock/login input, accepted the correct
Colemak input, verified the selected layout and all four binary hashes against
the receipt, saved/reopened a manuscript, exercised password-protected maintenance
and sudo, checked secrets and shut down cleanly. Earlier failed attempts remain
recorded; the final greeter retry fix waits for the account form to reset after
PAM cancellation. Tests cover both a successful cancellation and greetd's
explicit restart path.

The owner permits automatic NTP/update lookups and network package/driver
retrieval. The install test captures attempted traffic while requiring completion
inside a fresh network namespace without Internet links. It does not prohibit
those defaults. [Issue 70](https://github.com/j5cfk7qm49-web/WrOS/issues/70)
is resolved under this revised policy. The remaining installer matrix and
current-source release work continue in #3/#13.

Completed build trees, isolated test sources, disposable VM disks and generated
credentials were removed. Cleanup journals account for 81.76 GiB removed before
the final small temporary checkout cleanup. One interim ISO/receipt/checksum,
build manifests/logs, smoke results and exact VM scripts remain at
`/home/builder/rc11-qualified-evidence.20261111` in the Debian builder.
Full reports, screenshots and packet capture remain in the original VM run
directories; the build source snapshot and reusable caches remain.

The original installed-boot result records an unknown harness commit. A separate
`qualification-source.json` retains the exact driver files and hashes; all seven
files matched the merged PR source. Small result/cleanup reports and the source
comparison are also retained locally under `.audit-tools/rc11-qualification/`.
