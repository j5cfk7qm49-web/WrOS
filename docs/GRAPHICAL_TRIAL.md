# Graphical renderer trial

WrOS 0.1.0~rc16 uses native DRM graphics by default. Cage/Foot and the OpenGL
window remain available as optional writing sessions. The launcher selects the
backend in [wros-session](../os/scripts/wros-session); NixOS packages its tool
paths through [helpers.nix](../nixos/pkgs/helpers.nix).

## Available renderers

| Renderer | Current path | Limit |
|---|---|---|
| Native DRM | Shared Ratatui screens rendered directly to DRM/KMS, with native libinput/XKB writing input | Default on installed and live systems; unavailable DRM falls back to the console |
| Foot under Cage | Patched Cage runs Foot, which runs the terminal writing app | Explicit installed-session choice |
| OpenGL window under Cage | Patched Cage runs the `gui-wgpu` writing renderer | Explicit installed-session choice; the launcher checks the Wayland EGL library |
| Console | Terminal output and input | Available from the login switcher and text-start recovery path |

[base.nix](../nixos/modules/base.nix) includes patched Cage and Foot.
[rust-packages.nix](../nixos/pkgs/rust-packages.nix) includes `gui-drm`, `gui-wgpu`
and `audio` in release Grimoire. No package installation or optional-feature
rebuild is needed to select a packaged renderer. The native screens use GNU
Unifont's 8x16 bitmaps in
[wros-graphics](../crates/wros-graphics/src/lib.rs). See
[Graphical screens](GRAPHICAL_SESSION.md) for input, fonts and recovery.

## Selecting a trial session

For Foot, set `~/.config/wros/session.json` to:

```json
{"version":1,"session":"graphical","backend":"foot"}
```

Set `backend` to `wgpu` for the OpenGL window. The choice takes effect at the
next login. The usual native default needs no session file. An explicit
`"session":"console"` chooses text; the login switcher's Console session choice
applies to one login. Live sessions keep the native default.

The launcher keeps a first-frame marker and failure state for each compositor
backend. Failure before the first frame opens the console; after two failures
that backend stays on the console until re-enabled. The implementation is in
[wros-session](../os/scripts/wros-session).

## Measurement tooling

The retained [graphical_trial.py](../tools/graphical_trial.py) creates private
demo projects and homes, records executable hashes and process memory, and measures
first draw. Its optional key test includes Wayland screenshot/capture overhead;
it does not measure native DRM key-to-panel latency.

The Cage/Foot measurement tool does not work unchanged on NixOS. It replaces
PATH with `/usr/local/bin:/usr/bin:/bin` and launches `cage`, `foot` and `grim` by
name. The packaged Cage and Foot use Nix store paths; Grim is absent from the
base package list. The script needs NixOS path adaptation and its measurement
dependencies before a NixOS run can be claimed. Non-headless runs require authenticated tty1
and refuse root, desktop terminals and SSH. This tool is separate from the
packaged session launcher.

## Input, lock and remaining checks

The [main-thread handoff](../src/console_lock.rs) retires input and display before
the independent vlock/PAM locker starts. Guarded suspend depends on installed
session support and the [systemd locker guard](../nixos/modules/guards.nix).
Compositor colour behavior and calibration limits are described in
[Warm screen](WARM_SCREEN.md).

The [NixOS live-graphical check](../nixos/tests/live-graphical.nix) checks the
native DRM session selection and continued app lifetime. The
[console-lock check](../nixos/tests/console-lock.nix) checks tty8 ownership,
authentication and input isolation in a console session. Those definitions do
not establish compositor trial passes or physical display, keyboard, colour,
latency, power-use or sleep/resume qualification. The
[Nix workflow](../.github/workflows/nix-ci.yml) evaluates the checks and runs the
driven OpenGL-window test under headless Cage, but not these VMs.

## History

The 2026-11-08 Debian renderer comparison and its executable identities remain in
the [measurement record](evidence/rc10-graphical-trial-current.json); Debian
lock regressions remain in the [lock record](evidence/rc10-lock-action.json).
The retained [Debian trial driver](../os/tests/graphical_session_vm.py) uses
cached `.deb` files and APT lists. Those measurements and guest results do not
qualify the current NixOS release.
