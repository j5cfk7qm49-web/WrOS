#!/usr/bin/env python3
"""Run every Python test in os/tests for the NixOS build, except the Debian-only ones below.

Each exclusion names the Debian machinery it tests. NixOS WrOS neither builds nor ships that
code, and there will be no more Debian releases (rc13 was the last). A new test file runs by
default. A skipped test fails the run, so a missing tool is never mistaken for a pass.

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
    'test_native_image': 'audits a staged Debian rootfs with dpkg',
    'test_wifi_policy': 'checks Wi-Fi firmware in a synthetic dpkg database',
    'test_packaging': 'builds and installs the Debian wros package',
    'test_collect_update_runtime': "chooses a Debian update bundle's .deb packages",
    'test_live_installer_pool': "parses Debian's installer package pool (python-debian)",
    'test_native_installer_capsule': 'stages the Debian native-installer capsule (python-debian)',
    'test_installer_media': 'audits the Debian installer ISO',
    'test_installer_keymaps': "adds Colemak to Debian Installer's keymap question",
    'test_live_boot_menu': 'audits the Debian live ISO boot menu',
    'test_rc_vm': 'drives the Debian release VM chain',
    'test_rc_vm_chain': 'drives the Debian release VM chain with dpkg',
    'test_rc_vm_fakes': 'fakes for the Debian release VM chain',
    'test_grub_recovery': "Debian's grub.d recovery scripts; NixOS uses a GRUB specialisation",
    'test_rescue': 'the Debian os/scripts/wros-rescue; nixos/scripts/wros-rescue is the NixOS one (#150)',
    'test_live_session': 'the Debian os/scripts/wros-live-session; NixOS has its own',
    'test_install_session': 'the Debian os/scripts/wros-install-session; test_nixos_installer covers NixOS',
    'test_prose_staging': 'stages the prose runtime into a Debian /opt payload; prose-runtime.nix builds it on NixOS',
    'test_prose_runtime.ProseRuntime.test_exact_two_git_source_repacks_reproduce_locked_wheels_and_runtime_payload':
        'reproduces the Debian build/prose-runtime wheel set; prose-runtime.nix pins it on NixOS',
    'test_prose_runtime.ProseRuntime.test_fresh_materialization_is_reproducible_and_forged_or_failed_outputs_are_not_trusted':
        'materializes the Debian prose runtime with /usr/bin/python3; prose-runtime.nix builds it on NixOS',
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
