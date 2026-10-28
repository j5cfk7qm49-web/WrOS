# Live WrOS session

The experimental `build-installer.sh --live` profile uses Debian live-build,
live-boot, live-config and user-setup. Grimoire is the actual live writing desk,
with the same theme, panels, keyboard navigation and settings as the installed
session. Its home and settings identify the live session and explain that work
must be saved somewhere persistent before restarting.

Debian live-config creates the temporary `wros-live` account at boot. A final
live-config component checks the explicit `boot=live wros.live=1` selection,
locked root account, reserved non-root account and read-only live medium. It
publishes a temporary greetd configuration under `/run/wros-live`. The one-time
initial session enters the normal WrOS session wrapper; greetd retains its
authenticated default session. The stock live-config getty generator is masked
so it cannot also open accounts automatically on other consoles. Documented
`noautologin` and `nottyautologin` aliases still prevent automatic entry.

The immutable SquashFS contains neither the temporary account nor its runtime
configuration. `/etc/greetd/config.toml` remains the authenticated installed
configuration. The maintained live-installer copies that immutable filesystem,
rather than the running live overlay. Actual installation and reboot checks
remain necessary to qualify the complete profile.

This public live account has Debian's standard temporary live credentials and
live administration privileges. It is not a personal authenticated session.
WrOS disables its personal screen-lock action in both the interface and the
privileged lock helper. Installed sessions retain the existing lock path.
Sleep remains disabled pending separate qualification.

The native installer frontend is staged for further integration. This profile
does not yet provide the privileged in-session installer supervisor. The
separate Debian Installer boot entry remains the available installation path;
the native frontend's protocol tests do not certify an installation from the
live writing desk. Matching in-session kernel/backend components and a complete
offline package pool must be integrated before that path can be enabled.

`tools/audit-live-session.py` checks the immutable staged profile. Unit tests
exercise account/boot selection, manual-entry options, publication ordering,
file preservation, permissions and staging leaks using private fixture files.
`live_session_uses_shared_surfaces_and_never_requests_personal_lock` exercises
the actual Grimoire controller and renderer at four terminal sizes. These
checks are not evidence of an actual live boot or physical hardware support.
