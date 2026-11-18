"""Two-way readable Google Drive sync (os/scripts/wros-sync) against a real rclone
alias remote standing in for Drive. Google is never contacted."""
import importlib.machinery
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
module = importlib.machinery.SourceFileLoader('wros_sync', str(ROOT / 'os/scripts/wros-sync')).load_module()


def history_files(drive):
    base = drive / 'WrOS-History'
    return sorted(p.relative_to(base).as_posix() for p in base.rglob('*') if p.is_file()) if base.exists() else []


class Plan(unittest.TestCase):
    def test_every_row_of_the_decision_table(self):
        b, x, y = 'base', 'x', 'y'
        rows = {
            # path: (base, local, remote) -> action
            'local edit': ((b, x, b), 'upload'),
            'drive edit': ((b, b, y), 'download'),
            'local delete': ((b, None, b), 'delete_remote'),
            'drive delete': ((b, b, None), 'delete_local'),
            'both edit': ((b, x, y), 'park'),
            'same edit': ((b, x, x), 'adopt'),
            'unchanged': ((b, b, b), 'none'),
            'local delete, drive edit': ((b, None, y), 'download'),
            'drive delete, local edit': ((b, x, None), 'upload'),
            'new local': ((None, x, None), 'upload'),
            'new drive': ((None, None, y), 'download'),
            'new both, different': ((None, x, y), 'park'),
            'both deleted': ((b, None, None), 'forget'),
        }
        for name, ((base, local, remote), action) in rows.items():
            with self.subTest(name):
                state = lambda value: {'f.md': value} if value else {}
                self.assertEqual(module.plan(state(base), state(local), state(remote), first=False),
                                 [('f.md', action)])

    def test_a_first_sync_never_deletes_and_parks_differences(self):
        plan = dict(module.plan({}, {'a.md': 'x', 'b.md': 'x', 'c.md': 'x'}, {'a.md': 'x', 'b.md': 'y', 'd.md': 'z'},
                                first=True))
        self.assertEqual(plan, {'a.md': 'adopt', 'b.md': 'park', 'c.md': 'upload', 'd.md': 'download'})

    def test_a_drive_file_without_a_checksum_is_never_treated_as_missing(self):
        path = 'f.md'
        for first, base, local in ((True, {}, {path: 'local'}), (False, {}, {}),
                                   (False, {path: 'base'}, {path: 'base'}),
                                   (False, {path: 'base'}, {})):
            with self.subTest(first=first, base=base, local=local):
                self.assertEqual(module.plan(base, local, {path: None}, first),
                                 [(path, 'unverifiable')])


class Sync(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='wros-sync-test-')
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.book = base / 'My Book'
        (self.book / 'manuscript/01-Part').mkdir(parents=True)
        (self.book / 'manuscript/01-Part/01-Scene.md').write_text('Opening.\n\nMiddle.\n\nEnd.\n')
        (self.book / 'notes').mkdir()
        (self.book / 'notes/ideas.md').write_text('Ideas.\n')
        (self.book / 'research').mkdir()
        (self.book / 'research/map.bin').write_bytes(bytes(range(256)))
        (self.book / '.grimoire').mkdir()
        (self.book / '.grimoire/layout.json').write_text('{}')
        (self.book / 'notes/ideas (from bazzite, 2026-11-01 10-00).md').write_text('Parked.\n')
        self.drive = base / 'drive'
        self.drive.mkdir()
        backup_state = base / 'backup-state'
        backup_state.mkdir(mode=0o700)
        config = backup_state / 'rclone.conf'
        config.write_text(f'[wros-google]\ntype = alias\nremote = {self.drive}\n')
        config.chmod(0o600)
        self.service = module.Service(base / 'sync-state', backup_state)
        self.remote = self.drive / 'WrOS' / 'My Book'
        self.timeline = self.drive / 'WrOS-History' / 'My Book' / 'timeline'

    def sync(self, **kwargs):
        return self.service.sync(str(self.book), **kwargs)

    def upload_bundle(self, name, machine='laptop-a123'):
        bundle = self.book.parent / 'local.bundle'
        bundle.write_bytes(b'Local bundle bytes')
        return self.service.timeline_upload(str(self.book), machine, str(bundle), name)

    def test_upload_uses_a_part_file_then_the_final_name(self):
        self.service.enable(str(self.book))
        name = '000001 2026-11-16 2140.bundle'
        machine = self.timeline / 'laptop-a123'
        rclone = self.service.rclone

        def check_part(*arguments, **kwargs):
            if arguments[0] == 'moveto':
                parts = list(machine.glob('.part-*'))
                self.assertEqual(len(parts), 1)
                self.assertEqual(parts[0].read_bytes(), b'Local bundle bytes')
                self.assertFalse((machine / name).exists())
            return rclone(*arguments, **kwargs)

        with mock.patch.object(self.service, 'rclone', side_effect=check_part) as remote:
            self.assertEqual(self.upload_bundle(name), {'uploaded': name})
        self.assertTrue(any(call.args[0] == 'moveto' for call in remote.call_args_list))
        self.assertEqual(list(machine.iterdir()), [machine / name])
        self.assertEqual((machine / name).read_bytes(), b'Local bundle bytes')

    def test_upload_never_replaces_an_existing_name(self):
        self.service.enable(str(self.book))
        name = '000001 2026-11-16 2140.bundle'
        final = self.timeline / 'laptop-a123' / name
        final.parent.mkdir(parents=True)
        rclone = self.service.rclone

        def create_at_move(*arguments, **kwargs):
            if arguments[0] == 'moveto':
                final.write_bytes(b'Existing history')
            return rclone(*arguments, **kwargs)

        for created_late in (False, True):
            with self.subTest(created_late=created_late):
                if not created_late:
                    final.write_bytes(b'Existing history')
                with mock.patch.object(self.service, 'rclone', side_effect=create_at_move):
                    with self.assertRaises(ValueError) as refused:
                        self.upload_bundle(name)
                self.assertEqual(str(refused.exception),
                                 'A timeline file with this name is already on Drive; nothing was replaced.')
                self.assertEqual(final.read_bytes(), b'Existing history')
                final.unlink()

    def test_interrupted_upload_retries_same_number(self):
        self.service.enable(str(self.book))
        name = '000001 2026-11-16 2140.bundle'
        machine = self.timeline / 'laptop-a123'
        rclone = self.service.rclone

        def interrupted(*arguments, **kwargs):
            if arguments[0] == 'moveto':
                raise module.Offline('Connection dropped')
            return rclone(*arguments, **kwargs)

        with mock.patch.object(self.service, 'rclone', side_effect=interrupted):
            with self.assertRaises(module.Offline):
                self.upload_bundle(name)
        self.assertFalse((machine / name).exists())
        parts = list(machine.glob('.part-*'))
        self.assertEqual(len(parts), 1)
        self.assertEqual(self.service.timeline_list(str(self.book)), {'machines': {}})
        self.assertEqual(self.upload_bundle(name), {'uploaded': name})
        self.assertEqual((machine / name).read_bytes(), b'Local bundle bytes')
        self.assertEqual(list(machine.glob('.part-*')), parts)
        self.assertEqual(parts[0].read_bytes(), b'Local bundle bytes')

    def test_upload_requires_matching_drive_checksum(self):
        self.service.enable(str(self.book))
        name = '000001 2026-11-16 2140.bundle'
        machine = self.timeline / 'laptop-a123'
        rclone = self.service.rclone

        def corrupt_part(*arguments, **kwargs):
            result = rclone(*arguments, **kwargs)
            if arguments[0] == 'copyto':
                next(machine.glob('.part-*')).write_bytes(b'Different bytes')
            return result

        with mock.patch.object(self.service, 'rclone', side_effect=corrupt_part):
            with self.assertRaisesRegex(ValueError, 'checksum'):
                self.upload_bundle(name)
        self.assertFalse((machine / name).exists())

    def test_upload_refuses_unsafe_names(self):
        self.service.enable(str(self.book))
        name = '000001 2026-11-16 2140.bundle'
        for machine, bundle in (('../outside', name), ('Uppercase', name), ('a' * 65, name),
                                ('laptop-a123', '../' + name), ('laptop-a123', '1.bundle')):
            with self.subTest(machine=machine, bundle=bundle):
                with self.assertRaisesRegex(ValueError, 'name'):
                    self.upload_bundle(bundle, machine)
        self.assertEqual(history_files(self.drive), [])

    def test_list_ignores_part_files(self):
        self.service.enable(str(self.book))
        self.assertEqual(self.service.timeline_list(str(self.book)), {'machines': {}})
        machine = self.timeline / 'laptop-a123'
        machine.mkdir(parents=True)
        for name in ('000002 2026-11-15 2300.bundle', '000001 2026-11-16 2140.bundle',
                     '.part-000003 2026-11-17 0200.bundle', '123.bundle'):
            (machine / name).write_bytes(b'Bundle bytes')
        parts_only = self.timeline / 'desktop-b234'
        parts_only.mkdir()
        (parts_only / '.part-retry').write_bytes(b'Still staging')
        invalid = self.timeline / 'Invalid'
        invalid.mkdir()
        (invalid / '000001 2026-11-16 2140.bundle').write_bytes(b'Bundle bytes')
        self.assertEqual(self.service.timeline_list(str(self.book)), {'machines': {'laptop-a123': [1, 2]}})

    def test_fetch_copies_every_machine(self):
        self.service.enable(str(self.book))
        sources = [
            ('desktop-b234', 1, '000001 2026-11-18 0200.bundle'),
            ('laptop-a123', 1, '000001 2026-11-16 2140.bundle'),
            ('laptop-a123', 2, '000002 2026-11-15 2300.bundle'),
        ]
        for machine, _, name in sources:
            source = self.timeline / machine / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text(f'{machine}/{name}')
        (self.timeline / 'laptop-a123' / '.part-unused').write_bytes(b'Unfinished')
        into = self.book.parent / 'bundles'
        into.mkdir()
        (into / 'laptop-a123').symlink_to(self.book, target_is_directory=True)
        result = self.service.timeline_fetch(str(self.book), str(into))
        self.assertEqual([(b['machine'], b['number']) for b in result['bundles']],
                         [(machine, number) for machine, number, _ in sources])
        for entry, (machine, _, name) in zip(result['bundles'], sources):
            path = Path(entry['path'])
            self.assertTrue(path.resolve().is_relative_to(into.resolve()))
            self.assertEqual(path.name, name)
            self.assertEqual(path.read_text(), f'{machine}/{name}')
        self.assertEqual(list(self.book.glob('*.bundle')), [])

    def test_fetch_refuses_a_linked_destination(self):
        self.service.enable(str(self.book))
        self.upload_bundle('000001 2026-11-16 2140.bundle')
        into = self.book.parent / 'bundles'
        into.symlink_to(self.book, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'destination'):
            self.service.timeline_fetch(str(self.book), str(into))
        self.assertEqual(list(self.book.rglob('*.bundle')), [])

    def test_timeline_requests_use_op_and_the_operation_lock(self):
        self.service.enable(str(self.book))
        state = self.service.state

        def call(op, **fields):
            request = {'op': op, 'project': str(self.book), **fields}
            stdin = mock.Mock(buffer=io.BytesIO(json.dumps(request).encode()))
            with mock.patch.object(module.sys, 'stdin', stdin), \
                    mock.patch('sys.stdout', new_callable=io.StringIO) as stdout:
                module.main()
            return json.loads(stdout.getvalue())

        with mock.patch.dict(module.os.environ, {'WROS_SYNC_STATE_DIR': str(state),
                                               'WROS_BACKUP_STATE_DIR': str(self.service.config.parent)}), \
                mock.patch.object(module.os, 'geteuid', return_value=1000), \
                mock.patch.object(module.signal, 'signal'):
            with (state / 'operation.lock').open('wb') as lock:
                module.fcntl.flock(lock, module.fcntl.LOCK_EX | module.fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    call('timeline-list')
            self.assertEqual(call('timeline-list'), {'ok': True, 'data': {'machines': {}}})
            bundle = self.book.parent / 'local.bundle'
            bundle.write_bytes(b'Local bundle bytes')
            name = '000001 2026-11-16 2140.bundle'
            self.assertEqual(call('timeline-upload', machine='laptop-a123', bundle=str(bundle), name=name),
                             {'ok': True, 'data': {'uploaded': name}})
            reply = call('timeline-fetch', into=str(self.book.parent / 'bundles'))
            self.assertTrue(reply['ok'])
            self.assertEqual(len(reply['data']['bundles']), 1)
            entry = reply['data']['bundles'][0]
            self.assertEqual((entry['machine'], entry['number']), ('laptop-a123', 1))
            self.assertEqual(Path(entry['path']).read_bytes(), bundle.read_bytes())

    def test_remote_paths_must_stay_inside_the_book(self):
        for path in ('/outside.md', '../outside.md', 'notes/../../outside.md',
                     'notes//scene.md', 'notes\\scene.md', ''):
            with self.subTest(path=path), mock.patch.object(self.service, 'rclone') as remote:
                remote.return_value.returncode = 0
                remote.return_value.stdout = json.dumps([{'Path': path, 'Hashes': {'md5': 'a'}}]).encode()
                with self.assertRaisesRegex(ValueError, 'path'):
                    self.service.scan_remote('My Book')

    def test_duplicate_drive_names_are_unverifiable(self):
        rows = [{'Path': 'scene.md', 'Hashes': {'md5': digest}} for digest in ('a', 'b')]
        with mock.patch.object(self.service, 'rclone') as remote:
            remote.return_value.returncode = 0
            remote.return_value.stdout = json.dumps(rows).encode()
            self.assertEqual(self.service.scan_remote('My Book'), {'scene.md': None})

    def test_project_identity_cannot_choose_paths_outside_sync_state(self):
        for project_id, folder in (('../outside', 'My Book'), ('/tmp/outside', 'My Book'),
                                   ('12345678-1234-1234-1234-123456789abc', '../outside')):
            with self.subTest(project_id=project_id, folder=folder):
                (self.book / '.grimoire/sync.json').write_text(json.dumps({
                    'version': 1, 'project_id': project_id, 'drive_folder': folder,
                }))
                with mock.patch.object(self.service, 'rclone') as remote:
                    with self.assertRaisesRegex(ValueError, 'identity'):
                        self.service.enable(str(self.book))
                    remote.assert_not_called()

    def test_first_sync_uploads_the_book_but_no_hidden_or_parked_files(self):
        enabled = self.service.enable(str(self.book))
        self.assertEqual(enabled['drive_folder'], 'My Book')
        identity = json.loads((self.book / '.grimoire/sync.json').read_text())
        self.assertEqual(json.loads((self.remote / '.wros-sync.json').read_text())['project_id'], identity['project_id'])
        result = self.sync()
        self.assertEqual((result['status'], result['uploaded']), ('synced', 3))
        uploaded = sorted(p.relative_to(self.remote).as_posix() for p in self.remote.rglob('*') if p.is_file())
        self.assertEqual(uploaded, ['.wros-sync.json', 'manuscript/01-Part/01-Scene.md', 'notes/ideas.md',
                                    'research/map.bin'])
        self.assertEqual(self.sync()['uploaded'], 0)

    def test_first_sync_after_drive_import_keeps_identity_and_changes_nothing(self):
        # Materialize an existing Drive book, then record its marker's identity before enable.
        self.remote = self.drive / 'WrOS' / 'Clip says Hello World.'
        shutil.copytree(self.book, self.remote)
        identity = {'version': 1, 'project_id': '12345678-1234-4234-8234-123456789abc',
                    'drive_folder': self.remote.name}
        (self.remote / '.wros-sync.json').write_text(json.dumps({
            'version': 1, 'project_id': identity['project_id'],
        }))
        shutil.rmtree(self.book)
        self.book = self.book.with_name('Imported book 2')
        shutil.copytree(self.remote, self.book)
        (self.book / '.grimoire/sync.json').write_text(json.dumps(identity))
        before = {p.relative_to(self.remote): p.read_bytes()
                  for p in self.remote.rglob('*') if p.is_file()}
        enabled = self.service.enable(str(self.book))
        self.assertEqual(enabled['drive_folder'], identity['drive_folder'])
        self.assertEqual(json.loads((self.book / '.grimoire/sync.json').read_text()), identity)
        with mock.patch.object(self.service, 'rclone', wraps=self.service.rclone) as rclone:
            result = self.sync()
        self.assertEqual((result['status'], result['uploaded'], result['downloaded'],
                          result['deleted'], result['parked'], result['pending']),
                         ('synced', 0, 0, 0, [], []))
        self.assertFalse(any(call.args[0] in ('copyto', 'moveto', 'deletefile', 'rcat')
                             for call in rclone.call_args_list))
        self.assertEqual({p.relative_to(self.remote): p.read_bytes()
                          for p in self.remote.rglob('*') if p.is_file()}, before)
        self.assertEqual(history_files(self.drive), [])

        home = self.service.state / identity['project_id']
        scene = 'manuscript/01-Part/01-Scene.md'
        self.assertEqual(json.loads((home / 'base.json').read_text()), {
            path: module.digests(self.book / path)[0]
            for path in (scene, 'notes/ideas.md', 'research/map.bin')
        })

        revised = 'Opening, revised right after import.\n'
        (self.book / scene).write_text(revised)
        result = self.sync()
        self.assertEqual((result['status'], result['uploaded'], result['parked'], result['pending']),
                         ('synced', 1, [], []))
        self.assertEqual((self.remote / scene).read_text(), revised)

    def test_missing_drive_checksum_preserves_both_versions_and_reports_the_skip(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        local, remote = self.book / scene, self.remote / scene
        before = local.read_bytes()
        remote.write_bytes(b'Drive revision')
        listing = self.service.scan_remote('My Book')
        listing[scene] = None

        with mock.patch.object(self.service, 'scan_remote', return_value=listing):
            result = self.sync()
        self.assertEqual(result['skipped'], [scene])
        self.assertIn('without a Google Drive checksum', result['message'])
        self.assertEqual(result['deleted'], 0)
        self.assertEqual(local.read_bytes(), before)
        self.assertEqual(remote.read_bytes(), b'Drive revision')

    def test_upload_updates_the_live_file_without_a_history_copy(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        revised = 'Opening, revised.\n\nMiddle.\n\nEnd.\n'
        (self.book / scene).write_text(revised)
        with mock.patch.object(self.service, 'rclone', wraps=self.service.rclone) as rclone:
            self.assertEqual(self.sync()['uploaded'], 1)
        self.assertEqual((self.remote / scene).read_text(), revised)
        self.assertEqual(history_files(self.drive), [])
        self.assertEqual(list((self.remote / 'manuscript/01-Part').iterdir()), [self.remote / scene])
        self.assertFalse(any(call.args[0] == 'moveto' for call in rclone.call_args_list))
        self.assertEqual(self.sync()['uploaded'], 0)

    def test_local_replacements_and_remote_deletions_still_go_to_history(self):
        self.service.enable(str(self.book))
        self.sync()
        (self.remote / 'notes/ideas.md').write_text('Ideas from the phone.\n')
        self.assertEqual(self.sync()['downloaded'], 1)
        self.assertEqual((self.book / 'notes/ideas.md').read_text(), 'Ideas from the phone.\n')
        (self.book / 'manuscript/01-Part/01-Scene.md').write_text('Opening, revised.\n\nMiddle.\n\nEnd.\n')
        self.assertEqual(self.sync()['uploaded'], 1)
        self.assertEqual((self.remote / 'manuscript/01-Part/01-Scene.md').read_text(),
                         'Opening, revised.\n\nMiddle.\n\nEnd.\n')
        history = history_files(self.drive)
        self.assertEqual(len(history), 1, history)
        self.assertTrue(any('ideas (' in h and 'from WrOS).md' in h for h in history), history)
        (self.book / 'notes/ideas.md').unlink()
        self.assertEqual(self.sync()['deleted'], 1)
        self.assertFalse((self.remote / 'notes/ideas.md').exists())
        self.assertEqual(len(history_files(self.drive)), 2)

    def test_history_copies_only_without_timeline_entry(self):
        self.service.enable(str(self.book))
        self.sync()
        path = 'notes/ideas.md'
        local, remote = self.book / path, self.remote / path

        remote.write_text('First phone edit.\n')
        self.assertEqual(self.sync(timeline=True)['downloaded'], 1)
        self.assertEqual(local.read_text(), 'First phone edit.\n')
        self.assertEqual(history_files(self.drive), [])

        remote.write_text('Second phone edit.\n')
        self.assertEqual(self.sync(timeline=False)['downloaded'], 1)
        self.assertEqual(local.read_text(), 'Second phone edit.\n')
        history = history_files(self.drive)
        self.assertEqual(len(history), 1, history)
        self.assertEqual((self.drive / 'WrOS-History' / history[0]).read_text(), 'First phone edit.\n')

    def test_reply_lists_changed_files(self):
        self.service.enable(str(self.book))
        self.assertEqual(self.sync()['changed'], [])
        downloaded, added = 'manuscript/01-Part/01-Scene.md', 'notes/new.md'
        deleted_here, deleted_on_drive = 'notes/ideas.md', 'research/map.bin'
        (self.remote / downloaded).write_text('Edited on Drive.\n')
        (self.remote / added).write_text('Added on Drive.\n')
        (self.book / deleted_here).unlink()
        (self.remote / deleted_on_drive).unlink()
        with mock.patch.object(self.service, 'rclone', wraps=self.service.rclone) as rclone:
            reply = self.sync(timeline=True)
        self.assertEqual((reply['status'], reply['downloaded'], reply['deleted']), ('synced', 2, 2))
        self.assertEqual(reply['changed'], [downloaded, added, deleted_on_drive])
        self.assertFalse((self.book / deleted_on_drive).exists())
        self.assertFalse((self.remote / deleted_here).exists())
        self.assertEqual([call.args for call in rclone.call_args_list if call.args[0] == 'deletefile'],
                         [('deletefile', module.REMOTE + f'WrOS/My Book/{deleted_here}', '--drive-use-trash=true')])
        self.assertEqual(history_files(self.drive), [])
        self.assertEqual(self.sync(timeline=True)['changed'], [])

    def test_edits_on_both_sides_park_drives_version_and_pause_that_file(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        (self.book / scene).write_text('Opening, mine.\n\nMiddle.\n\nEnd.\n')
        (self.remote / scene).write_text('Opening.\n\nMiddle.\n\nEnd, theirs.\n')
        result = self.sync()
        self.assertEqual(result['uploaded'], 0)
        self.assertEqual(result['changed'], [])
        self.assertEqual(history_files(self.drive), [])
        self.assertEqual(len(result['pending']), 1)
        pending = result['pending'][0]
        parked = self.book / pending['parked']
        self.assertEqual(pending['path'], scene)
        self.assertRegex(parked.name, r'^01-Scene \(from Drive, \d{4}-\d\d-\d\d \d\d-\d\d\)\.md$')
        self.assertEqual(parked.read_text(), 'Opening.\n\nMiddle.\n\nEnd, theirs.\n')
        self.assertEqual(Path(pending['merge_base']).read_text(), 'Opening.\n\nMiddle.\n\nEnd.\n')
        self.assertEqual((self.book / scene).read_text(), 'Opening, mine.\n\nMiddle.\n\nEnd.\n')
        # Paused until settled: neither side is overwritten, and the parked copy is never uploaded.
        again = self.sync()
        self.assertEqual((again['uploaded'], again['downloaded']), (0, 0))
        self.assertEqual((self.remote / scene).read_text(), 'Opening.\n\nMiddle.\n\nEnd, theirs.\n')
        # Settled (merged text saved, parked copy gone): the merge goes up.
        (self.book / scene).write_text('Opening, mine.\n\nMiddle.\n\nEnd, theirs.\n')
        parked.unlink()
        self.assertEqual(self.sync()['uploaded'], 1)
        self.assertEqual((self.remote / scene).read_text(), 'Opening, mine.\n\nMiddle.\n\nEnd, theirs.\n')
        self.assertEqual(self.sync()['pending'], [])

    def test_more_than_five_deletions_stop_and_ask(self):
        for n in range(6):
            (self.book / f'notes/n{n}.md').write_text(f'{n}\n')
        self.service.enable(str(self.book))
        self.sync()
        for n in range(6):
            (self.book / f'notes/n{n}.md').unlink()
        result = self.sync()
        self.assertEqual((result['status'], result['deletes']), ('confirm', 6))
        self.assertTrue((self.remote / 'notes/n0.md').exists())
        self.assertEqual(self.sync(allow_deletes=True)['deleted'], 6)
        self.assertFalse((self.remote / 'notes/n0.md').exists())

    def test_googles_rate_limits_are_ridden_out_and_named_when_they_win(self):
        # Seen on the laptop: Google's per-minute quota refused requests, and rclone gave up
        # after two quick retries, so the whole sync failed as "could not be reached".
        calls = []

        def limited(argv, **kwargs):
            calls.append(argv)
            return mock.Mock(returncode=1, stdout=b'', stderr=(
                b"googleapi: Error 403: Quota exceeded for quota metric 'Queries' and limit "
                b"'Requests per minute', rateLimitExceeded"))
        with mock.patch.object(module.subprocess, 'run', side_effect=limited):
            with self.assertRaisesRegex(module.Offline, 'Google is limiting requests'):
                self.service.rclone('lsjson', 'wros-google:WrOS')
        argv = calls[0]
        self.assertEqual(argv[argv.index('--low-level-retries') + 1], '10')
        self.assertGreaterEqual(int(argv[argv.index('--retries') + 1]), 3)

    def test_saves_during_upload_do_not_change_the_uploaded_merge_base(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        local, remote = self.book / scene, self.remote / scene
        uploaded = 'Opening, revised.\n\nMiddle.\n\nEnd.\n'
        during = uploaded + '\nWords saved during the upload.\n'
        latest = during + '\nMore words saved as the upload finished.\n'
        local.write_text(uploaded)
        rclone = self.service.rclone

        def writer_saves(*arguments, **kwargs):
            if arguments[0] == 'copyto' and arguments[-1] == module.REMOTE + f'WrOS/My Book/{scene}':
                local.write_text(during)
                result = rclone(*arguments, **kwargs)
                local.write_text(latest)
                return result
            return rclone(*arguments, **kwargs)

        with mock.patch.object(self.service, 'rclone', side_effect=writer_saves):
            self.assertEqual(self.sync()['uploaded'], 1)
        self.assertEqual(remote.read_text(), uploaded)
        self.assertEqual(local.read_text(), latest)
        home = self.service.state / self.service.identity(self.book)['project_id']
        self.assertEqual((home / 'base' / scene).read_text(), uploaded)
        self.assertEqual(json.loads((home / 'base.json').read_text())[scene], module.digests(remote)[0])

        # A later phone edit must merge against what actually reached Drive,
        # never against the newer words that only exist on this computer.
        remote.write_text(uploaded.replace('Middle.', 'Middle, edited on the phone.'))
        pending = self.sync()['pending'][0]
        self.assertEqual(Path(pending['merge_base']).read_text(), uploaded)
        self.assertEqual(local.read_text(), latest)

    def test_a_save_before_upload_staging_skips_the_stale_plan(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        local, remote = self.book / scene, self.remote / scene
        before = remote.read_text()
        local.write_text('Scanned revision.\n')
        apply = self.service.apply

        def writer_saves(root, folder, home, base, relative, action, *arguments):
            if relative == scene and action == 'upload':
                local.write_text('Newer revision saved after the scan.\n')
            return apply(root, folder, home, base, relative, action, *arguments)

        with mock.patch.object(self.service, 'apply', side_effect=writer_saves):
            result = self.sync()
        self.assertEqual(result['skipped'], [scene])
        self.assertEqual(result['uploaded'], 0)
        self.assertEqual(remote.read_text(), before)
        self.assertEqual(self.sync()['uploaded'], 1)
        self.assertEqual(remote.read_text(), local.read_text())

    def test_a_save_while_adopting_an_equal_version_cannot_become_the_merge_base(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        local, remote = self.book / scene, self.remote / scene
        shared = 'The same saved edit on both sides.\n'
        local.write_text(shared)
        remote.write_text(shared)
        remember = self.service.remember

        def writer_saves(home, base, relative, md5, source=None):
            if relative == scene:
                local.write_text(shared + '\nNew words only saved here.\n')
            return remember(home, base, relative, md5, source)

        with mock.patch.object(self.service, 'remember', side_effect=writer_saves):
            self.sync()
        home = self.service.state / self.service.identity(self.book)['project_id']
        self.assertFalse((home / 'base' / scene).exists(), 'an unchecked text base must not be kept')
        self.assertEqual(json.loads((home / 'base.json').read_text())[scene], module.digests(remote)[0])
        remote.write_text('A later edit on the phone.\n')
        pending = self.sync()['pending'][0]
        self.assertIsNone(pending['merge_base'], 'both copies remain available for a manual settlement')

    def test_an_inconsistent_base_from_an_older_run_is_not_used_for_merging(self):
        self.service.enable(str(self.book))
        self.sync()
        scene = 'manuscript/01-Part/01-Scene.md'
        local, remote = self.book / scene, self.remote / scene
        mine = 'Local words not yet uploaded.\n'
        theirs = 'A separate edit on the phone.\n'
        local.write_text(mine)
        remote.write_text(theirs)
        home = self.service.state / self.service.identity(self.book)['project_id']
        (home / 'base' / scene).write_text(mine)
        pending = self.sync()['pending'][0]
        self.assertIsNone(pending['merge_base'])
        self.assertEqual(local.read_text(), mine)
        self.assertEqual((self.book / pending['parked']).read_text(), theirs)

    def test_an_emptied_drive_folder_deletes_nothing_locally(self):
        self.service.enable(str(self.book))
        self.sync()
        for path in list(self.remote.rglob('*')):
            if path.is_file() and path.name != '.wros-sync.json':
                path.unlink()
        self.assertEqual(self.sync()['status'], 'confirm')
        self.assertTrue((self.book / 'notes/ideas.md').exists())

    def test_a_drive_folder_owned_by_another_book_is_refused(self):
        (self.remote).mkdir(parents=True)
        (self.remote / '.wros-sync.json').write_text(json.dumps({'project_id': 'another-book'}))
        enabled = self.service.enable(str(self.book))
        self.assertEqual(enabled['drive_folder'], 'My Book (2)')
        identity = self.book / '.grimoire/sync.json'
        identity.write_text(json.dumps(json.loads(identity.read_text()) | {'drive_folder': 'My Book'}))
        result = self.sync()
        self.assertEqual(result['status'], 'refused')
        self.assertIn('belongs to another WrOS book or computer', result['message'])

    def test_a_file_that_changes_during_the_run_is_left_alone(self):
        self.service.enable(str(self.book))
        self.sync()
        (self.remote / 'notes/ideas.md').write_text('From the phone.\n')

        def writer_saves(path):
            (self.book / 'notes/ideas.md').write_text('Typed meanwhile.\n')

        self.service.before_replace = writer_saves
        result = self.sync()
        self.assertEqual((self.book / 'notes/ideas.md').read_text(), 'Typed meanwhile.\n')
        self.assertEqual(result['downloaded'], 0)
        self.assertEqual(result['changed'], [])
        self.assertEqual(result['skipped'], ['notes/ideas.md'])


if __name__ == '__main__':
    unittest.main()
