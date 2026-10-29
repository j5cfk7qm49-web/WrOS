"""Actual compiled annotation/revision workflow; fresh demo/HOME, no providers."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'the writing app must never run as root')
class ReviewNotesPTY(unittest.TestCase):
    def test_annotations_reanchor_undo_status_restart_and_original_bytes_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-notes-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                # A new fixture, with an original YAML field and multibyte prose.
                before = '---\ntitle: "The Brass Key"\ncustom: "λ preserved"\n---\n\nThe silver key. Café 🦊 original.\n'
                primary.write_text(before, encoding='utf-8')
                metadata = project/'.grimoire/writingdeck/review-notes.json'
                tutorial = project/'.grimoire/writingdeck/tutorial.json'
                tutorial.parent.mkdir(parents=True, exist_ok=True)
                tutorial.write_text(json.dumps({'version': 1, 'active': 32, 'completed': [], 'skipped': []})+'\n')
                def state():
                    return json.loads(metadata.read_text(encoding='utf-8')) if metadata.exists() else None
                def saved_comment():
                    value = state()
                    return value and len(value['notes']) == 1
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.screen_shows('The silver key.')
                    terminal.send(b'\x1b[H' + b'\x1b[1;2C'*14)  # Home, Shift-Right x14
                    terminal.send(b'\x0bRevision notes\r')
                    terminal.screen_shows('Revision notes · Open')
                    terminal.send(b'n')
                    terminal.screen_shows('Revision notes · Comment')
                    terminal.paste('Who owns this key? λ')
                    terminal.send(b'\r')
                    terminal.until(saved_comment, 'one saved annotation')
                    terminal.screen_shows('Revision notes · Review')
                    self.assertEqual(state()['notes'][0]['current']['anchor']['quote'], 'The silver key')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.paste('xnog'); terminal.send(b'\x18\x1a')
                    terminal.send(b's')
                    terminal.until(lambda: state()['notes'][0]['current']['status'] == 'InProgress', 'in-progress note')
                    terminal.send(b'g')
                    terminal.screen_shows('Revision passage selected')
                    terminal.paste('The golden key')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'The golden key.' in primary.read_bytes(), 'normal guarded source edit')
                    terminal.send(b'\x1b[H' + b'\x1b[1;2C'*14)
                    terminal.send(b'\x0bRevision notes\r')
                    terminal.screen_shows('Revision notes · Open')
                    terminal.send(b'\r')
                    terminal.screen_shows('Anchor changed or ambiguous')
                    terminal.send(b'a')
                    terminal.screen_shows('Revision notes · Re-anchor')
                    terminal.screen_shows('▸ Cancel')
                    terminal.send(b'\x1b[B\r')
                    terminal.until(lambda: state()['notes'][0]['current']['anchor']['quote'] == 'The golden key', 'new anchor saved')
                    terminal.until(lambda: 32 in json.loads(tutorial.read_text())['completed'], 'real demo re-anchor lesson')
                    terminal.screen_shows('Revision notes · Review')
                    terminal.send(b'x')
                    terminal.until(lambda: state()['notes'][0]['current']['status'] == 'Resolved', 'revision-bound resolution')
                    terminal.screen_shows('Resolved saved')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/review-note-screens'
                        captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'review-notes-{width}x{height}.txt').write_text(terminal.screen.text+'\n', encoding='utf-8')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Revision notes · Open')
                    terminal.send(b'\x1b')
                    # ESC followed immediately by a control byte is an Alt
                    # sequence to the terminal parser. Observe the exit before Undo.
                    terminal.until(lambda: 'Revision notes ·' not in terminal.screen.text
                                   and 'The golden key.' in terminal.screen.text, 'returned to source editor')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before.encode(), 'exact source Undo including YAML')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bRevision notes\r')
                    terminal.screen_shows('Revision notes · Open')
                    terminal.send(b'f\r')
                    terminal.screen_shows('Revision notes · Review')
                    terminal.screen_shows('Resolved on an older/unavailable revision')
                    self.assertEqual(state()['documents'][0]['id'], 1)
                    self.assertEqual(state()['notes'][0]['previous'][0]['comment'], 'Who owns this key? λ')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Revision notes · All')
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'Revision notes ·' not in terminal.screen.text, 'restart returned to editor')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'restart normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
