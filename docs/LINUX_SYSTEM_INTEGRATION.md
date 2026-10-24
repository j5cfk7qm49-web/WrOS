# Linux system integration

## Implemented boundary

`writingdeck-sys::Backend` receives typed requests and returns snapshots, messages or keyboard-preview tokens. The TUI's single bounded worker performs commands; rendering/input only uses cached data. The Linux runner uses fixed absolute executable paths, argument vectors (never a shell), null stdin, an eight-second command deadline and a 64 KiB retained output cap. Missing packages, D-Bus, devices or authorization are errors/unavailable states, not success.

| Capability | Current adapter | Verification / limit |
|---|---|---|
| Network status | `nmcli -t -f STATE,CONNECTIVITY general` | Linux parsing/interface tests and offline VM; no manuscript transmission |
| Wi-Fi radio / scan | Explicit `nmcli radio wifi on/off`, `device wifi list --rescan yes` | Typed command tests; physical radio/SSID/connection tests pending |
| Battery | Read-only sysfs energy-weighted battery aggregation, capacity fallback | Missing-device/fixture tests; no physical discharge/charging test; UPower evaluated/installed but not yet queried by the adapter |
| Brightness | Read sysfs; explicit `brightnessctl --class=backlight set N%` | 1–100 validation and no-device behavior; actual permissions/backlight pending |
| Power saver | UI low-power setting plus explicit `powerprofilesctl set` | UI works independently; unsupported OS profile reported separately |
| Suspend / restart / shutdown | `systemctl --no-ask-password ...` via maintained system service authorization | UI confirmation, manuscript save gate, typed adapter tests; never exercised on host/physical hardware |
| Keyboard | Exact polkit helper; fixed root-owned maps, independent timer | Rootless transaction tests + real VM console/polkit/timer tests |
| Timezone / disk free / OS | Local `/etc` reads, `statvfs`, Debian release identity | Read-only; setting timezone/clock remains planned |
| Sound, updates, restore, screen lock | Design only | No fake success or broad root helper |

The first settings screen is a foundation, not all requested panels. New network credential entry, volume, account/first-run setup, updater/rollback and lock integration remain open gates.

## Privileged keyboard interface

Files installed **only into the staged image** by current scripts:

- `/usr/libexec/writingdeck-keyboard`: isolated Python interpreter, root-owned executable.
- `/usr/share/polkit-1/actions/org.writingdeck.keyboard.policy`: exact executable annotation.
- `/etc/polkit-1/rules.d/50-writingdeck-keyboard.rules`: only active local `writingdeck` group members; no general sudo or arbitrary systemctl/localectl permission.
- `/usr/share/writingdeck/keymaps/{qwerty,colemak,colemak-dh-ansi,colemak-dh-iso}.map`: compiled from maintained XKB.
- `writingdeck-keyboard.service`: applies committed layout after console setup and before tty1.
- `writingdeck-keyboard-revert.timer/.service`: independent rollback ownership.

The helper allows preview/layout-ID, confirm/token and cancel/token through polkit. Boot/rollback are reserved for root-owned systemd units. A root-owned flock serializes transactions. Preview writes a private token/deadline, arms the timer **before** loadkeys and refuses another unresolved preview. Confirmation checks token and monotonic expiry, then fsyncs/atomically commits the layout. A generation token prevents a crash after commit but before timer cancellation from undoing the committed layout. Reboot reapplies the committed record; an unconfirmed preview never becomes persistent configuration.

The helper cannot execute arbitrary programs, load arbitrary paths, format storage, read manuscripts or enroll a cloud account. Its service hardening denies home access and makes the filesystem read-only except the small state locations. Do not add a generic privileged JSON command bridge to avoid writing specific policies.

## Authentication and maintenance

The wrapper checks UID and refuses root. Production uses an actual PAM login; the VM demo explicitly bypasses passwords for tty1 and is not deployable. Keep tty2 available for authenticated maintenance, and keep root/admin recovery credentials outside the manuscript.

`loginctl lock-session` alone is not a console lock: an actual locker must own the display/input. A future integration must pause Grimoire's event reader before handing a tty to an upstream PAM locker, handle failure safely, and lock before suspend. This is not implemented; do not equate a blank screen or disk encryption with session security.

## Future service contracts

- Wi-Fi enrollment: a password-safe TTY/D-Bus secret agent, never argv/logging; explicit scan/connect consent.
- UPower/logind subscriptions if polling costs or multiple devices require them; cache status, no 4 Hz subprocess loop.
- Timezone/volume: narrow maintained APIs with validation, explicit capability discovery and recovery.
- Updates: upstream signed apt packages plus a chosen upstream snapshot/rollback facility; no blanket passwordless apt/shell policy.
- Backup worker: separate opt-in queue, secure credentials, hashes and conflict states; no access through the keyboard helper.

Every new action needs denied/unavailable/timeout tests, a non-root path and a tutorial task before it is called implemented.

## Independent editors do not need privilege

M3b file ownership and both Editor contexts live in portable core/TUI code and use ordinary project files. F11, clipboard routing, workspace state, autosave and recovery do not call the keyboard helper or require a root service. The actual binary/PTY checks refuse root and isolate HOME. The registry is advisory in-process ownership, not a privileged filesystem lock or a promise that another program cannot race a save.
