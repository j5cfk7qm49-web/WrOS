# Experimental native WrOS login

`wros-greeter` is a separate Rust/Ratatui executable. It shares only presentation
with `wros-shell`, and has no Grimoire/editor dependency. Debian's greetd and PAM
perform authentication and session setup. The editor never receives login input.
This is a login implementation, **not a session locker or permission to suspend**.

## Protocol and credential boundaries

The implementation follows the maintained [greetd IPC protocol](https://manpages.debian.org/trixie/greetd/greetd-ipc.7.en.html)
and [configuration contract](https://manpages.debian.org/trixie/greetd/greetd.5.en.html).
The small JSON wire codec is independently implemented here; the GPL-3.0-only
`greetd_ipc` Rust crate is not linked into the MIT workspace. Debian greetd remains
a separately distributed GPL component with its package notices/source obligations.

Each connection verifies that the Unix-socket server runs as root. Responses have a
16 KiB limit checked before allocation, and each exchange has a total 15-second
deadline. A bounded worker keeps terminal input/cancellation responsive. Malformed,
oversized, truncated, disconnected and unexpected responses fail closed. Errors
are fixed text; raw protocol/PAM error descriptions are never logged.

The conversation accepts arbitrary sequences of secret, visible, informational
and error messages. Informational/error messages require explicit acknowledgment
and send a null response. A rejected authentication is cancelled before another
account attempt. If greetd cannot acknowledge cancellation of the failed PAM
worker, the rejection remains visible and Enter exits for a fresh greetd service;
uncertain session state is never reused. A successful authentication offers explicit entry
into the fixed `/usr/libexec/writingdeck-session` command with fixed locale/terminal
environment; no user-supplied command is accepted. That wrapper refuses root.

Secret input never appears on screen, even as a length-dependent mask. Typed
responses and serialized requests are zeroized after use. Input is bounded to
1,024 UTF-8 bytes, control characters are rejected, and removed characters are
wiped before truncation. Core dumps and process dumpability are disabled before
input. These measures do not claim protection against privileged memory access,
swap recovery, or all allocator/runtime copies. Full-device encryption is separate.

The greeter has one terminal reader; no editor exists during authentication.
Pending-phase input is discarded. Kernel and parsed-event typeahead are drained
at protocol transitions and before terminal handoff. Escape cancels an idle PAM
conversation; during an outstanding request it disconnects and exits for greetd
to restart. SIGTERM/HUP/INT restore the terminal and exit without starting a session.

## Experimental installation

The existing installer is unchanged. In an isolated Debian staging root with the
maintained `greetd` package and WrOS session already installed, explicitly run:

```sh
cargo build -p wros-greeter --locked
sudo bash os/scripts/stage-greeter.sh STAGING_ROOT target/debug/wros-greeter
```

The script rejects `/`. It installs the native greeter as greetd's unprivileged
`_greetd` account, owns only tty1, and keeps the authenticated tty2 maintenance
path. `source_profile=false` avoids the legacy profile's automatic session launch.
An explicit WrOS unit replaces the package's tty7 service configuration, orders
after the former tty1 getty, and starts from multi-user.target. Only tty1's getty
is masked; maintenance gettys remain available. A drop-in cannot remove the
package's old terminal dependencies.
There is no initial-session/autologin stanza. This profile must not be enabled on
a live machine as a substitute for image qualification.

## Verification and remaining gates

`cargo test -p wros-greeter --locked --offline` exercises the conversation, fixed
command, cancellation, rendering at four supported sizes and very small sizes,
UTF-8 editing, malformed frames, socket EOF and total deadlines.
`os/tests/greeter_vm.py` uses a private disposable overlay of the marked VM-demo
image, a real Debian greetd package, a fresh synthetic password and no network.
It verifies crash recovery, actual PAM rejection/success, non-echoing VT input,
the UID-1000 writing session, logout, selection/authentication of a separate
UID-1001 account and absence of the synthetic secret from the journal and both
users' files. The [complete software baseline](evidence/native-greeter-software.json) passed
947 Rust tests per overlapping configuration, both Clippys, 96 Python methods
with zero skips and 18 checks. After the final cache-only display refinement,
[focused final checks](evidence/native-greeter-focused.json) passed all 15 greeter
tests in both configurations, both workspace Clippys, full build and static gates.
The full baseline retains its own prior source identity.

The [final unattended guest report](evidence/native-greeter-vm.json) passed in
147.06 seconds. The [separate cold-boot report](evidence/native-greeter-reboot.json)
verifies automatic native login startup and three seconds of complete visible
text, followed by six identical complete framebuffer captures. The [actual
screen](evidence/native-greeter-reboot.png) was visually inspected by the Codex
agent; the [one-off observer harness, with LF-normalized text](evidence/native-greeter-reboot-harness.txt)
is retained for audit. It observes the screen and does not start greetd manually
or submit credentials. Both guest tests preserve their base/fixture disks.
Final focused, PAM and reboot source/binary identities match. Full remote CI is
now recorded separately: Debian/macOS [run 40568739789](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40568739789)
passed final implementation `e758294`, with [947 Rust/configuration, 96 Python,
both Clippys and 17 CI checks](evidence/native-greeter-ci.json). Its source matches
the final guest; CI and local binaries are recorded independently. These records
are not physical/installer/locker proof. The separate Windows workflow was still
running when recorded.

After building the marked VM-demo base as described in [installation](INSTALLATION.md),
run the guest harness as a normal user with the exact Debian package and current
binaries:

```sh
python3 os/tests/greeter_vm.py --package /path/to/greetd.deb \
  --binary target/debug/wros-greeter --editor target/debug/grimoire \
  --artifacts-directory build/greeter-vm
```

The VM fixture is not a fresh installer, LUKS, physical keyboard, secure locking,
lid/resume, update/rollback or full release test. Local accounts can be selected
with Up/Down, or entered by name; enumeration reads only a bounded local passwd
file, excludes system/nonlogin accounts and never reads shadow or network NSS.
PAM still decides whether an account is permitted. [Native keyboard recovery](GREETER_KEYBOARD.md)
now opens with F2 before authentication, using the existing console layout
helper with independent rollback. Its focused, real guest and current CI records
are tracked separately from the older login evidence above. Issue #27 stays
open for the remaining failure/transition matrix and integration; the independent
secure locker, physical keyboards and sleep/resume remain separate gates.
