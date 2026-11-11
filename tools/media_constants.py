"""Constants shared by the installer-media tools (stager, live audit, media audit).

Each value has exactly one owner. This module only reads those owners, so a
re-baseline is made in one place and every consumer follows it:

* the cdebconf passthrough module hash is owned by the production supervisor
  (``os/scripts/wros-install-session``), which refuses to run against a capsule
  or an unpacked installer whose module differs;
* the qualified live kernel ABI is owned by ``tools/build-live-installer-pool.py``;
* the Debian installer launcher pins are defined here because no other file may
  decide what the supervisor loads from ``/usr/share/debian-installer-launcher``.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
INSTALL_SESSION = ROOT / 'os/scripts/wros-install-session'
POOL_BUILDER = ROOT / 'tools/build-live-installer-pool.py'

# Rust binaries in both the live image and the installed target.
BINARY_PATHS = {'grimoire': 'usr/bin/grimoire', 'wros-greeter': 'usr/bin/wros-greeter',
                'wros-boot': 'usr/lib/wros/wros-boot', 'wros-installer': 'usr/bin/wros-installer'}

# Debian trixie debian-installer-launcher 44 (arch all). live-build does not
# install it, and wros-install-session sources plugins/live from it, so the
# live package list requests it and every audit pins the loaded files by hash.
LAUNCHER_PACKAGE = 'debian-installer-launcher'
LAUNCHER_VERSION = '44'
LAUNCHER_FILES = {
    'usr/share/debian-installer-launcher/plugins/live':
        ('ca3501d8ecb1e9f6b206390d0f07034a1bcb8795a84988e7ef04864104210ff8', 0o644),
    'usr/share/debian-installer-launcher/debconf_kernel.sh':
        ('e500c3c009c2c51837b7d76925fd8dcf4553bdd128542cbe60ee21e3ce37ef79', 0o755),
}
# Programs the launcher plugin runs inside the live session (unpack and cleanup).
LAUNCHER_RUNTIME = ('usr/bin/cpio', 'usr/bin/fuser')


def _single(path, pattern, label):
    found = re.findall(pattern, path.read_text(encoding='utf-8'))
    if len(found) != 1:
        raise ValueError(f'expected exactly one {label} in {path.name}, found {len(found)}')
    return found[0]


def module_sha256(install_session=INSTALL_SESSION):
    """The cdebconf passthrough hash the live supervisor will accept."""
    return _single(Path(install_session), r"(?m)^MODULE_SHA256 = '([0-9a-f]{64})'$", 'MODULE_SHA256')


def qualified_kernel_abi(pool_builder=POOL_BUILDER):
    return _single(Path(pool_builder), r"(?m)^ABI = '([0-9]+\.[0-9]+\.[0-9]+\+deb13-amd64)'$", 'kernel ABI')
