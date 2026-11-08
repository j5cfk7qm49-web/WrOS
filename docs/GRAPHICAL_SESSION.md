# Graphical screens

Graphical rendering is the default for disk unlock, loading, login and writing,
as requested by the owner on 2026-11-07. The installed build includes `gui-drm`.
`soft_ratatui` shapes the shared Ratatui screens with DejaVu Sans Mono and sends
pixels directly to a DRM/KMS dumb buffer. It needs neither a compositor nor
Vulkan drivers. It keeps the existing keyboard layout and console input path.

The writing session starts with `WROS_SESSION=drm`. Missing session settings mean
native graphics; an explicit `session: "console"` retains text. Settings › System
› Display › Graphical session changes the next login. The login form also offers
a console choice for one login. Live sessions default to native graphics too.

The existing cage/foot trial remains available with
`{"version":1,"session":"graphical","backend":"foot"}` in
`~/.config/wros/session.json`, after installing Debian's `cage` and `foot` packages.
Its first-frame marker and two-failure fallback are retained. The comparison with
the wgpu backend is still a separate rc10 trial task.

## Lock, sleep and screen colour

The native writer stays in its authenticated tty1 session. Before starting the
independent console locker, it retires its input process and releases the native
display, restores the old scanout, and returns the VT to text mode. After the
maintained locker authenticates, it flushes input, creates a fresh display and
starts a new input process. Authentication continues to belong to vlock/PAM;
Grimoire never receives lock credentials. The same handoff is used for suspend
and the live installer.

For an ordinary VT switch, the renderer acknowledges release, restores the
console and releases DRM master. It redraws on acquisition. Between modesets it
does not hold DRM master, so the existing narrow warm-screen helper can set
gamma. The cage/foot trial still needs its separate gamma and lock integration.

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

The release VM chain accepts `--bootscreen` to include graphical unlock,
loading/login handoff, writing, VT switching, lock, renderer SIGKILL and text
recovery after the saved-system update and feature checks. Its screenshots use
the exact glyph atlas from `cargo run -p wros-graphics --example font_atlas`;
set `WROS_NATIVE_FONT` to that PPM file when running the VM harness.
