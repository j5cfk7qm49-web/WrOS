"""Real non-root operation ledger/goals/calendar/sprint/challenge/restart input.

Only disposable demo/HOME fixtures. No host devices, network, credentials or
manuscripts. The shared ANSI/CPR shim is not a complete terminal emulator.
"""
import json
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


def records(project):
    return [entry for file in sorted((project/'.grimoire/writingdeck/ledger').glob('*/*.json'))
            for entry in json.loads(file.read_text())['entries']]


def counter(project, origin, field):
    return sum(e['counts'][field] for e in records(project)
               if e['origin'] == origin and e['source']['role'] == 'manuscript')


def field(terminal, target):
    terminal.send(b'\x7f'*16 + target.encode() + b'\r')


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing application must be non-root')
class ProductivityPTY(unittest.TestCase):
    def test_actual_origins_calendar_goals_sprint_challenge_and_loaded_process_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-productivity-pty-') as tmp:
                project, home, primary = fixture(Path(tmp))
                meta = project/'.grimoire/writingdeck/productivity.json'

                def settings():
                    return json.loads(meta.read_text()) if meta.exists() else {}

                def assignment():
                    files = list((project/'.grimoire/writingdeck/challenges').glob('*.json'))
                    return json.loads(files[0].read_text()) if len(files) == 1 else None

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\rKeyboard recorded words. \x13')
                    terminal.until(lambda: counter(project, 'typing', 'added') == 3, 'actual per-key gross words')
                    terminal.paste('Copied separate passage. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: counter(project, 'paste', 'added') == 3, 'actual bracketed-paste origin')
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: counter(project, 'undo', 'removed') == 3, 'actual source undo replay')
                    self.assertNotIn(b'Copied separate passage.', primary.read_bytes())
                    terminal.send(b'\x19\x13')
                    terminal.until(lambda: counter(project, 'redo', 'added') == 3, 'actual redo replay')
                    terminal.send(b'\x0bCalendar and rest days\rn ')
                    terminal.until(lambda: (settings().get('schedule') or {}).get('target') == 50000 and
                                   len(settings()['schedule']['rest_days']) == 1,
                                   'explicit saved November target and rest-day choice')
                    terminal.send(b'2n\r')
                    field(terminal, '25')
                    terminal.until(lambda: bool(settings().get('goals')) and settings()['goals'][0].get('target') == 25,
                                   'actual configurable measured goal target')
                    terminal.send(b'4n\x1b[B\r')
                    field(terminal, '3')
                    terminal.until(lambda: (settings().get('sprint') or {}).get('kind') == 'words',
                                   'actual word sprint started')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.paste('Sprint actual words. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: settings()['sprint']['status'] == 'completed', 'measured word sprint result')
                    self.assertEqual(settings()['sprint']['progress'], 3)
                    terminal.send(b'\x0bDaily challenges\ra')
                    terminal.until(lambda: assignment() is not None and assignment()['assignments'][0]['status'] == 'accepted',
                                   'explicit daily assignment acceptance')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.paste('exercise '*50)
                    terminal.send(b'\x13\x0bDaily challenges\rc')
                    terminal.until(lambda: assignment()['assignments'][0]['status'] == 'completed', 'persisted genuinely measured subsequent work')
                    self.assertEqual(assignment()['assignments'][0]['evidence'], 'measured')
                    original = primary.read_bytes()
                    assignment_bytes = next((project/'.grimoire/writingdeck/challenges').glob('*.json')).read_bytes()
                    serialized = '\n'.join(p.read_text() for p in (project/'.grimoire/writingdeck/ledger').glob('*/*.json'))
                    for prose in ['Keyboard recorded words', 'Copied separate passage', 'Sprint actual words', 'exercise exercise']:
                        self.assertNotIn(prose, serialized)
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        terminal.screen_shows('[Completed]')
                        terminal.screen_shows('50 / 50')
                        path = ROOT/'docs/evidence'/f'productivity-pty-{width}x{height}.txt'
                        path.write_text(terminal.screen.text + '\n')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal metric checkpoint and quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    restarted.send(b'\x0bDaily challenges\r')
                    restarted.screen_shows('measured')
                    self.assertEqual(next((project/'.grimoire/writingdeck/challenges').glob('*.json')).read_bytes(), assignment_bytes)
                    self.assertEqual(settings()['goals'][0]['target'], 25)
                    self.assertEqual(settings()['sprint']['progress'], 3)
                    self.assertEqual(len(settings()['schedule']['rest_days']), 1)
                    restarted.send(b'\x0bThe Brass Key\r')
                    restarted.paste('Loaded original ledger words. ')
                    restarted.send(b'\x13')
                    restarted.until(lambda: b'Loaded original ledger words.' in primary.read_bytes(), 'actual loaded-source edit after separate-process restart')
                    restarted.send(b'\x1a\x13')
                    restarted.until(lambda: primary.read_bytes() == original, 'loaded original isolated undo after restart')
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal restarted checkpoint')
                    self.assertEqual(restarted.process.returncode, 0)
                    self.assertEqual(counter(project, 'typing', 'added'), 3)
                    self.assertEqual(counter(project, 'paste', 'added'), 60)
                    self.assertEqual(counter(project, 'undo', 'removed'), 7)
                    self.assertEqual(counter(project, 'redo', 'added'), 3)
                finally:
                    restarted.close()

    def test_future_ledger_keeps_original_blocks_normal_quit_and_explicit_retry_has_no_duplicate_groups(self):
        with tempfile.TemporaryDirectory(prefix='wd-productivity-failure-pty-') as tmp:
            project, home, primary = fixture(Path(tmp))
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bThe Brass Key\rFirst \x13')
                terminal.until(lambda: counter(project, 'typing', 'added') == 1, 'first durable actual group')
                part = next((project/'.grimoire/writingdeck/ledger').glob('*/*.json'))
                saved = part.read_bytes()
                future = b'{"version":999,"future":"retain all bytes"}\n'
                part.write_bytes(future)
                terminal.send(b'Second \x13\x11')
                terminal.screen_shows('Q again to quit anyway')
                self.assertIsNone(terminal.process.poll())
                self.assertIn(b'Second', primary.read_bytes())
                self.assertEqual(part.read_bytes(), future)
                part.write_bytes(saved)
                terminal.send(b'\x0bProductivity ledger\rr')
                terminal.until(lambda: counter(project, 'typing', 'added') == 2, 'explicit original-preserving ledger retry')
                entries = records(project)
                self.assertEqual(len({e['id'] for e in entries}), len(entries))
                terminal.send(b'\x11')
                terminal.until(lambda: terminal.process.poll() is not None, 'normal quit after actual retry')
                self.assertEqual(terminal.process.returncode, 0)
            finally:
                terminal.close()
