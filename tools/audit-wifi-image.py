#!/usr/bin/env python3
"""Read-only Wi-Fi package/firmware gate for a staged Debian image, not hardware proof."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def audit(root):
    if root.is_symlink() or root.resolve() == Path('/'):
        raise ValueError('inspect a staged image directory, never the live root')
    root = root.resolve(strict=True)
    policy = json.loads((ROOT/'os/config/wifi-firmware.json').read_text())
    required = [policy['kernel_package'], *policy['connection_packages'],
                *(item['package'] for item in policy['firmware'])]
    query = subprocess.check_output(['dpkg-query', '--admindir='+str(root/'var/lib/dpkg'),
                                     '-W', '-f=${Package}\t${Version}\t${Status}\n', *required], text=True)
    packages = {}
    for line in query.splitlines():
        name, version, status = line.split('\t', 2)
        if status != 'install ok installed':
            raise ValueError('required Wi-Fi package is not installed: '+name)
        packages[name] = version
    if set(packages) != set(required):
        raise ValueError('Wi-Fi package inventory incomplete')
    firmware_files = {}
    for item in policy['firmware']:
        name = item['package']
        listing = root/'var/lib/dpkg/info'/(name+'.list')
        paths = listing.read_text().splitlines()
        payload = [p for p in paths if p.startswith(('/usr/lib/firmware/', '/lib/firmware/'))
                   and (root/p.lstrip('/')).is_file()]
        if not payload:
            raise ValueError('firmware package has no installed firmware files: '+name)
        notice = root/'usr/share/doc'/name/'copyright'
        if not notice.is_file():
            raise ValueError('retained firmware notice missing: '+name)
        firmware_files[name] = {'file_count': len(payload),
                                'copyright_sha256': hashlib.sha256(notice.read_bytes()).hexdigest()}
    if not any((root/path).is_file() for path in ('usr/bin/nmtui', 'bin/nmtui')):
        raise ValueError('interactive connection tool nmtui is missing')
    if not list((root/'lib/modules').glob('*/kernel/drivers/net/wireless')) and not list((root/'usr/lib/modules').glob('*/kernel/drivers/net/wireless')):
        raise ValueError('kernel wireless modules are absent')
    return {'schema_version': 1, 'scope': 'Installed image payload and retained notices; no physical radio test',
            'packages': packages, 'firmware_payload': firmware_files,
            'connection_tool': 'nmtui', 'physical_adapters_verified': []}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.root), indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        raise SystemExit('Wi-Fi image audit failed: '+str(exc))
