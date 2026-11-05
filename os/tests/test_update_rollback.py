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


class Kept(Fixture):
    def manifest_versions(self, tree):
        manifest = json.loads((self.system.state / tree / 'manifest.json').read_text())
        return [p['version'] for p in manifest['packages']]

    def install(self, name, version, date_ago, returncode=0):
        self.stage(self.bundle(name, version=version, date_ago=date_ago))
        return self.apply(runner=self.installer(version=version, returncode=returncode))


class KeepInstalled(Kept):
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



class GoBack(Kept):
    """Going back installs exactly the retained previous tree, once, and never moves replay back."""

    def setUp(self):
        super().setUp()
        self.install('one', '0.1.0~rc2', timedelta(hours=2))
        self.install('two', '0.1.0~rc3', timedelta(hours=1))
        self.replay = (self.system.state / 'replay.json').read_bytes()

    def go_back(self, **extra):
        return update.stage_rollback(now=self.now + timedelta(minutes=20), **self.paths(**extra))

    def apply_back(self, version='0.1.0~rc2', returncode=0):
        return update.execute_offline_boot_transaction(
            runner=self.installer(version=version, returncode=returncode),
            now=self.now + timedelta(minutes=25), **self.paths())

    def installed_version(self):
        return update.load_installed_dpkg_database(self.system.dpkg)['wros']['version']

    def test_going_back_installs_the_previous_version_and_keeps_replay(self):
        staged = self.go_back()
        self.assertEqual((staged['status'], staged['rollback'], staged['packages']), ('staged', True, 1))
        self.assertTrue(update.owned_trigger(self.system.trigger, self.system.state / 'staged'))
        self.assertEqual(self.apply_back()['status'], 'completed')
        self.assertTrue(any('--allow-downgrades' in call for call in self.runner_calls))
        self.assertEqual(self.installed_version(), '0.1.0~rc2')
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc2'])
        self.assertFalse((self.system.state / 'previous').exists())
        last = update.Store(self.system.state).records()[-1]
        self.assertEqual((last['state'], last.get('rollback')), ('completed', True))
        # The version gone back from is never accepted again; only a newer one is.
        self.assertEqual((self.system.state / 'replay.json').read_bytes(), self.replay)
        with self.assertRaisesRegex(ValueError, 'not newer|replay'):
            self.review(self.media / 'two')
        self.assertIsNone(update.read_public_status(self.system.state)['previous_version'])
        with self.assertRaisesRegex(ValueError, 'no previous version'):
            self.go_back()

    def test_without_a_previous_version_nothing_is_staged(self):
        update.shutil.rmtree(self.system.state / 'previous')
        with self.assertRaisesRegex(ValueError, 'no previous version'):
            self.go_back()
        self.assertFalse(update.os.path.lexists(self.system.trigger))

    def test_a_changed_retained_byte_is_refused(self):
        deb = next((self.system.state / 'previous' / 'packages').iterdir())
        deb.write_bytes(deb.read_bytes()[:-1] + b'x')
        with self.assertRaisesRegex(ValueError, 'SHA-256'):
            self.go_back()
        self.assertFalse(update.os.path.lexists(self.system.trigger))

    def test_only_from_the_version_installed_kept_as_installed(self):
        self.system.status.write_text(transactions.support.status_stanza('wros', '0.1.0~rc9', arch='amd64'))
        with self.assertRaisesRegex(ValueError, 'installed'):
            self.go_back()

    def test_a_staged_go_back_that_is_not_the_previous_tree_is_refused(self):
        self.go_back()
        manifest_path = self.system.state / 'previous' / 'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        manifest['review_id'] = 'f' * 64
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, 'not newer|replay'):
            self.apply_back()
        self.assertEqual(self.installed_version(), '0.1.0~rc3')

    def test_a_failed_go_back_keeps_both_versions(self):
        self.go_back()
        with self.assertRaises(ValueError):
            self.apply_back(returncode=1)
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc3'])
        self.assertEqual(self.manifest_versions('previous'), ['0.1.0~rc2'])


    def test_an_interrupted_go_back_repaired_at_boot_still_keeps_replay(self):
        self.go_back()
        store = update.Store(self.system.state)
        store.append('applying', json.loads((self.system.state / 'staged/manifest.json').read_text())['transaction_id'],
                     self.now, packages=['wros 0.1.0~rc2'])
        self.system.trigger.unlink()
        self.system.status.write_text(transactions.support.status_stanza(
            'wros', '0.1.0~rc2', state='install ok half-configured', arch='amd64'))

        def configure(command):
            self.system.status.write_text(transactions.support.status_stanza('wros', '0.1.0~rc2', arch='amd64'))
            return transactions.types.SimpleNamespace(returncode=0, stdout='', stderr='')

        self.assertEqual(update.reconcile(runner=configure, now=self.now + timedelta(minutes=30),
                                          **self.paths())['status'], 'recovered')
        self.assertEqual((self.system.state / 'replay.json').read_bytes(), self.replay)
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc2'])
        self.assertFalse((self.system.state / 'previous').exists())


class AdoptFromCache(Kept):
    """An engine that kept nothing (rc4) leaves the online download cache: the next engine adopts it."""

    def setUp(self):
        super().setUp()
        self.install('one', '0.1.0~rc2', timedelta(hours=2))
        # What rc4 leaves behind: no kept tree, only the online cache of the bundle it installed.
        update.shutil.rmtree(self.system.state / 'installed')
        self.cache = self.system.state.parent / 'online-cache'
        self.cache.mkdir()
        update.shutil.copytree(self.media / 'one', self.cache / ('ab' * 8))

    def reconcile(self):
        return update.reconcile(runner=self.installer(version='0.1.0~rc2'), now=self.now + timedelta(days=3),
                                cache=self.cache, **self.paths())

    def test_the_cached_bundle_of_the_installed_update_becomes_installed(self):
        self.reconcile()
        self.assertEqual(self.manifest_versions('installed'), ['0.1.0~rc2'])
        replay = json.loads((self.system.state / 'replay.json').read_text())
        kept = json.loads((self.system.state / 'installed/manifest.json').read_text())
        self.assertEqual(kept['review_id'], replay['review_id'])
        # It is then the previous version once the next update installs.
        self.install('two', '0.1.0~rc3', timedelta(hours=1))
        self.assertEqual(self.manifest_versions('previous'), ['0.1.0~rc2'])
        self.assertEqual(update.stage_rollback(now=self.now + timedelta(minutes=20), **self.paths())['status'],
                         'staged')

    def test_a_cached_bundle_that_is_not_the_one_applied_is_ignored(self):
        update.shutil.rmtree(self.cache / ('ab' * 8))
        self.bundle('other', version='0.1.0~rc2', date_ago=timedelta(minutes=90))
        update.shutil.copytree(self.media / 'other', self.cache / ('cd' * 8))
        self.reconcile()
        self.assertFalse((self.system.state / 'installed').exists())

    def test_nothing_is_adopted_when_a_tree_is_already_kept(self):
        self.install('two', '0.1.0~rc3', timedelta(hours=1))
        before = (self.system.state / 'installed/manifest.json').read_bytes()
        self.reconcile()
        self.assertEqual((self.system.state / 'installed/manifest.json').read_bytes(), before)

if __name__ == '__main__':
    unittest.main()
