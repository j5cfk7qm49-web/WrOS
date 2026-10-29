# WrOS unified shell — architecture decision and execution order

On 2026-10-28 the user prioritized completing the installed writing environment
before further full installer qualification. Finish and preserve the current
build checkpoint, then close native everyday workflow gaps and remaining writing
features. Keep basic smoke checks; qualify the final installer after its package,
service, default and first-run contents are stable. All interface and data
boundaries below continue to apply.

The user's 2026-10-26 directive supersedes the earlier assumption that a Linux
appliance launching Grimoire is the finished interface. **Grimoire evolves into
WrOS.** Normal writing, settings, networking, accounts, storage, help and system
operations belong to one keyboard-driven interface. Keep the existing editor,
portable manuscripts, ownership registry, guarded saves, undo and workspace
recovery. A conventional desktop and unrelated management TUIs are not the goal.

## Audit of the current source

The table below records the initial 2026-10-26 audit. Current login, manual
locking and display results are in [STATUS](STATUS.md). The user's subsequent
interface directive also covers the live installer and every future system
surface: share Grimoire's theme, widgets, spacing, selection, navigation and
help/status code. The [native installer increment](NATIVE_INSTALLER.md) extracts
the actual palette and theme format into `wros-shell`, retaining editor behavior.

The current shared implementation covers Settings, login/layout and the
installer forms, progress and help. Cohesion applies to every ordinary WrOS
surface, including errors, unlock, updates, recovery and backup. Reused engines
must sit behind those shared screens. The existing vlock prompt has separate
security qualification; its Grimoire presentation is still unfinished. Shared
presentation must preserve the separately qualified credential/input owners.

| Area | Actual implementation | Consequence |
|---|---|---|
| Editor/panes | `App`, independent retained Editor contexts, exclusive source ownership, guarded save/recovery, workspace restoration | Extend in place; Settings must retain these objects, selection, free scroll, focus and undo |
| Navigation | `Overlay`, per-feature `Page` enums, menus/palette, F10–F12 and logical aliases | Add a shared bounded navigation model and adapters; preserve existing routes during migration |
| Settings | Native editor preferences plus a flat 13-action system pane | Migrate available actions into Writing, Interface, System and Accounts; do not simulate missing capabilities |
| Services | `writingdeck-sys::Backend`, typed requests, cached Snapshot, one bounded off-thread worker and explicit errors | Reuse; no drawing/input-loop I/O, shell commands or manuscript arguments |
| Visuals/help | Existing Ratatui theme roles, menus/modals and event-driven tutorial | Extract reusable presentation without linking the editor into the greeter; add real feature exercises |
| Login/lock | Installer-created PAM account/tty login plus experimental isolated native greetd client; secure locker absent | Greeter qualification is tracked separately from locker/input ownership; credentials stay outside the editor |
| Cloud | No qualified OAuth/Drive mount/verified remote backup | Separate account, browse/cache, sync and backup state machines |
| Display warmth | No system color-temperature implementation | Native warm-screen controls and a verified display-service backend are required; theme colors are insufficient |

The recovered M7b milestone is qualified separately. Experimental installer
media, installed Wi-Fi payload, cancellation/no-disk and post-interruption boot
evidence remain preserved in [INSTALLER_MEDIA](INSTALLER_MEDIA.md). Those do not
qualify the unified shell, greeter, locking or cloud features.

## Process and data boundaries

```text
kernel / initramfs / DRM / maintained Linux services
  ├─ greetd + PAM ─ isolated wros-greeter ─ unprivileged WrOS session
  ├─ logind / NetworkManager / power / narrowly scoped display service
  └─ separate lock/input-ownership service (must qualify before suspend)

shared wros-shell presentation/navigation/theme/notification models
  ├─ greeter: authentication conversation only; no editor/core dependency
  └─ Grimoire-derived WrOS: retained local editors + native Settings/files/help
       └─ bounded async service adapters; public status/progress only
          ├─ user account/OAuth credential broker
          ├─ user Drive browser/mount/cache service
          ├─ independent conflict-safe sync
          └─ independent versioned backup/verified restore
```

The shared UI library owns no passwords, OAuth tokens, manuscripts, process
privileges or Linux devices. Presentation reuse does not imply shared input
queues or a shared authentication process. Keep compatibility CLI/configuration
identifiers until tested migrations; user-facing identity is WrOS — Writer's OS.

Settings navigation retains the writing objects instead of serializing/reopening
them. Back returns through a bounded history; returning to writing restores the
same active document/pane, cursor, selection, free scroll and workspace. Preserve
unknown/future preferences; invalid UI preferences cannot block manuscript saves.
Async completion updates a bounded notification/status model without stealing
focus. Network/cloud work must never be added to rendering, typing or autosave.

## Native settings and maintenance

Writing contains editor/manuscript/focus/goals/challenges/revision/footnotes;
Interface contains themes/workspaces/shortcuts/layout/accessibility/notifications;
System contains network/display/power/storage/time/updates/recovery; Accounts
contains local users/Google/Drive/sync/backup. Existing actions remain reachable
during incremental migration. Unavailable controls explain what is missing.
Native Wi-Fi controls call NetworkManager through typed adapters. Packaged
`nmtui` is an emergency maintenance tool, not the ordinary WrOS workflow.
Disruptive actions retain confirmation and successful-save barriers. No root
writing process or arbitrary-command privileged service is allowed.

Warm screen belongs to System → Display. Provide manual on/off, temperature,
strength and reversible preview; include very warm settings around 2200K and
lower supported values. No location/network is necessary. Evaluate maintained
DRM/KMS gamma support for the console session, with narrowly scoped device
ownership, real output verification and restoration of original calibration.
Do not assume X11/Wayland or report success from a palette change or an ignored
output. Greeter/session/hotplug/resume behavior and physical displays need tests.
[Debian gammastep](https://manpages.debian.org/trixie/gammastep/gammastep.1.en.html)
provides manual adjustment options; its DRM adapter must be evaluated for actual
supported-output verification rather than treating exit status as qualification.

## Authentication and cloud decisions

The [experimental separate Rust/Ratatui greeter](NATIVE_GREETER.md) uses greetd's existing
[authentication/session protocol](https://docs.rs/greetd_ipc/latest/greetd_ipc/).
PAM verifies local accounts; the editor never receives authentication keystrokes.
Use a fixed session command, bounded IPC and secret-free error reporting. A
greetd login is **not** a session lock. Issue #4 still requires seat/VT protection,
protected terminal ownership, independent authentication and lock-before-suspend.
The [editor reader-retirement primitive](INPUT_OWNERSHIP.md) now supplies a
single-use pause proof and a fresh parser on explicit resume. This is necessary
input cleanup, not a secure locker or a lock-ready acknowledgment.
Fail closed and retain authenticated maintenance/recovery. Early LUKS unlock
runs before ordinary encrypted user space; consistent branding cannot remove
that boundary or bypass encryption.

Google enrollment is optional. Tokens belong to the authenticated local user's
encrypted credential service, outside manuscripts and logs; no plaintext-token
fallback when storage is locked/unavailable. Show connect/status/reauthorize/
disconnect natively. Use PKCE/state and approved provider consent. Google's
[desktop loopback flow](https://developers.google.com/identity/protocols/oauth2/resources/loopback-migration)
remains available for desktop clients; its deprecated OOB copy/paste mechanism
must not return. The [device flow](https://developers.google.com/identity/protocols/oauth2/limited-input-device)
requires an eligible registered client and limits Drive scopes to app data and
files the app may access. General Drive browsing needs an appropriately approved
flow/scope; a phone pairing screen must not claim broader access. Live provider
registration and account tests require explicit authorization when reached.

Evaluate rclone FUSE/VFS under the user service. Its
[mount/cache limitations](https://rclone.org/commands/rclone_mount/)
require explicit cached/missing/offline indicators. Start with read-only browsing
and explicit verified local materialization. Never edit an active manuscript on
a remote mount or mistake a VFS cache for durable offline storage. Mounting,
conflict-safe synchronization and versioned verified backups remain separate.

## Execution and acceptance

1. Shared UI/navigation framework and native categorized Settings, reusing the
   existing backend. Test old preferences/actions, two retained editors, focus,
   free scroll, undo, paste guards, narrow terminals, pending/error responses and
   responsiveness. Add warm-display service/control as a dedicated increment.
2. Native network/account/storage panels with typed mockable services and
   contextual help; no fake-completed tutorial lessons.
3. Separate greeter/PAM/session and secure locker/input handoff; real guest
   rejection/success/crash/layout/suspend tests before enabling production use.
4. Approved OAuth and encrypted credential service; user-isolation/offline/
   expiry/revocation tests, then authorized live acceptance.
5. Drive browsing/local copy, then independent conflict-safe sync/verified backup.

Tracking: #26 shell/settings, #27 greeter, #28 Google account, #29 Drive browse,
#30 warm screen, plus existing #3/#4/#5/#10/#11/#14. Full release still requires
installer/encryption/recovery/update safety, complete notices/source, independent
content review and identified physical hardware acceptance. The target is the
user's full boot → native login → restored writing → Settings/network/account/
Drive → offline writing → lock/suspend/unlock → exact writing position sequence.
This document is a decision and implementation sequence, not evidence that the
unimplemented parts already work.

## Executed first-increment software qualification

The shared library and initial native Settings adapter passed
[fresh software qualification](evidence/unified-shell-software.json): **922 Rust
tests per overlapping configuration**, default **5 ignored** / no-audio **4
ignored**, both warning-denying Clippys, **76 Python methods with zero skips**
and **18 gates** including the separate staged Wi-Fi payload check. Source and
verifier input hashes were unchanged across the run. Source fingerprint is
`22e7e0e4383a1d4c6842d7ee35f085d5d644c247b7f7b0c1e9491fdda8a65e9a`
over 274 files; previous M7b and installer evidence remains separate.

Five new actual editor UI cases exercise retained two-pane positions/selections/
free scroll/focus/undo at four sizes, preference/theme action reuse and cancellation,
future/external preference preservation, backend failure and power confirmation.
The compiled PTY test verifies actual caret insertion, read-only paste/cut/undo,
save/undo/redo, normal quit and per-user Settings selection across independent
processes at 80×24, 100×30, 120×40 and 160×50.

The restoration test first failed because Deck pages were missing from the global
cut/undo guard. The corrected guard covers all Deck pages, with explicit paste
handling for the isolated keyboard test field. An initial full Rust invocation
without the required model-runtime environment failed genuine integration tests;
the final verifier provisioned it and passed without skips. The first PTY barrier
also matched a title present in its parent; the final test waits for a unique
child-page control before sending Escape. No acceptance assertion was removed.

This qualifies the initial software slice. There is no unified-shell ISO/greeter/
lock/suspend/physical display/Google/Drive acceptance yet. The subsequent
[warm-screen increment](WARM_SCREEN.md) adds its actual output service and native
controls; unsupported drivers remain explicitly unavailable.
