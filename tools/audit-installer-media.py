#!/usr/bin/env python3
"""Check the actual live-build installer tree, not just requested packages."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

REPO = Path(__file__).resolve().parents[1]


def audit(base):
    base = Path(base)
    spec = importlib.util.spec_from_file_location('wifi_audit', REPO / 'tools/audit-wifi-image.py')
    wifi = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wifi)
    # This invokes dpkg-query against the staged database and checks real files.
    result = wifi.audit(base / 'chroot')
    root = base / 'chroot'
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
    for package in sorted((base / 'binary/pool').rglob('*.deb')):
        name = subprocess.check_output(['dpkg-deb', '--field', str(package), 'Package'], text=True).strip()
        if name in expected:
            packages.add(name)
    missing = expected - packages
    if missing:
        raise ValueError('installer media missing offline firmware packages: ' + ', '.join(sorted(missing)))
    return {'schema_version': 1, 'passed': True, 'installed_rootfs_wifi': result,
            'native_session': native_result,
            'installer_firmware_packages': sorted(packages), 'vm_profile_absent': True,
            'physical_hardware_verified': False, 'installation_verified': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build_tree', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(audit(args.build_tree), indent=2))
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from None
