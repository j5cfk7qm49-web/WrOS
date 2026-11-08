#!/usr/bin/env python3
"""Run the installed RC update and feature phases and report their run directories."""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


@dataclass(frozen=True)
class Outcome:
    label: str
    passed: bool
    run: Path | None = None
    skipped: bool = False


def run_phase(root, label, *args):
    base = Path(tempfile.mkdtemp(prefix='p-', dir=root))
    env = os.environ | {'RC_VM_BASE': str(base)}
    script = Path(__file__).with_name('rc_vm_phases.py')
    status = subprocess.call([sys.executable, '-u', str(script), *map(str, args)], env=env)
    results = list(base.glob('*/result.json'))
    if len(results) != 1:
        return Outcome(label, False, base)
    result = results[0]
    try:
        passed = status == 0 and json.loads(result.read_text())['passed'] is True
    except (ValueError, KeyError, OSError):
        passed = False
    return Outcome(label, passed, result.parent)


def qualify(root, install, bundle, expect, existing=None, start=None, *, bootscreen=False):
    """Update an installed system to the bundle, then check the features on it. The start is
    rc1 as installed, or `start` = (disk, version): a saved disk at the version the test
    laptop runs, which is what an update really meets."""
    source = ('--install-run', install)
    if start:
        disk, version = start
        update = run_phase(root, version + '-update', 'update', *source, '--bundle', bundle,
                           '--base-disk', disk, '--from-version', version)
    else:
        update = run_phase(root, 'rc1-update', 'update', *source, '--bundle', bundle)
    if update.passed:
        features = run_phase(root, 'features', 'rc5', *source, '--expect', expect,
                             '--base-disk', update.run / 'disk.qcow2')
    else:
        features = Outcome('features', False, skipped=True)
    outcomes = [update, features]
    if bootscreen:
        outcomes.append(run_phase(root, 'bootscreen', 'bootscreen', *source,
                                  '--base-disk', update.run / 'disk.qcow2') if update.passed
                        else Outcome('bootscreen', False, skipped=True))
    if existing:
        outcomes.append(run_phase(root, 'rc6-migration', 'update', *source, '--bundle', bundle,
                                  '--base-disk', existing, '--from-version', '0.1.0~rc6',
                                  '--check-migration'))
    return outcomes


def summarize(outcomes):
    for result in outcomes:
        state = 'PASS' if result.passed else 'SKIP' if result.skipped else 'FAIL'
        print(f'{state} {result.label}  {result.run or "the update failed"}', flush=True)
    return 0 if all(result.passed for result in outcomes) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', required=True, type=Path)
    parser.add_argument('--bundle', required=True, type=Path)
    parser.add_argument('--expect', required=True)
    parser.add_argument('--existing-disk', type=Path, help='optional rc6 qcow2 base for the migration')
    parser.add_argument('--start-disk', type=Path, help='update this saved installed disk instead of rc1')
    parser.add_argument('--start-version', help='the WrOS version on --start-disk')
    parser.add_argument('--bootscreen', action='store_true', help='also qualify native startup and text recovery')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    base = Path(os.environ.get('RC_VM_BASE', '/home/builder/rc-vm-runs'))
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    root = Path(tempfile.mkdtemp(prefix='rc-', dir=base))
    if bool(args.start_disk) != bool(args.start_version):
        parser.error('--start-disk and --start-version go together')
    start = (args.start_disk, args.start_version) if args.start_disk else None
    return summarize(qualify(root, args.install_run, args.bundle, args.expect, args.existing_disk, start,
                             bootscreen=args.bootscreen))


if __name__ == '__main__':
    sys.exit(main())
