# Native live-installer interface

The target is a live WrOS session with an installer that feels like Grimoire.
`wros-installer` is now a working, separate, unprivileged TUI for Debian
cdebconf's passthrough questions. It is **not yet a complete live installer**:
the privileged launcher, actual media integration and install/reboot qualification
remain open. The current delivered ISO still uses its earlier Debian text path.

## Shared Grimoire presentation

`wros-shell::grimoire_theme` now owns Grimoire's original nineteen presets,
twelve roles, Custom settings and theme-file parser/formatter. The editor keeps
its existing filesystem adapter and atomic saves; old `theme.toml`, Pomodoro
world and visualizer choices retain their format and behavior. Native Settings,
login/layout recovery and installation project their colors from that same code.
The installer reads the existing saved per-user Grimoire theme on startup.
Before authentication, login uses the shared device default.

Forms, lists and progress share `render_panel` geometry with the existing menu:
the same border, accent, selection marker and fixed status/help lines. Up/Down
chooses, Enter continues, Escape goes back when supported, Space marks a
checklist, Page Up/Down reads long explanations and F1 opens contextual help.
Help cannot accidentally submit a field. System surfaces must continue this
shared implementation as update, recovery and backup interfaces are added.

## Backend and credential boundary

The frontend has no disk-writing command, password verifier, privileged policy,
installer package script or editor dependency. It accepts an inherited local
stream from a supervisor; invoking it as root is rejected. The supervisor is
not yet implemented or installed. There is no ordinary-user disk-writing grant.

The adapter handles string, password, boolean, select, multiselect, note, error
and progress questions. Translated labels map back to canonical Debian values.
Every boolean starts at No; defaults do not authorize partition changes.
Answers are released only after explicit field completion. Passwords have a
constant hidden placeholder, bounded zeroed storage, no debug formatting and no
per-character repaint. Back navigation clears secrets. Protocol/transport errors
stop the frontend with fixed diagnostics and never claim installation success.
The later [terminal readiness increment](TERMINAL_INPUT.md) adds actual
multi-read raw ASCII/Unicode secret-entry and post-submit typeahead regression
at four sizes, plus long typed values against the maintained Debian backend.
Bracketed-paste support is a separate, unclaimed contract.

Debian cdebconf 0.280's passthrough module needed a small, retained
[protocol patch](../os/config/installer/README.md). It fixes multiline field
framing, response buffer accounting and password whitespace handling. It changes
no partitioning, encryption, account or bootloader engine. The
[non-root build recipe](../os/scripts/build-installer-frontend.sh) checks pinned
Debian source archives and retains the module's original BSD notice, full
source/copyright index and a build manifest. A later
[matching-kernel component recipe](LIVE_INSTALLER_IMAGE.md) now embeds the
module in an actual initrd. Complete live media integration remains open.
Native question tests do not certify arbitrary upstream package security.

## Verification and remaining integration

[Eleven focused gates](evidence/native-installer-focused.json) cover 34 shared/
greeter/installer component tests, ten retained theme cases, workspace Clippy and
build, three actual installer PTY methods and the retained Settings/editor PTY
at four sizes. [Eleven real Debian checks](evidence/native-installer-backend.json)
passed with an observed loaded-module identity. The
[pinned module build and notices](evidence/native-installer-cdebconf-build.json)
and [original failed attempts](evidence/native-installer-attempts.json) are
retained separately. [Debian CI](evidence/native-installer-ci.json) passed
971 Rust/configuration, 123 Python/zero skips, both Clippys and 17 gates. Its
[provenance](evidence/native-installer-ci-provenance.json) matches the focused and
real-backend source. None of this is a live-image installation claim.

The later [failure-boundary increment](INSTALLER_BACKEND.md) passed a full
20-gate local run and [19-gate Debian CI](evidence/installer-closed-ci.json), with
972 Rust/configuration, 123 Python/zero skips, both Clippys, 12 native backend
checks and 14 protocol cases. It rejects transport failures before confirmation
can reach the configuration script and fixes backend-cleared answer reuse.

The [password form](evidence/native-installer-password.png) and
[progress form](evidence/native-installer-progress.png) are painted from actual
synthetic renderer cells using Grimoire's existing cell painter. Their
[provenance](evidence/native-installer-frames.json) distinguishes them from guest
or physical screenshots; the progress value is synthetic.

Focused coverage includes the existing theme round trips and login tests, all
nineteen palettes, four supported terminal sizes, actual compiled TUI help,
password/back/confirmation behavior and saved Custom colors. A real Debian
cdebconf package runs synthetic templates and configuration scripts in a private
RAM mount namespace, as a normal user with no network or target disk. It checks
multiple paragraphs, back status, translated canonical values, progress, typed
password spaces, long prefilled answers, literal backslashes and absence of
credentials from output/logs. Reports bind source and the actual loaded module.

Before the live installer can be exposed:

1. Reuse Debian's live launch preparation with a matching live/installer kernel
   and the compatible passthrough module. Keep the backend outside the editor,
   with a single terminal/credential owner and supervised process lifetime.
2. Stop the whole backend on frontend loss or malformed protocol. The
   [failure-boundary patch](INSTALLER_BACKEND.md) makes cdebconf abort those
   protocol errors; the future supervisor must also stop all child workers and
   clean the private mount/terminal state.
3. Keep per-run answers and credentials in private RAM. Shipping defaults must
   contain no target disk, password or destructive confirmation. The live account
   and live-only startup configuration must not become an installed login.
4. Qualify disk review, cancel/back, wrong choices, LUKS, interruption, module and
   kernel mismatch, actual install and authenticated reboot in fresh no-NIC COW
   guests. Preserve the existing timezone, input-reader, lock and sleep guards.
5. Rebuild and audit the image and corresponding source/notices, then complete
   authorized physical installation, firmware, radio, power and recovery tests.
