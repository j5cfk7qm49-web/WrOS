#!/usr/bin/env python3
"""Read-only gate for native login and power guards in a staged WrOS image."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

REPO = Path(__file__).resolve().parents[1]
SLEEP_TARGETS = ('sleep', 'suspend', 'hibernate', 'hybrid-sleep', 'suspend-then-hibernate')


def regular(root, relative):
    path = root / relative
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError('missing or unsafe native session file: ' + relative)
    return path


def audit(root):
    root = Path(root)
    if root.is_symlink() or root.resolve() == Path('/'):
        raise ValueError('inspect a staged image directory, never the live root')
    root = root.resolve(strict=True)
    package = subprocess.check_output([
        'dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-W',
        '-f=${Version}\t${Status}', 'greetd'], text=True).split('\t')
    if len(package) != 2 or package[1] != 'install ok installed':
        raise ValueError('maintained Debian greetd package is not installed')
    binaries = {}
    for relative in ('usr/sbin/greetd', 'usr/bin/wros-greeter', 'usr/bin/grimoire',
                     'usr/libexec/writingdeck-session', 'usr/libexec/writingdeck-keyboard'):
        path = regular(root, relative)
        if not path.stat().st_mode & 0o111 or path.stat().st_mode & 0o6022:
            raise ValueError('unsafe native session executable permissions: ' + relative)
        binaries[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    regular(root, 'etc/pam.d/greetd')
    regular(root, 'usr/share/doc/greetd/copyright')
    config = tomllib.loads(regular(root, 'etc/greetd/config.toml').read_text())
    if config != {'terminal': {'vt': 1, 'switch': True},
                  'general': {'source_profile': False},
                  'default_session': {'command': '/usr/bin/wros-greeter', 'user': '_greetd'}}:
        raise ValueError('native login must use the fixed unprivileged tty1 greeter without autologin')
    accounts = [row.split(':') for row in (root / 'etc/passwd').read_text().splitlines()
                if row.startswith('_greetd:')]
    passwords = [row.split(':')[1] for row in (root / 'etc/shadow').read_text().splitlines()
                 if row.startswith('_greetd:')]
    if (len(accounts) != 1 or len(accounts[0]) != 7 or not accounts[0][2].isdigit()
            or int(accounts[0][2]) == 0 or not accounts[0][6].endswith(('/nologin', '/false'))
            or len(passwords) != 1 or not passwords[0].startswith(('!', '*'))):
        raise ValueError('native greeter service account must be non-root, locked and nonlogin')
    for source, relative in (
            ('greetd.service', 'etc/systemd/system/greetd.service'),
            ('51-wros-greeter-keyboard.rules', 'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules'),
            ('60-wros-power-guard.conf', 'etc/systemd/logind.conf.d/60-wros-power-guard.conf'),
            ('60-wros-sleep-guard.conf', 'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf')):
        if regular(root, relative).read_bytes() != (REPO / 'os/config' / source).read_bytes():
            raise ValueError('native session policy differs from qualified source: ' + relative)
    enabled = root / 'etc/systemd/system/multi-user.target.wants/greetd.service'
    if not enabled.is_symlink() or os.readlink(enabled) not in (
            '../greetd.service', '/etc/systemd/system/greetd.service'):
        raise ValueError('native greetd service is not enabled')
    for name in ('getty@tty1.service', *(target + '.target' for target in SLEEP_TARGETS)):
        path = root / 'etc/systemd/system' / name
        if not path.is_symlink() or os.readlink(path) != '/dev/null':
            raise ValueError('unsafe native image: unmasked ' + name)
    maintenance = root / 'etc/systemd/system/getty@tty2.service'
    if maintenance.is_symlink() and os.readlink(maintenance) == '/dev/null':
        raise ValueError('authenticated tty2 maintenance must remain available')
    return {'schema_version': 1, 'scope': 'Staged native session/power policy; not installed boot or secure locking',
            'passed': True, 'greetd_package_version': package[0], 'binary_sha256': binaries,
            'native_tty1_login': True, 'autologin': False, 'greeter_account_locked': True,
            'sleep_targets_masked': list(SLEEP_TARGETS), 'automatic_power_actions_disabled': True,
            'secure_locking_verified': False, 'physical_hardware_verified': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(audit(args.root), indent=2))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from None
