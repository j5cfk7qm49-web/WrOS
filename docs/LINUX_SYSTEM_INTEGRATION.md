# Linux system integration

WrOS 0.1.0~rc16 runs on NixOS 26.05. The installed system imports its service
modules from [installed.nix](../nixos/modules/installed.nix); the live system uses
[live.nix](../nixos/modules/live.nix).

## Implemented boundary

`wros-sys::Backend` receives typed requests and returns snapshots, messages or
preview tokens. The TUI sends requests through a bounded background worker in
[src/app/wros.rs](../src/app/wros.rs). The Linux runner uses fixed executable
paths and argument vectors, without a shell. It supports private stdin for
secrets and rejects output over 64 KiB per stream. Command deadlines are 8 seconds
normally and 20 for `nmcli`; update operations use explicit budgets, including
180 seconds for review and 900 for staging. These limits are defined in
[linux.rs](../crates/wros-sys/src/linux.rs). A failed or timed-out command does
not establish success.

NixOS compiles tool and helper paths from
[paths.nix](../nixos/pkgs/paths.nix) into the app.
[helpers.nix](../nixos/pkgs/helpers.nix) packages the scripts with Nix store paths.
[polkit.nix](../nixos/modules/polkit.nix) uses the same helper paths for exact
executable annotations and loads the narrow authorization rules.

| Capability | Current adapter | Verification / limit |
|---|---|---|
| Network status and Wi-Fi | Fixed `nmcli` requests for status, radio, scan, joining and saved profiles; passphrases use private stdin and connection changes have readback | NixOS enables NetworkManager and grants its listed Wi-Fi actions to active local `wros` members; physical radio behavior needs separate qualification |
| Battery | Read-only sysfs aggregation with capacity fallback | UPower is enabled, but this adapter reads sysfs |
| Brightness and power saver | `brightnessctl --class=backlight set N%` and `powerprofilesctl set` | Brightness is restricted to 1-100; an unavailable device or service is reported |
| Suspend | Main-thread input/display handoff to `wros-lock --suspend` when supported | Direct backend suspend stays refused; the installed suspend service requires the locker proof. Physical sleep/resume qualification is separate. |
| Restart and shutdown | `systemctl --no-ask-password` requests | The app confirms the action and saves before handing off |
| Keyboard | Fixed-layout helper with token-bound preview, confirm and cancel | Independent systemd rollback; see the interface below |
| Warm screen | Typed preview, Keep and Cancel through the display helper; compositor sessions use a separate colour worker | [Warm screen](WARM_SCREEN.md) describes readback, rollback and driver limits |
| Timezone | Validated local catalog and `timedatectl --no-ask-password`, followed by authoritative readback | [Timezone policy](NATIVE_TIMEZONE.md) grants only the timezone action |
| Manual clock and network time | Narrow `wros-clock` helper | RTC controls remain unavailable |
| Sound | `wros-sound` status, volume and mute requests | Volume is restricted to 0-100 |
| Disk free and OS | `statvfs` and WrOS/NixOS release identity from `/etc/os-release` | Read-only |
| Login and account creation | greetd/PAM login; the encrypted live installer creates the writer account | Root remains locked; tty2 is available for authenticated maintenance |
| Manual console lock | Independent Nixpkgs vlock/PAM locker on reserved tty8 after input/display retirement | The [console-lock VM check](../nixos/tests/console-lock.nix) covers tty8 ownership, authentication and input isolation; physical qualification is separate |
| Signed updates and recovery | Packaged `wros-update` verifies signed NixOS closure manifests and binary-cache hashes, stages a generation for restart, and supports go back and repair | See [NixOS updates](NIXOS_UPDATES.md); the helper has a compiled Nix store path |

The adapter requests are defined in [linux.rs](../crates/wros-sys/src/linux.rs).
The service declarations are in [network.nix](../nixos/modules/network.nix),
[hardware.nix](../nixos/modules/hardware.nix),
[session.nix](../nixos/modules/session.nix) and
[guards.nix](../nixos/modules/guards.nix). The
[installer core](../nixos/scripts/wros-install-core) partitions and encrypts the
selected disk, copies the prebuilt system closure and creates the writer.

## Privileged keyboard interface

NixOS installs the helper and four compiled keymaps in the Nix store. The app and
polkit use the same helper path from [paths.nix](../nixos/pkgs/paths.nix).
[keymaps.nix](../nixos/pkgs/keymaps.nix) builds QWERTY, Colemak, Colemak-DH ANSI
and Colemak-DH ISO maps from XKB data.

[session.nix](../nixos/modules/session.nix) declares `wros-keyboard.service`,
`wros-keyboard-revert.service` and `wros-keyboard-revert.timer`. The committed
layout is applied after `systemd-vconsole-setup` and before greetd. The
[helper](../os/scripts/wros-keyboard) arms the independent 15-second timer before
applying a preview. Confirmation and cancellation require the preview token.
The shared [writer rule](../os/config/50-wros-keyboard.rules) requires an active
local session and membership in both `wros` and the `writingdeck` compatibility group. The separate
[greeter rule](../os/config/51-wros-greeter-keyboard.rules) grants only the keyboard
actions to the active local `_greetd` account.

## Authentication and maintenance

[session.nix](../nixos/modules/session.nix) enables the native greeter on tty1.
The packaged [session wrapper](../os/scripts/wros-session) refuses root. The
[installer](../nixos/scripts/wros-install-core) creates the writer account, and
[guards.nix](../nixos/modules/guards.nix) configures Linux-PAM and passworded sudo
for wheel members. tty2 remains the authenticated maintenance path; tty8 is
reserved for locking.

The [main-thread lock handoff](../src/console_lock.rs) retires writing input and
display before starting the independent locker. Guarded suspend is offered only
when the installed session supports it. The systemd suspend service runs
`wros-lock assert-suspend` first. Direct backend suspend remains refused.

## Service contracts and remaining work

- Updates: signed NixOS bundles, offline generation switching, rollback and
  current-generation repair are implemented in the
  [update engine](../nixos/scripts/wros-update-engine.py) and
  [update units](../nixos/modules/updates.nix). See [NixOS updates](NIXOS_UPDATES.md).
- Google Drive backup and readable sync use separate background workers in
  [backup_service.rs](../src/backup_service.rs) and
  [sync_service.rs](../src/sync_service.rs). Their helpers are packaged by NixOS.
- Physical security, radio, display and sleep/resume qualification remain
  separate from software and VM checks.

## Independent editors do not need privilege

The system worker receives typed OS requests, not an editor or a manuscript.
Portable document and workspace services remain in `grimoire-core`; the OS
adapter and narrow privileged helpers handle system changes. See
[Architecture](ARCHITECTURE.md) for the boundary.

## Verification

[flake.nix](../flake.nix) defines checks for encrypted installation, boot/unlock
and login, keyboard preview, console lock, generation updates/go back, recovery,
live sessions and helper paths. The definitions live in `nixos/tests/`; they are
not pass records. The [Nix workflow](../.github/workflows/nix-ci.yml) evaluates
all flake packages and checks, builds the Python suite, selected WrOS packages
and the installed system. It does not run the VM checks.

## History

Before rc14, the Debian prototype staged FHS helpers and policies and exercised
login, keyboard, timezone and display behavior in disposable VM fixtures.
[Archived STATUS](archive/STATUS.md) and the
[timezone record](evidence/native-timezone-vm.json) retain that work. Those
results do not qualify the current NixOS release or physical sleep/resume.
