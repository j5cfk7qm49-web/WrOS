# Graphical screens

Graphical rendering is the default for disk unlock, loading, login and writing,
as requested by the owner on 2026-11-07. The installed build includes `gui-drm`.
`soft_ratatui` shapes the shared Ratatui screens with DejaVu Sans Mono and sends
pixels directly to a DRM/KMS dumb buffer. It needs neither a compositor nor
Vulkan drivers. Disk unlock, loading and login keep their console input path.
The native writing session uses libinput devices and xkbcommon in its disposable
input process. It reads Debian's keyboard layout, watches layout changes, and
supports compose, key repeat and a cell-based mouse/touchpad with a visible cursor.
Only root-owned XKB and Compose data are loaded. PTYs and unavailable native
devices retain terminal input.

The writing session starts with `WROS_SESSION=drm`. Missing session settings mean
native graphics; an explicit `session: "console"` retains text. Settings › System
› Display › Graphical session changes the next login. The login form also offers
a console choice for one login. Live sessions default to native graphics too.

The existing cage/foot trial remains available with
`{"version":1,"session":"graphical","backend":"foot"}` in
`~/.config/wros/session.json`, after installing Debian's `cage` and `foot` packages.
Its first-frame marker and two-failure fallback are retained. Build the optional
`gui-wgpu` feature and select `"backend":"wgpu"` in the same file to run Grimoire
directly under cage. This uses ratatui-wgpu with OpenGL and a Wayland-only winit
window. It does not install or select Vulkan drivers. Each compositor backend has
its own startup failure count. New sessions still use the native renderer.
The trial needs `cage`, `libegl1`, `libgles2`, `libgl1-mesa-dri`,
`libwayland-egl1` and `libxkbcommon0`; foot also needs the `foot` package.

The window sends logical keys, compose/IME commits and cell-based pointer events
through the editor's existing event dispatch. It keeps the compositor's keyboard
layout. A real cage headless test types Colemak and composed é through Wayland,
saves the scene, and checks that the private test-driver tree contains no prose.
Run `cargo test --no-default-features --features test-driver,gui-wgpu --test driven`
with cage, Mesa, Wayland/XKB development packages and a writable `/dev/shm`.

## Lock, sleep and screen colour

The native writer stays in its authenticated tty1 session. Before starting the
independent console locker, it retires its input process and releases the native
display, restores the old scanout, and returns the VT to text mode. After the
maintained locker authenticates, it flushes input, creates a fresh display and
starts a new input process. Authentication continues to belong to vlock/PAM;
Grimoire never receives lock credentials. The same handoff is used for suspend
and the live installer.

Native devices are delegated through logind on the existing local tty1/seat0
session. The writer receives no input-group membership. Before lock, the child
closes every device and synchronously releases its seat controller; a killed or
failed native child cannot acknowledge safe retirement. On a VT switch it closes
devices and clears compose, modifier, drag and repeat state before resuming with
fresh device descriptors. The renderer reasserts its VT signals after each seat
acquisition without replacing the original restoration mode.

For an ordinary VT switch, the renderer acknowledges release, restores the
console and releases DRM master. It redraws on acquisition. Between modesets it
does not hold DRM master, so the existing narrow warm-screen helper can set
gamma. Compositor sessions can redirect the independent lock helper to the
authenticated, active tty1. The window clears its frame, retires queued input and
composition, and pumps Wayland without dispatching keys until authentication has
finished. The root helper accepts logind's Wayland type only when the packaged
cage executable is an ancestor with the same UID and session. It still requires
the active local tty1, seat0, VT1, user class, trusted writer and independent
vlock/PAM authentication. Gamma integration remains trial work.

## Recovery and packaging

**WrOS (text start)** selects Debian's disk prompt and text rendering for loading,
login and writing. Recovery's console override also disables graphics. Missing
or inaccessible DRM devices fall back to the console automatically.

The package carries all three graphical-capable executables, the font notice,
the independent console-reset executable, a sysusers rule giving the locked
`_greetd` account video-device access, and the boot-image hook. Updates apply the
rule and rebuild the boot image unless the writer explicitly chose plain text.
The original Debian password prompt is retained in every wrapped image.

Building the ordinary terminal app with `--no-default-features` omits native
graphics. WrOS release builders explicitly add `--features gui-drm`; desktop
terminal launches continue to use their terminal unless the session launcher
asks for native output.

Native writing builds need `libinput-dev`, `libudev-dev`, `libseat-dev` and
`libxkbcommon-dev` in addition to libdrm. The package declares the corresponding
runtime libraries, `xkb-data` and `libx11-data` for the system Compose table.

`grimoire --wros-renderers` reports the compiled renderer features as JSON without
opening a book. `gui-wgpu` belongs only to the writing executable, so enabling the
trial does not pull its window/graphics dependencies into the unlock or login
executables.

The release VM chain accepts `--bootscreen` to include graphical unlock,
loading/login handoff, writing, VT switching, lock, renderer SIGKILL and text
recovery after the saved-system update and feature checks. Its screenshots use
the exact glyph atlas from `cargo run -p wros-graphics --example font_atlas`;
set `WROS_NATIVE_FONT` to that PPM file when running the VM harness.

The [native input qualification record](evidence/rc10-native-input.json) retains
the isolated installed-VM checks for physical Colemak, compose, live layout
changes, repeat, cursor pixels, device retirement on VT switches, two locks,
suspend/wake and reader-crash recovery. The final native run is
`graphical-drm-20261108T123102Z`. Foot passed the lock/suspend regression. OpenGL
passed a 12-lock stress run. A later controlled regression reproduced a second
lock refused while the post-login health query was busy. The Lock row now shows
that it is waiting and becomes actionable after the request finishes.
[The follow-up record](evidence/rc10-lock-action.json) keeps the red/green checks
for [#101](https://github.com/j5cfk7qm49-web/WrOS/issues/101), including six locks
and suspend/wake. Earlier failed results and full software-check failures remain
in the input record.
