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
The separate real guest/PAM acceptance and final-source Debian CI results must
be recorded before treating this increment as qualified. The previous native
login evidence retains its original source identity.

This remains an explicitly staged experimental image profile. It is separate
from installer keyboard recovery, early LUKS prompts, secure session locking,
sleep/resume and physical keyboard qualification. Suspend remains unavailable.
