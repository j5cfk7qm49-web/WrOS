"""Real native encrypted backup/restore controls, isolated books/HOME, no network."""
import json
import os
from pathlib import Path
import tempfile
import subprocess
import time
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


def choose(terminal, text):
    from rc_vm_forms import inside_box
    complete = lambda: terminal.output.endswith(b'\x1b[?25l')
    # Home selects the first option, which can sit below a description row.
    # Read inside the Settings box: page text sits beside it from rc13.
    box = lambda: inside_box(terminal.screen.text.splitlines())
    first = lambda: next((i for i, row in enumerate(box()) if '▸' in row), None)
    at_first = lambda: complete() and first() is not None and not any(
        row.strip('│ ') and '▸' not in row and not row.lstrip('│').startswith('    ')
        for row in box()[1:first()])
    # A key pressed while a page is still answering its first request can go unread: press again.
    for attempt in range(5):
        terminal.send(b'\x1b[H')
        try:
            terminal.until(at_first, 'Home selects the first native menu option', timeout=3)
            break
        except AssertionError:
            if attempt == 4:
                raise
    for _ in range(30):
        terminal.until(lambda: '▸' in terminal.screen.text, 'selected native menu row')
        if any('▸ ' + text in row for row in terminal.screen.text.splitlines()):
            terminal.send(b'\r')
            return
        previous = next(row for row in terminal.screen.text.splitlines() if '▸' in row)
        terminal.send(b'\x1b[B')
        terminal.until(lambda: complete() and next((row for row in terminal.screen.text.splitlines() if '▸' in row), '') != previous,
                       'next native menu selection')
    raise AssertionError('Native menu entry not found: ' + text + '\n' + terminal.screen.text)


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'native writing shell runs unprivileged')
class NativeBackupPTY(unittest.TestCase):
    def open_backups(self, terminal):
        terminal.send(b'\x0bWrOS settings\r')
        terminal.screen_shows('WrOS · Settings')
        choose(terminal, 'Backups and accounts')
        terminal.screen_shows('Settings › Backups and accounts')
        choose(terminal, 'Backups')
        terminal.screen_shows('Settings › Backups and accounts › Backups')
        terminal.until(lambda: terminal.output.endswith(b'\x1b[?25l') and 'Cancel current backup operation' not in terminal.screen.text,
                       'offline backup status finished')

    def open_subpage(self, terminal, page):
        choose(terminal, page)
        terminal.screen_shows('Settings › Backups and accounts › Backups › ' + page)

    def back_to_backups(self, terminal):
        terminal.send(b'\x1b')
        # The subpages' titles begin with this one's, so match the whole title.
        terminal.until(lambda: terminal.screen.text.splitlines()[0].startswith(
            '┌WrOS · Settings › Backups and accounts › Backups─'), 'back on the Backups page')

    def open_google_drive(self, terminal):
        terminal.send(b'\x0bWrOS settings\r')
        terminal.screen_shows('WrOS · Settings')
        choose(terminal, 'Backups and accounts')
        choose(terminal, 'Google Drive')
        terminal.screen_shows('Settings › Backups and accounts › Google Drive')

    def test_native_setup_backup_and_restoration_preserve_original_editing_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-native-backup-') as temporary:
                base = Path(temporary)
                project, home, primary = fixture(base)
                terminal = TerminalProcess(project, home, width, height)
                prefix = 'Original λ words for encrypted backup. '
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.paste(prefix)
                    terminal.send(b'\x13')
                    terminal.until(lambda: prefix.encode() in primary.read_bytes(), 'original saved writing')
                    original = primary.read_bytes()
                    self.open_backups(terminal)
                    terminal.screen_shows('No repository configured')
                    self.open_subpage(terminal, 'Where backups go')
                    choose(terminal, 'New backup on a local drive')
                    terminal.screen_shows('New encrypted backup')
                    repository = base/'encrypted snapshots'
                    terminal.paste(str(repository))
                    terminal.send(b'\r\r')
                    terminal.screen_shows('Repository connected.')
                    self.assertTrue((repository/'config').is_file())
                    self.back_to_backups(terminal)
                    choose(terminal, 'Create encrypted backup')
                    terminal.screen_shows('Backup verified by a complete restore')
                    state = home/'data/grimoire/wros/backup/repository.json'
                    config = json.loads(state.read_text())
                    self.assertEqual(len(config['verified']), 1)
                    identifier = next(iter(config['verified']))
                    self.assertEqual(Path(config['key']).stat().st_mode & 0o777, 0o600)
                    terminal.send(b'\x1b[F\r')
                    terminal.screen_shows('Restore snapshot into a NEW folder')
                    terminal.send(b'\r\r')
                    terminal.screen_shows('Restore verified')
                    restored = project.parent/(project.name + '-restored-' + identifier[:8])
                    self.assertEqual((restored/primary.relative_to(project)).read_bytes(), original)
                    self.assertEqual(primary.read_bytes(), original)
                    if os.environ.get('WROS_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/backup-screens'
                        captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'backup-{width}x{height}.txt').write_text(terminal.screen.text + '\n', encoding='utf-8')
                    # A second restore to its default existing target must refuse.
                    terminal.send(b'\x1b[F\r')
                    terminal.screen_shows('Restore snapshot into a NEW folder')
                    terminal.send(b'\r\r')
                    terminal.screen_shows('Destination already exists')
                    self.assertEqual(primary.read_bytes(), original)
                    terminal.send(b'\x1b')
                    terminal.until(lambda: terminal.screen.text.splitlines()[0].startswith(
                        '┌WrOS · Settings › Backups and accounts─'), 'back on Backups and accounts')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('▸ Backups and accounts')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Original λ words for')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: prefix.encode() not in primary.read_bytes(), 'original Undo retained after backup and restore')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal writer quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()

    def test_google_authorization_paste_never_renders_and_escape_keeps_it_out_of_the_book(self):
        with tempfile.TemporaryDirectory(prefix='wros-google-form-') as temporary:
            project, home, primary = fixture(Path(temporary))
            before = primary.read_bytes()
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                self.open_google_drive(terminal)
                choose(terminal, 'Connect Google Drive')
                # Opening the page asks the backup helper for its status; a form waits for it.
                for _ in range(10):
                    if 'Paste authorization JSON' in terminal.screen.text:
                        break
                    time.sleep(0.5)
                    if any(row.startswith('│▸ Connect Google Drive') for row in terminal.screen.text.splitlines()):
                        terminal.send(b'\r')
                terminal.screen_shows('Paste authorization JSON')
                terminal.paste('{"access_token":"PRIVATE_ACCESS_FIXTURE","refresh_token":"PRIVATE_REFRESH_FIXTURE"}')
                terminal.screen_shows('[hidden]')
                self.assertNotIn(b'PRIVATE_ACCESS_FIXTURE', terminal.output)
                self.assertNotIn(b'PRIVATE_REFRESH_FIXTURE', terminal.output)
                terminal.send(b'\x1b')
                terminal.screen_shows('Settings › Backups and accounts › Google Drive')
                self.assertFalse((home/'data/grimoire/wros/backup/rclone.conf').exists())
                self.assertEqual(primary.read_bytes(), before)
            finally:
                terminal.close()

    def test_cancel_stops_a_real_large_backup_and_an_explicit_retry_remains_available(self):
        with tempfile.TemporaryDirectory(prefix='wros-backup-cancel-') as temporary:
            base = Path(temporary)
            project, home, primary = fixture(base)
            source = primary.read_bytes()
            payload = project/'research/cancel-fixture.bin'
            payload.parent.mkdir(exist_ok=True)
            with payload.open('xb') as stream:
                stream.truncate(512 * 1024**2)
            env = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(home/'data'), XDG_CONFIG_HOME=str(home/'config'))
            subprocess.run([str(BINARY), 'wros', 'backup', 'create', str(base/'repository')],
                           env=env, capture_output=True, check=True, timeout=20)
            terminal = TerminalProcess(project, home, 80, 24)
            state = home/'data/grimoire/wros/backup'
            try:
                terminal.shows('The Brass Key')
                self.open_backups(terminal)
                choose(terminal, 'Create encrypted backup')
                terminal.until(lambda: any(state.glob('wros-backup-*')), 'real private backup staging started')
                choose(terminal, 'Cancel current backup operation')
                terminal.screen_shows('Cancelling; refresh snapshots')
                terminal.until(lambda: not list(state.glob('wros-backup-*')), 'cancelled worker cleaned private staging')
                self.assertEqual(primary.read_bytes(), source)
                self.assertEqual(json.loads((state/'repository.json').read_text())['verified'], {})
                # Remove only our oversized disposable fixture, then retry via
                # the real controls. No source manuscript or snapshot is deleted.
                payload.unlink()
                self.open_subpage(terminal, 'Recovery key')
                choose(terminal, 'Recover an interrupted repository lock')
                terminal.screen_shows('Stale repository locks removed')
                self.back_to_backups(terminal)
                choose(terminal, 'Create encrypted backup')
                terminal.screen_shows('Backup verified by a complete restore')
                self.assertEqual(primary.read_bytes(), source)
                self.assertEqual(len(json.loads((state/'repository.json').read_text())['verified']), 1)
            finally:
                terminal.close()

    def test_a_drive_backup_never_takes_the_writing_console_from_the_editor(self):
        # Restic hands its terminal's foreground to the rclone it starts for a
        # Drive repository. On a login console that left the editor's input
        # reader with EIO on the next key, and the session ended mid-backup.
        # rclone keeps this "Drive" in a local folder; Google is never contacted.
        with tempfile.TemporaryDirectory(prefix='wros-backup-console-') as temporary:
            base = Path(temporary)
            project, home, primary = fixture(base)
            source = primary.read_bytes()
            payload = project/'research/console-fixture.bin'
            payload.parent.mkdir(exist_ok=True)
            payload.write_bytes(os.urandom(48 * 1024**2))
            state = home/'data/grimoire/wros/backup'
            state.mkdir(parents=True, mode=0o700)
            (base/'drive').mkdir()
            config = state/'rclone.conf'
            config.write_text(f'[wros-google]\ntype = alias\nremote = {base/"drive"}\n')
            config.chmod(0o600)
            env = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(home/'data'), XDG_CONFIG_HOME=str(home/'config'))
            subprocess.run([str(BINARY), 'wros', 'backup', 'create', 'drive:WrOS-Test'],
                           env=env, capture_output=True, check=True, timeout=60)
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                self.open_backups(terminal)
                choose(terminal, 'Create encrypted backup')
                rclone = lambda: subprocess.run(['pgrep', '-x', 'rclone'], capture_output=True).returncode == 0
                terminal.until(rclone, 'restic started rclone for the Drive repository', timeout=60)
                terminal.screen_shows('Working: Encrypting and saving the snapshot')
                terminal.send(b'\x1b[B')
                terminal.until(lambda: 'Backup verified by a complete restore' in terminal.screen.text or terminal.process.poll() is not None,
                               'Drive backup finished with the editor still running', timeout=120)
                self.assertIsNone(terminal.process.poll(), terminal.output[-400:])
                terminal.screen_shows('Backup verified by a complete restore')
                self.assertEqual(primary.read_bytes(), source)
            finally:
                terminal.close()

    def test_offline_backup_queue_and_google_disconnect_confirmation_preserve_manuscript(self):
        with tempfile.TemporaryDirectory(prefix='wros-backup-queue-pty-') as temporary:
            base = Path(temporary)
            project, home, primary = fixture(base)
            original = primary.read_bytes()
            state = home/'data/grimoire/wros/backup'
            state.mkdir(parents=True, exist_ok=True, mode=0o700)
            rclone_conf = state/'rclone.conf'
            rclone_conf.write_bytes(b'[wros-google]\ntype = drive\ntoken = {"refresh_token":"SECRET"}\n')
            rclone_conf.chmod(0o600)
            account_json = state/'google-account.json'
            account_json.write_text(json.dumps({
                'version': 1,
                'connected_at': '2026-10-31T04:00:00Z',
                'verified_at': '2026-10-31T04:00:00Z',
                'token_expiry': '2026-11-01T00:00:00Z',
                'state': 'reauthorization-required',
                'last_error': 'Google Drive authorization expired or was revoked.',
            }) + '\n')
            account_json.chmod(0o600)
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key')
                self.open_backups(terminal)
                self.open_subpage(terminal, 'Offline queue')
                choose(terminal, 'Queue saved project for offline backup')
                terminal.screen_shows('Project queued for encrypted backup (1 pending)')
                self.assertTrue((state/'queue.json').is_file())
                self.back_to_backups(terminal)
                self.open_subpage(terminal, 'Where backups go')
                choose(terminal, 'New backup on a local drive')
                repository = base/'queued-repository'
                terminal.paste(str(repository))
                terminal.send(b'\r\r')
                terminal.screen_shows('Repository connected.')
                self.back_to_backups(terminal)
                self.open_subpage(terminal, 'Offline queue')
                choose(terminal, 'Run queued backups and retry interrupted jobs')
                terminal.screen_shows('Completed all queued backups')
                self.assertEqual(json.loads((state/'queue.json').read_text())['items'], [])
                self.assertEqual(len(json.loads((state/'repository.json').read_text())['verified']), 1)
                self.back_to_backups(terminal)
                terminal.send(b'\x1b')
                terminal.screen_shows('Settings › Backups and accounts')
                choose(terminal, 'Google Drive')
                terminal.screen_shows('reauthorization-required')
                terminal.until(lambda: terminal.output.endswith(b'\x1b[?25l')
                               and 'Backup operation running' not in terminal.screen.text,
                               'local Google account status refresh finished')
                choose(terminal, 'Disconnect Google Drive and remove local credentials')
                terminal.screen_shows('Cancel and keep saved Google Drive credentials')
                terminal.send(b'\r')  # Default Cancel preserves credentials.
                terminal.screen_shows('Settings › Backups and accounts › Google Drive')
                self.assertTrue(rclone_conf.exists())
                choose(terminal, 'Disconnect Google Drive and remove local credentials')
                terminal.send(b'\x1b[B\r')  # Confirm disconnect.
                terminal.screen_shows('Google Drive credentials removed')
                self.assertFalse(rclone_conf.exists())
                self.assertFalse(account_json.exists())
                self.assertEqual(primary.read_bytes(), original)
            finally:
                terminal.close()


if __name__ == '__main__':
    unittest.main()
