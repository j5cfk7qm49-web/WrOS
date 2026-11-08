# Warm screen / blue-light filter

Open **WrOS Settings → System → Display → Warm screen**. Check display support,
turn the filter on, choose **1000–6500 K** and **0–100% strength**, then select
**Preview**. Lower temperatures look warmer; the initial choice is **2200 K**.
Choose **Keep** within the remaining preview time, or **Cancel / Esc** to restore
the previous colors. Leaving WrOS or losing the client cannot confirm a change.
Off restores the original color curves. Confirmed settings are reapplied at boot
when the driver supports them. The filter is initially off and needs no network.

These are approximate manual warmth controls, not a measured display white point
or a medical claim. The transform uses relative Planck spectral ratios at three
representative wavelengths, normalized to 6500 K, with green/blue floors for
legibility. On the native display and console it scales captured calibration
instead of replacing it with a linear ramp. Adjustments start from that calibration.
6500 K, zero strength and Off preserve its exact sample values.

## Under cage (foot and the OpenGL window)

Both compositor sessions offer the same Warm screen page. The writing executable
includes a small private colour worker. Its entry point runs before project or
input setup. It binds only display outputs and the gamma-control protocol; it never
binds a keyboard, pointer or surface. A bounded pipe carries only status, preview,
Keep and Cancel. The worker has no book path or password terminal.

Before cage starts, the authorized display helper stops the native boot colour
service and restores any verified native baseline. A failed handoff disables
compositor warmth while writing still opens. Once cage exits, the launcher queues
the existing native service again, including before a console fallback. The two
colour paths therefore do not try to own an active compositor's display together.

The colour worker applies bounded curves through cage. Its own poll deadline
reverts an unconfirmed preview after 15 seconds even if the writing process is
stopped. Closing the request pipe ends the worker; losing its Wayland connection
or destroying the controls returns outputs to the compositor's original curves.
Off, zero strength and 6500 K release the controls instead of writing identity
curves. Any failed output releases all controls. Hotplug invalidates the session's
colour controls; reopen writing to check support again. Colour-worker failure
leaves the writing app usable and makes warmth unavailable.

This protocol supplies curve sizes and failure events, **no calibration readback
or presentation acknowledgement**. Warmth uses linear curves while on and may
change calibrated colours. Off restores the compositor's original curves. A
successful reply means the compositor accepted the request, not that WrOS read
back a physical display. The page labels this as an accepted session preference.
The [protocol](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-gamma-control-unstable-v1.xml)
defines exclusive controls and restoration on destruction;
[cage 0.2](https://github.com/cage-kiosk/cage/blob/v0.2.0/cage.c) exports that interface.

Keep persists only bounded preferences through the existing polkit helper. The
root helper never opens a Wayland socket. It checks the reviewed configuration
hash, rejects native transaction state and preserves unknown settings. The
fsynced generation is the commit receipt, including when a reply is lost. Keep
must reach the colour worker before expiry; a failed save restores the previous
confirmed choice. Saved compositor choices are applied at the next login.

The protocol fixture exercises two outputs, malformed/missing/oversized support,
partial failure, disconnect, hotplug and Off restoration. A separate-process test
stops a disposable parent while the real worker expires its preview, checks that
the worker has no input devices or tty, then closes its pipe and verifies exit.
The real cage/OpenGL app test checks the headless driver's honest refusal while
retaining physical Colemak, compose and saved Unicode text. These are protocol
and software tests, not positive HP Stream gamma or calibration qualification.

[The rc10 colour checkpoint](evidence/rc10-compositor-warmth.json) records the
source identity, every focused software check and all three installed VM runs.
The [foot VM screen](evidence/rc10-compositor-warmth.png) shows the honest
unsupported-driver result. Both compositor VMs also pass two locks, suspend/wake,
password scans and colour-worker crash isolation; the same executable passes the
native input, lock, suspend and reader-crash regression.

## Native service and security boundary

Grimoire uses its existing bounded worker and typed `wros-sys` requests.
The render/input thread does no device, process or password work. Shared Settings
retains both editors, selection, caret, scroll, focus and undo history. A late
preview reply is cancelled without reopening Settings.

The narrow root-owned `/usr/libexec/wros-display` endpoint accepts only status,
bounded preview choices and token-bound Keep/Cancel. The `org.wros.display`
polkit rule permits active local members of the existing `wros` group;
it grants no shell, command, path or device access. Other callers are denied.
`pkexec --disable-internal-agent` prevents a competing terminal password reader
([upstream authentication-agent documentation](https://www.freedesktop.org/software/polkit/docs/latest/pkexec.1.html)).
The same flag also protects existing keyboard helper calls. Systemd-only boot
and rollback verbs are refused to polkit clients.

The non-setuid C bridge links Debian's maintained libdrm. It opens only fixed
`/dev/dri/card0..63` devices, requires DRM ownership, and captures **every active
output or fails**. It bounds counts, sizes and total data, verifies connector,
EDID, CRTC, mode and ramp identity, then applies and reads back all curves. A
partial setter or readback failure attempts exact restoration of every original
curve. A changed physical output is never given another monitor's calibration.
Drivers without readable/writable gamma controls report unavailable. Another
graphics session owning DRM also causes refusal; there is no palette/theme
substitute or unverified-success fallback.

Root-only state under `/run/wros-display` records the original baseline, expected
current curves and pre-preview restoration snapshot. A locked transaction arms
the independent **15-second systemd rollback timer before any color write**.
Failed or interrupted writes retain recovery state. Confirmation checks the
requesting UID, random token, monotonic deadline, actual readback and unchanged
configuration. An atomically written and fsynced generation in
`/etc/wros/display.json` is the durable commit point; a delayed timer cannot undo
it. Off confirmation clears the per-boot baseline after restoration. Unknown
configuration versions and external configuration changes are preserved. A gamma
mismatch before a new preview refuses that preview. While a preview owns the
output, its original snapshot remains the rollback target; concurrent color
managers during that transaction are not qualified.

Production staging compiles the hardened bridge on the declared Debian builder,
installs the helper, exact policy and units, and includes `libdrm2` in the offline
media payload. Build prerequisites include `libdrm-dev`. The fake DRM test library
is never staged or used by the installed root manager, which clears its child
environment. No host display was modified during development.

## Qualification and limits

Twenty normal-user Python cases exercise the actual hardened C bridge against a
test-only fake multi-output DRM implementation and real temporary manager state.
They cover malformed/duplicate/truncated input, unsupported outputs, changed
topology, partial failure, lying readback, incomplete restoration, calibration,
timer-arm failure, expiry/ownership, future/external preferences, durable-commit
crashes, startup and Off restoration. These are controlled fault tests, not
physical display or kernel power-cut qualification.

Actual retained-editor UI cases cover four terminal sizes, Keep/Cancel/timeout,
unavailable services and delayed replies. The compiled PTY navigation also
enters the real warm-screen page while checking paste/cut/undo and source bytes.
The [fresh full software run](evidence/warm-screen-software.json) passed **928
Rust tests per overlapping configuration**, both Clippys, **96 Python methods /
zero skips** and **18 gates**. Default/no-audio intentionally ignore 5/4 separate
external/performance cases. Source and verifier inputs stayed unchanged; binary
SHA `911c8675789e57474155a084adfdffdb1d230bab8ed089b1f44c70fee9613c7c`
matches the guest-tested binary. Historical M7b evidence remains untouched.
Published head `8e2aeeb` also passed real Debian/macOS CI
[40466790817](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40466790817);
the [unmodified CI report](evidence/warm-screen-ci.json) has the same source
fingerprint and 928/96 totals. The CI tested merge revision is recorded separately
from the source branch head.
The initial history secret scan flagged three deterministic simulator/invalid-reply
tokens. Each was inspected and confirmed to be a noncredential fixture. The
repository's `.gitleaksignore` lists only those exact commit/file/rule/line
fingerprints; no source path or token pattern is broadly excluded.

`os/tests/display_vm.py` runs the current binary, helper, bridge, policy and units
in a new VM-demo COW layer with no NIC or host disk/display exposure. A real
emulated driver refusal is recorded separately from positive gamma/timer/Off
acceptance. This fixture does not qualify a newly built ISO or physical screen.

The [current-source guest run](evidence/warm-screen-vm.json) passed in **88.51 s**
on two emulated CPUs / 512 MiB / no NICs. Source fingerprint
`30691d88e1a667be4cea780ffdf2b9b46fcf62dba08d3bb7cd3dd42c89edc863`
(287 files) and the base disk remained unchanged. The real virtio GPU reported
**no supported gamma controls**: refusal/no changes, root-only bridge, active
writer authorization, inactive-user denial and native Settings navigation passed.
This is explicitly **not positive gamma-application qualification**. The actual
[native page](evidence/warm-screen-vm.png) was inspected. The app used 35,768 KiB
RSS; an on-demand unsupported-driver status took 0.834 s / 0.770 CPU s under
TCG, with child RSS high water 17,468 KiB including earlier service commands.
Those figures do not measure real hardware filtering or battery consumption.

Two earlier automation attempts failed to enter Settings because the dashboard
owns its keys. Their reports remain failed. The successful fixture returns to
the retained writing view before opening the palette and waits for actual
child-page controls at each step; no acceptance check was removed.

Hotplug, mode changes, suspend/VT transitions and another color manager can
invalidate ownership or topology. The helper conservatively refuses mismatched
state and does not automatically reset another output. Full recovery across
those transitions, physical multi-monitor/colorimeter checks, complete renewed
installer acceptance and production display qualification remain on
the hardware checklist. The native boot fix was
[confirmed on the HP Stream for rc6](https://github.com/j5cfk7qm49-web/WrOS/issues/30#issuecomment-6691737700).
That does not qualify the new compositor colour path, which remains part of
[#92](https://github.com/j5cfk7qm49-web/WrOS/issues/92). Do not infer universal
display support or physical colour accuracy from VM and protocol tests.
