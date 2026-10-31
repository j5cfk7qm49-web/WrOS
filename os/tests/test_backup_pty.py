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
    terminal.send(b'\x1b[H')
    complete = lambda: terminal.output.endswith(b'\x1b[?25l')
    terminal.until(lambda: complete() and '▸' in terminal.screen.text.splitlines()[1], 'Home selects the first native menu row')
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
        choose(terminal, 'Accounts')
        terminal.screen_shows('Settings › Accounts')
        choose(terminal, 'Backups')
        terminal.screen_shows('Settings › Backups')
        terminal.until(lambda: terminal.output.endswith(b'\x1b[?25l') and 'Cancel current backup operation' not in terminal.screen.text,
                       'offline backup status finished')

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
                    choose(terminal, 'New backup on a local drive')
                    terminal.screen_shows('New encrypted backup')
                    repository = base/'encrypted snapshots'
                    terminal.paste(str(repository))
                    terminal.send(b'\r\r')
                    terminal.screen_shows('Repository connected.')
                    self.assertTrue((repository/'config').is_file())
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
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
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
                    terminal.screen_shows('Settings › Accounts')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('▸ Accounts')
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
                self.open_backups(terminal)
                choose(terminal, 'Connect Google Drive')
                terminal.screen_shows('Paste authorization JSON')
                terminal.paste('{"access_token":"PRIVATE_ACCESS_FIXTURE","refresh_token":"PRIVATE_REFRESH_FIXTURE"}')
                terminal.screen_shows('[hidden]')
                self.assertNotIn(b'PRIVATE_ACCESS_FIXTURE', terminal.output)
                self.assertNotIn(b'PRIVATE_REFRESH_FIXTURE', terminal.output)
                terminal.send(b'\x1b')
                terminal.screen_shows('Settings › Backups')
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
                choose(terminal, 'Recover an interrupted repository lock')
                terminal.screen_shows('Stale repository locks removed')
                choose(terminal, 'Create encrypted backup')
                terminal.screen_shows('Backup verified by a complete restore')
                self.assertEqual(primary.read_bytes(), source)
                self.assertEqual(len(json.loads((state/'repository.json').read_text())['verified']), 1)
            finally:
                terminal.close()


if __name__ == '__main__':
    unittest.main()
