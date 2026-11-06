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


def qualify(root, install, bundle, expect, existing=None):
    source = ('--install-run', install)
    update = run_phase(root, 'rc1-update', 'update', *source, '--bundle', bundle)
    if update.passed:
        features = run_phase(root, 'rc7-features', 'rc5', *source, '--expect', expect,
                             '--base-disk', update.run / 'disk.qcow2')
    else:
        features = Outcome('rc7-features', False, skipped=True)
    outcomes = [update, features]
    if existing:
        outcomes.append(run_phase(root, 'rc6-migration', 'update', *source, '--bundle', bundle,
                                  '--base-disk', existing, '--from-version', '0.1.0~rc6',
                                  '--check-migration'))
    return outcomes


def summarize(outcomes):
    for result in outcomes:
        state = 'PASS' if result.passed else 'SKIP' if result.skipped else 'FAIL'
        print(f'{state} {result.label}  {result.run or "rc1 update failed"}', flush=True)
    return 0 if all(result.passed for result in outcomes) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', required=True, type=Path)
    parser.add_argument('--bundle', required=True, type=Path)
    parser.add_argument('--expect', required=True)
    parser.add_argument('--existing-disk', type=Path, help='optional rc6 qcow2 base for the migration')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    base = Path(os.environ.get('RC_VM_BASE', '/home/builder/rc-vm-runs'))
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    root = Path(tempfile.mkdtemp(prefix='rc-', dir=base))
    return summarize(qualify(root, args.install_run, args.bundle, args.expect, args.existing_disk))


if __name__ == '__main__':
    sys.exit(main())
