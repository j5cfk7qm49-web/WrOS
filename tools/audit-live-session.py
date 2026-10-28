#!/usr/bin/env python3
"""Audit only an immutable live staging root, before installer account setup."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def audit(root):
    root = Path(root)
    spec = importlib.util.spec_from_file_location('wros_native_audit', ROOT/'tools/audit-native-image.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    result = native.audit(root)
    files = {
        'os/scripts/writingdeck-session': 'usr/libexec/writingdeck-session',
        'os/scripts/wros-live-session': 'usr/libexec/wros-live-session',
        'os/config/live/9998-wros-session': 'lib/live/config/9998-wros-session',
        'os/config/live/live.conf': 'etc/live/config.conf.d/90-wros.conf',
        'os/config/live/greetd-live.conf': 'etc/systemd/system/greetd.service.d/90-wros-live.conf',
    }
    for source, name in files.items():
        path = native.regular(root, name)
        if path.read_bytes() != (ROOT/source).read_bytes() or path.stat().st_mode & 0o6022:
            raise ValueError('Live profile differs from source or has unsafe permissions: ' + name)
    for name in ('usr/bin/wros-installer', 'usr/bin/live-config', 'usr/lib/user-setup/user-setup-apply',
                 'usr/libexec/wros-live-session', 'lib/live/config/9998-wros-session'):
        if not os.access(native.regular(root, name), os.X_OK):
            raise ValueError('Missing live runtime executable: ' + name)
    dropins = root/'etc/systemd/system/greetd.service.d'
    if {p.name for p in dropins.glob('*.conf')} != {'90-wros-live.conf'}:
        raise ValueError('Unexpected native login override in the live profile')
    mask = root/'etc/systemd/system-generators/live-config-getty-generator'
    if not mask.is_symlink() or os.readlink(mask) != '/dev/null':
        raise ValueError('Stock live console entry generator must be masked')
    for name in ('run/wros-live', 'etc/sudoers.d/live'):
        if (root/name).exists() or (root/name).is_symlink():
            raise ValueError('Runtime live state leaked into immutable staging root: ' + name)
    accounts = [row.split(':') for row in (root/'etc/passwd').read_text().splitlines()]
    if any(row[0] == 'wros-live' or (len(row) == 7 and row[2] == '1000') for row in accounts):
        raise ValueError('Temporary live account leaked into immutable staging root')
    return {'passed': True, 'native_session': result,
            'helper_sha256': hashlib.sha256((root/'usr/libexec/wros-live-session').read_bytes()).hexdigest(),
            'temporary_account_absent': True, 'shipping_initial_session_absent': True,
            'stock_console_entry_generator_masked': True,
            'scope': 'Immutable live staging files only; no actual live boot, installed authentication or privileged installer qualification.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    print(json.dumps(audit(parser.parse_args().root), indent=2))
