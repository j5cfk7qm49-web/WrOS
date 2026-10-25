"""Real compiled, non-root journal/source/save/restart and failed-log controls.

Only newly created demo/HOME fixtures; no host keymaps, services or networking.
The shared PTY shim answers startup CPR, not a full terminal emulation.
"""
from datetime import datetime
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_secondary_pty import BINARY, TerminalProcess


def fixture(base):
    project, home = base/'demo', base/'home'
    home.mkdir()
    env = os.environ | {'HOME': str(home), 'XDG_DATA_HOME': str(home/'data'),
                        'XDG_CONFIG_HOME': str(home/'config')}
    subprocess.run([str(BINARY), 'writingdeck', 'tutorial-demo', str(project)],
                   env=env, capture_output=True, text=True, check=True, timeout=15)
    primary = next(p for p in (project/'manuscript').rglob('*.md')
                   if 'The Brass Key' in p.read_text())
    return project, home, primary


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing app must not run as root')
class JournalPTY(unittest.TestCase):
    def test_saved_observations_reflection_paste_guard_undo_and_loaded_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='writingdeck-journal-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                journal_dir = project/'.grimoire/writingdeck/journal'

                def fact_file():
                    paths = list(journal_dir.glob('*.json'))
                    return paths[0] if len(paths) == 1 else None

                def facts():
                    path = fact_file()
                    return json.loads(path.read_text()) if path else None

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.paste('PTY manuscript journal words. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: facts() is not None and len(facts()['facts']) == 1,
                                   'real saved manuscript observation')
                    path = fact_file()
                    saved_manuscript, first_record = primary.read_bytes(), path.read_bytes()
                    self.assertIn(b'PTY manuscript journal words.', saved_manuscript)
                    first = facts()['facts'][0]
                    self.assertEqual(first['kind'], 'saved-checkpoint')
                    o = first['observation']
                    self.assertGreater(o['manuscript_words'], o['initial_manuscript_words'])
                    self.assertEqual(o['at'][:10], path.stem)
                    for invented in ('gross_words', 'deleted_words', 'active_seconds', 'duration'):
                        self.assertNotIn(invented, first_record.decode())
                    reflection = project/'notes/journal'/f'{path.stem}.md'
                    self.assertFalse(reflection.exists())
                    terminal.send(b'\x0bWriting journal\r')
                    terminal.paste('ecrte These pasted commands must not run.')
                    terminal.send(b'\t\r')  # real tab to source reader, explicit Enter editor
                    terminal.paste('PTY personal reflection. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: reflection.is_file() and b'PTY personal reflection.' in reflection.read_bytes(),
                                   'original reflection save')
                    original_reflection = reflection.read_bytes()
                    self.assertNotIn(b'pasted commands', original_reflection)
                    self.assertEqual(primary.read_bytes(), saved_manuscript)
                    self.assertEqual(path.read_bytes(), first_record)
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: b'PTY personal reflection.' not in reflection.read_bytes(),
                                   'reflection-only undo and save')
                    self.assertEqual(primary.read_bytes(), saved_manuscript)
                    terminal.send(b'\x19\x13')
                    terminal.until(lambda: reflection.read_bytes() == original_reflection,
                                   'reflection-only redo and save')
                    terminal.send(b'\x0bWriting journal\r')
                    terminal.paste('ecrte Bracketed prose stays read-only.')
                    terminal.send(b'\x11')
                    # Graceful exit is the processing barrier. Do not infer
                    # input completion from differential title fragments.
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal journal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                    self.assertEqual(primary.read_bytes(), saved_manuscript)
                    self.assertEqual(reflection.read_bytes(), original_reflection)
                    self.assertEqual(len(facts()['facts']), 2)
                    self.assertEqual(facts()['facts'][-1]['kind'], 'exit-requested')
                    self.assertNotIn('personal reflection', path.read_text())
                    final_record = path.read_bytes()
                finally:
                    terminal.close()

                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    restarted.send(b'\x0bWriting journal\r\t\r')
                    restarted.paste('Loaded restart reflection. ')
                    restarted.send(b'\x13')
                    restarted.until(lambda: b'Loaded restart reflection.' in reflection.read_bytes(),
                                    'restart uses the loaded original source, not unchanged disk alone')
                    self.assertIn(b'PTY personal reflection.', reflection.read_bytes())
                    self.assertEqual(path.read_bytes(), final_record)
                    restarted.send(b'\x1a\x13')
                    restarted.until(lambda: reflection.read_bytes() == original_reflection,
                                    'restart source undo returns exact previously persisted bytes')
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal restart quit')
                    self.assertEqual(restarted.process.returncode, 0)
                    self.assertEqual(path.read_bytes(), final_record)
                    self.assertEqual(primary.read_bytes(), saved_manuscript)
                finally:
                    restarted.close()

    def test_future_log_or_unknown_totals_refuse_normal_exit_and_force_warning_is_explicit(self):
        with tempfile.TemporaryDirectory(prefix='writingdeck-journal-future-pty-') as temporary:
            project, home, primary = fixture(Path(temporary))
            day = datetime.now().astimezone().date().isoformat()
            path = project/'.grimoire/writingdeck/journal'/f'{day}.json'
            path.parent.mkdir(parents=True)
            original = json.dumps({'version': 999, 'date': day, 'next_id': 1, 'facts': []}).encode()
            path.write_bytes(original)
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bThe Brass Key\r')
                terminal.paste('Manuscript saves despite incompatible journal. ')
                terminal.send(b'\x13')
                terminal.until(lambda: b'Manuscript saves despite incompatible journal.' in primary.read_bytes(),
                               'source save independent of log format')
                terminal.send(b'\x0bWriting journal\rr\x11')
                terminal.screen_shows('Q again to quit anyway')
                self.assertIsNone(terminal.process.poll())
                self.assertEqual(path.read_bytes(), original)
                terminal.send(b'\x11')  # conscious force after visible warning
                terminal.until(lambda: terminal.process.poll() is not None, 'explicit forced quit')
                self.assertEqual(terminal.process.returncode, 0)
                self.assertEqual(path.read_bytes(), original)
                self.assertIn(b'Manuscript saves despite incompatible journal.', primary.read_bytes())
                self.assertFalse((project/'notes/journal'/f'{day}.md').exists())
            finally:
                terminal.close()


        # A save performed while the reader is open can leave an unavailable-
        # total warning with no queued fact. That warning must not hide the
        # explicit force-exit instruction either.
        with tempfile.TemporaryDirectory(prefix='writingdeck-journal-unknown-pty-') as temporary:
            project, home, primary = fixture(Path(temporary))
            unknown = project/'manuscript/unreadable-journal.md'
            original = b'\xff\xfe unreadable manuscript is not zero words'
            unknown.write_bytes(original)
            day = datetime.now().astimezone().date().isoformat()
            path = project/'.grimoire/writingdeck/journal'/f'{day}.json'
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bThe Brass Key\r')
                terminal.paste('Known manuscript still saves with unknown total. ')
                terminal.send(b'\x0bWriting journal\r\x13')
                terminal.until(lambda: b'Known manuscript still saves' in primary.read_bytes(),
                               'manuscript saved while reader open despite unavailable total')
                terminal.screen_shows('Journal observation unavailable')
                terminal.send(b'\x11')
                terminal.screen_shows('Q again to quit anyway')
                self.assertIsNone(terminal.process.poll())
                self.assertFalse(path.exists())
                self.assertEqual(unknown.read_bytes(), original)
                terminal.send(b'\x11')
                terminal.until(lambda: terminal.process.poll() is not None, 'explicit unknown-total forced exit')
                self.assertEqual(terminal.process.returncode, 0)
                self.assertFalse(path.exists())
                self.assertEqual(unknown.read_bytes(), original)
                self.assertFalse((project/'notes/journal'/f'{day}.md').exists())
            finally:
                terminal.close()


if __name__ == '__main__':
    unittest.main()
