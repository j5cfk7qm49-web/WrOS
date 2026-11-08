# Graphical renderer trial

Native graphics remains the default for unlock, loading, login and writing,
following the owner's decision. The optional OpenGL writing window now works
alongside the foot trial. Native device input and compositor warmth are built.
The HP Stream measurements and positive compositor-colour checks are still
required before this trial is complete.

## Measurements, 2026-11-08

This checkpoint includes native libinput/XKB and the compositor colour process.
It precedes the remaining interface libraries, so it is not a final release
measurement. The [earlier console-reader baseline](evidence/rc10-graphical-trial.json)
is preserved separately; its latency values are summaries, not individual samples.

Release binaries used the same app source and Cargo lock, with audio disabled.
The foot binary includes `gui-drm`; the window binary adds `gui-wgpu`. Both use
DejaVu Sans Mono: foot's existing 11 pt setting and the native/window renderers'
embedded 16 px font. The OpenGL instance explicitly selects GL;
no Mesa Vulkan driver was installed. Three runs used fresh temporary homes and
the same tutorial demo book. The measured executables were extracted from the
unsigned comparison packages after their final ELF strip; their hashes match
the package payloads. [Current observations and hashes](evidence/rc10-graphical-trial-current.json)
retain every run and all 36 key/capture observations per compositor.

| Renderer and environment | Median first draw | Median total RSS | App binary | Update package | Median key to screenshot |
| --- | ---: | ---: | ---: | ---: | ---: |
| foot/cage, headless Debian chroot | 52.8 ms | 76.5 MiB | 27.71 MiB | 10.08 MiB | 55.5 ms |
| wgpu/cage, OpenGL with software Mesa in that chroot | 275.3 ms | 195.8 MiB | 34.25 MiB | 12.28 MiB | 82.2 ms |
| Native software DRM, separate QEMU/KVM guest | 45.9 ms | 39.0 MiB | 27.71 MiB | 10.08 MiB | Hardware check pending |

The marker records the app's first completed draw, polled every 5 ms. Foot may
still be painting those terminal bytes. The three startup samples were
160.7/47.0/52.8 ms for foot and 463.8/275.3/275.3 ms for wgpu; filesystem and driver
startup costs are visible in the variation. These were repeated starts, without
forcing a cold filesystem cache.

The key test sends physical Colemak keys through cage and waits for changed
editor pixels using Wayland screen capture at 1280×720. Each run has 12 samples;
the table pools all 36, retained to 0.001 ms. The number
includes roughly 38–39 ms of capture and PNG decoding overhead; it is an upper
measurement through that capture path, with no overhead subtracted. Focus mode
removes animated decorations, and two matching captures establish a stationary
editor before each key. This does not measure the laptop panel's scanout.

Memory sums the run's complete process tree, including foot's separate PTY
session, the independent input reader and the separate colour process. RSS can
count shared pages twice.
PSS is also recorded where readable; the protected input reader intentionally
does not expose it. Native DRM needs a display device absent from this chroot,
so its sample comes from a separate installed 1280×800 guest and is not a timing
ranking against the other two rows. The guest verified real delegated input
devices during measurement. Native key-to-screen latency remains unmeasured.

The unsigned measurement packages contain identical boot/login payloads and
omit Google client configuration. The only differing payload is Grimoire itself.
Their size difference is about 2.20 MiB; the executable difference is about
6.53 MiB. Cage, Mesa and foot's Debian package
costs are separate. These packages are local measurement artifacts.

**Recommendation:** keep the chosen native default while completing its hardware
checks. Keep foot available as the compositor alternative. The chroot results give
no reason to make wgpu the default: it takes more memory and starts more slowly
with software Mesa. The HP Stream's Intel driver, typing delay, power use and a
day of writing still decide whether that holds on the laptop.

## Running the tool

`tools/graphical-trial.sh` uses a temporary demo book and private home for every
run. It records sizes, hashes, startup and process memory in a new output folder.
It never opens the writer's book, changes session settings or handles passwords.
The native trial verifies graphics mode and refuses to count a text fallback.

Build the two writing executables in the Debian build environment, preserving
each result before building the next:

```sh
cargo build --release --locked --no-default-features --features gui-drm -p grimoire-tui
# Preserve target/release/grimoire as /path/to/native/grimoire.
cargo build --release --locked --no-default-features --features gui-drm,gui-wgpu -p grimoire-tui
# Preserve target/release/grimoire as /path/to/wgpu/grimoire.
```

For the headless comparison:

```sh
sh tools/graphical-trial.sh --headless --backend foot --backend wgpu \
  --binary /path/to/native/grimoire --wgpu-binary /path/to/wgpu/grimoire \
  --runs 3 --latency 12 --output /path/to/new-results
```

This needs cage, foot, Mesa, `grim`, Pillow, a C compiler, `libwayland-dev` and
`libxkbcommon-dev`, plus a writable `/dev/shm`. The test keyboard and its licensed
protocol live in `os/tests/`; generated bindings stay in the temporary folder.
`--package /path/to/wros.deb` records an exact package size. Use separate
invocations when comparing different packages.

On the laptop, run as the writer from an authenticated tty1 with the current
writing session stopped. An ordinary desktop terminal or SSH session is refused.
Setting up that temporary tty1 login is an owner administration step. Omit
`--headless` and select one backend; for example:

```sh
sh tools/graphical-trial.sh --backend drm --binary /path/to/native/grimoire \
  --runs 3 --output /path/to/new-native-results
```

This native invocation passed three real DRM samples on a disposable VM's tty1.
For a compositor, `--latency 12` adds the same virtual-key/capture measurement.
Measure native display latency separately with a physical keyboard and panel
recording. Never run the app or tool as root.

## Input, lock and remaining checks

The real cage/OpenGL driven test types physical Colemak keys, composes é and
saves Unicode prose. Both compositor backends passed installed no-NIC VM tests
for wrong-password rejection, two locks in one boot, blocked VT switching,
actual suspend/wake, the same editor process after authentication, and password
scans of books, recovery files and logs. The canonical driver is
`os/tests/graphical_session_vm.py`; it uses a debug `test-driver` executable in a
read-only fixture image and records the exact helper and executable hashes.

VM run directories under `/home/builder/rc-vm-runs/`:

- `graphical-foot-20261108T072444Z`: compositor locks and suspend/wake, PASS.
- `graphical-wgpu-20261108T072744Z`: the same checks for OpenGL, PASS.
- `native-trial-20261108T073033Z`: native measurement tool on tty1, PASS.
- `graphical-drm-20261108T123102Z`: native device input, VT/lock/suspend and reader-crash recovery, PASS.
- `graphical-wgpu-20261108T123305Z`: 12 locks and suspend/wake, PASS.
- `graphical-foot-20261108T185909Z` and `graphical-wgpu-20261108T190209Z`:
  colour-worker isolation, honest driver refusal, two locks and suspend/wake, PASS.
- `graphical-drm-20261108T190412Z`: native input/VT/lock/suspend and reader-crash
  regression on the compositor-capable executable, PASS.

The second-lock race in [#101](https://github.com/j5cfk7qm49-web/WrOS/issues/101)
was reproduced by delaying the post-login health query: the screen offered Lock
while its busy guard refused the action. The row now explains the wait and its
private action becomes available only when the request finishes. The VM client
waits for that action and verifies the actual independent locker.
[The lock regression record](evidence/rc10-lock-action.json) retains the controlled
failure, its passing OpenGL and foot six-lock/suspend runs, the native regression,
and the earlier failures. The original intermittent run had no busy-state capture;
the controlled test establishes this cause without reconstructing that run's timing.

Native evdev/xkbcommon, compose, live layout changes, key repeat and pointer input
run in the disposable reader. Logind delegates and revokes device descriptors;
the editor receives no input-group grant. Its VT release, repeated lock and
suspend/wake contract is described in [Graphical screens](GRAPHICAL_SESSION.md).
Compositor warmth now uses a separate colour process and cage's gamma controls,
with an independent preview deadline and Off restoration. Protocol fault tests
cover two outputs; the real headless driver honestly reports unavailable.
The [colour checkpoint](evidence/rc10-compositor-warmth.json) retains exact source
and executable identities, the separate-process expiry proof and the VM results.
Neither protocol acceptance nor screenshots establish calibrated physical output.
Native hardware latency, positive compositor-colour checks and the HP Stream's
day-of-writing comparison remain open. [Warm screen](WARM_SCREEN.md) explains
the calibration limits and the difference from the native colour helper.
