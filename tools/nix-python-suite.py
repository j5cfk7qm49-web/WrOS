#!/usr/bin/env python3
"""Run every Python test in os/tests for the NixOS build, except the Debian-only ones below.

Each exclusion names a Debian path in the shared update engine. A new test file
runs by default. A skipped test fails the run, so a missing tool is never
mistaken for a pass.

Usage: python3 tools/nix-python-suite.py [--list]
"""
import argparse
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / 'os/tests'

# Module or module.Class.test -> why NixOS does not run it.
DEBIAN_ONLY = {
    'test_update':"the Debian update engine's dpkg/APT paths",
    'test_update_live': "the Debian update engine's app-only dpkg updates",
    'test_update_rollback': "the Debian update engine's dpkg rollback",
    'test_update_transactions': "the Debian update engine's dpkg transactions",
    'test_update_support': 'fixtures for the Debian update engine tests',
}


def excluded(test_id):
    parts = test_id.split('.')
    return any('.'.join(parts[:n]) in DEBIAN_ONLY for n in range(1, len(parts) + 1))


def flatten(suite):
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from flatten(item)
        else:
            yield item


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--list', action='store_true', help='print the exclusions and exit')
    args = parser.parse_args()
    if args.list:
        for name, why in sorted(DEBIAN_ONLY.items()):
            print(f'{name}: {why}')
        return 0
    sys.path.insert(0, str(TESTS))
    loaded = unittest.defaultTestLoader.discover(str(TESTS), pattern='test_*.py', top_level_dir=str(TESTS))
    tests, dropped = [], 0
    for test in flatten(loaded):
        # A module that fails to import is a _FailedTest named after the module.
        name = test.id().removeprefix('unittest.loader._FailedTest.')
        if excluded(name):
            dropped += 1
        else:
            tests.append(test)
    stale = [name for name in DEBIAN_ONLY if not (TESTS / (name.split('.')[0] + '.py')).is_file()]
    if stale:
        print(f'exclusions name missing test files: {stale}', file=sys.stderr)
        return 1
    print(f'{len(tests)} tests, {dropped} Debian-only tests excluded', flush=True)
    # durations: the log ends with the slowest tests, so a slow one is seen when it lands.
    result = unittest.TextTestRunner(verbosity=2, durations=30).run(unittest.TestSuite(tests))
    if result.skipped:
        print(f'FAILED: {len(result.skipped)} tests skipped; provide what they need', file=sys.stderr)
        for test, why in result.skipped:
            print(f'  {test.id()}: {why}', file=sys.stderr)
        return 1
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    sys.exit(main())
