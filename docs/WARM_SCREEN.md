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
legibility. It scales the captured calibration instead of replacing it with a
linear ramp. Repeated adjustments always start from that original calibration.
6500 K, zero strength and Off preserve its exact sample values.

## Service and security boundary

Grimoire uses its existing bounded worker and typed `writingdeck-sys` requests.
The render/input thread does no device, process or password work. Shared Settings
retains both editors, selection, caret, scroll, focus and undo history. A late
preview reply is cancelled without reopening Settings.

The narrow root-owned `/usr/libexec/wros-display` endpoint accepts only status,
bounded preview choices and token-bound Keep/Cancel. The `org.wros.display`
polkit rule permits active local members of the existing `writingdeck` group;
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
installer acceptance and production display qualification remain open under
**issue #30**. Do not claim production or universal laptop support from VM tests.
