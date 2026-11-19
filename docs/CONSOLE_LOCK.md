# Manual console locking

**Debian qualification history, rc13 and earlier.** Focused software, the full
isolated-guest sequence and [Debian CI](evidence/console-lock-ci.json)
passed: 964 Rust/configuration, 120 Python/zero skips, both Clippys and 17 gates.
The rc7 VM also exercises suspend and wake with this locker. Physical security,
lid wake and Wi-Fi reconnection still require the HP Stream.

Current NixOS coverage is defined in [nixos/tests/console-lock.nix](../nixos/tests/console-lock.nix):
reserved units, wrong/right authentication, and input/password isolation. This
does not claim a new pass result or NixOS suspend/wake qualification.

## WrOS lock screen (rc19)

`openvt` allocates unused tty8, switches to it and waits for the locker. By
default it starts `wros-lock-screen` with the writer's account name. The screen
starts as root, disables SysRq, takes `VT_PROCESS` ownership of tty8 and turns
echo off. It then drops to the writer, draws its own clock and reads the
password. It uses `/etc/pam.d/vlock`. The `pam_unix` module verifies the password
through its setuid `unix_chkpwd` helper.

The worker checks that the child is the selected executable, tty8 is active
and in `VT_PROCESS` mode, echo is off and SysRq is zero. Before reporting an
unlock it requires readiness and a successful exit, and for the WrOS screen a
proof as well. The screen runs as the writer, so the writer's own programs could
kill it, and openvt on NixOS reports a killed child as a success. So the worker
creates an empty root-only `/run/wros-lock/unlocked`. The screen opens it while
still root and writes `unlocked` into it only after PAM accepted the password,
and the worker unlocks only when it reads exactly that. A killed screen leaves
it empty: the lock fails and the writing session closes.

NixOS writes the choice to root-owned `/etc/wros/locker`. Set
`wros.lockScreen = "vlock"` to use the vlock fallback. An absent file selects
`wros`. A present file must contain `wros` or `vlock`, with an optional final
newline. An invalid choice prevents locking.

## Maintained authentication and console ownership

Settings → System → Battery and power offers **Lock writing session** only in
an installed, non-root tty1 writing session with the required local payload.
The marked passwordless VM demo does not offer it. Both editors must save
successfully, with no dirty scene, stale tree or unresolved source conflict.
The existing conflict/rescue behavior is retained.

The main thread retires and reaps the separate input reader, discards queued
events and partial parser state, and holds its single-use pause proof. It clears
the old console and stops rendering and terminal queries. It keeps tty1
non-echoing while starting the fixed polkit helper. The editor never prompts for
or receives an unlock password.

The native graphical reader additionally closes its libinput devices and releases
logind's seat controller synchronously before its successful exit. A crash or
forced kill cannot supply that retirement acknowledgment. Unlock starts a new
reader with fresh XKB/compose state and device descriptors, then restores the
renderer as the VT owner. Terminal and window input retain their existing paths.

The root-owned helper requires the direct installed editor parent, matching
non-root UIDs, an active local logind user session on seat0/tty1, tty1 descriptors,
a password-bearing local writer and a locked root account. It checks fixed
root-owned program/module/PAM files and the reserved tty8 service masks.
Its public caller option is `--suspend`; it accepts no program paths or user configuration.

The fallback runs `vlock-main all nosysrq` through `openvt`. The fixed command
bypasses the user-shell wrapper and excludes user `.vlockrc`, environment-selected
plugins and shell expansion.
The helper uses a cleared environment. Both lockers use the fixed
existing-session PAM stack, including the WrOS-declared `common-auth` stack.
An earlier requisite `pam_succeed_if` check requires the authenticated account's
UID to be positive. It rejects vlock's unusable locked-root fallback without
asking the writer for a root password; the helper also requires root locked.

NixOS packages these tools and modules at fixed Nix store paths and rewrites the
shared PAM includes to upstream Linux-PAM syntax.

After unlock, the editor flushes typeahead, restores its display and starts a fresh
acknowledged input reader. The same in-memory editor retains its writing context.
Failure leaves the proof unconsumed and closes the writing session through the
existing save/rescue path.

**Debian qualification history, rc13 and earlier.** The rc10 cage sessions use
the same independent locker. Foot retires its PTY
reader; the OpenGL window clears its frame, disables composition and discards
all pending window input. It pumps Wayland without dispatching keys during
authentication. The helper gets fixed tty1 descriptors. Logind's Wayland type
is accepted only with the packaged cage executable as an ancestor, matching
UIDs and the same logind session; all active/local seat0, VT1, tty1 and user-class
checks still apply. Both compositor modes passed repeated lock and actual
suspend/wake VM checks, including password isolation; see
[the graphical trial](GRAPHICAL_TRIAL.md).

After validating the caller, the helper uses `systemd-run --wait --collect` to
place the fixed worker, `openvt` and the locker in a system service cgroup.
A Linux PID handle lets the worker detect if the original editor exits and
fail. systemd stops remaining children and runs the fixed cleanup
command, which uses `chvt` and restores the prior SysRq setting. It returns
the service's completion status to the waiting editor. Worker/cleanup roles
require a root process started by PID 1 and reject a pkexec environment.

**Debian qualification history, rc13 and earlier.** The
[guest sequence](evidence/console-lock-vm.json) verifies service ownership,
five lock cycles and editor/locker/client/worker failure recovery. [Focused
checks](evidence/console-lock-focused.json) cover the interface and policy; all
[original failed attempts](evidence/console-lock-attempts.json) remain failed.

## Suspend and wake

The active writer can choose **Lock and suspend**. A discharging battery at its
reserve requests the same action. The editor saves both buffers and waits for an
active backup, sync or system request to finish. The battery reserve is the
device's critical flag or 8 percent, whichever comes first.

WrOS says what it is about to do before it does it. On battery the status bar
warns at 20% and 10% that at 8% WrOS saves, locks and sleeps. At the reserve it
saves at once, then counts down 30 seconds in the status bar before locking and
sleeping; plugging in cancels the countdown. After waking, the unlock message says
the battery caused the sleep, and a laptop still at its reserve gets two minutes
before the next sleep. Where sleep is unavailable, the reserve saves and says so.
Settings keeps each warning in its notices.

Closing the lid does what the writer chose in Settings › System › Battery and
power › **When the lid closes**: lock and sleep (the default), lock and stay
awake, or do nothing. Where sleep is unavailable, lock and sleep only locks. The
choice is kept in `~/.config/grimoire/lid.json`, so backups carry it. The
battery reserve always locks and sleeps, whatever the lid choice.

The lock worker disables keyboard delivery to tty8 before starting the locker for
suspend. Once the locker holds tty8, the worker writes a short-lived proof in its
root-only runtime directory. The systemd suspend service independently checks
the worker, the active protected VT, the disabled keyboard and the retired
reader, and that the lock service is running and owns the locker. It has no
unit dependency on the lock service: one kept the finished service loaded, so
systemd refused every later lock in the same boot (#93). The worker asks logind
to suspend so NetworkManager can prepare and resume with the system. It waits
for logind to finish before flushing typeahead and restoring the keyboard for
PAM unlock. If completion cannot be verified, the writing session closes
safely and the lock service restores the keyboard during cleanup.

## Limits and verification

The WrOS lock screen and vlock's `all` plugin use Linux VT release/acquire ownership.
The kernel resets that mode when its owning process dies. A different protected
VT keeps locker credentials separate from tty1 if the editor exits. Qualification
must test actual VT switching, wrong/right PAM, reader absence, original console
stability, context restoration and process-death behavior; pure policy tests do
not establish those properties.

Automatic suspend is monitored during an active editor session. Check the lid,
resume and battery reserve on the HP Stream. A manual lock remains active if the
lid is closed during that lock. Root changes and a forced power-off are outside
the session-lock boundary.

Debian reference manuals: [vlock-main](https://manpages.debian.org/trixie/vlock/vlock-main.8.en.html),
[plugins](https://manpages.debian.org/trixie/vlock/vlock-plugins.5.en.html) and
[openvt](https://manpages.debian.org/trixie/kbd/openvt.1.en.html).

Debian tracks [CVE-2026-72693](https://security-tracker.debian.org/tracker/CVE-2026-72693)
in openvt's privileged `-u` passwordless-login path. The described path is
excluded here by the fixed argument vector: WrOS never requests `-u`, `-l` or
`login`, and executes only the selected locker. Caller checks use actual process
UIDs and logind session properties rather than inferring process ownership from
the owner of a TTY device node. This scoped check is not a blanket package-security audit.
