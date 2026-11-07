# Graphical session (preview)

WrOS can run the writing app full screen in a graphical terminal: cage (a
one-window Wayland compositor) runs foot (a terminal), which runs grimoire. The
console session stays, and it is what WrOS falls back to. This is the groundwork
for the graphical release planned for rc10 (#92).

## Turning it on

1. Install cage and foot (needs the network and your password, once):
   `sudo apt-get install --no-install-recommends cage foot`
2. Settings › System › Display › **Graphical session (preview)**. The row shows
   only on an installed WrOS with both programs present. The choice takes
   effect at the next login.

The choice is kept in `~/.config/wros/session.json`. WrOS updates do not install
cage or foot: until the installer ships them, they come from Debian directly.

## How a login chooses

`/usr/libexec/wros-session` (run after the login on tty1) starts the console
session unless all of these hold:

- the writer turned the graphical session on;
- cage and foot are installed, and the login has a runtime folder;
- this is not the live installer session;
- recovery did not ask for the console this boot (below), and the boot line has
  no `wros.session=console`;
- the graphical session has not failed twice in a row.

The app writes `$XDG_RUNTIME_DIR/wros-session-ready` once it has drawn its first
screen. If cage ends before that, the launcher counts a failure in
`~/.local/state/wros/session.json`, puts tty1 back into text mode, and starts the
console session straight away. The app then says why: once, that the graphical
session will be tried again at the next login; after two failures in a row,
that it stays on the console until the writer turns it on again (which also
clears the count). A graphical session that reached its first screen clears the
count.

The recovery menu (Esc at boot) offers **Continue in the console session**
when the writer has the graphical session on. It lasts until the next restart.

The graphical session gets the console's keyboard layout. A layout chosen in
WrOS maps to XKB (`colemak` to `us(colemak)`, `colemak-dh-ansi` to
`us(colemak_dh)`, `colemak-dh-iso` to `us(colemak_dh_iso)`). Before one is chosen,
Debian's installer layout in `/etc/default/keyboard` is used, as the console does.

foot's settings are `/usr/share/wros/foot.ini`: DejaVu Sans Mono 11, black
background, and no binding that opens another terminal window (under cage it
would cover the app with a shell).

## Not in the graphical session yet

- **Lock and sleep on lid close.** The lock hands tty1 to vlock on tty8, and the
  app's checks need tty1 itself, so both say "not in the graphical session yet".
  cage has no lock protocol, so rc10's lock will keep switching to a console.
- **Warm screen.** The warm-screen helper drives the display directly, which it
  cannot do while cage holds it. cage supports the gamma-control protocol, so
  rc10 can tint through cage instead.

## Measured on the HP Stream (2026-11-06)

cage 0.2.0 and foot 1.21.0 from Debian 13: 88 packages, about 59 MB to download
and 235 MB installed (Mesa and LLVM are most of it). On the headless backend,
cage, foot and the app used about 18, 17 and 20 MB.

cage offers, among others: `zwlr_gamma_control_manager_v1` (warm screen),
`zwlr_screencopy_manager_v1` (exact screenshots for tests),
`zwp_virtual_keyboard_manager_v1` (typed input for tests) and
`ext_idle_notifier_v1`. It has no `ext_session_lock_v1`.

Not yet measured on the real display: start time, typing latency, and memory
with Mesa's GPU renderer.

## Tests

- `os/tests/test_session_launcher.py`: every choice, the fallback and its
  count, the recovery flag and the boot line, and the XKB mapping, with fakes.
- `os/tests/test_session_headless.py`: real cage and foot on wlroots' headless
  backend (skipped when they are missing; needs a writable `/dev/shm`). With
  `WROS_HEADLESS_GRIMOIRE` set to a built grimoire, the real app must report its
  first screen from inside foot.
- `src/session_ui_tests.rs`: the Settings toggle and the console-only rows.
- `os/tests/test_recovery_menu.py`: the recovery menu's console choice.
