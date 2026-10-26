"""Actual opt-in Harper worker, source edits, ignores, failures and restart.

Disposable demo + HOME only. No credentials, networking, host devices, user
manuscript or synthetic analyzer output. ANSI cells are not a full emulator.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_journal_pty import fixture
from test_productivity_pty import records
from test_secondary_pty import BINARY, ROOT, TerminalProcess

TEXT = 'Café 🦊 e\u0301. “I have a apple,” she said.\n\nA footnote.[^x]\n\n[^x]: An dog waits.\n'
NOVEL = '.grimoire/writingdeck/checks.json'
PERSONAL = '.grimoire/writingdeck/checks-global.json'


def check_fixture(base):
    project, home, primary = fixture(base)
    head, delimiter, _ = primary.read_text().partition('\n---\n')
    if not delimiter:
        raise AssertionError('demo fixture must retain original frontmatter')
    primary.write_text(head + '\n---\n\n' + TEXT)
    return project, home, primary


def open_checks(terminal):
    terminal.send(b'\x0bWriting checks\r')
    terminal.screen_shows('Writing checks')


def ready(terminal, count):
    terminal.until(lambda: f'{count} current suggestions' in terminal.screen.text and
                   (count == 0 or 'AnA' in terminal.screen.text) and
                   terminal.output.endswith(b'\x1b[?25l'),
                   'completed real local-analyzer frame', timeout=45)


def quit_normally(terminal):
    terminal.send(b'\x11')
    terminal.until(lambda: terminal.process.poll() is not None, 'normal source/ledger checkpoint and exit')
    if terminal.process.returncode != 0:
        raise AssertionError('normal app exit failed')


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive app must be non-root')
class ChecksPTY(unittest.TestCase):
    def test_real_once_permission_unicode_source_apply_undo_restart_and_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-checks-pty-') as tmp:
                project, home, primary = check_fixture(Path(tmp))
                before = primary.read_bytes()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    open_checks(terminal)
                    terminal.screen_shows('No current result')
                    terminal.send(b'r')
                    terminal.screen_shows('explicit permission')
                    terminal.paste('ylyranaGi These pasted commands must not run.')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('No current result')
                    self.assertFalse((project/NOVEL).exists())
                    self.assertFalse((home/PERSONAL).exists())
                    terminal.send(b'ry')
                    ready(terminal, 1)
                    self.assertIn('Dialogue', terminal.screen.text)
                    self.assertFalse((project/NOVEL).exists(), 'once consent is not persisted')
                    self.assertFalse((home/PERSONAL).exists())
                    self.assertEqual(primary.read_bytes(), before)
                    self.assertEqual(records(project), [], 'analysis is not authored input/activity')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        (ROOT/'docs/evidence'/f'checks-pty-{width}x{height}.txt').write_text(terminal.screen.text+'\n')
                    terminal.send(b'a\x13')
                    changed = before.replace(b'a apple', b'an apple', 1)
                    terminal.until(lambda: primary.read_bytes() == changed and any(e['origin'] == 'replace' and e['counts']['operations'] for e in records(project)), 'explicit same-original grammar replacement/save and durable origin')
                    self.assertTrue(any(e['origin'] == 'replace' and e['counts']['operations'] for e in records(project)))
                    self.assertFalse(any(e['origin'] == 'typing' for e in records(project)))
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before and any(e['origin'] == 'undo' for e in records(project)), 'native single-step undo of exact UTF-8 source and durable replay')
                    self.assertTrue(any(e['origin'] == 'undo' for e in records(project)))
                    quit_normally(terminal)
                finally:
                    terminal.close()
                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    open_checks(restarted)
                    restarted.screen_shows('live OFF')
                    restarted.send(b'r')
                    restarted.screen_shows('explicit permission')
                    self.assertEqual(primary.read_bytes(), before)
                    self.assertFalse((project/NOVEL).exists())
                    self.assertFalse((home/PERSONAL).exists())
                    restarted.send(b'\x1b')
                    quit_normally(restarted)
                finally:
                    restarted.close()

    def test_actual_once_novel_personal_mutes_and_live_permission_survive_independent_processes(self):
        with tempfile.TemporaryDirectory(prefix='wd-check-scopes-') as tmp:
            base = Path(tmp)
            project, home, primary = check_fixture(base)
            before = primary.read_bytes()
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key'); open_checks(terminal)
                terminal.send(b'ry'); ready(terminal, 1)
                terminal.send(b'i'); ready(terminal, 0)
                self.assertFalse((project/NOVEL).exists())
                terminal.send(b'r'); ready(terminal, 1)
                terminal.send(b'n')
                terminal.until(lambda: (project/NOVEL).exists() and
                               json.loads((project/NOVEL).read_text())['options']['disabled_rules'], 'novel rule identifiers saved')
                terminal.send(b'r'); ready(terminal, 0)
                terminal.send(b'sxy')  # settings, clear novel mutes, explicit yes
                terminal.until(lambda: not json.loads((project/NOVEL).read_text())['options']['disabled_rules'], 'only novel mutes cleared')
                terminal.send(b'r'); ready(terminal, 1)
                terminal.send(b'Gy')
                terminal.until(lambda: (home/PERSONAL).exists() and
                               json.loads((home/PERSONAL).read_text())['muted_rules'], 'confirmed personal rule mute')
                self.assertIsNone(json.loads((home/PERSONAL).read_text())['allowed_runtime'])
                quit_normally(terminal)
            finally:
                terminal.close()
            # A new project with the SAME personal profile: no novel-local mute.
            second_base = base/'second'; second_base.mkdir()
            second, unused_home, second_primary = check_fixture(second_base)
            second_before = second_primary.read_bytes()
            restarted = TerminalProcess(second, home, 100, 30)
            try:
                restarted.shows('The Brass Key'); open_checks(restarted)
                restarted.send(b'r'); restarted.screen_shows('explicit permission')
                restarted.send(b'y'); ready(restarted, 0)
                self.assertFalse((second/NOVEL).exists())
                restarted.send(b'sXy')
                restarted.until(lambda: not json.loads((home/PERSONAL).read_text())['muted_rules'], 'personal mutes explicitly cleared')
                restarted.send(b'ly')
                restarted.until(lambda: (second/NOVEL).exists() and
                                json.loads((second/NOVEL).read_text())['live_consent'] is not None and
                                json.loads((home/PERSONAL).read_text())['allowed_runtime'] is not None,
                                'both personal and novel runtime permission saved')
                ready(restarted, 1)
                self.assertEqual(primary.read_bytes(), before)
                self.assertEqual(second_primary.read_bytes(), second_before)
                quit_normally(restarted)
            finally:
                restarted.close()
            # Even this previously-consented HOME cannot enable a copied novel
            # at another location without a local root/runtime permission.
            (project/NOVEL).write_bytes((second/NOVEL).read_bytes())
            imported = TerminalProcess(project, home, 80, 24)
            try:
                imported.shows('The Brass Key'); open_checks(imported)
                imported.screen_shows('live OFF'); imported.screen_shows('No current result')
                self.assertEqual(json.loads((home/PERSONAL).read_text())['allowed_runtime'],
                                 json.loads((second/NOVEL).read_text())['live_consent'])
                self.assertEqual(primary.read_bytes(), before)
                quit_normally(imported)
            finally:
                imported.close()
            third = TerminalProcess(second, home, 100, 30)
            try:
                third.shows('The Brass Key'); open_checks(third)
                third.screen_shows('live ON'); ready(third, 1)
                third.send(b'l'); third.screen_shows('Live checks off')
                self.assertIsNone(json.loads((second/NOVEL).read_text())['live_consent'])
                quit_normally(third)
            finally:
                third.close()
            for path in [project/NOVEL, second/NOVEL, home/PERSONAL]:
                value = path.read_text()
                self.assertNotIn('apple', value); self.assertNotIn('Café', value)
                self.assertNotIn('expected', value); self.assertNotIn('snippet', value)

    def test_future_metadata_and_external_backing_source_survive_real_analysis_and_rejected_actions(self):
        with tempfile.TemporaryDirectory(prefix='wd-check-future-') as tmp:
            project, home, primary = check_fixture(Path(tmp))
            metadata = project/NOVEL; metadata.parent.mkdir(parents=True, exist_ok=True)
            future = b'{"version":999,"unknown":{"original":"retain this data"}}'
            metadata.write_bytes(future)
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key'); open_checks(terminal)
                terminal.screen_shows('settings unavailable')
                terminal.send(b'ry'); ready(terminal, 1)
                before = primary.read_bytes()
                terminal.send(b'ns')  # mute fails, settings page is processing barrier
                terminal.screen_shows('Profile:')
                self.assertEqual(metadata.read_bytes(), future)
                terminal.send(b'\x1b')
                terminal.screen_shows('AnA')
                external = before + b'\nExternal original source revision.\n'
                primary.write_bytes(external)
                terminal.send(b'a')
                terminal.until(lambda: any(s in terminal.screen.text for s in
                               ['backing source changed', 'unresolved disk change', 'no current finding']),
                               'explicit stale/external-source action refused')
                self.assertEqual(primary.read_bytes(), external)
                self.assertEqual(metadata.read_bytes(), future)
                quit_normally(terminal)
            finally:
                terminal.close()
            audit = subprocess.run([str(BINARY), 'writingdeck', 'check-config', str(project)],
                                   env=os.environ | {'HOME': str(home)}, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(audit.returncode, 0, 'read-only audit must recognize unknown diagnostic schema')
            self.assertEqual(metadata.read_bytes(), future)
            self.assertEqual(primary.read_bytes(), external)

    def test_changed_original_while_a_real_worker_runs_is_rechecked_in_new_source_coordinates(self):
        with tempfile.TemporaryDirectory(prefix='wd-check-stale-') as tmp:
            project, home, primary = check_fixture(Path(tmp))
            before = primary.read_bytes()
            terminal = TerminalProcess(project, home, 100, 30)
            prefix = 'Changed '
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
                open_checks(terminal); terminal.send(b'ry')
                terminal.screen_shows('Local check running')
                terminal.send(b'\x1b')
                terminal.until(lambda: 'Writing checks' not in terminal.screen.text and 'Café' in terminal.screen.text, 'Esc returned to the original editor, not an Alt-letter sequence')
                terminal.send(prefix.encode())
                terminal.screen_shows(prefix.strip())
                open_checks(terminal)
                terminal.screen_shows('No current result')
                terminal.send(b'r'); ready(terminal, 1)
                column = TEXT.index('a apple') + len(prefix) + 1
                self.assertIn(f'1:{column}', terminal.screen.text)
                terminal.send(b'a\x13')
                expected = before.replace(TEXT.encode(), (prefix + TEXT).replace('a apple', 'an apple', 1).encode(), 1)
                terminal.until(lambda: primary.read_bytes() == expected, 'new-revision source coordinates, not old article offset')
                quit_normally(terminal)
            finally:
                terminal.close()

    def test_supplemental_grammar_lesson_preserves_spelling_history_and_needs_a_real_applied_fix(self):
        with tempfile.TemporaryDirectory(prefix='wd-grammar-lesson-') as tmp:
            project, home, primary = check_fixture(Path(tmp))
            progress = project/'.grimoire/writingdeck/tutorial.json'
            progress.parent.mkdir(parents=True, exist_ok=True)
            # Historical fixture, not evidence of a newly performed lesson 26.
            progress.write_text(json.dumps({'version': 1, 'active': None, 'completed': [26], 'skipped': []}))
            before = primary.read_bytes()
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
                terminal.send(b'\x0bInteractive tutorial\r'); terminal.screen_shows('Interactive tutorial')
                terminal.send(b'\x1b[F\r'); terminal.screen_shows('Lesson 40')
                terminal.send(b'\r')
                terminal.until(lambda: json.loads(progress.read_text())['active'] == 40, 'real demo lesson started, not auto-completed')
                self.assertEqual(primary.read_bytes(), before)
                open_checks(terminal); terminal.send(b'ry'); ready(terminal, 1)
                self.assertEqual(json.loads(progress.read_text())['completed'], [26])
                terminal.send(b'\r')
                terminal.until(lambda: 'Writing checks' not in terminal.screen.text, 'navigation to source, not a replacement')
                self.assertEqual(json.loads(progress.read_text())['completed'], [26])
                open_checks(terminal); terminal.send(b'a\x13')
                terminal.until(lambda: json.loads(progress.read_text())['completed'] == [26, 40]
                               and primary.read_bytes() == before.replace(b'a apple', b'an apple', 1),
                               'genuine current grammar replacement and preserved historical spelling')
                terminal.send(b'\x1a\x13')
                terminal.until(lambda: primary.read_bytes() == before, 'tutorial edit remains normally undoable')
                quit_normally(terminal)
            finally:
                terminal.close()
            restarted = TerminalProcess(project, home, 100, 30)
            try:
                restarted.shows('The Brass Key')
                restarted.send(b'\x0bInteractive tutorial\r'); restarted.screen_shows('2 / 40 completed')
                self.assertEqual(primary.read_bytes(), before)
                self.assertEqual(json.loads(progress.read_text())['completed'], [26, 40])
                quit_normally(restarted)
            finally:
                restarted.close()
