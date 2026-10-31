# Status — WrOS (Writer’s OS) 0.1.0-dev, through M7b software

## Missing shared-registry guard (2026-10-30)

[Story identity checks](STORY_IDENTITIES.md) now stop missing/incomplete
registries from assigning historical story IDs to another scene. The previous
wrong-scene restore was independently reproduced in a disposable compiled-app
fixture. Core checks and actual four-size refusal, ordinary edit/save/Undo/help
and original-registry recovery passed. Story metadata and source remain intact;
no scene-order/title/text matching rebuilds the mapping. Full applied-source
Debian qualification is in progress. Broader writing and recovery work remain open.


## Reviewed draft and passage restore (2026-10-30)

[Draft restore](PASSAGE_RESTORE.md) now binds the exact reviewed saved version,
fixing the independently reproduced second-entry/first-restore bug. One changed
passage can be reviewed in shared panels and restored after explicit selection
from default Cancel. Existing history diff, recovery variants, dated snapshots,
original editor Undo and guarded source/metadata persistence remain in use.
Focused core/UI and real four-size milestone/variant/help/Undo/restart tests
passed; the [independent full Debian run](evidence/passage-restore-local.json) passed
1072 Rust tests per configuration, 184 Python tests with zero skips, both Clippys and 21 gates. [Provenance](evidence/passage-restore-local-provenance.json) binds the tested source and binary.
Character knowledge, broader preflight, paperback notes and full M8/M9 remain open.


## Native EPUB notes and third Argon review (2026-10-30)

The [third contribution review](ARGON_THIRD_REVIEW_2026-10-30.md) retains the
current shared identity and Word/PDF implementation. Its two writing regressions
are integrated with corrections to the milestone fixture and real guarded APIs.
[Native EPUB footnotes](EPUB_FOOTNOTES.md) now use the same public-source parser,
the maintained HTML renderer, same-file note/return links and preserved paragraphs.
Actual EPUBCheck, four-size edit/export/help/Undo/restart, Word/PDF pagination
regressions and strict no-audio Clippy passed. The [independent full Debian run](evidence/epub-footnotes-local.json) passed
1065 Rust tests per configuration, 182 Python tests with zero skips, both Clippys and 21 gates. [Provenance](evidence/epub-footnotes-local-provenance.json) binds its source and binary. Paperback notes, broader preflight, full M8/M9 and OS/release gates
remain open; earlier paragraphs retain their historical scope.

## Manuscript page footnotes (2026-10-30)

[Manuscript Word/PDF notes](MANUSCRIPT_FOOTNOTES.md) now have real OOXML parts
and page-bottom rendering through the existing LibreOffice adapter. Focused
package/refusal, actual PDF pagination and four-size terminal checks passed.
Lesson 34 requires the actual completed Word/PDF workflow. The
[independent Debian run](evidence/manuscript-footnotes-local.json) passed
1060 Rust tests per configuration, 180 Python tests with zero skips, both Clippys and 21 gates. EPUB/paperback notes and wider M9 preflight,
backup/sync, installation and physical qualification remain open. Historical
footnote limitations below describe their earlier checkpoints.

## Reviewed Argon writing integration (2026-10-30)

The remote `development` branch was synchronized before reviewing the supplied
ZIP and patch. The [contribution review](ARGON_REVIEW_2026-10-30.md) records
input hashes, repairs and parts that remain unfinished. Continuity, durable
milestone/variant restore, chapter read-through and local file comparison are
qualified by the [independent Debian run](evidence/argon-writing-local.json):
1057 Rust tests per configuration, 177 Python tests with zero skips, both Clippys and 21 gates. Genuine rich footnotes, maintained backup/sync, protected Wi-Fi
enrollment, signed update application and authenticated suspend remain open.

## Export selection review

The export dialog can review the selected loaded source before writing: it
inspects footnote diagnostics and public editorial markers, and reports the
existing rich-footnote export block. This is a bounded snapshot, not a link,
continuity, asset, revision or rendered-page audit. The [independent Debian
software run](evidence/export-preflight-local.json) passed 1047 Rust tests per configuration, 175 Python tests with zero skips, both Clippys and 21 gates.
[Provenance](evidence/export-preflight-local-provenance.json) and four terminal
captures retain its source and binary binding. Printed page footnotes and the
rich-export guard remain unchanged.

## Contextual footnote workflow

Post-merge CI for PR #60 passed in run40972331871. Development now adds a
caret-aware public footnote dock, full bounded definition reading, repeated
reference selection and original-source editing. Parser/UI and actual terminal
qualification passed in the [independent Debian run](evidence/footnote-context-local.json):
1044 Rust tests per configuration, 174 Python tests with zero skips, both Clippys and 21 gates. [Provenance](evidence/footnote-context-local-provenance.json)
binds the source and binary; four captured terminal sizes accompany the receipt.
The [first local run](evidence/footnote-context-local-initial-failure.json)
failed an installer prompt synchronization test; a distinct fresh prompt fixed
the test, and the full suite was rerun. Printed DOCX/PDF page footnotes and
installer qualification remain open; the existing rich-export refusal is retained.

## Writing workflow integration on development (2026-10-29)

Remote PRs #54, #55, #57 and #58 were audited and retained. PR #59 was
independently corrected, qualified and merged as `82ede406`: source ownership
and backing-file checks now guard insertion, and the maintained parser must
recognize both the reference and its appended definition. Its revised
[Debian CI](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40916740650)
passed; independent [local evidence](evidence/footnote-audit-local.json) records
1022 Rust tests per configuration, 171 Python tests with zero skips, both
Clippys and 21 gates. This receipt covers the footnote audit source only.

`development` includes those changes and a local revision-note workflow with
stable document IDs, guarded metadata history, explicit re-anchoring and
revision-bound resolution. Owned rename/reorder/trash/restore preserves IDs;
external or interrupted moves require explicit reconnection. Footnote browsing
now guards stale source positions, shares wros-shell widgets, and jumps to
original definitions for normal editing. Lessons 32 and 35 open their actual
workflows. Independent [full Debian qualification](evidence/development-writing-local.json)
passed **1037 Rust tests/configuration, 173 Python/zero skips, both Clippys and
21 gates** on source `9a734ebd...26a20` / 450 files. [Provenance and four review
frames](evidence/development-writing-local-provenance.json) bind the result to
the tested binary. The earlier documentation-inventory failure is retained
separately; the corrected run passed all gates.

Genuine DOCX/PDF page footnotes, full M8/M9, OS image refresh, encrypted
installation/authenticated reboot and physical hardware remain unqualified.
The existing local voice adapters and scene-history engine remain in use.

## Native feature contribution reviewed and qualified in Debian CI

The feature contribution is preserved and [reviewed](FEATURE_REVIEW.md). The
workspace compiles again. Native read-only Wi-Fi/profile, local-account,
filesystem and cached package-plan views now use the shared Grimoire interface.
Unimplemented cloud/backup/update/recovery actions report unavailable; fake
Google/Drive success and password arguments have been removed. New story models
have guarded persistence and Unicode/Markdown corrections, with their missing
editor, export and speech integration explicitly retained as open work.

[Debian CI 40816807534](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40816807534)
passed **1008 Rust tests/configuration, 170 Python/zero skips, both Clippys and
20 gates**. [Artifact provenance](evidence/native-feature-ci-provenance.json)
binds the tested merge to source `815ecb26...ce352` / 439 files. The native
availability workflow passed at four terminal sizes with retained manuscript,
caret and undo and no fabricated account file. This is not a completed M8/M9
or a new live image.

The independent [local qualification](evidence/native-feature-local.json) passed
the same test sets and **21 gates**. [Provenance and four actual terminal frames](evidence/native-feature-local-provenance.json)
retain the binary/source binding. The corrected runner uses normal signal
defaults; its earlier failed run remains separate. [Draft PR 52](https://github.com/j5cfk7qm49-web/WrOS/pull/52)
contains this environment increment.

## Environment first; installer composition checkpoint retained

The current priority is the everyday native writing environment and remaining
writing workflows. Installer qualification resumes when those contents settle.
The [composition checkpoint](evidence/native-offline-provider-checkpoint.json)
retains the refreshed live image, its verified 401-archive capsule and maintained
Debian CD-ROM APT provider. All seven encryption/EFI runtime packages have
matching offline archives. One automatic diskless live boot passed in **94.99
seconds**; [actual Home](evidence/native-offline-provider-home.png) is retained.

[Debian CI 40796270591](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40796270591)
passed **973 Rust tests/configuration, 169 Python/zero skips, both Clippys and
20 gates**. [Provenance](evidence/native-offline-provider-ci-provenance.json)
binds source `1bdd9a39...dc484` / 431 files to the image/backend checkpoint.
This evidence predates the separate new feature changes under review.
Full encrypted installation/authenticated boot, production handoff and owned
post-write cleanup remain open. No physical hardware has been qualified.

## Native encrypted target installation remains unqualified

The guarded native fixture now attempts a complete installation on its own fresh
virtual disk and requires a separate encrypted UEFI boot and authenticated
Grimoire entry. Fourteen focused policy/identity/actual-root ancestry checks
pass. Earlier fixture and passphrase-adapter CI each passed 973 Rust tests per
configuration, 168 Python tests/zero skips, both Clippys and 20 gates; their
receipts keep their original source identities.

The first actual attempt stopped at Debian's frontend-specific entropy-plugin
request. The next reached hidden passphrase entry and system copy, then was
explicitly stopped during APT setup after a source audit found a media-rebind
flaw. Both remain failed receipts. A single overlay media view now tests Debian's
nonrecursive bind before native input. The later full-copy attempt exposed APT
provider/archive composition faults. The corrected build and software CI passed;
another full installation retry is deferred while the environment is completed.
This adds no production root launcher or completed encrypted
installation claim. Read the [scope and retained attempts](NATIVE_ENCRYPTED_INSTALL.md).

## Offline encryption runtime and refreshed live image qualified

The refreshed live image now contains Debian's complete encryption runtime
and six matching offline archives. Its [actual media audit](evidence/encrypted-media-image-build.json)
passes installed-state, runtime-byte, package-version, firmware and native
login-policy checks. The [first image's missing archive failure](evidence/encrypted-media-first-build.json)
is retained. Maintained live-build binary package lists now populate the pool.

[Current Debian CI 40780133313](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40780133313)
passed **973 Rust/configuration, 165 Python/zero skips, both Clippys and 20 gates**.
[Artifact provenance](evidence/encrypted-media-ci-provenance.json) matches image
and guest source `d4c4772d...7a0d7c` / 428 files. The earlier full local checkpoint
retains its own source; it is not relabelled current.

[Three actual live modes](evidence/encrypted-media-guests.json) passed automatic
entry, manual authentication and unmarked fallback in **100.79 / 126.46 / 116.37
seconds**. The ISO's shipped native frontend also passed disk-write refusal in
**484.97 seconds**, returning to partitioning with the complete new **16 GiB**
disk unchanged and both processes/private mounts retired. No frontend override,
host disk or network was used. Actual [Home](evidence/encrypted-media-home.png)
and [write-question](evidence/encrypted-media-review.png) captures are retained.

This is image composition, live-role and refusal acceptance. Full encrypted
target installation/authenticated boot, owned interruption cleanup, production
installer handoff/completion UI and physical qualification remain open. Read the
[encrypted-media scope](ENCRYPTED_INSTALLER_MEDIA.md). [Draft PR 50](https://github.com/j5cfk7qm49-web/WrOS/pull/50)
retains this increment.

## Native disk-write refusal passed in the Grimoire TUI

Debian's real partitioner now reaches its LVM disk-write question in the native
Grimoire interface. The [no-NIC guest](evidence/native-target-review-vm.json)
verified No selected, declined the write and returned to partitioning. The
complete newly generated **16 GiB disk stayed byte-identical**. Service stop
retired both processes and private mounts before poweroff; **478.14 seconds**.
The [actual question](evidence/native-target-review.png) and
[returned overview](evidence/native-target-declined.png) are retained.

The [full local software run](evidence/native-target-local.json) passed **973
Rust/configuration, 162 Python/zero skips, both Clippys and 21 gates**. The
[provenance](evidence/native-target-local-provenance.json) binds source
`8128d68d...4fa26` / 427 files and the exact compiled guest frontend. A passive
500-message measurement improved from 7.8773 to 0.3667 seconds after removing
per-message keyboard delays; hidden-password, raw-input and typeahead checks
pass. This is transport timing, not installation throughput. Original failed
attempts remain separate.

[Draft PR 49](https://github.com/j5cfk7qm49-web/WrOS/pull/49) retains this increment;
[Debian CI 40757537144](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40757537144)
passed [973 Rust/configuration, 162 Python/zero skips, both Clippys and 20 gates](evidence/native-target-ci.json).
The [artifact provenance](evidence/native-target-ci-provenance.json) matches
local/guest source and verifier inputs, retaining tested merge `639a8b3d...007db`. No new live image or production root launcher is
claimed. Encrypted target installation/authenticated boot, owned interruption
cleanup, production handoff, refreshed media and physical qualification remain
open. Read the [target-review scope](NATIVE_INSTALLER_TARGET.md).

## Authenticated installer capsule — local software, diskless native menu and Debian CI passed

The matching Debian backend and 401-package offline set now compose into a
verified regular-file capsule. Original signed Debian metadata, all archive
hashes and upstream licenses are retained. The native private media view keeps
the live kernel and initrd matched, excludes the GTK preference, preserves the
ordinary binary pool and checks the exact paths Debian reads.

The [full local run](evidence/native-engine-local.json) passed **973
Rust/configuration, 153 Python/zero skips, both Clippys and 21 gates**.
[Eighteen focused checks](evidence/native-engine-focused.json), the
[actual capsule](evidence/native-capsule-build.json) and the
[diskless no-NIC guest](evidence/native-engine-menu-vm.json) passed. In **122.03
seconds**, Debian's actual startup/menu showed actionable language and location
forms in the non-root shared Grimoire TUI; stopping its standard temporary
service retired both processes and private backend mounts before poweroff.
[Provenance](evidence/native-engine-local-provenance.json) binds source
`41f5d815...b1da7` / 425 files, reports and the observed frontend executable.
Actual [language](evidence/native-engine-language.png) and
[location](evidence/native-engine-location.png) captures are retained alongside
[original failed observations](evidence/native-engine-attempts.json).

This is a guarded VM fixture using a separate read-only QA medium. The delivered
live ISO has not been rebuilt with the capsule, and no production root launcher
is installed. Trusted editor handoff, disk review/LUKS cleanup, fresh target
installation/reboot, UEFI/Secure Boot, sleep and physical qualification remain
open. [Debian CI 40740207303](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40740207303)
passed [973 Rust/configuration, 153 Python/zero skips, both Clippys and 20 gates](evidence/native-engine-ci.json).
Its [verified artifact provenance](evidence/native-engine-ci-provenance.json)
matches local/guest source and verifier inputs, retaining tested merge
`dabf179afc0af76571a09cbf9f12e9de9f5c0a62`. The extra local gate audits the
existing Wi-Fi staging payload. [Draft PR 48](https://github.com/j5cfk7qm49-web/WrOS/pull/48)
retains this increment.
Read the [backend integration boundaries](NATIVE_INSTALLER_ENGINE.md).

## Live Grimoire session — image, local software, guest and Debian CI passed

`codex/live-session` delivers a new development live ISO with the actual
Grimoire home and shared Settings surfaces. Maintained Debian live-config and
user-setup create the public temporary account only in the live overlay; the
immutable installed login configuration stays authenticated. Normal and
failsafe live entries carry the WrOS selection, and live-medium recognition
keeps the public role visible even when that selection is omitted.

The [final local run](evidence/live-session-local.json) passed **973
Rust/configuration, 135 Python/zero skips, both Clippys and 21 gates**.
The [actual compressed ISO audit](evidence/live-session-image.json) verifies
payload bytes, locked root, absence of the temporary account/runtime/sudoers,
and all live boot entries. The
[automatic](evidence/live-session-auto-vm.json),
[manual](evidence/live-session-manual-vm.json) and
[omitted-flag](evidence/live-session-unmarked-vm.json) diskless, no-NIC guests
passed native home/Settings/power screens, non-root process identity and clean
shutdown in **94.12, 181.20 and 181.24 seconds**. Public live sessions keep
personal locking unavailable.

The separate [authenticated lock regression](evidence/live-session-normal-lock-vm.json)
passed **347.59 seconds**, including original writing context, real PAM,
protected tty8 and process-death recovery. [Provenance](evidence/live-session-local-provenance.json)
binds current source `253d7f0a...93575` / 420 files, software, guest reports and
executables. Stripping the exact tested editor reproduces the shipped binary.
The ISO was built at `55aa13e` / source `67844e12...518a1`; only the VM test driver
changed afterward, and all runtime sources and compressed payloads were checked
unchanged. Earlier [failed observations](evidence/live-session-attempts.json)
remain failed under their original source.

The [development artifact receipt](evidence/live-session-release.json) records
**1,221,148,672 bytes**, SHA-256 `531efcbd...3f4933e6` and its local filename.
Actual [home](evidence/live-session-auto-live-home.png),
[login](evidence/live-session-manual-manual-login.png) and
[live power](evidence/live-session-unmarked-live-power.png) captures are retained.
Read the [profile and limits](LIVE_SESSION.md).

[Debian CI 40716348352](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40716348352)
passed [973 Rust/configuration, 135 Python/zero skips, both Clippys and 20 gates](evidence/live-session-ci.json).
The [verified artifact receipt](evidence/live-session-ci-provenance.json) matches
local and guest qualification source; tested merge is `f6e87cbdf47d3ef757654290e0412db01e119e1a`.
The extra local gate audits the existing staged Wi-Fi image.
[Draft PR 47](https://github.com/j5cfk7qm49-web/WrOS/pull/47) retains this increment.
The privileged native installer, complete matching offline backend integration,
fresh target install/LUKS/reboot, UEFI/Secure Boot, sleep and physical hardware
qualification remain open. This increment qualifies live startup and its shared
interface, not the completed OS.

## Terminal input readiness — local software, guest and Debian CI passed

`codex/terminal-readiness` retains the locked Crossterm parser/API with an
explicit Linux readiness patch. The earlier 3000-byte raw burst stalled after
the first read; the corrected implementation preserves pending readability,
checks byte availability before reading blocking stdin and keeps descriptor
flags unchanged. The original MIT crate and 77-file hash inventory are retained;
only two files differ, with an offline reverse-patch audit required in CI.

The [full local run](evidence/terminal-input-local.json) passed **972
Rust/configuration, 124 Python/zero skips, both Clippys and 21 gates**. Actual
four-size native PTYs cover long hidden ASCII/Unicode input and clearing
post-submit typeahead before another confirmation. The maintained Debian
[component fixture](evidence/terminal-input-components-frontend.json) and
[actual image backend](evidence/terminal-image-backend-frontend.json) each passed
13 native checks; both passed 14 protocol cases. [Original exploratory failures](evidence/terminal-input-observations.json)
remain separate, including the first blocking-stdin prototype.

The [fresh no-NIC COW guest](evidence/terminal-input-vm.json) passed in **228.09
seconds** with the exact new editor/greeter binaries: real PAM, protected tty8,
credential hiding, original editor context, fresh readers and editor/locker/
client/worker-death recovery. The base image and source stayed unchanged.
[Provenance](evidence/terminal-input-local-provenance.json) binds source
`4dfdd630...e167c` / 413 files, actual binaries, source/patch audit and captures.
[Debian CI 40687062920](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40687062920)
passed [972 Rust/configuration, 124 Python/zero skips, both Clippys and 20 gates](evidence/terminal-input-ci.json).
Its [artifact provenance](evidence/terminal-input-ci-provenance.json) matches the
local/guest/image source; tested merge is `8a5119cfb73c3fe30ba109adf0b12d442764c961`.
The extra local gate audits the existing staged Wi-Fi image. Read the
[input boundaries](TERMINAL_INPUT.md).

This does not deliver a new live ISO, native lock presentation, privileged live
launcher, LUKS installation, suspend/resume or physical qualification.

## Matching live-installer image components — local build/backend and Debian CI passed

`codex/live-backend-image` adds a retained non-root recipe over Debian's installer
build. It uses the maintained Debian Security `6.12.113+deb13-amd64` kernel and
udebs, and packages the qualified native passthrough transport as one local udeb.
The [actual component build](evidence/live-image-build.json) passed: 103 udeb
hashes retained, output kernel bytes equal the signed Debian package, and the
exact module verified inside the compressed initrd. Source is
`0857c7d7...a9455` / 333 files. This resolves the old ISO's live/installer kernel
mismatch for the new components; it does not update that delivered ISO.

The image-assembled Debian backend passed
[12 native form checks](evidence/live-image-backend-frontend.json) and
[14 protocol cases](evidence/live-image-backend-faults.json) with synthetic
no-network/noexec-RAM fixtures. [Provenance](evidence/live-image-local-provenance.json)
binds source, recipe and drivers; [original failures](evidence/live-image-attempts.json)
remain failed.
[Debian CI 40673836623](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40673836623)
passed [972 Rust/configuration, 123 Python/zero skips, both Clippys and 19 gates](evidence/live-image-ci.json).
Its [verified artifact provenance](evidence/live-image-ci-provenance.json) matches
the local source and exact module; tested merge is
`3ee24e819de4558327b9047f6174802c00ac71ed`. The actual initrd build and its image
backend checks are separate local qualification; CI does not repeat that build.

Read the [component recipe and live boundaries](LIVE_INSTALLER_IMAGE.md).
Root supervision, live-only startup, complete matching offline media, disk/LUKS
review and fresh live install/reboot qualification remain open. No new complete
ISO or disk-writing grant is delivered by this increment.

## Installer transport failure handling — local software and Debian CI passed

`codex/installer-fail-closed` makes the maintained Debian cdebconf passthrough
abort on a lost, truncated or invalid frontend response. Actual tests reproduced
the earlier backend continuing with its original confirmation default; that
[failing baseline](evidence/installer-faults-baseline.json) remains failed. The
native TUI also clears an old answer when Debian presents an empty default.

The [full local run](evidence/installer-closed-local.json) passed **972
Rust/configuration, 123 Python/zero skips, both Clippys and 20 gates**. Required
component gates build the hash-pinned maintained backend and run
[12 native frontend checks](evidence/installer-closed-components-frontend.json)
and [14 protocol cases](evidence/installer-closed-components-faults.json) as a
normal user with private RAM fixtures. The separate
[packaged Debian backend](evidence/installer-closed-components-package.json)
also passed the native checks. [Provenance](evidence/installer-closed-local-provenance.json)
binds source `2f2c9211...15fc3` / 331 files, the built module and retained text
captures. The [original full-run observer failure](evidence/installer-closed-attempts.json)
is preserved separately.
[Debian CI 40668541104](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40668541104)
passed [972 Rust/configuration, 123 Python/zero skips, both Clippys and 19 gates](evidence/installer-closed-ci.json),
including both actual Debian component gates. [CI provenance](evidence/installer-closed-ci-provenance.json)
matches the local source and records tested merge `a1980a13d2bd3f4e05d20915f1ea222c1438c43e`.
The [original APT provisioning failure](evidence/installer-closed-ci-attempts.json)
remains failed. The extra local gate audits the existing staged Wi-Fi image.

Read the [backend boundary](INSTALLER_BACKEND.md). Root launch supervision, live
media staging and fresh install/reboot qualification remain open. No new ISO or
disk-writing permission is delivered by this increment.

## Shared Grimoire themes and native installer frontend — software and Debian CI passed

`codex/live-installer` extracts Grimoire's original palette and theme format into
the shared presentation crate. Settings, login/layout recovery and the new
installer use that same code; the installer also reads the saved Grimoire theme.
Its native forms, choices, progress and help passed
[eleven focused gates](evidence/native-installer-focused.json), including the
retained editor/Settings PTY and theme round trips. The
[eleven actual Debian cdebconf checks](evidence/native-installer-backend.json)
also passed with a retained protocol patch and pinned upstream build recipe.
[Debian CI 40652566144](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40652566144)
passed [971 Rust/configuration, 123 Python/zero skips, both Clippys and 17 gates](evidence/native-installer-ci.json).
Its source matches the focused/backend record; [provenance](evidence/native-installer-ci-provenance.json)
binds implementation `3514b4a` to tested merge `120bd335e52e711f1c73d47e474798ba370e4896`.
Read the [implementation boundaries and remaining live integration](NATIVE_INSTALLER.md).

This is not yet a complete live installer or a new ISO. The privileged launcher,
backend-death supervision, media staging and fresh install/reboot qualification
remain open. Earlier source-bound evidence below retains its original scope.

## Manual console locker — software, guest and Debian CI passed

`codex/console-lock` integrates Debian openvt/vlock/PAM on reserved root tty8.
The editor saves both buffers, retires its reader, clears tty1 and waits without
rendering or terminal queries. Unlock success requires observed VT ownership,
echo/SysRq suppression and maintained-child success before a fresh parser starts.
The root helper validates the installed caller, logind session, account, exact
policy and reserved console. Failure closes the writing session without resuming
its retired reader. See [manual lock boundaries](CONSOLE_LOCK.md).

[Eleven focused gates](evidence/console-lock-focused.json) passed: seven pure
privilege/command/role checks, staged-image mutations, four-size controller and
retained editor PTY coverage, no-audio Clippy/build and static checks. The
[fresh no-NIC guest](evidence/console-lock-vm.json) passed in **239.11 seconds**:
actual PID-1 system-service ownership, protected tty8, no editor reader or
terminal activity, rejected/correct PAM, hidden credentials, a fresh reader,
same-editor caret restoration and five lock cycles. Editor, locker, client and
service-worker death all led safely to login; original saved bytes and SysRq
restoration passed. The base image and source stayed unchanged.

Source `73720477d334e13ca767304aa5384ecfe0b1f102c72d4dc82782d4f5eab19843`
/ 320 files and exact binaries/policies match the archived guest fixture. The
[eleven original failed attempts](evidence/console-lock-attempts.json) remain
failed, including fixture-control problems and the real session-lifecycle
defect. Maintained systemd lifecycle handling replaces the larger unqualified
request/path-service prototype. [Debian CI 40641387973](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40641387973) passed
[964 Rust/configuration, 120 Python/zero skips, both Clippys and 17 gates](evidence/console-lock-ci.json).
Its source matches the focused/guest record; [provenance](evidence/console-lock-ci-provenance.json)
records implementation `3c8612b` and tested merge `a24860420d193c0d0389aa68a324b62c4304a237`. Physical security and
lock-before-suspend/resume remain unqualified; sleep stays disabled. The
delivered native ISO below predates manual locking and timezone selection.

## Debian-first, TUI-first reuse

The [component decisions](FOSS_REUSE.md) and [63-candidate inventory](foss-reuse.json)
record 60 available Debian packages and the selected backends for remaining OS
work. The target installer is a live WrOS session with its own TUI over Debian's
installation components. The maintained launcher text path and kernel-matching
condition were inspected; the new native installer frontend has software and
Debian CI qualification, while privileged live integration remains open.
[Packaged Restic's local backend test](evidence/restic-backend-fit.json) passed
encrypted versioned snapshots, full-data integrity, wrong-password rejection and
byte-exact restore into a separate directory without changing originals. This
does not claim the native backup interface or live cloud integration is done.

## Native timezone selection — software, guest and Debian CI passed

`codex/native-timezone` / [draft PR #41](https://github.com/j5cfk7qm49-web/WrOS/pull/41)
adds local catalog search, Cancel-default confirmation, a narrow active/local
timedated permission and matching readback before success. Status also reads the
service rather than a potentially stale `/etc/timezone`. Failed requests retain
the choice and editor context. Clock, RTC and NTP control are not granted.
Read [native timezone boundaries and evidence](NATIVE_TIMEZONE.md).

[Ten focused gates](evidence/native-timezone-focused.json) passed: 20 backend
tests, the new four-size controller case, workspace Clippy/build, retained
four-size Settings/cursor/undo/restart PTY and static gates. The [no-NIC guest](evidence/native-timezone-vm.json)
passed in 480.58 seconds with actual search/cancel/apply/readback, inactive and
unrelated policy denial and retained login/keyboard/accounts/input/fault/secret
checks. Source and exact local binaries match the archived fixture; base unchanged.
[Debian CI 40614744076](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40614744076)
passed implementation `c88c66c`: [963 Rust/configuration, 108 Python/zero skips,
both Clippys and 17 gates](evidence/native-timezone-ci.json). Source matches
`3c4f44b61749cfaf29ca15902ac04b73b2ff54964fd1741eadc2d648a8a51569` / 311 files;
tested merge `139e2c4dba96dbbc0e4c985469f72c387b34d111`. No new timezone ISO or
physical claim is made; the integrated image below predates these controls.

## Integrated native-login image — normal installation and installed boot passed

`codex/integrated-installer` / [draft PR #40](https://github.com/j5cfk7qm49-web/WrOS/pull/40)
brings the current editor/input reader, native greetd/PAM login, Settings and the
separate installer-keyboard fix into one offline Debian image. Staging also
ignores automatic lid/idle/power/sleep-key actions, disables the four sleep modes
and masks sleep targets while secure locking remains unqualified.

The [clean native build](evidence/integrated-installer-build.json) passed its
authenticated Debian input chain and actual firmware/login/PAM/account/service/
power-policy audits. The ISO is 1,219,952,640 bytes, SHA-256
`cdb2be91b244e7fa9c6465c0ab0c538044f08cc00a2afe2cadf9d6204665d8ff`.
[Debian CI 40611457588](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40611457588)
passed [959 Rust/configuration, both Clippys, 108 Python/zero skips and 17 gates](evidence/integrated-installer-ci.json).
Its source matches the build exactly: `97e9afe2c17c7c6028ab40ffe66fed06c2950e1ab942ea091052e3bce41d2741`
/ 309 files; implementation is `9e7d99c`, tested merge `4b821f15b9e8416a8c6c4326a1d113ac1d2225fb`.

The [fresh no-NIC installation](evidence/integrated-installer-vm.json) completed
normally in 1,263.38 seconds. A separate 97.76-second authenticated-boot retry
passed UEFI boot, native wrong/right PAM, the actual UID-1000 editor and reader,
tty1 device binding, running/installed executable hashes, offline Wi-Fi payload,
all five sleep masks, logind denial (`no`) and logout to login without readers.
No sleep request was issued. Four [original verifier failures](evidence/integrated-installer-attempts.json)
remain failed: missing journal telemetry, controlling-TTY accounting rather than
the actual descriptor, mixed console line endings and expecting only one of the
documented denied capability values. Five parser regressions cover framing and
capability responses. ISO/runtime inputs stayed unchanged; the report separately
records the updated authentication verifier's identity. Its full [Debian CI
40619901511](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40619901511) passed
[959 Rust/configuration, 113 Python/zero skips, both Clippys and 17 gates](evidence/integrated-installer-verifier-ci.json).
Verifier source `96569207...1cc2eeb` / 310 matches the actual boot probe; tested
merge is `1231c581d0bb69768a738837f2f63352accab951`.
The [actual complete dashboard](evidence/integrated-installed-writing.png) has
[capture provenance](evidence/integrated-installed-writing.json) from the initial
probe and was inspected at original resolution. This is not LUKS, recovery,
secure lock/suspend or physical qualification.
The later [native timezone increment](https://github.com/j5cfk7qm49-web/WrOS/pull/41)
is separately tested and is not contained in this ISO. Older image evidence below
retains its original source identity.

## Editor input ownership — focused, full Debian CI and guest qualification passed

`codex/input-ownership` / [draft PR #39](https://github.com/j5cfk7qm49-web/WrOS/pull/39)
replaces Linux's blocking editor input thread with an unprivileged re-exec reader
in the same installed binary. The independent MIT `wros-input` crate provides
reader retirement/reaping, queued/partial-input cleanup, kernel typeahead flush,
an opaque single-use pause proof and explicit fresh-parser resume. Reader death
triggers the existing two-buffer save/rescue path; editor death kills its reader.
There is no new credential prompt or locker. See [input ownership](INPUT_OWNERSHIP.md).

[Focused checks](evidence/input-ownership-focused.json) passed four unit tests,
one thread-affinity compile-fail test and four real private-PTY scenarios in both
configurations, both workspace Clippys/build, three actual editor fault/context
tests and the retained four-size Settings/cursor/undo/restart PTY. All 11 gates
passed without external networking. Source is
`0f668f5d8bb5e757138060d0321c2bd50bfa5ba161b2655bac8e1ad5f8e17aba` / 304 files.
[Real guest integration](evidence/input-ownership-vm.json) passed in **376.22 seconds**:
actual reader UID/parent checks, writer SIGKILL and normal logout without orphan
readers, PAM/account switching, keyboard recovery and the secret scan. The no-NIC
base stayed unchanged, and focused/guest binaries match the archived fixture.
[Debian CI 40601108193](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40601108193)
passed implementation `930ee9e`: [959 Rust/configuration, both Clippys, 99 Python
tests/zero skips and 17 gates](evidence/input-ownership-ci.json), plus the four
standalone ownership scenarios in each configuration. Its source matches;
tested merge is `25c2b9fd1b7b132f5f966c94c47fe9e6a0b30f95`. The older keyboard/login
reports below retain their original source identities.

Protected seat/VT ownership, an independent maintained locker, stopping all
editor rendering/terminal queries during handoff, verified unlock and an
authoritative lock-ready acknowledgment before sleep remain issue #4. Suspend
is unavailable; no host terminal, account, credential or power state was changed.

## Login keyboard recovery — software, real guest and Debian CI passed

`codex/greeter-keyboard` / [draft PR #38](https://github.com/j5cfk7qm49-web/WrOS/pull/38)
adds F2 recovery before PAM, all four supported console layouts, explicit
preview/keep/revert, timed expiry and crash-surviving rollback. Unresolved helper
state blocks authentication; typeahead is discarded at transitions. A separate
staged policy grants only the active local greeter's narrow keyboard action.
Read [keyboard recovery](GREETER_KEYBOARD.md).

[Focused checks](evidence/greeter-keyboard-focused.json) passed **22 greeter tests
per configuration**, both workspace Clippys/build, 13 existing helper tests and
static/documentation checks. The [actual guest](evidence/greeter-keyboard-vm.json)
passed in **302.81 seconds**: four maps/cancel, expiry, greeter-crash rollback,
commit/recovery, real PAM, account switching/logout and a secret scan. Its base
disk stayed unchanged. [Package-only executable proof](evidence/greeter-keyboard-package-build.json)
matches the guest and archived fixture; the workspace binary is independently
recorded. Source matches across records: `18bc11d96e690e6f931537a3256288496bac55119a50865b398dbdf8a195b445` / 299 files.
A [separate cold boot](evidence/greeter-keyboard-boot.json) passed in **65.93 seconds**
with three seconds of complete login/keyboard text and six identical keyboard
frames; the actual [keyboard screen](evidence/greeter-keyboard-boot.png) and
[login screen](evidence/greeter-keyboard-login.png) were inspected at full resolution.
Fresh [Debian CI 40582861536](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40582861536)
passed implementation `4381d58`: [954 Rust/configuration, both Clippys, 96 Python
tests/zero skips and 17 checks](evidence/greeter-keyboard-ci.json), matching the
focused and guest source. Its tested merge is `f1cf70511be97ab3158157541d0c4fc6118c2ae6`.
Earlier reports below remain qualified only for their original sources. #27/#4 and physical, installer,
encryption, recovery and release gates remain open. Suspend is unavailable.

WrOS is the platform: [PR #37](https://github.com/j5cfk7qm49-web/WrOS/pull/37)
removes Windows/macOS jobs and archives imported application publishing workflows.
Its sole Debian job [passed](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40580883303).
See [CI scope](CI.md); historical cross-platform results below are retained only
as past evidence.

## Native login — qualified parent increment

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

Fresh Debian/macOS [CI 40568739789](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40568739789)
then **passed on implementation head `e758294`**. Its [unmodified final-source
report](evidence/native-greeter-ci.json) contains **947 Rust/configuration,
96 Python/zero skips, both Clippys and 17 CI checks**, matching the final focused
and guest source fingerprint below. CI lacks the local staged Wi-Fi image, so
that separate local baseline audit is not counted as a CI gate. Tested PR merge
revision is `2a01e71a6cfa8e6bb51c470c5ce1e3531cca249c`. The separate Windows
workflow was still running when this record was written.

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
the display or adding cursor queries to authentication input. This remains an opt-in experimental image profile;
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
