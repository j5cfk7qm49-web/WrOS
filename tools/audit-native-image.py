#!/usr/bin/env python3
"""Read-only native login, manual-lock payload and power-policy image gate."""
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
    locker = subprocess.check_output([
        'dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-W',
        '-f=${Version}\t${Status}', 'vlock'], text=True).split('\t')
    if len(locker) != 2 or locker[1] != 'install ok installed':
        raise ValueError('maintained Debian vlock package is not installed')
    binaries = {}
    for relative in ('usr/sbin/greetd', 'usr/bin/wros-greeter', 'usr/bin/grimoire',
                     'usr/libexec/writingdeck-session', 'usr/libexec/writingdeck-keyboard',
                     'usr/libexec/wros-lock', 'usr/bin/openvt', 'usr/bin/systemd-run', 'usr/bin/chvt'):
        path = regular(root, relative)
        if not path.stat().st_mode & 0o111 or path.stat().st_mode & 0o6022:
            raise ValueError('unsafe native session executable permissions: ' + relative)
        binaries[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    regular(root, 'etc/pam.d/greetd')
    regular(root, 'usr/share/doc/greetd/copyright')
    for relative in ('usr/sbin/vlock-main', 'usr/lib/vlock/modules/all.so',
                     'usr/lib/vlock/modules/nosysrq.so'):
        path = regular(root, relative)
        mode = path.stat().st_mode
        if not mode & 0o111 or mode & 0o022 or (relative != 'usr/sbin/vlock-main' and mode & 0o6000):
            raise ValueError('unsafe maintained locker permissions: ' + relative)
        if relative == 'usr/sbin/vlock-main' and mode & 0o7777 != 0o4755:
            raise ValueError('maintained vlock-main mode must match the Debian package')
        binaries[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    regular(root, 'usr/share/doc/vlock/copyright')
    pam_module = regular(root, 'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so')
    if pam_module.stat().st_mode & 0o6022:
        raise ValueError('unsafe maintained PAM guard permissions')
    binaries['usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so'] = hashlib.sha256(pam_module.read_bytes()).hexdigest()
    if regular(root, 'usr/libexec/wros-lock').read_bytes() != (REPO / 'os/scripts/wros-lock').read_bytes():
        raise ValueError('manual-lock helper differs from qualified source')
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
            ('50-wros-lock.rules', 'etc/polkit-1/rules.d/50-wros-lock.rules'),
            ('org.wros.lock.policy', 'usr/share/polkit-1/actions/org.wros.lock.policy'),
            ('vlock.pam', 'etc/pam.d/vlock'),
            ('60-wros-power-guard.conf', 'etc/systemd/logind.conf.d/60-wros-power-guard.conf'),
            ('60-wros-sleep-guard.conf', 'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf'),
            ('60-wros-no-connectivity-check.conf', 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf')):
        if regular(root, relative).read_bytes() != (REPO / 'os/config' / source).read_bytes():
            raise ValueError('native session policy differs from qualified source: ' + relative)
    enabled = root / 'etc/systemd/system/multi-user.target.wants/greetd.service'
    if not enabled.is_symlink() or os.readlink(enabled) not in (
            '../greetd.service', '/etc/systemd/system/greetd.service'):
        raise ValueError('native greetd service is not enabled')
    for name in ('getty@tty1.service', 'getty@tty8.service', 'autovt@tty8.service',
                 *(target + '.target' for target in SLEEP_TARGETS)):
        path = root / 'etc/systemd/system' / name
        if not path.is_symlink() or os.readlink(path) != '/dev/null':
            raise ValueError('unsafe native image: unmasked ' + name)
    maintenance = root / 'etc/systemd/system/getty@tty2.service'
    if maintenance.is_symlink() and os.readlink(maintenance) == '/dev/null':
        raise ValueError('authenticated tty2 maintenance must remain available')
    return {'schema_version': 2, 'scope': 'Staged native session/manual-lock payload/power policy; not runtime locking qualification',
            'passed': True, 'greetd_package_version': package[0], 'binary_sha256': binaries,
            'vlock_package_version': locker[0], 'manual_lock_payload_present': True,
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
