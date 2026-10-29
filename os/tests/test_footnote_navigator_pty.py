"""Original-source footnote navigation in the compiled Debian application."""
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'the writing app must never run as root')
class FootnoteNavigatorPTY(unittest.TestCase):
    def test_reference_definition_edit_undo_and_modal_help_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-footnote-nav-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = '---\ntitle: "The Brass Key"\ncustom: "λ preserved"\n---\n\nCafé 🦊[^note] original.\n\n[^note]: Original definition.\n'
                primary.write_text(before, encoding='utf-8')
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('Original definition.')
                    terminal.send(b'\x0bFootnotes\r')
                    terminal.screen_shows('Footnotes · Original Markdown')
                    terminal.paste('drewritten')
                    terminal.send(b'\x18\x1a')
                    terminal.send(b'\x1bOP')  # F1, contextual offline help
                    terminal.screen_shows('Notes & TKs')
                    terminal.send(b'\x18\x1a')
                    terminal.paste('hidden prose')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Footnotes · Original Markdown')
                    terminal.send(b'd')
                    terminal.screen_shows('footnote [^note] · defined')
                    terminal.paste('Added λ. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'Added ' in primary.read_bytes(), 'definition saved in original source')
                    self.assertEqual(primary.read_text(encoding='utf-8'), before.replace('Original definition.', 'Added λ. Original definition.'))
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'definition Undo retains source and YAML')
                    terminal.send(b'\x0bFootnotes\r')
                    terminal.screen_shows('Footnotes · Original Markdown')
                    terminal.send(b'\r')
                    terminal.screen_shows('footnote [^note] · defined')
                    terminal.paste('[^renamed]')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'Caf\xc3\xa9 \xf0\x9f\xa6\x8a[^renamed]' in primary.read_bytes(), 'exact reference selected after multibyte prose')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'reference Undo restores exact original')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
