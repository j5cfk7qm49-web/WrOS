"""Compiled native Markdown footnote workflow in fresh demo/HOME fixtures."""
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run as an ordinary user')
class FootnoteInsertionPTY(unittest.TestCase):
    def test_original_primary_secondary_typing_undo_redo_save_and_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-footnote-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                secondary = next(p for p in (project/'manuscript').rglob('*.md') if 'The Other Door' in p.read_text())
                primary.write_text('---\ntitle: "The Brass Key"\ncustom: "λ original"\n---\n\nλ Original prose.\n', encoding='utf-8')
                secondary.write_text('---\ntitle: "The Other Door"\n---\n\n🦊 Other original.\n', encoding='utf-8')
                originals = {p: p.read_bytes() for p in (primary, secondary)}
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('λ Original prose.')
                    for path, prefix in [(primary, 'λ'), (secondary, '🦊')]:
                        if path == secondary:
                            terminal.send(b'\x1b[23~Other Door\r')
                            terminal.screen_shows('Secondary')
                        terminal.send(b'\x1b[H\x1b[C')
                        terminal.send(b'\x0bInsert a Markdown footnote at the cursor\r')
                        terminal.screen_shows('[^fn-1] added')
                        terminal.paste('Author note λ.')
                        terminal.send(b'\x13')
                        terminal.until(lambda: b'[^fn-1]: Author note' in path.read_bytes(), 'footnote source saved')
                        self.assertIn((prefix+'[^fn-1]').encode(), path.read_bytes())
                        terminal.send(b'\x1a\x1a\x13')
                        terminal.until(lambda: path.read_bytes() == originals[path], 'both undo steps restore original bytes')
                        terminal.send(b'\x19\x19\x13')
                        terminal.until(lambda: b'[^fn-1]: Author note' in path.read_bytes(), 'both redo steps restore footnote')
                    saved = {p: p.read_bytes() for p in originals}
                    self.assertIn(b'custom: "\xce\xbb original"', saved[primary])
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Other Door')
                    terminal.screen_shows('Author note λ.')
                    for path, content in saved.items():
                        self.assertEqual(path.read_bytes(), content)
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'reopened normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
