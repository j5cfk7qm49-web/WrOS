#!/usr/bin/env python3
"""Read-only installation survey/planner. There is deliberately NO apply mode.

Use Debian's supported installer for partitioning/account/LUKS provisioning.
This prototype cannot authorize or perform a physical disk installation.
"""
import argparse
import json
from pathlib import Path
import platform
import re
import subprocess
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

LAYOUTS = ('qwerty', 'colemak', 'colemak-dh-ansi', 'colemak-dh-iso')


def mounted(device):
    points = device.get('mountpoints') or []
    if isinstance(points, str):
        points = [points]
    return any(p for p in points) or any(mounted(c) for c in device.get('children', []))


def plan(inventory, disk=None, encrypt=False, username='writer', timezone='Etc/UTC', keyboard='qwerty'):
    if username == 'root' or not re.fullmatch(r'[a-z_][a-z0-9_-]{0,31}', username):
        raise ValueError('choose a valid non-root Linux username')
    try:
        ZoneInfo(timezone)
    except (ZoneInfoNotFoundError, ValueError):
        raise ValueError('choose a known IANA timezone') from None
    if keyboard not in LAYOUTS:
        raise ValueError('choose a registered keyboard layout')
    devices = inventory.get('blockdevices', [])
    selected = next((d for d in devices if d.get('path') == disk), None) if disk else None
    errors = []
    if disk and selected is None:
        errors.append('selected disk was not found in the read-only inventory')
    if selected:
        if selected.get('type') != 'disk':
            errors.append('target is not a whole disk')
        if selected.get('ro'):
            errors.append('target is read-only')
        if mounted(selected):
            errors.append('target or a child partition is mounted; installation must not proceed')
        if int(selected.get('size', 0)) < 8 * 1024**3:
            errors.append('target is below the provisional 8 GiB minimum')
    return {
        'version': 1, 'mode': 'read-only plan', 'can_apply': False,
        'destructive_operations_executed': False,
        'implementation_status': 'planner only; use an explicitly authorized Debian installation',
        'host_architecture': platform.machine(),
        'boot_mode': 'UEFI' if Path('/sys/firmware/efi').is_dir() else 'BIOS/container/unknown',
        'disks': devices, 'selected_disk': disk,
        'selection_eligible_for_review': bool(selected) and not errors,
        'errors': errors,
        'requested': {'encryption': 'LUKS via Debian installer' if encrypt else 'unencrypted (not recommended for mobile hardware)',
                      'username': username, 'timezone': timezone, 'keyboard': keyboard},
        'required_before_any_installation': [
            'Back up and verify a restore independently of this laptop.',
            'Review the exact physical disk model, size, partitions and backups.',
            'Obtain explicit destructive-operation authorization outside this planner.',
            'Use Debian installer confirmation screens for partitioning/LUKS and recovery-secret custody.',
            'Set an authenticated non-root account and test maintenance/recovery login.',
            'Provision console layout before manuscript input; test preview/revert.',
            'Configure Wi-Fi explicitly if needed; no credentials in this plan.',
            'Verify boot, lock-before-suspend, power and update/recovery on the actual laptop.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk')
    parser.add_argument('--encrypt', action='store_true')
    parser.add_argument('--username', default='writer')
    parser.add_argument('--timezone', default='Etc/UTC')
    parser.add_argument('--keyboard', choices=LAYOUTS, default='qwerty')
    parser.add_argument('--inventory-fixture', type=Path, help='test-only lsblk JSON instead of probing the host')
    args = parser.parse_args()
    if args.inventory_fixture:
        inventory = json.loads(args.inventory_fixture.read_text())
    else:
        inventory = json.loads(subprocess.check_output([
            '/usr/bin/lsblk', '--json', '--bytes', '--output', 'PATH,TYPE,SIZE,MOUNTPOINTS,RO,MODEL,TRAN'
        ], text=True, timeout=5))
    result = plan(inventory, args.disk, args.encrypt, args.username, args.timezone, args.keyboard)
    print(json.dumps(result, indent=2))
    return 1 if result['errors'] else 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc))
