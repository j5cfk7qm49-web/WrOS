# Native login keyboard recovery

On the experimental native login screen, **F2** opens keyboard recovery before
an account is submitted. Up/Down selects QWERTY, Colemak, Colemak-DH ANSI or
Colemak-DH ISO. Enter previews it, a second Enter saves it, and Escape reverts
it. An unconfirmed preview expires. Escape from the idle selector returns to
login. If already answering PAM, Escape cancels that conversation first; the
selector cannot open while credentials or authenticated session state exist.

The selector reuses the existing root-owned `writingdeck-keyboard` helper. Its
token-bound transaction and systemd-owned 15-second rollback timer remain the
authority; the timer is armed before changing the console map and survives a
greeter crash. The layout change happens in the Linux console, without editing
manuscript characters or reinterpreting a password in the application.

The [experimental staging script](../os/scripts/stage-greeter.sh) installs a
separate [polkit rule](../os/config/51-wros-greeter-keyboard.rules) for the active
local `_greetd` account and the exact existing keyboard action. It grants no
display, shell or general system-command action and adds no group membership.
The service account remains locked with its maintained package configuration.
The helper accepts only four registered IDs and bounded transaction tokens.
See the upstream [polkit rule contract](https://www.freedesktop.org/software/polkit/docs/latest/polkit.8.html)
and [pkexec contract](https://www.freedesktop.org/software/polkit/docs/latest/pkexec.1.html).

A bounded worker queue runs the fixed helper with a minimal fixed environment,
closed standard input, a disabled textual authentication agent and a 4 KiB
reply limit. No PAM value enters that worker. The single greeter event reader
discards typeahead when entering/exiting recovery and after helper replies.
Outstanding keyboard actions block authentication; a failed or malformed reply
keeps recovery open until a successful preview/revert resolves the uncertainty.
A stalled helper also keeps authentication blocked; the authenticated tty2
maintenance path remains available. Diagnostics are fixed text, never raw
subprocess output. A successful confirmation clears the account field before
returning to login.

[Focused software checks](evidence/greeter-keyboard-focused.json) cover 22 greeter
tests in both configurations, both workspace Clippys, the workspace build, the
13 existing helper tests, syntax, staging ShellCheck and documentation checks.
The [real guest/PAM run](evidence/greeter-keyboard-vm.json) passed in 302.81 seconds
with no NICs and an unchanged base disk. It exercises narrow active/inactive
authorization, all four actual console mappings/cancel, expiry, rollback after
killing the greeter, durable confirmation, QWERTY recovery, PAM refusal/success,
hidden input, UID-1000/1001 writing sessions, account switching, logout and the
secret scan. The [package-only build](evidence/greeter-keyboard-package-build.json)
matches the exact guest executable and archived fixture; the focused report
separately records the workspace feature-unified build. All share source
`18bc11d96e690e6f931537a3256288496bac55119a50865b398dbdf8a195b445`.
Fresh [Debian CI 40582861536](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40582861536)
passed implementation `4381d58`: [954 Rust tests per configuration, both Clippys,
96 Python tests/zero skips and 17 checks](evidence/greeter-keyboard-ci.json). Its
source fingerprint matches the focused and guest records. Tested merge is
`f1cf70511be97ab3158157541d0c4fc6118c2ae6`; CI binaries are recorded independently
from local builds. Previous native login evidence retains its original source identity.

The [separate cold-boot observer](evidence/greeter-keyboard-boot.json) passed in
65.93 seconds using that same guest executable. Automatic service startup,
three seconds of complete login and keyboard text, and six identical keyboard
framebuffer captures were verified. The agent inspected the actual full-resolution
[login](evidence/greeter-keyboard-login.png) and [keyboard](evidence/greeter-keyboard-boot.png)
screens. The [retained observer harness](evidence/greeter-keyboard-boot-harness.txt)
only reads the guest screen; the host presses F2 and sends no credentials. Its
completed fixture disk remains unchanged. Test-probe status output is disabled
for this visual check; the preceding acceptance run retains ordinary status output.

This remains an explicitly staged experimental image profile. It is separate
from installer keyboard recovery, early LUKS prompts, secure session locking,
sleep/resume and physical keyboard qualification. Suspend remains unavailable.
