"""Compiled export review against disposable demo projects at four sizes."""
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must never run as root')
class ExportPreflightPTY(unittest.TestCase):
    def test_review_preserves_original_and_export_choices_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-preflight-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = ('---\ntitle: "The Brass Key"\ncustom: "λ kept"\n---\n\n'
                          'λ Intro[^lost]. TODO public.\n\n%% [^private]: PRIVATE_SECRET %%\n')
                primary.write_text(before, encoding='utf-8')
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('λ Intro')
                    terminal.send(b'\x1b[H')
                    terminal.paste('Added ')
                    terminal.send(b'\x13')
                    changed = before.replace('λ Intro', 'Added λ Intro')
                    terminal.until(lambda: primary.read_bytes() == changed.encode(), 'original edit saved')
                    terminal.send(b'\x0bExport for readers\r')
                    terminal.screen_shows('EXPORT FOR READERS')
                    terminal.send(b'p')
                    terminal.screen_shows('EXPORT PREFLIGHT')
                    terminal.send(b'\x1b[F')
                    terminal.screen_shows('lost')
                    terminal.screen_shows('TODO')
                    self.assertNotIn('PRIVATE_SECRET', terminal.screen.text)
                    self.assertNotIn('private', terminal.screen.text.lower())
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/export-preflight-screens'
                        captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'export-preflight-{width}x{height}.txt').write_text(terminal.screen.text+'\n', encoding='utf-8')
                    terminal.paste('x overwrite'); terminal.send(b'\x18\x1a\x13x')
                    self.assertEqual(primary.read_bytes(), changed.encode())
                    self.assertFalse((project/'exports').exists())
                    terminal.send(b'\x1bOP')
                    terminal.screen_shows('Compile: the manuscript')
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'Compile: the manuscript' not in terminal.screen.text and 'EXPORT PREFLIGHT' in terminal.screen.text, 'help returned to preflight')
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'EXPORT PREFLIGHT' not in terminal.screen.text and 'EXPORT FOR READERS' in terminal.screen.text, 'returned to export settings')
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'EXPORT FOR READERS' not in terminal.screen.text, 'returned to editor')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'Undo preserves original YAML and private bytes')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
