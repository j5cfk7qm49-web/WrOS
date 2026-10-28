#!/usr/bin/env python3
"""Check the actual live-build installer tree, not just requested packages."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

from debian.debfile import DebFile

REPO = Path(__file__).resolve().parents[1]

# Actual runtime files copied to an encrypted target, not a request-list check.
# The shared cryptsetup shell functions are data; the other roles execute.
ENCRYPTION_PAYLOAD = {
    'cryptsetup': (('usr/lib/cryptsetup/functions', False),),
    'cryptsetup-bin': (('usr/sbin/cryptsetup', True),),
    'cryptsetup-initramfs': (('usr/share/initramfs-tools/hooks/cryptroot', True),
                           ('usr/share/initramfs-tools/scripts/local-top/cryptroot', True)),
    'dmsetup': (('usr/sbin/dmsetup', True),),
    'lvm2': (('usr/sbin/lvm', True),),
    'systemd-cryptsetup': (('usr/lib/systemd/systemd-cryptsetup', True),),
}


def encryption_inventory(root):
    try:
        query = subprocess.check_output(['dpkg-query', '--admindir='+str(root/'var/lib/dpkg'),
            '-W', '-f=${Package}\t${Version}\t${Status}\n', *sorted(ENCRYPTION_PAYLOAD)], text=True)
    except subprocess.CalledProcessError as error:
        raise ValueError('encrypted-target package inventory incomplete') from error
    versions = {}
    for line in query.splitlines():
        name, version, status = line.split('\t', 2)
        if name not in ENCRYPTION_PAYLOAD or name in versions or status != 'install ok installed':
            raise ValueError('encrypted-target package is not installed: '+name)
        versions[name] = version
    if set(versions) != set(ENCRYPTION_PAYLOAD):
        raise ValueError('encrypted-target package inventory incomplete')
    for paths in ENCRYPTION_PAYLOAD.values():
        for relative, executable in paths:
            path = root/relative
            if not path.is_file() or path.stat().st_size == 0 or (executable and not path.stat().st_mode & 0o111):
                raise ValueError('encrypted-target runtime missing or not executable: '+relative)
    return versions


def audit(base):
    base = Path(base)
    spec = importlib.util.spec_from_file_location('wifi_audit', REPO / 'tools/audit-wifi-image.py')
    wifi = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wifi)
    # This invokes dpkg-query against the staged database and checks real files.
    result = wifi.audit(base / 'chroot')
    root = base / 'chroot'
    encryption = encryption_inventory(root)
    native_spec = importlib.util.spec_from_file_location('native_audit', REPO / 'tools/audit-native-image.py')
    native = importlib.util.module_from_spec(native_spec)
    native_spec.loader.exec_module(native)
    native_result = native.audit(root)
    for relative in ('etc/writingdeck/VM-DEMO-ONLY',
                     'etc/systemd/system/getty@tty1.service.d/writingdeck-vm.conf',
                     'etc/systemd/system/writingdeck-boot-probe.service'):
        if (root / relative).exists():
            raise ValueError('VM-only integration present: ' + relative)
    for unit in (root / 'etc/systemd/system').rglob('*'):
        if unit.is_file() and not unit.is_symlink() and 'autologin' in unit.read_text(errors='replace'):
            raise ValueError('autologin configured in installed system: ' + str(unit.relative_to(root)))
    root_password = next(line.split(':')[1] for line in (root / 'etc/shadow').read_text().splitlines()
                         if line.startswith('root:'))
    if not root_password.startswith(('!', '*')):
        raise ValueError('root account must be locked before installer account setup')
    if any(line.split(':')[0] in ('writer', 'user', 'builder') for line in (root / 'etc/passwd').read_text().splitlines()):
        raise ValueError('build/demo account present before installer account setup')
    for relative in ('binary/live/filesystem.squashfs', 'binary/install/vmlinuz',
                     'binary/install/initrd.gz', 'chroot/usr/bin/grimoire'):
        if not (base / relative).is_file() or (base / relative).stat().st_size == 0:
            raise ValueError('missing installer payload: ' + relative)
    expected = {row['package'] for row in json.loads((REPO / 'os/config/wifi-firmware.json').read_text())['firmware']}
    packages = set()
    encryption_archives = {}
    for archive in sorted((base / 'binary/pool').rglob('*.deb')):
        if archive.is_symlink() or not archive.is_file():
            raise ValueError('offline package must be a regular archive: '+archive.name)
        with DebFile(str(archive)) as package:
            control = package.debcontrol()
            name = control['Package']
            if name in expected:
                packages.add(name)
            if name not in encryption or control['Version'] != encryption[name]:
                continue
            for relative, _ in ENCRYPTION_PAYLOAD[name]:
                # Read the regular payload directly from the maintained archive
                # parser, without extracting it into the staged filesystem.
                if (not package.data.has_file(relative)
                        or package.data.get_content(relative) != (root/relative).read_bytes()):
                    raise ValueError('encrypted-target payload differs from offline archive: '+relative)
            with archive.open('rb') as stream:
                checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
            encryption_archives[name] = {'version': control['Version'],
                'file': archive.relative_to(base/'binary').as_posix(), 'sha256': checksum}
    missing = expected - packages
    if missing:
        raise ValueError('installer media missing offline firmware packages: ' + ', '.join(sorted(missing)))
    missing = set(encryption) - encryption_archives.keys()
    if missing:
        raise ValueError('installer media missing matching offline encryption packages: '+', '.join(sorted(missing)))
    return {'schema_version': 1, 'passed': True, 'installed_rootfs_wifi': result,
            'native_session': native_result,
            'installer_firmware_packages': sorted(packages), 'vm_profile_absent': True,
            'encrypted_target_packages': encryption, 'offline_encryption_archives': encryption_archives,
            'physical_hardware_verified': False, 'installation_verified': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build_tree', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(audit(args.build_tree), indent=2))
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from None
