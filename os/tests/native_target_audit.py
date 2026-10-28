#!/usr/bin/env python3
"""Read-only target proof inside the marked, newly generated QEMU installation.

Never installed in the product. The guarded fixture invokes this in-target;
the command-line marker contains no credential. No disk write or unlock occurs.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tomllib

PACKAGES = ('cryptsetup', 'cryptsetup-bin', 'cryptsetup-initramfs', 'dmsetup',
            'lvm2', 'systemd-cryptsetup', 'greetd', 'vlock')


def require(value, reason):
    if not value:
        raise ValueError(reason)


def crypttab_entry(contents):
    rows = [line.split() for line in contents.splitlines()
            if line.strip() and not line.lstrip().startswith('#')]
    require(len(rows) == 1 and len(rows[0]) == 4, 'Exactly one encrypted fixture volume required')
    name, source, key, options = rows[0]
    require(re.fullmatch(r'[A-Za-z0-9_-]{1,128}', name) is not None
            and re.fullmatch(r'UUID=[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}', source) is not None
            and key == 'none' and 'luks' in options.split(','),
            'Fixture encryption must use a LUKS UUID and an interactive passphrase')
    return {'mapping': name, 'uuid': source[5:]}


def account_state(passwd, shadow, groups):
    users = [line.split(':') for line in passwd.splitlines()]
    require(all(len(row) == 7 for row in users)
            and len({row[0] for row in users}) == len(users), 'Unique target accounts required')
    require(not any(row[0] in ('wros-live', 'writer', 'builder', 'user') for row in users),
            'Live or build account persisted')
    matches = [row for row in users if len(row) == 7 and (row[0] == 'fixture' or row[2] == '1000')]
    require(len(matches) == 1 and matches[0][0] == 'fixture'
            and matches[0][2:4] == ['1000', '1000']
            and matches[0][5:] == ['/home/fixture', '/bin/bash'], 'Normal fixture account required')
    roots = [row for row in users if row[0] == 'root' or row[2] == '0']
    greeters = [row for row in users if row[0] == '_greetd']
    require(len(roots) == 1 and roots[0][0] == 'root' and roots[0][2] == '0'
            and len(greeters) == 1 and int(greeters[0][2]) > 0
            and greeters[0][5:] == ['/nonexistent', '/usr/sbin/nologin'],
            'Unprivileged native greeter required')
    secrets = [line.split(':') for line in shadow.splitlines()]
    require(all(len(row) >= 2 for row in secrets) and len({row[0] for row in secrets}) == len(secrets),
            'Unique target credential records required')
    passwords = {row[0]: row[1] for row in secrets}
    require(passwords.get('root', '').startswith(('!', '*'))
            and passwords.get('_greetd', '').startswith(('!', '*'))
            and passwords.get('fixture', '').startswith('$6$'),
            'Locked root/greeter and generated hashed account required')
    memberships = {row[0] for line in groups.splitlines()
                   if len(row := line.split(':')) == 4 and 'fixture' in row[3].split(',')}
    require({'writingdeck', 'sudo'} <= memberships, 'Fixture administration/writing groups missing')


def query(*arguments):
    reply = subprocess.run(list(arguments), stdin=subprocess.DEVNULL, capture_output=True,
                           check=True, timeout=90, env={'PATH': '/usr/sbin:/usr/bin:/sbin:/bin',
                                                       'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'})
    require(len(reply.stdout) <= 4*1024*1024 and len(reply.stderr) <= 65536, 'Bounded target query required')
    return reply.stdout.decode('utf-8')


def digest(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022,
            'Trusted target payload required')
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fixture_guard(token):
    require(os.geteuid() == 0 and re.fullmatch('[0-9a-f]{32}', token) is not None,
            'Marked target audit required')
    require(Path('/sys/class/dmi/id/product_name').read_text().strip() == 'WrOS-Installer-Fixture'
            and Path('/sys/class/dmi/id/product_serial').read_text().strip() == token,
            'Only the marked disposable guest may be audited')
    root = Path('/sys/class/block/vda')
    require((root/'size').read_text().strip() == str(16*1024**3//512)
            and not (root/'partition').exists(), 'Only the fresh fixed-size target is accepted')
    serial = base64.b85encode(bytes.fromhex(token)).decode('ascii')
    serials = [path.read_text().strip() for path in (root/'serial', root/'device/serial') if path.is_file()]
    require(serials and set(serials) == {serial}, 'Target nonce differs')
    major, minor = map(int, (root/'dev').read_text().strip().split(':'))
    device = Path('/dev/vda').stat()
    require(stat.S_ISBLK(device.st_mode) and (os.major(device.st_rdev), os.minor(device.st_rdev)) == (major, minor),
            'Target device differs')
    return root


def audit(token):
    target = fixture_guard(token)
    account_state(Path('/etc/passwd').read_text(), Path('/etc/shadow').read_text(), Path('/etc/group').read_text())
    config = tomllib.loads(Path('/etc/greetd/config.toml').read_text())
    require('initial_session' not in config
            and config.get('default_session') == {'command': '/usr/bin/wros-greeter', 'user': '_greetd'},
            'Installed native authentication must be preserved')
    require(not Path('/etc/sudoers.d/live').exists() and not Path('/etc/writingdeck/VM-DEMO-ONLY').exists(),
            'Live privilege/demo state must not persist')
    versions = {}
    for line in query('/usr/bin/dpkg-query', '-W', '-f=${Package}\t${Version}\t${Status}\n', *PACKAGES).splitlines():
        name, version, status = line.split('\t', 2)
        require(name in PACKAGES and name not in versions and status == 'install ok installed',
                'Required installed target package missing')
        versions[name] = version
    require(set(versions) == set(PACKAGES), 'Target package inventory incomplete')
    entry = crypttab_entry(Path('/etc/crypttab').read_text())
    device = Path(query('/usr/sbin/blkid', '-U', entry['uuid']).strip()).resolve(strict=True)
    require(device.parent == Path('/dev') and re.fullmatch(r'vda[0-9]+', device.name) is not None,
            'Encrypted volume must belong to the fresh target')
    partition = Path('/sys/class/block')/device.name
    require((partition/'partition').exists() and partition.resolve().parent == target.resolve(),
            'Encrypted partition ancestry differs')
    require(query('/usr/sbin/cryptsetup', 'luksUUID', str(device)).strip() == entry['uuid'],
            'LUKS identity differs from target configuration')
    fstab = [line.split() for line in Path('/etc/fstab').read_text().splitlines()
             if line.strip() and not line.lstrip().startswith('#')]
    require(sum(len(row) >= 3 and row[1:3] == ['/', 'ext4'] for row in fstab) == 1,
            'The fixture requires one ext4 root')
    boot = Path('/boot/grub/grub.cfg').read_text()
    require('console=ttyS0,115200' in boot and 'console=tty1' in boot
            and not any(value in boot for value in ('boot=live', 'wros.live=', 'wros.native_probe=')),
            'Installed boot arguments differ from the authenticated fixture')
    release = os.uname().release
    kernel, initrd = Path('/boot')/('vmlinuz-'+release), Path('/boot')/('initrd.img-'+release)
    entries = query('/usr/bin/lsinitramfs', str(initrd)).splitlines()
    require(any(name.endswith('sbin/cryptsetup') for name in entries)
            and any(name.endswith('sbin/lvm') for name in entries)
            and 'cryptroot/crypttab' in entries, 'Installed initrd lacks encrypted-root support')
    loaders = [path for path in Path('/boot/efi/EFI').glob('*/*')
               if path.is_file() and path.suffix.lower() == '.efi']
    require(0 < len(loaders) <= 32, 'EFI target payload missing')
    logs = Path('/var/log/installer')
    require(logs.is_dir(), 'Maintained installer logs missing')
    total = 0
    for path in logs.rglob('*'):
        if path.is_file() and not path.is_symlink():
            total += path.stat().st_size
            require(total <= 64*1024*1024, 'Bounded installer logs required')
            require(b'wros-qa-' not in path.read_bytes() and b'wros-user-' not in path.read_bytes(),
                    'Generated fixture credential appeared in persistent logs')
    return {'passed': True, 'scope': 'Guarded installed target snapshot before unmount; actual target boot remains separate.',
            'normal_account': True, 'root_locked': True, 'native_authentication_preserved': True,
            'live_account_absent': True, 'interactive_luks_uuid': entry['uuid'],
            'encrypted_device': device.name, 'packages': versions,
            'binary_sha256': {name: digest(Path('/usr/bin')/name) for name in ('grimoire', 'wros-greeter', 'wros-installer')},
            'kernel_sha256': digest(kernel), 'initrd_sha256': digest(initrd),
            'efi_sha256': {str(path.relative_to('/boot/efi')): digest(path) for path in loaders},
            'plaintext_fixture_credentials_in_logs': False}


def main():
    require(len(sys.argv) == 3 and sys.argv[1] == '--fixture-token', 'Fixed target audit operation required')
    print(json.dumps(audit(sys.argv[2])), flush=True)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        print('Disposable encrypted target audit failed.', file=sys.stderr)
        raise SystemExit(1) from None
