"""Update transaction trust, authorization, interruption and recovery tests.

Real /usr/bin/gpg, gpgv, dpkg-deb and apt-get --simulate against throwaway keys and a
temporary dpkg database. Only the final package installation is replaced by a fake
runner that edits the temporary status file. No host package or root path is touched.
"""
from datetime import datetime, timedelta, timezone
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_update_support as support  # noqa: E402

update = support.load_engine()
MOUNTINFO = '22 1 254:1 / / rw,relatime - ext4 /dev/mapper/root rw\n'


class Transactions(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()

    def setUp(self):
        self.now = datetime.now(timezone.utc).replace(microsecond=0)
        self.system = support.SystemFixture(
            self.gpg, status=support.status_stanza('wros', '0.1.0~rc1', arch='amd64'))
        self.addCleanup(self.system.cleanup)
        self.media = Path(tempfile.mkdtemp(prefix='wros-media-'))
        self.addCleanup(subprocess.run, ['rm', '-rf', str(self.media)])
        self.prefixes = (str(self.media) + '/',)
        self.debs = Path(tempfile.mkdtemp(prefix='wros-debs-'))
        self.addCleanup(subprocess.run, ['rm', '-rf', str(self.debs)])
        self.runner_calls = []

    # ---------------------------------------------------------------- helpers

    def bundle(self, name='bundle', version='0.1.0~rc2', signer='archive', date_ago=timedelta(hours=1)):
        debs = self.debs / name
        debs.mkdir()
        deb = support.make_deb(debs, 'wros', version, arch='amd64')
        headers = support.fresh_headers(self.now, date_ago=date_ago)
        return support.write_bundle(self.media / name, [deb], self.gpg, signer=signer, headers=headers)

    def paths(self, **extra):
        return {'keyring': self.system.keyring, 'signers': self.system.signers, 'dpkg_dir': self.system.dpkg,
                'state_dir': self.system.state, 'trigger_path': self.system.trigger, 'host_arch': 'amd64'} | extra

    def review(self, bundle):
        return update.review_bundle(bundle, keyring=self.system.keyring, signers=self.system.signers,
                                    dpkg_dir=self.system.dpkg, state_dir=self.system.state, now=self.now,
                                    host_arch='amd64')

    def stage(self, bundle, review_id=None):
        review_id = review_id or self.review(bundle)['review_id']
        return update.stage_update(str(bundle), review_id, now=self.now, allowed_prefixes=self.prefixes,
                                   **self.paths())

    def journal_states(self):
        return [r['state'] for r in update.Store(self.system.state).records()]

    def installer(self, version='0.1.0~rc2', returncode=0):
        """Fake package runner: asserts the protocol state APT would run in, then 'installs'."""
        def run(command):
            self.runner_calls.append(command)
            if '--configure' not in command:
                self.assertFalse(os.path.lexists(self.system.trigger), 'trigger must be gone before APT runs')
                self.assertEqual(self.journal_states()[-1], 'applying', "'applying' must be durable first")
            if returncode == 0:
                self.system.status.write_text(support.status_stanza('wros', version, arch='amd64'))
            return types.SimpleNamespace(returncode=returncode, stdout='', stderr='')
        return run

    def apply(self, runner=None, **extra):
        return update.execute_offline_boot_transaction(
            runner=runner or self.installer(), now=self.now + timedelta(minutes=5), **self.paths(**extra))

    def recovery(self):
        return update.inspect_recovery(mountinfo_text=MOUNTINFO, dpkg_dir=self.system.dpkg,
                                       state_dir=self.system.state, trigger_path=self.system.trigger)

    # ------------------------------------------------------- review and staging

    def test_review_lists_exact_changes_and_staging_is_bound_to_that_review(self):
        bundle = self.bundle()
        review = self.review(bundle)
        self.assertEqual(review['status'], 'reviewed')
        self.assertEqual(review['signer'], self.gpg.keys['archive']['primary'])
        self.assertEqual(review['packages'], [{'name': 'wros', 'from_version': '0.1.0~rc1',
                                               'to_version': '0.1.0~rc2', 'action': 'upgrade'}])
        self.assertEqual(review['removals'], [])
        with self.assertRaisesRegex(ValueError, 'changed since it was reviewed'):
            self.stage(bundle, review_id='0' * 64)
        self.assertFalse(os.path.lexists(self.system.trigger))
        # The bundle changes after review (re-signed with a new date): the old approval is void.
        old = review['review_id']
        support.write_bundle(bundle, sorted((bundle / 'pool').glob('*.deb')), self.gpg,
                             headers=support.fresh_headers(self.now, date_ago=timedelta(minutes=30)))
        with self.assertRaisesRegex(ValueError, 'changed since it was reviewed'):
            self.stage(bundle, review_id=old)
        self.assertFalse(os.path.lexists(self.system.trigger))
        self.assertNotIn('staged', self.journal_states())

    def test_staging_retains_exact_signed_metadata_and_publishes_the_trigger_last(self):
        bundle = self.bundle()
        result = self.stage(bundle)
        self.assertEqual(result['status'], 'staged')
        staged = self.system.state / 'staged'
        self.assertEqual(os.readlink(self.system.trigger), str(staged))
        manifest = json.loads((staged / 'manifest.json').read_text())
        self.assertEqual(manifest['review_id'], result['review_id'])
        self.assertEqual((staged / 'metadata/InRelease').read_bytes(), (bundle / 'InRelease').read_bytes())
        self.assertEqual((staged / 'metadata/Packages').read_bytes(), (bundle / 'Packages').read_bytes())
        self.assertEqual(self.journal_states(), ['staged'])
        self.assertEqual(self.recovery()['update_state'], 'staged')
        # A second stage while one is pending is refused and leaves the original untouched.
        with self.assertRaisesRegex(ValueError, 'already pending'):
            self.stage(self.bundle('second'))
        self.assertEqual(os.readlink(self.system.trigger), str(staged))

    def test_a_crash_before_publication_leaves_no_trigger_and_staging_can_be_retried(self):
        bundle = self.bundle()
        with mock.patch.object(update.os, 'rename', side_effect=OSError('power lost')):
            with self.assertRaises(OSError):
                self.stage(bundle)
        self.assertFalse(os.path.lexists(self.system.trigger))
        self.assertFalse((self.system.state / 'staged').exists())
        self.assertEqual(list(self.system.state.glob('staged.*.partial')), [])
        # An orphaned partial from a real crash is junk that the next stage removes.
        orphan = self.system.state / ('staged.%s.partial' % ('a' * 32))
        orphan.mkdir()
        (orphan / 'half.deb').write_bytes(b'x')
        self.assertEqual(self.stage(bundle)['status'], 'staged')
        self.assertFalse(orphan.exists())

    def test_cancel_only_owned_staged_updates_and_never_a_foreign_marker(self):
        self.stage(self.bundle())
        result = update.cancel_staged(state_dir=self.system.state, trigger_path=self.system.trigger, now=self.now)
        self.assertEqual(result['status'], 'cancelled')
        self.assertFalse(os.path.lexists(self.system.trigger))
        self.assertFalse((self.system.state / 'staged').exists())
        self.assertEqual(self.journal_states(), ['staged', 'cancelled'])
        self.assertEqual(self.recovery()['update_state'], 'idle')
        # A foreign updater's marker is preserved and blocks staging.
        foreign = Path(tempfile.mkdtemp(prefix='foreign-'))
        self.addCleanup(subprocess.run, ['rm', '-rf', str(foreign)])
        self.system.trigger.symlink_to(foreign)
        self.assertEqual(update.cancel_staged(state_dir=self.system.state, trigger_path=self.system.trigger,
                                              now=self.now)['status'], 'none')
        self.assertEqual(os.readlink(self.system.trigger), str(foreign))
        with self.assertRaisesRegex(ValueError, 'already pending'):
            self.stage(self.bundle('third'))

    def test_cancel_after_application_began_is_refused_truthfully(self):
        self.stage(self.bundle())
        update.Store(self.system.state).append('applying', 'b' * 32, self.now)
        result = update.cancel_staged(state_dir=self.system.state, trigger_path=self.system.trigger, now=self.now)
        self.assertEqual(result['status'], 'none')
        self.assertIn('cannot be cancelled', result['message'])
        self.assertTrue((self.system.state / 'staged').is_dir())

    # ------------------------------------------------------------ boot apply

    def test_offline_apply_completes_records_replay_and_removes_retained_archives(self):
        self.stage(self.bundle())
        result = self.apply()
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(len(self.runner_calls), 1)
        self.assertEqual(self.journal_states(), ['staged', 'applying', 'completed'])
        self.assertFalse((self.system.state / 'staged').exists())
        self.assertFalse(os.path.lexists(self.system.trigger))
        status = self.recovery()
        self.assertEqual(status['update_state'], 'completed')
        self.assertFalse(status['interrupted_dpkg'])
        # Replaying the same signed bundle is refused at review and at staging.
        with self.assertRaisesRegex(ValueError, 'not newer|replay'):
            self.review(self.media / 'bundle')

    def test_boot_revalidation_refuses_tampering_rotation_and_stale_staging_without_running_apt(self):
        def tamper(staged):
            deb = next((staged / 'packages').iterdir())
            data = bytearray(deb.read_bytes())
            data[-1] ^= 0xFF
            deb.write_bytes(bytes(data))

        def rotate(staged):
            self.system.signers.write_text(self.gpg.keys['other']['primary'] + '\n')

        def stray(staged):
            (staged / 'packages' / ('extra_%s.deb' % ('0' * 64))).write_bytes(b'x')

        def metadata(staged):
            (staged / 'metadata/Packages').write_bytes(b'Package: wros\n')

        cases = {'archive bytes changed': (tamper, {}),
                 'signer no longer pinned': (rotate, {}),
                 'unindexed archive added': (stray, {}),
                 'retained index edited': (metadata, {}),
                 'staged more than 30 days ago': (lambda staged: None, {'age': timedelta(days=31)})}
        for name, (mutate, options) in cases.items():
            with self.subTest(name=name):
                self.setUp()
                self.stage(self.bundle())
                staged = self.system.state / 'staged'
                mutate(staged)
                when = self.now + options.get('age', timedelta(minutes=5))
                with self.assertRaises(ValueError):
                    update.execute_offline_boot_transaction(runner=self.installer(), now=when, **self.paths())
                self.assertEqual(self.runner_calls, [])
                self.assertEqual(self.journal_states()[-1], 'failed')
                self.assertFalse(os.path.lexists(self.system.trigger), 'a failed boot must not loop')
                self.assertTrue(staged.is_dir(), 'authenticated archives are retained for rescue')
                self.assertEqual(self.recovery()['update_state'], 'failed')
                self.system.cleanup()

    def test_failed_package_transaction_is_recorded_and_never_reported_as_success(self):
        self.stage(self.bundle())
        with self.assertRaisesRegex(ValueError, 'offline apt-get'):
            self.apply(runner=self.installer(returncode=100))
        self.assertEqual(self.journal_states(), ['staged', 'applying', 'failed'])
        self.assertTrue((self.system.state / 'staged').is_dir())
        # APT "succeeded" but did not install the reviewed version: still a failure.
        self.setUp()
        self.stage(self.bundle())
        with self.assertRaisesRegex(ValueError, 'reviewed package versions'):
            self.apply(runner=self.installer(version='0.1.0~rc1'))
        self.assertEqual(self.journal_states()[-1], 'failed')

    def test_foreign_or_missing_trigger_is_not_applied(self):
        self.stage(self.bundle())
        self.system.trigger.unlink()
        foreign = Path(tempfile.mkdtemp(prefix='foreign-'))
        self.addCleanup(subprocess.run, ['rm', '-rf', str(foreign)])
        self.system.trigger.symlink_to(foreign)
        self.assertEqual(self.apply()['status'], 'not-owned')
        self.assertEqual(self.runner_calls, [])
        self.assertEqual(os.readlink(self.system.trigger), str(foreign))

    # ------------------------------------------------- interruption and repair

    def test_reconcile_names_an_orphaned_apply_interrupted_then_repairs_from_retained_archives(self):
        self.stage(self.bundle())
        store = update.Store(self.system.state)
        store.append('applying', json.loads((self.system.state / 'staged/manifest.json').read_text())['transaction_id'],
                     self.now, packages=['wros 0.1.0~rc2'])
        self.system.trigger.unlink()  # removed before APT ran, as in the real protocol
        self.system.status.write_text(support.status_stanza('wros', '0.1.0~rc2', state='install ok half-configured',
                                                            arch='amd64'))
        self.assertTrue(self.recovery()['interrupted_dpkg'])

        def configure(command):
            self.runner_calls.append(command)
            self.system.status.write_text(support.status_stanza('wros', '0.1.0~rc2', arch='amd64'))
            return types.SimpleNamespace(returncode=0, stdout='', stderr='')

        result = update.reconcile(runner=configure, now=self.now, **self.paths())
        self.assertEqual(result['status'], 'recovered')
        self.assertEqual(self.runner_calls[0][-2:], ['--configure', '-a'])
        self.assertEqual(self.journal_states(), ['staged', 'applying', 'interrupted', 'recovered'])
        self.assertFalse(self.recovery()['interrupted_dpkg'])
        self.assertFalse((self.system.state / 'staged').exists(), 'fully installed: retained archives released')

    def test_reconcile_leaves_a_running_transaction_alone(self):
        self.stage(self.bundle())
        store = update.Store(self.system.state)
        store.append('applying', 'c' * 32, self.now)
        store.prepare()
        holder = os.open(store.lock_path, os.O_RDWR | os.O_CREAT, 0o600)
        self.addCleanup(os.close, holder)
        fcntl.flock(holder, fcntl.LOCK_EX | fcntl.LOCK_NB)
        result = update.reconcile(runner=self.installer(), now=self.now, **self.paths())
        self.assertEqual(result['status'], 'busy')
        self.assertEqual(self.journal_states()[-1], 'applying')
        self.assertEqual(self.runner_calls, [])

    def test_repair_that_cannot_fix_dpkg_reports_failure_and_the_rescue_route(self):
        self.system.status.write_text(support.status_stanza('wros', '0.1.0~rc2', state='install ok half-configured',
                                                            arch='amd64'))

        def broken(command):
            self.runner_calls.append(command)
            return types.SimpleNamespace(returncode=1, stdout='', stderr='')

        result = update.repair_updates(runner=broken, now=self.now, **{k: v for k, v in self.paths().items()})
        self.assertEqual(result['status'], 'failed')
        self.assertIn('wros-rescue', result['message'])
        self.assertEqual(self.recovery()['update_state'], 'failed')

    def test_an_unseen_failure_is_not_hidden_by_a_later_staging(self):
        self.stage(self.bundle())
        with self.assertRaises(ValueError):
            self.apply(runner=self.installer(returncode=100))
        # dpkg is consistent (the fake failed before touching it); the writer stages a newer bundle.
        update.cancel_staged(state_dir=self.system.state, trigger_path=self.system.trigger, now=self.now)
        self.stage(self.bundle('newer', version='0.1.0~rc3', date_ago=timedelta(minutes=20)))
        public = update.read_public_status(self.system.state)
        self.assertEqual(public['update_state'], 'staged')
        self.assertEqual(public['unresolved_failure']['state'], 'failed')

    # ------------------------------------------------------ status privacy

    @unittest.skipIf(os.geteuid() == 0, 'permission denial cannot be observed as root')
    def test_recovery_status_reads_only_the_public_mirror(self):
        self.stage(self.bundle())
        private = [self.system.state / 'journal.jsonl', self.system.state / 'staged']
        for path in private:
            path.chmod(0)
        self.addCleanup(lambda: [p.chmod(0o700) for p in private])
        status = self.recovery()
        self.assertEqual(status['update_state'], 'staged')
        self.assertEqual(status['staged_packages'], ['wros 0.1.0~rc2'])
        self.assertEqual(oct((self.system.state / 'public/status.json').stat().st_mode & 0o777), '0o644')

    # --------------------------------------------------- privilege boundary

    def test_launchers_refuse_anything_but_an_exact_polkit_invocation(self):
        good = self.media / 'bundle'
        review_id = 'a' * 64
        cases = {
            'not root': ('stage', [str(good), review_id], '1000', 1000, 'must run as root'),
            'no polkit caller': ('stage', [str(good), review_id], None, 0, 'through polkit'),
            'forged caller': ('stage', [str(good), review_id], '1000; rm', 0, 'through polkit'),
            'extra argument': ('stage', [str(good), review_id, 'x'], '1000', 0, 'usage'),
            'bad review id': ('stage', [str(good), 'A' * 64], '1000', 0, 'invalid review id'),
            'path outside media': ('stage', ['/home/writer/bundle', review_id], '1000', 0, 'removable-media'),
            'parent traversal': ('stage', ['/media/../etc', review_id], '1000', 0, 'unsupported component'),
            'cancel with arguments': ('cancel', ['now'], '1000', 0, 'usage'),
            'repair with arguments': ('repair', ['--force'], '1000', 0, 'usage'),
            'media without mount': ('media', [], '1000', 0, 'usage'),
            'unknown verb': ('shell', [], '1000', 0, 'unavailable'),
        }
        for name, (verb, argv, uid, euid, pattern) in cases.items():
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, pattern):
                    update.launcher_main(verb, argv, uid, euid=euid, allowed_prefixes=self.prefixes, **self.paths())
        self.assertFalse(os.path.lexists(self.system.trigger))
        self.assertEqual(self.journal_states(), [])

    def test_reserved_boot_verbs_refuse_polkit_and_ordinary_callers(self):
        for argv, marker in ((['run-offline-boot'], 'WROS_SYSTEMD_OFFLINE_UPDATE'),
                             (['reconcile'], 'WROS_SYSTEMD_RECONCILE'), (['rescue-repair'], 'WROS_RESCUE')):
            with self.subTest(argv=argv):
                with self.assertRaisesRegex(ValueError, 'unavailable'):
                    update.main(argv, {marker: '1', 'PKEXEC_UID': '1000'})
                with self.assertRaisesRegex(ValueError, 'unavailable'):
                    update.main(argv, {})

    def test_failed_boot_continues_to_a_login_and_never_reboots(self):
        calls = []
        fake_run = lambda command, **kwargs: calls.append(command) or types.SimpleNamespace(returncode=0)
        with mock.patch.object(update, 'reserved_role'), \
                mock.patch.object(update, 'owned_trigger', return_value=True), \
                mock.patch.object(update.subprocess, 'run', fake_run):
            with mock.patch.object(update, 'execute_offline_boot_transaction', side_effect=ValueError('bad')):
                with self.assertRaisesRegex(ValueError, 'bad'):
                    update.main(['run-offline-boot'], {})
            self.assertEqual(calls, [['/usr/bin/systemctl', '--no-block', 'isolate', 'multi-user.target']])
            calls.clear()
            with mock.patch.object(update, 'execute_offline_boot_transaction', return_value={'status': 'completed'}):
                with mock.patch('builtins.print'):
                    update.main(['run-offline-boot'], {})
            self.assertEqual(calls, [['/usr/bin/systemctl', '--no-block', 'reboot']])
        calls.clear()
        with mock.patch.object(update, 'reserved_role'), \
                mock.patch.object(update, 'owned_trigger', return_value=False), \
                mock.patch.object(update.subprocess, 'run', fake_run), \
                mock.patch.object(update, 'execute_offline_boot_transaction', side_effect=ValueError('foreign')):
            with self.assertRaises(ValueError):
                update.main(['run-offline-boot'], {})
        self.assertEqual(calls, [], 'a foreign or unowned marker never isolates or reboots')


if __name__ == '__main__':
    unittest.main()
