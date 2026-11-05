"""Keeping the installed update and the one before it, and going back to it.

Same fixture as test_update_transactions: real gpg, gpgv, dpkg-deb and apt-get --simulate
against throwaway keys and a temporary dpkg database; only the final installation is a fake
runner that edits the temporary status file.
"""
from datetime import timedelta
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_update_transactions as transactions  # noqa: E402

update = transactions.update
# The transaction tests' fixture and helpers, without running their tests a second time.
Fixture = type('Fixture', (unittest.TestCase,),
               {k: v for k, v in vars(transactions.Transactions).items() if not k.startswith('test_')})


class KeepInstalled(Fixture):
    def manifest_versions(self, tree):
        manifest = json.loads((self.system.state / tree / 'manifest.json').read_text())
        return [p['version'] for p in manifest['packages']]

    def install(self, name, version, date_ago, returncode=0):
        self.stage(self.bundle(name, version=version, date_ago=date_ago))
        return self.apply(runner=self.installer(version=version, returncode=returncode))

    def test_the_first_install_is_kept_and_nothing_is_previous(self):
        self.assertEqual(self.install('one', '0.1.0~rc2', timedelta(hours=2))['status'], 'completed')
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc2'])
        self.assertFalse((self.system.state / 'previous').exists())
        self.assertFalse((self.system.state / 'staged').exists())
        self.assertIsNone(update.read_public_status(self.system.state)['previous_version'])

    def test_the_next_install_moves_the_kept_one_to_previous(self):
        self.install('one', '0.1.0~rc2', timedelta(hours=2))
        self.install('two', '0.1.0~rc3', timedelta(hours=1))
        self.assertEqual(self.manifest_versions('previous'), ['0.1.0~rc2'])
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc3'])
        self.assertEqual(update.read_public_status(self.system.state)['previous_version'], 'wros 0.1.0~rc2')
        # Only one previous version is kept.
        self.install('three', '0.1.0~rc4', timedelta(minutes=30))
        self.assertEqual(self.manifest_versions('previous'), ['0.1.0~rc3'])
        self.assertEqual(sorted(p.name for p in self.system.state.iterdir() if p.is_dir()),
                         ['installed', 'previous', 'public', 'work'])

    def test_a_failed_install_keeps_the_pair_it_had(self):
        self.install('one', '0.1.0~rc2', timedelta(hours=2))
        with self.assertRaises(ValueError):
            self.install('two', '0.1.0~rc3', timedelta(hours=1), returncode=1)
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc2'])
        self.assertFalse((self.system.state / 'previous').exists())

    def test_a_crash_between_the_renames_is_finished_at_the_next_boot(self):
        self.install('one', '0.1.0~rc2', timedelta(hours=2))
        real_rename = update.os.rename
        calls = []

        def crash_on_the_third(source, target):
            calls.append(target)
            if len(calls) == 3:  # staging, installed -> previous, then staged -> installed
                raise OSError('power lost')
            real_rename(source, target)

        update.os.rename = crash_on_the_third
        try:
            with self.assertRaises(OSError):
                self.install('two', '0.1.0~rc3', timedelta(hours=1))
        finally:
            update.os.rename = real_rename
        self.assertFalse((self.system.state / 'installed').exists())
        update.reconcile(runner=self.installer(version='0.1.0~rc3'), now=self.now + timedelta(minutes=10),
                         **self.paths())
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc3'])
        self.assertEqual(self.manifest_versions('previous'), ['0.1.0~rc2'])


if __name__ == '__main__':
    unittest.main()
