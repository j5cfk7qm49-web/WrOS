"""Compiled contextual reader, original editing and exact Undo in fresh fixtures."""
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must never run as root')
class FootnoteContextPTY(unittest.TestCase):
    def test_context_full_note_references_definition_help_undo_and_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-footnote-context-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = ('---\ntitle: "The Brass Key"\ncustom: "λ preserved"\n---\n\n'
                          'λ Intro[^a] and again[^a].\n\n'
                          '[^a]: First public paragraph. %% PRIVATE_SECRET %%\n\n    '
                          + 'Original continuation 🦊. ' * 80 + 'END_CONTEXT\n')
                primary.write_text(before, encoding='utf-8')
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('λ Intro')
                    terminal.send(b'\x1b[H' + b'\x1b[C' * 8)
                    terminal.send(b'\x0bRead footnote at caret\r')
                    terminal.screen_shows('Footnote context · Source Markdown')
                    terminal.screen_shows('First public paragraph.')
                    self.assertNotIn('PRIVATE_SECRET', terminal.screen.text.split('Footnote context', 1)[1])
                    terminal.paste('rdoverwrite'); terminal.send(b'\x18\x1a\x13')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b[F')
                    terminal.screen_shows('END_CONTEXT')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/footnote-context-screens'
                        captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'footnote-context-{width}x{height}.txt').write_text(terminal.screen.text+'\n', encoding='utf-8')
                    terminal.send(b'\x1b[H\x1b[C')
                    terminal.screen_shows('reference 2 / 2')
                    terminal.send(b'r')
                    terminal.until(lambda: 'Footnote context ·' not in terminal.screen.text, 'returned to second original reference')
                    terminal.paste('[^changed]'); terminal.send(b'\x13')
                    changed = before.replace('again[^a]', 'again[^changed]')
                    terminal.until(lambda: primary.read_bytes() == changed.encode(), 'selected exactly the second reference')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'reference Undo retains all original bytes')
                    terminal.send(b'\x1b[H' + b'\x1b[C' * 8)
                    terminal.send(b'\x0bRead footnote at caret\r')
                    terminal.screen_shows('Footnote context · Source Markdown')
                    terminal.send(b'd')
                    terminal.until(lambda: 'Footnote context ·' not in terminal.screen.text, 'original definition opened')
                    terminal.paste('Added λ. '); terminal.send(b'\x13')
                    changed = before.replace('[^a]: First', '[^a]: Added λ. First')
                    terminal.until(lambda: primary.read_bytes() == changed.encode(), 'normal source definition edit saved')
                    terminal.send(b'\x0bRead footnote at caret\r')
                    terminal.screen_shows('Footnote context · Source Markdown')
                    terminal.send(b'\x1bOP')
                    terminal.screen_shows('Notes & TKs')
                    terminal.send(b'\x18\x1a')
                    terminal.send(b'\x1b')
                    # Help and the dock can coexist during a partial repaint.
                    # Wait for Help to disappear before sending another Esc.
                    terminal.until(lambda: 'Notes & TKs' not in terminal.screen.text
                                   and 'Footnote context · Source Markdown' in terminal.screen.text,
                                   'help returned to the context reader')
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'Footnote context ·' not in terminal.screen.text
                                   and 'Notes & TKs' not in terminal.screen.text, 'returned to definition editor')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'definition Undo retains YAML and private source')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('λ Intro')
                    terminal.send(b'\x1b[H' + b'\x1b[C' * 8)
                    terminal.send(b'\x0bRead footnote at caret\r')
                    terminal.screen_shows('Footnote context · Source Markdown')
                    terminal.send(b'\x1b[F')
                    terminal.screen_shows('END_CONTEXT')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'Footnote context ·' not in terminal.screen.text, 'restart returned to editor')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'restart normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
