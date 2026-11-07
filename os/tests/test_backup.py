"""Real Restic encrypted repositories and verified restore, with zero network."""
import importlib.machinery
import hashlib
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
        def provider(args, env):
            self.assertEqual(args, ['about', 'wros-google:', '--json'])
            self.assertNotIn('PRIVATE_', ' '.join(args))
            candidate = Path(env['RCLONE_CONFIG'])
            self.assertEqual(stat.S_IMODE(candidate.stat().st_mode), 0o600)
            self.assertIn('PRIVATE_REFRESH_FIXTURE', candidate.read_text())
            self.assertFalse(self.service.google.exists())
            return True, b'{"total":123456,"used":0}', ''
        with mock.patch.object(module, 'classify_rclone_failure', side_effect=provider):
            result = self.service.connect_google(json.dumps(token))
        self.assertTrue(result['google_connected'])
        self.assertEqual(result['google_state'], 'connected')
        self.assertEqual(result['google_expiry'], '2099-01-01T00:00:00Z')
        self.assertEqual(stat.S_IMODE(self.service.account.stat().st_mode), 0o600)
        self.assertNotIn('PRIVATE_', json.dumps(result))
        before = self.service.google.read_bytes()
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'offline')):
            with self.assertRaises(ValueError):
                self.service.connect_google(json.dumps(token))
        self.assertEqual(self.service.google.read_bytes(), before)
        self.assertEqual(self.service.repository('drive:WrOS-Backups'), 'rclone:wros-google:WrOS-Backups')
        for descriptor in ('drive:/root', 'drive:../escape', 'drive:a//b', 'sftp:host:book', 'relative/path'):
            with self.assertRaises(ValueError):
                self.service.repository(descriptor)

    def test_google_authorization_file_from_windows_connects_and_bad_files_never_echo(self):
        token = {'access_token': 'PRIVATE_ACCESS_FIXTURE', 'refresh_token': 'PRIVATE_REFRESH_FIXTURE',
                 'expiry': '2099-01-01T00:00:00Z', 'token_type': 'Bearer'}
        stick = self.base/'stick'
        stick.mkdir()
        # Notepad's byte-order mark and CRLF, and a token line copied out of an rclone.conf.
        for name, data in (('notepad.json', b'\xef\xbb\xbf' + json.dumps(token).encode() + b'\r\n'),
                           ('from-conf.json', b'token = ' + json.dumps(token).encode() + b'\n')):
            (stick/name).write_bytes(data)
            self.assertEqual(json.loads(module.read_authorization_file(str(stick/name))), token)
            with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":123456,"used":0}', '')):
                result = module.dispatch(self.service, {'operation': 'connect-google-file', 'path': str(stick/name)})
            self.assertTrue(result['google_connected'])
            self.assertIn('PRIVATE_REFRESH_FIXTURE', self.service.google.read_text())
            self.assertNotIn('PRIVATE_', json.dumps(result))
        (stick/'broken.json').write_text('PRIVATE_NOT_JSON')
        (stick/'large.json').write_bytes(b' ' * (module.AUTHORIZATION_LIMIT + 1))
        (stick/'link.json').symlink_to(stick/'notepad.json')
        for path, message in ((str(stick/'broken.json'), 'only the JSON'),
                              (str(stick/'large.json'), 'too large'),
                              (str(stick/'link.json'), 'Cannot open'),
                              (str(stick), 'regular file'),
                              (str(stick/'missing.json'), 'Cannot open'),
                              ('relative.json', 'absolute path')):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, message) as caught:
                module.read_authorization_file(path)
            self.assertNotIn('PRIVATE_', str(caught.exception))
        env = dict(os.environ, XDG_DATA_HOME=str(self.base/'data'))
        request = json.dumps({'operation': 'connect-google-file', 'path': str(stick/'broken.json')}).encode()
        result = subprocess.run(['/usr/bin/python3', '-I', str(SCRIPT)], input=request, capture_output=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(b'PRIVATE_', result.stdout + result.stderr)
        self.assertIn('only the JSON', json.loads(result.stdout)['error'])

    def test_status_names_each_books_latest_verified_snapshot_by_its_real_folder_offline(self):
        # The update gate asks for status without contacting the repository, and looks for a
        # snapshot of exactly this book's folder; restic itself only knows the staging copy.
        self.initialize()
        self.assertEqual(self.service.status(refresh=False)['snapshots'], [])
        made = self.service.backup(str(self.project))
        identifier = made['snapshots'][0]['id']
        with mock.patch.object(module, 'run', side_effect=AssertionError('status must stay offline')):
            offline = module.Service(self.service.state).status(refresh=False)['snapshots']
        self.assertEqual([(s['id'], s['path'], s['project']) for s in offline],
                         [(identifier, str(self.project.resolve()), self.project.name)])
        self.assertTrue(offline[0]['restore_checked_at'])
        self.assertEqual(made['snapshots'][0]['path'], str(self.project.resolve()))
        self.assertEqual(stat.S_IMODE((self.service.state / 'latest.json').stat().st_mode), 0o600)

    def test_backup_reports_each_step_and_restic_progress_without_file_names(self):
        import contextlib, io
        self.initialize()
        (self.project/'research/large.bin').write_bytes(os.urandom(8 * 1024**2))
        stream = io.StringIO()
        with contextlib.redirect_stderr(stream):
            result = self.service.backup(str(self.project))
        self.assertTrue(result['snapshots'][0]['restore_checked_at'])
        rows = [json.loads(line)['progress'] for line in stream.getvalue().splitlines()]
        phases = [row['phase'] for row in rows]
        steps = ['Checking the saved project', 'Copying the project to private staging',
                 'Encrypting and saving the snapshot', 'Checking the snapshot by restoring it',
                 'Comparing every restored file']
        self.assertEqual([p for i, p in enumerate(phases) if i == 0 or phases[i - 1] != p], steps)
        for row in rows:
            self.assertLessEqual(set(row), {'phase', 'percent'})
            self.assertTrue(0 <= row.get('percent', 0) <= 1)
        for private in ('A book with spaces', 'reference.bin', 'large.bin', 'manuscript'):
            self.assertNotIn(private, stream.getvalue())

    def test_google_connection_failures_say_which_problem_without_echoing_rclone(self):
        kinds = (
            ('oauth2: cannot fetch token: 400 Bad Request Response: {"error": "invalid_grant"}', 'reauthorization-required'),
            ('Failed to create file system: couldn\'t find root directory ID: Get "https://www.googleapis.com/drive/v3/files/root": '
             'tls: failed to verify certificate: x509: certificate has expired or is not yet valid: current time 2001-01-01T00:00:00Z', 'clock'),
            ('dial tcp: lookup www.googleapis.com: no such host', 'offline'),
            ('dial tcp 142.250.1.95:443: connect: network is unreachable', 'offline'),
            # Seen on the laptop with rclone's shared sign-in: Google's per-minute quota for that project.
            ("pacer: low level retry 2/2 (error googleapi: Error 403: Quota exceeded for quota metric 'Queries' "
             "and limit 'Previous quota: Requests per minute' of service 'drive.googleapis.com', rateLimitExceeded)",
             'rate-limited'),
            ('googleapi: Error 403: User Rate Limit Exceeded., userRateLimitExceeded', 'rate-limited'),
            ('PRIVATE_SOMETHING_ELSE went wrong', 'unknown'),
        )
        for text, kind in kinds:
            with self.subTest(kind=kind):
                self.assertEqual(module.rclone_failure_kind(text), kind)
        token = {'access_token': 'PRIVATE_ACCESS_FIXTURE', 'refresh_token': 'PRIVATE_REFRESH_FIXTURE',
                 'expiry': '2099-01-01T00:00:00Z', 'token_type': 'Bearer'}
        for kind, message in (('reauthorization-required', 'Google refused this authorization'),
                              ('clock', "This computer's clock says"),
                              ('offline', 'could not reach Google'),
                              ('rate-limited', 'Google is limiting requests'),
                              ('unknown', 'rclone could not connect')):
            with self.subTest(kind=kind), \
                    mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', kind)), \
                    self.assertRaisesRegex(ValueError, message) as caught:
                self.service.connect_google(json.dumps(token))
            self.assertNotIn('PRIVATE_', str(caught.exception))
            self.assertFalse(self.service.google.exists())
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1,"used":0}', '')):
            self.service.connect_google(json.dumps(token))
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'clock')), \
                self.assertRaisesRegex(ValueError, "This computer's clock says"):
            self.service.check_google()

    def test_google_credential_lifecycle_check_revocation_reauthorize_and_disconnect_scrub(self):
        token_a = {'access_token': 'PRIVATE_ACCESS_A', 'refresh_token': 'PRIVATE_REFRESH_A',
                   'expiry': '2026-11-01T00:00:00Z', 'token_type': 'Bearer'}
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1000,"used":10}', '')):
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
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":2000,"used":20}', '')):
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
            return expected_inv, None  # (the book's inventory, its settings folder if any)

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

    def test_drive_browsing_from_the_top_and_a_checked_local_copy(self):
        # Path traversal is refused before any rclone call.
        for bad in ('drive:..', 'drive:WrOS-Library/../secret', '/local/path'):
            with self.assertRaisesRegex(ValueError, 'unsafe|drive:'):
                self.service.drive_list(bad)
        with self.assertRaisesRegex(ValueError, 'Choose a file or folder'):
            self.service.drive_materialize('drive:', str(self.base / 'x'), str(self.project))

        # Connect synthetic Google configuration.
        module.write_private(self.service.google, b'[wros-google]\ntype = drive\nscope = drive.file\n', exclusive=True)
        listing_json = json.dumps([
            {'Name': 'Outline.md', 'Path': 'Outline.md', 'Size': 25, 'ModTime': '2026-10-31T10:00:00Z', 'IsDir': False},
            {'Name': 'ResearchFolder', 'Path': 'ResearchFolder', 'Size': 0, 'ModTime': '2026-10-31T09:00:00Z', 'IsDir': True},
        ]).encode('utf-8')
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, listing_json, '')) as rclone_mock:
            status = self.service.drive_list('drive:WrOS-Library')
        rclone_mock.assert_called_once()
        self.assertEqual(rclone_mock.call_args[0][0], ['lsjson', 'wros-google:WrOS-Library'])
        self.assertEqual(status['drive_folder'], 'drive:WrOS-Library')
        self.assertEqual(len(status['drive_entries']), 2)
        self.assertEqual(status['drive_entries'][0]['name'], 'ResearchFolder')
        self.assertTrue(status['drive_entries'][0]['is_dir'])
        self.assertEqual(status['drive_entries'][1]['name'], 'Outline.md')
        self.assertFalse(status['drive_entries'][1]['cached'])

        # Materializing inside the open project or over an existing path is refused.
        with self.assertRaisesRegex(ValueError, 'outside the currently open project'):
            self.service.drive_materialize('drive:WrOS-Library/Outline.md', str(self.project / 'Outline.md'), str(self.project))
        existing_file = self.base / 'already-here.md'
        existing_file.write_text('keep me', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.service.drive_materialize('drive:WrOS-Library/Outline.md', str(existing_file), str(self.project))

        # A copy is checked against Drive's checksum before it is kept.
        text = b'# Materialized Outline\n\n1. Act One\n'
        def fake_rclone(remote_md5):
            def run(args, env):
                if args[0] == 'copyto':
                    self.assertEqual(args[:2], ['copyto', 'wros-google:WrOS-Library/Outline.md'])
                    Path(args[2]).write_bytes(text)
                    return True, b'', ''
                self.assertEqual(args, ['lsjson', '--hash', '--stat', 'wros-google:WrOS-Library/Outline.md'])
                return True, json.dumps({'Name': 'Outline.md', 'Hashes': {'md5': remote_md5}}).encode(), ''
            return run

        bad_copy = self.base / 'bad-copy.md'
        with mock.patch.object(module, 'classify_rclone_failure', side_effect=fake_rclone('0' * 32)):
            with self.assertRaisesRegex(ValueError, "didn't match Google Drive"):
                self.service.drive_materialize('drive:WrOS-Library/Outline.md', str(bad_copy), str(self.project))
        self.assertFalse(bad_copy.exists())
        self.assertEqual([p for p in self.base.iterdir() if p.name.startswith('.wros-drive-')], [])

        dest = self.base / 'materialized-outline.md'
        with mock.patch.object(module, 'classify_rclone_failure',
                               side_effect=fake_rclone(hashlib.md5(text).hexdigest())):
            mat = self.service.drive_materialize('drive:WrOS-Library/Outline.md', str(dest), str(self.project))
        self.assertEqual(dest.read_bytes(), text)
        self.assertIn('checked against Google Drive', mat['notice'])
        outline = next(e for e in mat['drive_entries'] if e['name'] == 'Outline.md')
        self.assertTrue(outline['cached'])
        self.assertEqual(outline['local'], str(dest))

        # The top of the Drive lists; a Google Doc is marked as one.
        top = json.dumps([
            {'Name': 'Novel', 'Size': 0, 'ModTime': '2026-10-31T09:00:00Z', 'IsDir': True,
             'MimeType': 'inode/directory'},
            {'Name': 'Notes', 'Size': -1, 'ModTime': '2026-10-31T09:00:00Z', 'IsDir': False,
             'MimeType': 'application/vnd.google-apps.document'},
        ]).encode()
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, top, '')) as rclone_mock:
            status = self.service.drive_list('drive:')
        self.assertEqual(rclone_mock.call_args[0][0], ['lsjson', 'wros-google:'])
        self.assertEqual(status['drive_folder'], 'drive:')
        self.assertEqual([e['path'] for e in status['drive_entries']], ['drive:Novel', 'drive:Notes'])
        self.assertEqual([e['google_doc'] for e in status['drive_entries']], [False, True])
        self.assertEqual(status['drive_entries'][1]['size'], 0)

        # Revoked or offline errors update google_meta state cleanly.
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'reauthorization-required')):
            with self.assertRaisesRegex(ValueError, 'expired or was revoked'):
                self.service.drive_list('drive:WrOS-Library')
        self.assertEqual(self.service.google_meta['state'], 'reauthorization-required')



class Retention(unittest.TestCase):
    """Automatic snapshots: all from the last 7 days, the newest of each day to 90 days, of each week after."""

    NOW = module.datetime(2026, 11, 5, 12, 0, tzinfo=module.timezone.utc)

    def row(self, ident, days_ago, hours=0, tags=('wros-project-v1', 'project:Novel', 'auto')):
        moment = self.NOW - module.timedelta(days=days_ago, hours=hours)
        # restic writes nanoseconds and a local offset.
        return {'id': ident, 'time': moment.astimezone(module.timezone(module.timedelta(hours=1)))
                .isoformat().replace('+01:00', '.123456789+01:00'), 'tags': list(tags)}

    def test_the_schedule_keeps_a_week_then_days_then_weeks(self):
        rows = [self.row('week-a', 1), self.row('week-b', 1, 2), self.row('week-c', 6, 23),
                self.row('day-new', 10, 1), self.row('day-old', 10, 5), self.row('day-other', 11),
                self.row('wk-new', 120), self.row('wk-old', 121), self.row('wk-other', 130)]
        # 120 and 121 days ago share an ISO week here; 130 days ago does not.
        self.assertEqual(module.iso_week(self.NOW - module.timedelta(days=120)),
                         module.iso_week(self.NOW - module.timedelta(days=121)))
        self.assertEqual(sorted(module.retention_forget(rows, self.NOW)), ['day-old', 'wk-old'])

    def test_restic_times_keep_their_microseconds(self):
        moment = module.restic_time('2026-11-05T13:00:00.123456789+01:00')
        self.assertEqual((moment.hour, moment.microsecond, moment.utcoffset()),
                         (13, 123456, module.timedelta(hours=1)))

    def test_manual_snapshots_and_other_books_are_never_forgotten(self):
        rows = [self.row('manual', 200, tags=('wros-project-v1', 'project:Novel')),
                self.row('manual-2', 200, 1, tags=('wros-project-v1', 'project:Novel')),
                self.row('mine', 200, 2), self.row('other-book', 200, 3,
                                                   tags=('wros-project-v1', 'project:Other', 'auto'))]
        self.assertEqual(module.retention_forget(rows, self.NOW), [])


class AutomaticBackup(unittest.TestCase):
    setUp = EncryptedBackup.setUp
    initialize = EncryptedBackup.initialize

    def test_an_automatic_backup_is_verified_tagged_and_prunes_only_older_automatic_ones(self):
        self.initialize()
        manual = self.service.backup(str(self.project))['snapshots'][0]['id']
        first = self.service.backup(str(self.project), automatic=True)
        (self.project/'manuscript/01-scène.md').write_text('A second saved version.\n', encoding='utf-8')
        second = self.service.backup(str(self.project), automatic=True)
        self.assertIn('read back and matched', second['notice'])
        rows = json.loads(self.service.restic(['snapshots', '--tag', 'wros-project-v1,auto']))
        self.assertEqual(len(rows), 2, 'both automatic snapshots are recent: nothing pruned yet')
        newest = max(rows, key=lambda r: r['time'])['id']
        # Half a year later the two automatic snapshots share a week: the older one goes.
        later = module.datetime.now(module.timezone.utc) + module.timedelta(days=200)
        self.service.prune(now=later)
        remaining = {r['id'] for r in json.loads(self.service.restic(['snapshots', '--tag', 'wros-project-v1']))}
        self.assertEqual(remaining, {manual, newest})
        self.assertTrue(first['snapshots'])

    def test_the_newest_verified_snapshot_of_a_book_is_never_pruned(self):
        self.initialize()
        self.service.backup(str(self.project), automatic=True)
        (self.project/'novel.toml').write_text('title = "Renamed"\n')
        self.service.backup(str(self.project), automatic=True)
        latest = self.service.latest()[str(self.project.resolve())]['id']
        with mock.patch.object(module, 'retention_forget', return_value=[latest]):
            self.service.prune(now=module.datetime.now(module.timezone.utc))
        remaining = {r['id'] for r in json.loads(self.service.restic(['snapshots', '--tag', 'wros-project-v1']))}
        self.assertIn(latest, remaining)

    def test_an_automatic_backup_reads_back_only_changed_files_until_a_week_has_passed(self):
        self.initialize()
        first = self.service.backup(str(self.project), automatic=True)
        self.assertIn('complete restore', first['notice'])
        scene = self.project/'manuscript/01-scène.md'
        scene.write_text('A second saved version.\n', encoding='utf-8')
        dumped = []
        real_dump = self.service.dump_sha256
        def dump(snapshot, relative, limit):
            dumped.append(relative)
            return real_dump(snapshot, relative, limit)
        with mock.patch.object(self.service, 'restore_into', side_effect=AssertionError('no complete restore')), \
                mock.patch.object(self.service, 'dump_sha256', side_effect=dump):
            second = self.service.backup(str(self.project), automatic=True)
        self.assertIn('1 changed file read back', second['notice'])
        self.assertEqual(dumped, ['manuscript/01-scène.md'])
        newest = second['snapshots'][0]['id']
        self.assertIn(newest, self.service.current['verified'])

        # A changed file that reads back differently is never called checked: the complete
        # restore decides, and it fails too.
        scene.write_text('A third saved version.\n', encoding='utf-8')
        with mock.patch.object(self.service, 'dump_sha256', return_value=(0, '0' * 64)), \
                mock.patch.object(self.service, 'restore_into', side_effect=ValueError('restore differs')):
            with self.assertRaisesRegex(ValueError, 'restore differs'):
                self.service.backup(str(self.project), automatic=True)

        # A week after the last complete restore, the next one is complete again.
        later = module.datetime.now(module.timezone.utc) + module.timedelta(days=8)
        real_datetime = module.datetime
        class Later(real_datetime):
            @classmethod
            def now(cls, tz=None):
                return later
        with mock.patch.object(module, 'datetime', Later):
            third = self.service.backup(str(self.project), automatic=True)
        self.assertIn('complete restore', third['notice'])

    def test_back_up_now_always_restores_completely(self):
        self.initialize()
        self.service.backup(str(self.project), automatic=True)
        (self.project/'novel.toml').write_text('title = "Renamed"\n')
        self.assertIn('complete restore', self.service.backup(str(self.project))['notice'])

    def test_the_app_asks_for_an_automatic_backup_by_its_own_operation(self):
        self.initialize()
        result = module.dispatch(self.service, {'operation': 'auto-backup', 'project': str(self.project)})
        rows = json.loads(self.service.restic(['snapshots', '--tag', 'wros-project-v1,auto']))
        self.assertEqual([r['id'] for r in rows], [result['snapshots'][0]['id']])


class PhoneLinking(unittest.TestCase):
    """The link-phone operation: the helper's side of os/scripts/wros-link.py (tested in test_link)."""

    def setUp(self):
        EncryptedBackup.setUp(self)
        self.clients = self.base/'google-client.json'
        self.clients.write_text(json.dumps({'linking': {'client_id': 'web-id', 'client_secret': 'web-secret'},
                                            'mailbox': {'client_id': 'tv-id', 'client_secret': 'tv-secret'}}))
        self.service.clients_path = self.clients
        self.token = {'access_token': 'PRIVATE_ACCESS', 'refresh_token': 'PRIVATE_REFRESH',
                      'expiry': '2099-01-01T00:00:00Z', 'token_type': 'Bearer'}

    def fake_link(self, outcome):
        test = self

        class Session:
            def __init__(self, clients, *, listener=None, offer=None, **_):
                test.session = self
                self.clients, self.offer, self.events = clients, offer, []

            def run(self):
                self.offer({'link': 'https://example.invalid/l/#x', 'code': 'KXP-QKN-VLWD', 'qr': ['0101']})
                if isinstance(outcome, Exception):
                    raise outcome
                return outcome, 'drive'

            def finish(self, token, via):
                self.events.append(('finish', via))

            def fail(self, message):
                self.events.append(('fail', message))

        fake = mock.Mock(Link=Session, local_address=lambda: None, Listener=None)
        return mock.patch.object(module, 'load_link', return_value=fake)

    def test_linking_stores_the_token_with_the_client_that_issued_it(self):
        with self.fake_link(self.token), \
             mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1}', '')), \
             mock.patch.object(module, 'progress') as progress:
            result = module.dispatch(self.service, {'operation': 'link-phone'})
        self.assertTrue(result['google_connected'])
        stored = self.service.google.read_text()
        self.assertIn('client_id = web-id', stored)
        self.assertIn('client_secret = web-secret', stored)
        self.assertIn('scope = drive', stored)
        self.assertEqual(self.session.events, [('finish', 'drive')])
        offer = progress.call_args_list[0].kwargs['offer']
        self.assertEqual(offer['code'], 'KXP-QKN-VLWD')
        self.assertNotIn('PRIVATE_', json.dumps(result))

    def test_a_failed_link_tells_the_phone_and_stores_nothing(self):
        with self.fake_link(ValueError('Linking did not finish in 25 minutes.')), mock.patch.object(module, 'progress'):
            with self.assertRaisesRegex(ValueError, 'did not finish'):
                module.dispatch(self.service, {'operation': 'link-phone'})
        self.assertFalse(self.service.google.exists())

    def test_without_the_google_client_file_linking_says_why(self):
        self.service.clients_path = self.base/'missing.json'
        with self.assertRaisesRegex(ValueError, 'no Google sign-in client'):
            module.dispatch(self.service, {'operation': 'link-phone'})
        self.assertFalse(self.service.status(refresh=False)['phone_linking'])
        self.service.clients_path = self.clients
        self.assertTrue(self.service.status(refresh=False)['phone_linking'])

    def test_a_connection_on_rclones_shared_sign_in_is_named_so_the_app_can_suggest_relinking(self):
        # rclone's built-in client is shared by every rclone user, and Google throttles it.
        self.assertFalse(self.service.status(refresh=False)['google_shared_client'])  # not connected
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1}', '')):
            self.service.connect_google(json.dumps(self.token))
        self.assertTrue(self.service.status(refresh=False)['google_shared_client'])
        with self.fake_link(self.token), mock.patch.object(module, 'progress'), \
             mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1}', '')):
            module.dispatch(self.service, {'operation': 'link-phone'})
        self.assertFalse(self.service.status(refresh=False)['google_shared_client'])

    def test_a_rate_limited_check_says_google_is_slowing_requests(self):
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1}', '')):
            self.service.connect_google(json.dumps(self.token))
        with mock.patch.object(module, 'classify_rclone_failure', return_value=(False, b'', 'rate-limited')):
            with self.assertRaisesRegex(ValueError, 'Google is limiting requests'):
                self.service.check_google()
        self.assertTrue(self.service.google.exists())  # credentials stay

    def test_disconnecting_a_phone_link_revokes_it_at_google(self):
        with self.fake_link(self.token), mock.patch.object(module, 'progress'), \
             mock.patch.object(module, 'classify_rclone_failure', return_value=(True, b'{"total":1}', '')):
            module.dispatch(self.service, {'operation': 'link-phone'})
        with mock.patch.object(module, 'revoke_google', return_value=True) as revoke:
            result = self.service.disconnect_google()
        revoke.assert_called_once_with('PRIVATE_REFRESH')
        self.assertIn('ended at Google', result['notice'])
        self.assertFalse(self.service.google.exists())


class SettingsProfile(unittest.TestCase):
    """Each backup carries the writer's settings beside the book, in the same encrypted snapshot."""
    initialize = EncryptedBackup.initialize

    def setUp(self):
        EncryptedBackup.setUp(self)
        self.settings = self.base/'config/grimoire'
        self.settings.mkdir(parents=True)
        (self.settings/'settings.toml').write_text('theme = "Grimoire"\nspellcheck = true\n')
        (self.settings/'state.toml').write_text('book = "/home/ada/Books/A book with spaces"\n')
        (self.settings/'dictionary.txt').write_text('Élan\nWrOS\n', encoding='utf-8')
        (self.settings/'huge.log').write_bytes(b'x' * (module.PROFILE_FILE_LIMIT + 1))
        (self.settings/'cache').mkdir()
        (self.settings/'cache/skip.txt').write_text('not a setting')
        (self.settings/'elsewhere.toml').symlink_to(self.base/'private-state')
        keyboard = self.base/'keyboard.json'
        keyboard.write_text('{"version": 1, "layout": "colemak", "generation": "initial"}')
        timezone = self.base/'timezone'
        timezone.write_text('America/Los_Angeles\n')
        self.service.profile_config = self.settings
        self.service.keyboard_file = keyboard
        self.service.timezone_file = timezone
        self.saved = {name: (self.settings/name).read_bytes()
                      for name in ('settings.toml', 'state.toml', 'dictionary.txt')}

    def test_each_backup_carries_the_settings_and_restoring_the_book_alone_ignores_them(self):
        self.initialize()
        expected = module.inventory(self.project)
        snapshot = self.service.backup(str(self.project))['snapshots'][0]
        self.assertTrue(snapshot['has_profile'])
        restored = self.base/'restored'
        self.service.restore(snapshot['id'], str(restored))
        self.assertEqual(module.inventory(restored), expected)  # the book is exactly the book
        self.assertFalse((restored/module.PROFILE).exists())
        self.assertEqual([p.name for p in self.base.iterdir() if p.name.startswith('.wros-profile')], [])
        (self.settings/'settings.toml').write_text('theme = "Changed"\n')
        self.service.restore(snapshot['id'], str(self.base/'restored again'))
        self.assertEqual((self.settings/'settings.toml').read_text(), 'theme = "Changed"\n')

    def test_restoring_with_settings_puts_them_back_and_keeps_the_current_ones(self):
        self.initialize()
        snapshot = self.service.backup(str(self.project))['snapshots'][0]
        (self.settings/'settings.toml').write_text('theme = "Changed"\n')
        (self.settings/'dictionary.txt').unlink()
        result = self.service.restore(snapshot['id'], str(self.base/'restored'), settings=True)
        for name, data in self.saved.items():
            self.assertEqual((self.settings/name).read_bytes(), data, name)
        kept = [p for p in self.settings.iterdir() if p.name.startswith('before-restore-')]
        self.assertEqual(len(kept), 1)
        self.assertEqual((kept[0]/'settings.toml').read_text(), 'theme = "Changed"\n')
        self.assertIn('colemak', result['notice'])
        self.assertIn('America/Los_Angeles', result['notice'])

    def test_the_profile_holds_only_settings_never_keys_tokens_or_odd_files(self):
        result = self.initialize()
        (self.base/'private-state/rclone.conf').write_text('[wros-google]\ntoken = {"refresh_token":"SECRET-TOKEN"}\n')
        snapshot = self.service.backup(str(self.project))['snapshots'][0]
        raw = self.base/'raw'
        row = next(s for s in self.service.snapshots() if s['id'] == snapshot['id'])
        self.service.restic(['restore', row['id'] + ':' + row['path'], '--target', str(raw)])
        names = sorted(p.relative_to(raw/module.PROFILE).as_posix() for p in (raw/module.PROFILE).rglob('*'))
        self.assertEqual(names, ['grimoire', 'grimoire/dictionary.txt', 'grimoire/settings.toml',
                                 'grimoire/state.toml', 'system.json'])
        system = json.loads((raw/module.PROFILE/'system.json').read_text())
        self.assertEqual(system, {'version': 1, 'keyboard': 'colemak', 'timezone': 'America/Los_Angeles'})
        key = Path(result['recovery_key']).read_bytes().strip()
        for path in (raw/module.PROFILE).rglob('*'):
            if path.is_file():
                self.assertNotIn(key, path.read_bytes())
                self.assertNotIn(b'SECRET-TOKEN', path.read_bytes())

    def test_older_snapshots_without_settings_still_restore(self):
        self.initialize()
        self.service.profile_config = self.base/'no settings here'
        self.service.keyboard_file = self.base/'missing keyboard'
        self.service.timezone_file = self.base/'missing timezone'
        snapshot = self.service.backup(str(self.project))['snapshots'][0]
        result = self.service.restore(snapshot['id'], str(self.base/'restored'), settings=True)
        self.assertIn('no settings', result['notice'])
        self.assertEqual(module.inventory(self.base/'restored'), module.inventory(self.project))

    def test_a_book_cannot_hold_the_reserved_settings_folder(self):
        self.initialize()
        (self.project/module.PROFILE).mkdir()
        with self.assertRaisesRegex(ValueError, 'Reserved'):
            self.service.backup(str(self.project))

    def test_the_app_and_the_recovery_menu_ask_for_settings_by_one_flag(self):
        self.initialize()
        snapshot = self.service.backup(str(self.project))['snapshots'][0]
        (self.settings/'settings.toml').write_text('theme = "Changed"\n')
        module.dispatch(self.service, {'operation': 'restore', 'snapshot': snapshot['id'],
                                       'destination': str(self.base/'restored'), 'settings': True})
        self.assertEqual((self.settings/'settings.toml').read_bytes(), self.saved['settings.toml'])


if __name__ == '__main__':
    unittest.main()
