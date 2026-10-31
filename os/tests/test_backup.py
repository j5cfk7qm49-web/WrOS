"""Real Restic encrypted repositories and verified restore, with zero network."""
import importlib.machinery
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).parents[1]/'scripts/wros-backup'
module = importlib.machinery.SourceFileLoader('wros_backup', str(SCRIPT)).load_module()


class EncryptedBackup(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='wros-backup-test-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.project = self.base/'A book with spaces'
        (self.project/'manuscript/empty chapter').mkdir(parents=True)
        (self.project/'manuscript/01-scène.md').write_text('---\nprivate: false\n---\nÉlan — a saved scene.\n', encoding='utf-8')
        (self.project/'.grimoire/writingdeck').mkdir(parents=True)
        (self.project/'.grimoire/writingdeck/knowledge.json').write_text('{"private":"character secret"}')
        (self.project/'research').mkdir()
        (self.project/'research/reference.bin').write_bytes(bytes(range(256)))
        (self.project/'novel.toml').write_text('title = "A book"\n')
        self.service = module.Service(self.base/'private-state')
        self.repo = self.base/'encrypted repository'

    def initialize(self):
        return self.service.configure(str(self.repo))

    def test_encrypted_snapshot_restores_after_original_is_deleted_and_key_is_imported(self):
        result = self.initialize()
        key = Path(result['recovery_key'])
        self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o600)
        expected = module.inventory(self.project)
        result = self.service.backup(str(self.project))
        self.assertEqual(len(result['snapshots']), 1)
        self.assertTrue(result['snapshots'][0]['restore_checked_at'])
        self.assertIn('same filesystem', result['notice'])
        # Plain manuscript/annotation bytes must not occur in the encrypted pool.
        for path in (self.repo/'data').rglob('*'):
            if path.is_file():
                self.assertNotIn(b'character secret', path.read_bytes())
        recovery = self.base/'separate-recovery.key'
        self.service.export_key(str(recovery))
        self.assertEqual(key.read_bytes(), recovery.read_bytes())
        identifier = result['snapshots'][0]['id']
        shutil.rmtree(self.project)
        another_device = module.Service(self.base/'another-device')
        connected = another_device.configure(str(self.repo), str(recovery))
        self.assertEqual(connected['snapshots'][0]['restore_checked_at'], '')
        restored = self.base/'restored book'
        result = another_device.restore(identifier, str(restored))
        self.assertEqual(module.inventory(restored), expected)
        self.assertTrue(result['snapshots'][0]['restore_checked_at'])
        self.assertFalse((restored/module.MANIFEST).exists())

    def test_existing_restore_destination_and_repository_overlap_never_overwrite(self):
        self.initialize()
        result = self.service.backup(str(self.project))
        identifier = result['snapshots'][0]['id']
        before = module.inventory(self.project)
        for destination in (self.project, self.repo/'restored', self.base/'private-state/restored'):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                self.service.restore(identifier, str(destination))
        self.assertEqual(module.inventory(self.project), before)
        existing = self.base/'empty-existing'
        existing.mkdir()
        with self.assertRaises(ValueError):
            self.service.restore(identifier, str(existing))
        self.assertEqual(list(existing.iterdir()), [])
        source = self.base/'staged'
        source.mkdir()
        (source/'sentinel').write_text('staged')
        with self.assertRaises(OSError):
            module.publish_directory(source, existing)
        self.assertTrue((source/'sentinel').exists())

    def test_changed_sources_and_symlinks_cannot_become_verified_snapshots(self):
        self.initialize()
        original = module.inventory
        def change_after_copy(root, copy_to=None):
            result = original(root, copy_to)
            if copy_to:
                (root/'manuscript/01-scène.md').write_text('external edit retained', encoding='utf-8')
            return result
        with mock.patch.object(module, 'inventory', side_effect=change_after_copy):
            with self.assertRaisesRegex(ValueError, 'changed during staging'):
                self.service.backup(str(self.project))
        self.assertEqual(self.service.snapshots(), [])
        link = self.project/'research/external-link'
        link.symlink_to(self.base/'private-state')
        with self.assertRaisesRegex(ValueError, 'symbolic link'):
            self.service.backup(str(self.project))
        self.assertEqual(self.service.snapshots(), [])

    def test_invalid_manifest_is_kept_in_an_incomplete_restore_without_a_final_book(self):
        self.initialize()
        stage = self.base/'invalid-backup'
        stage.mkdir()
        (stage/'manuscript').mkdir()
        (stage/module.MANIFEST).write_text(json.dumps({'version': 1, 'inventory': {'files': {}}}))
        self.service.restic(['backup', '--tag', module.TAG, '--', str(stage)], stage)
        snapshot = self.service.snapshots()[0]
        final = self.base/'must-not-be-published'
        with self.assertRaisesRegex(ValueError, 'Restore incomplete'):
            self.service.restore(snapshot['id'], str(final))
        self.assertFalse(final.exists())
        self.assertEqual(self.service.current['verified'], {})
        self.assertTrue(list(self.base.glob('.wros-restore-incomplete-*')))

    def test_wrong_key_unknown_settings_and_credential_symlinks_are_preserved(self):
        result = self.initialize()
        wrong = self.base/'wrong.key'
        wrong.write_text('incorrect-recovery-password\n')
        another = module.Service(self.base/'another-device')
        with self.assertRaises(ValueError):
            another.configure(str(self.repo), str(wrong))
        self.assertFalse(another.config.exists())
        key = Path(result['recovery_key'])
        key.chmod(0o644)
        with self.assertRaisesRegex(ValueError, 'private'):
            self.service.snapshots()
        key.chmod(0o600)
        unknown = b'{"version":99,"future":"preserve me"}'
        module.atomic_private(self.service.config, unknown)
        with self.assertRaisesRegex(ValueError, 'Unknown backup'):
            module.Service(self.service.state)
        self.assertEqual(self.service.config.read_bytes(), unknown)

    def test_google_tokens_never_enter_arguments_and_are_committed_only_after_validation(self):
        token = {'access_token': 'PRIVATE_ACCESS_FIXTURE', 'refresh_token': 'PRIVATE_REFRESH_FIXTURE',
                 'expiry': '2099-01-01T00:00:00Z', 'token_type': 'Bearer'}
        def provider(program, args, env, cwd=None):
            self.assertEqual(program, '/usr/bin/rclone')
            self.assertEqual(args, ['about', 'wros-google:', '--json'])
            self.assertNotIn('PRIVATE_', ' '.join(args))
            candidate = Path(env['RCLONE_CONFIG'])
            self.assertEqual(stat.S_IMODE(candidate.stat().st_mode), 0o600)
            self.assertIn('PRIVATE_REFRESH_FIXTURE', candidate.read_text())
            self.assertFalse(self.service.google.exists())
            return b'{"total":123456,"used":0}'
        with mock.patch.object(module, 'run', side_effect=provider):
            result = self.service.connect_google(json.dumps(token))
        self.assertTrue(result['google_connected'])
        self.assertEqual(result['google_state'], 'connected')
        self.assertEqual(result['google_expiry'], '2099-01-01T00:00:00Z')
        self.assertEqual(stat.S_IMODE(self.service.account.stat().st_mode), 0o600)
        self.assertNotIn('PRIVATE_', json.dumps(result))
        before = self.service.google.read_bytes()
        with mock.patch.object(module, 'run', side_effect=ValueError('Provider unavailable')):
            with self.assertRaises(ValueError):
                self.service.connect_google(json.dumps(token))
        self.assertEqual(self.service.google.read_bytes(), before)
        self.assertEqual(self.service.repository('drive:WrOS-Backups'), 'rclone:wros-google:WrOS-Backups')
        for descriptor in ('drive:/root', 'drive:../escape', 'drive:a//b', 'sftp:host:book', 'relative/path'):
            with self.assertRaises(ValueError):
                self.service.repository(descriptor)

    def test_google_credential_lifecycle_check_revocation_reauthorize_and_disconnect_scrub(self):
        token_a = {'access_token': 'PRIVATE_ACCESS_A', 'refresh_token': 'PRIVATE_REFRESH_A',
                   'expiry': '2026-11-01T00:00:00Z', 'token_type': 'Bearer'}
        with mock.patch.object(module, 'run', return_value=b'{"total":1000,"used":10}'):
            self.service.connect_google(json.dumps(token_a))
        self.assertTrue(self.service.google.is_file())
        self.assertTrue(self.service.account.is_file())

        # Offline failure preserves credentials and marks state 'offline'.
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'offline')):
            with self.assertRaisesRegex(ValueError, 'could not be reached'):
                self.service.check_google()
        reloaded = module.Service(self.service.state)
        self.assertEqual(reloaded.status(refresh=False)['google_state'], 'offline')
        self.assertIn(b'PRIVATE_REFRESH_A', self.service.google.read_bytes())

        # Revoked/expired grant marks state 'reauthorization-required' without leaking secrets.
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'reauthorization-required')):
            with self.assertRaisesRegex(ValueError, 'expired or was revoked'):
                reloaded.check_google()
        reloaded = module.Service(self.service.state)
        self.assertEqual(reloaded.status(refresh=False)['google_state'], 'reauthorization-required')

        # Reauthorizing with a fresh grant restores 'connected' state.
        token_b = {'access_token': 'PRIVATE_ACCESS_B', 'refresh_token': 'PRIVATE_REFRESH_B',
                   'expiry': '2027-01-01T00:00:00Z', 'token_type': 'Bearer'}
        with mock.patch.object(module, 'run', return_value=b'{"total":2000,"used":20}'):
            status = reloaded.connect_google(json.dumps(token_b))
        self.assertEqual(status['google_state'], 'connected')
        self.assertEqual(status['google_expiry'], '2027-01-01T00:00:00Z')
        self.assertIn(b'PRIVATE_REFRESH_B', self.service.google.read_bytes())
        self.assertNotIn(b'PRIVATE_REFRESH_A', self.service.google.read_bytes())

        # Verify active connection updates verified_at.
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":2000,"used":20}', '')):
            checked = reloaded.check_google()
        self.assertEqual(checked['google_state'], 'connected')
        self.assertTrue(checked['google_verified_at'])

        # Disconnect scrubs rclone.conf before unlinking and clears account metadata.
        written_payloads = []
        orig_write_private = module.write_private
        def spy_write_private(path, contents, exclusive=False):
            if Path(path) == self.service.google:
                written_payloads.append(bytes(contents))
            return orig_write_private(path, contents, exclusive=exclusive)
        with mock.patch.object(module, 'write_private', side_effect=spy_write_private):
            disconnected = reloaded.disconnect_google()
        self.assertFalse(disconnected['google_connected'])
        self.assertEqual(disconnected['google_state'], 'not-connected')
        self.assertFalse(self.service.google.exists())
        self.assertFalse(self.service.account.exists())
        self.assertEqual(len(written_payloads), 1)
        self.assertTrue(all(b == 0 for b in written_payloads[0]))

    def test_durable_offline_queue_and_interruption_safe_deduplicating_retry(self):
        # Queueing works completely offline before a repository is even connected.
        with mock.patch.object(module, 'run', side_effect=AssertionError('Queue must be offline')):
            queued = self.service.queue_backup(str(self.project))
        self.assertEqual(len(queued['queued_backups']), 1)
        self.assertEqual(queued['queued_backups'][0]['attempts'], 0)
        self.assertEqual(stat.S_IMODE(self.service.queue_path.stat().st_mode), 0o600)

        # Configure synthetic repository and simulate a transport failure during backup.
        key_path = self.service.state/'recovery-fixture.key'
        module.write_private(key_path, b'0123456789abcdef0123456789abcdef\n', exclusive=True)
        self.service.current = {'version': 1, 'repository': str(self.repo), 'key': str(key_path), 'verified': {}}
        self.repo.mkdir()
        self.service.save()

        with mock.patch.object(self.service, 'restic', side_effect=ValueError('Destination offline')):
            with self.assertRaisesRegex(ValueError, 'Destination offline'):
                self.service.backup(str(self.project))
        reloaded = module.Service(self.service.state)
        self.assertEqual(len(reloaded.queue), 1)
        self.assertEqual(reloaded.queue[0]['attempts'], 1)
        self.assertIn('did not complete', reloaded.queue[0]['last_error'])

        # Simulate an interrupted backup where the snapshot was already uploaded
        # to the repository, but restore verification was interrupted before completion.
        snap_id = 'a' * 64
        expected_inv = module.inventory(self.project)
        def fake_snapshots(queue_id=None):
            if queue_id is not None:
                self.assertEqual(queue_id, reloaded.queue[0]['id'])
            return [{'id': snap_id, 'time': '2026-10-31T05:00:00Z', 'path': '/staged/book',
                     'project': 'A book with spaces',
                     'restore_checked_at': reloaded.current['verified'].get(snap_id, '')}]
        def fake_restore_into(snapshot, target):
            self.assertEqual(snapshot['id'], snap_id)
            return expected_inv

        with mock.patch.object(reloaded, 'restic', return_value=b''), \
             mock.patch.object(reloaded, 'snapshots', side_effect=fake_snapshots), \
             mock.patch.object(reloaded, 'restore_into', side_effect=fake_restore_into), \
             mock.patch.object(reloaded, 'backup', side_effect=AssertionError('Should verify existing unverified snapshot without re-uploading')):
            completed = reloaded.run_queue()
        self.assertEqual(completed['queued_backups'], [])
        self.assertIn(snap_id, reloaded.current['verified'])

        # Clear queue removes pending items without touching verified receipts.
        reloaded.queue_backup(str(self.project))
        self.assertEqual(len(reloaded.queue), 1)
        cleared = reloaded.clear_queue()
        self.assertEqual(cleared['queued_backups'], [])
        self.assertIn(snap_id, reloaded.current['verified'])

        # Unknown future schema versions in google-account.json or queue.json are preserved.
        future_queue = b'{"version":99,"items":[]}\n'
        module.atomic_private(self.service.queue_path, future_queue)
        with self.assertRaisesRegex(ValueError, 'Unknown backup queue'):
            module.Service(self.service.state)
        self.assertEqual(self.service.queue_path.read_bytes(), future_queue)

    def test_real_interrupted_upload_is_reverified_without_duplicate_snapshot(self):
        self.initialize()
        with mock.patch.object(self.service, 'restore_into', side_effect=InterruptedError('fixture interruption')):
            with self.assertRaises(InterruptedError):
                self.service.backup(str(self.project))
        pending = module.Service(self.service.state)
        self.assertEqual(len(pending.queue), 1)
        before = pending.snapshots()
        self.assertEqual(len(before), 1)
        self.assertFalse(before[0]['restore_checked_at'])
        # An identically named project is not sufficient: the engine must match
        # the durable queue operation tag and verify the actual restored bytes.
        tags = json.loads(pending.restic(['snapshots']))[0]['tags']
        self.assertIn('queue:' + pending.queue[0]['id'], tags)
        result = pending.run_queue()
        self.assertEqual(result['queued_backups'], [])
        self.assertEqual([s['id'] for s in result['snapshots']], [before[0]['id']])
        self.assertTrue(result['snapshots'][0]['restore_checked_at'])

    def test_queue_retain_and_count_failures_before_upload_and_for_missing_projects(self):
        self.initialize()
        self.service.queue_backup(str(self.project))
        with mock.patch.object(self.service, 'snapshots', side_effect=ValueError('offline')):
            with self.assertRaisesRegex(ValueError, 'remain queued'):
                self.service.run_queue()
        reloaded = module.Service(self.service.state)
        self.assertEqual(reloaded.queue[0]['attempts'], 1)
        self.project.rename(self.base/'temporarily-unavailable')
        with self.assertRaisesRegex(ValueError, 'remain queued'):
            reloaded.run_queue()
        self.assertEqual(module.Service(self.service.state).queue[0]['attempts'], 2)

    def test_status_is_offline_and_malformed_authorization_never_echoes_a_secret(self):
        with mock.patch.object(module, 'run', side_effect=AssertionError('No process expected')):
            self.assertFalse(self.service.status(refresh=False)['configured'])
        env = dict(os.environ, XDG_DATA_HOME=str(self.base/'data'))
        result = subprocess.run(['/usr/bin/python3', '-I', str(SCRIPT)],
                                input=b'{"operation":"connect-google","authorization":"PRIVATE_BROKEN_JSON"}',
                                capture_output=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(b'PRIVATE_', result.stdout + result.stderr)
        self.assertFalse(json.loads(result.stdout)['ok'])

    def test_real_rclone_transport_roundtrip_and_unavailable_remote_preserve_receipts_and_sources(self):
        # Actual rclone serve-restic transport, with its maintained local alias
        # backend instead of a real Google account. No provider qualification.
        remote = self.base/'disposable-remote'
        remote.mkdir()
        module.write_private(self.service.google,
            ('[wros-google]\ntype = alias\nremote = ' + str(remote) + '\n').encode(), exclusive=True)
        self.service.configure('drive:WrOS-Backups')
        result = self.service.backup(str(self.project))
        identifier = result['snapshots'][0]['id']
        target = self.base/'rclone-restored'
        self.service.restore(identifier, str(target))
        self.assertEqual(module.inventory(target), module.inventory(self.project))
        before = dict(self.service.current['verified'])
        original = module.inventory(self.project)
        remote.rename(self.base/'disconnected-remote')
        with self.assertRaises(ValueError):
            self.service.backup(str(self.project))
        self.assertEqual(self.service.current['verified'], before)
        self.assertEqual(module.inventory(self.project), original)


if __name__ == '__main__':
    unittest.main()
