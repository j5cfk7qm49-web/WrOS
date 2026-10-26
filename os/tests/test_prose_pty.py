"""Real no-root compiled prose/consent/receipts/voice workflows and originals.

Only new demo/HOME and deliberately synthetic material. Installed pinned tools,
not fabricated NLP reports. ANSI cells are not a full terminal emulator. These
cases are not physical, literary, authorship or independent security evidence.
"""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_checks_pty import PERSONAL, NOVEL, quit_normally
from test_journal_pty import fixture
from test_productivity_pty import records
from test_secondary_pty import BINARY, ROOT, TerminalProcess

TEXT = 'Café 🦊 e\u0301. We waited in order to see. He could of gone.\n'
REVIEW = '.grimoire/writingdeck/review.json'
VOICE = '.grimoire/writingdeck/voice-profiles.json'
TUTORIAL = '.grimoire/writingdeck/tutorial.json'
FICTION = '.grimoire/writingdeck/prose-settings.json'
RUNTIME = Path(os.environ.get('WRITINGDECK_TOOLS_PYTHON', ROOT/'build/prose-runtime/runtime/bin/python3'))


def prose_fixture(base, text=TEXT):
    project, home, primary = fixture(base)
    head, delimiter, _ = primary.read_text().partition('\n---\n')
    if not delimiter:
        raise AssertionError('original demo frontmatter missing')
    primary.write_text(head + '\n---\n\n' + text)
    return project, home, primary


def open_prose(terminal):
    terminal.send(b'\x0bProse and revision dashboard\r')
    terminal.screen_shows('Prose and revision dashboard')


def run_prose(terminal, command=b'r'):
    terminal.send(command)
    # A new pending frame, not the old cached completion, is the input barrier.
    terminal.screen_shows('Explicit local analysis pending')
    terminal.until(lambda: 'Current source results ready.' in terminal.screen.text and
                   'native Complete' in terminal.screen.text and
                   terminal.output.endswith(b'\x1b[?25l'),
                   'completed actual prose-tool frame', timeout=60)


def selected_line(terminal):
    return next((line for line in terminal.screen.text.splitlines() if line.strip('│ ').startswith('›')), '')


def inspect_rule(terminal, rule):
    terminal.send(b'\x1b[H')
    terminal.until(lambda: bool(selected_line(terminal)), 'actual selected queue row')
    for _ in range(40):
        previous = selected_line(terminal)
        if rule in previous:
            terminal.send(b'\r')
            terminal.screen_shows('Source selection preview:')
            return
        terminal.send(b'\x1b[B')
        terminal.until(lambda: selected_line(terminal) != previous,
                       f'next ranked row while looking for {rule}')
    raise AssertionError('genuine engine finding not found: ' + rule)


def selected_original(terminal, primary):
    terminal.send(b'\x1b[H')
    terminal.until(lambda: bool(selected_line(terminal)), 'explicit source selection row')
    for _ in range(20):
        previous = selected_line(terminal)
        if primary.name in previous:
            terminal.send(b' \r')
            terminal.screen_shows('Frozen reference label:')
            return
        terminal.send(b'\x1b[B')
        terminal.until(lambda: selected_line(terminal) != previous, 'next saved source candidate')
    raise AssertionError('saved original scene absent from explicit chooser')


def completed(project, lesson):
    path = project / TUTORIAL
    return path.is_file() and lesson in json.loads(path.read_text())['completed']


@unittest.skipUnless(BINARY.is_file(), 'build the current no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'the interactive writing app must be non-root')
class ProsePTY(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not RUNTIME.is_file():
            raise AssertionError('provision the exact local runtime; genuine prose tests must not silently skip')

    def test_real_native_original_replace_undo_paste_guards_restart_and_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-prose-pty-') as tmp:
                project, home, primary = prose_fixture(Path(tmp))
                before = primary.read_bytes()
                with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                    terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    open_prose(terminal)
                    terminal.screen_shows('No current result')
                    terminal.paste('adymwx These pasted commands are not actions.')
                    terminal.send(b'\x18\x1a\x16p')
                    terminal.screen_shows('Revision')
                    run_prose(terminal)
                    self.assertEqual(primary.read_bytes(), before)
                    self.assertEqual(records(project), [], 'analysis/review are not authored activity')
                    self.assertFalse((project/REVIEW).exists())
                    terminal.send(b'\r')
                    terminal.screen_shows('REVIEW QUEUE')
                    inspect_rule(terminal, 'wordy-phrase')
                    terminal.screen_shows('in order to')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        (ROOT/'docs/evidence'/f'prose-pty-{width}x{height}.txt').write_text(terminal.screen.text + '\n')
                    terminal.send(b'a')
                    terminal.screen_shows('One explicit original-source Replace.')
                    # An Escape immediately followed by another byte is a
                    # terminal Alt chord, not an independent Escape key.
                    terminal.send(b'\x1b')
                    terminal.until(lambda: 'Prose and revision dashboard' not in terminal.screen.text,
                                   'actual close to the native editor before Undo')
                    terminal.send(b'\x13')
                    changed = before.replace(b'in order to', b'to', 1)
                    terminal.until(lambda: primary.read_bytes() == changed and any(e['origin'] == 'replace' for e in records(project)),
                                   'explicit original-source Replace/save and durable true origin')
                    self.assertFalse(any(e['origin'] == 'typing' for e in records(project)))
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before and any(e['origin'] == 'undo' for e in records(project)),
                                   'exact UTF-8 native Undo/save, not an alternate source buffer')
                    quit_normally(terminal)
                finally:
                    terminal.close()
                with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                    restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    open_prose(restarted)
                    restarted.screen_shows('Revision')
                    restarted.screen_shows('No current result')
                    self.assertEqual(primary.read_bytes(), before)
                    self.assertIsNone(json.loads((project/NOVEL).read_text())['live_consent'])
                    self.assertFalse((home/PERSONAL).exists(), 'ordinary prose is not personal model permission')
                    quit_normally(restarted)
                finally:
                    restarted.close()

    def test_real_nabokov_once_direction_only_and_ordinary_r_does_not_reuse_consent(self):
        with tempfile.TemporaryDirectory(prefix='wd-prose-model-pty-') as tmp:
            project, home, primary = prose_fixture(Path(tmp), 'Café 🦊 e\u0301. The gate was opened by Mara.\n')
            before = primary.read_bytes()
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                terminal = TerminalProcess(project, home, 120, 40)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bThe Brass Key\r')
                open_prose(terminal)
                terminal.send(b'pd')
                terminal.screen_shows('ALLOW ONCE?')
                terminal.paste('y This pasted permission must not run.')
                terminal.send(b'\x1b')
                terminal.screen_shows('No current result')
                terminal.send(b'd')
                terminal.screen_shows('ALLOW ONCE?')
                run_prose(terminal, b'y')
                terminal.send(b'\r')
                terminal.screen_shows('REVIEW QUEUE')
                inspect_rule(terminal, 'NB302')
                terminal.screen_shows('was opened by Mara')
                terminal.screen_shows('No literal replacement')
                terminal.send(b'a')
                terminal.screen_shows('no such diagnostic replacement')
                self.assertEqual(primary.read_bytes(), before)
                open_prose(terminal)
                run_prose(terminal)
                terminal.screen_shows('Nabokov: not run')
                self.assertIsNone(json.loads((project/NOVEL).read_text())['live_consent'])
                self.assertFalse((home/PERSONAL).exists())
                self.assertEqual(records(project), [])
                self.assertEqual(primary.read_bytes(), before)
                quit_normally(terminal)
            finally:
                terminal.close()

    def test_real_review_receipts_survive_restart_and_future_metadata_and_missing_runtime_are_not_clean(self):
        with tempfile.TemporaryDirectory(prefix='wd-prose-receipt-pty-') as tmp:
            project, home, primary = prose_fixture(Path(tmp))
            before = primary.read_bytes()
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
                open_prose(terminal); run_prose(terminal)
                terminal.send(b'\r'); terminal.screen_shows('REVIEW QUEUE')
                inspect_rule(terminal, 'spelling.ve_of')
                terminal.send(b'w')
                terminal.until(lambda: (project/REVIEW).exists(), 'guarded source/hash receipt')
                receipt = json.loads((project/REVIEW).read_text())['receipts'][0]
                self.assertEqual(receipt['state'], 'reviewed')
                self.assertNotIn('could of', (project/REVIEW).read_text())
                self.assertNotIn('Café', (project/REVIEW).read_text())
                self.assertEqual(primary.read_bytes(), before)
                quit_normally(terminal)
            finally:
                terminal.close()
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                restarted = TerminalProcess(project, home, 100, 30)
            try:
                restarted.shows('The Brass Key'); open_prose(restarted); run_prose(restarted)
                restarted.send(b'\r\t'); restarted.screen_shows('All filter')
                inspect_rule(restarted, 'spelling.ve_of')
                restarted.screen_shows('Reviewed')
                self.assertEqual(json.loads((project/REVIEW).read_text())['receipts'][0], receipt)
                quit_normally(restarted)
            finally:
                restarted.close()
            future = b'{"schema_version":999,"opaque":{"keep":"unchanged"}}\n'
            (project/REVIEW).write_bytes(future)
            (project/FICTION).write_bytes(future)
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': '/no-such-writingdeck-prose-runtime'}):
                failed = TerminalProcess(project, home, 120, 40)
            try:
                failed.shows('The Brass Key'); open_prose(failed)
                failed.send(b'f')
                failed.screen_shows('prose settings unavailable')
                failed.send(b'p')
                failed.screen_shows('Revision')
                run_prose(failed)
                failed.screen_shows('Unavailable')
                failed.send(b'\r'); failed.screen_shows('REVIEW QUEUE')
                inspect_rule(failed, 'wordy-phrase')
                failed.send(b'w')
                failed.screen_shows('review metadata unavailable')
                self.assertEqual((project/REVIEW).read_bytes(), future)
                self.assertEqual((project/FICTION).read_bytes(), future)
                self.assertEqual(primary.read_bytes(), before)
                self.assertEqual(records(project), [])
                quit_normally(failed)
            finally:
                failed.close()

    def test_material_reference_prepare_save_restart_actual_comparison_lesson30_and_metadata_only_delete(self):
        # Synthetic threshold/flow fixture, not representative human voice.
        words = ['lexeme' + chr(97+i//26) + chr(97+i%26) for i in range(200)]
        material = '\n\n'.join(' '.join(words[(i+j)%200] for j in range(130)) + '.' for i in range(40)) + '\n'
        with tempfile.TemporaryDirectory(prefix='wd-prose-voice-pty-') as tmp:
            project, home, primary = prose_fixture(Path(tmp), material)
            before = primary.read_bytes()
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                terminal = TerminalProcess(project, home, 120, 40)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
                terminal.send(b'\x0bInteractive tutorial\r\x1b[H' + b'\x1b[B'*29 + b'\r\r')
                terminal.until(lambda: (project/TUTORIAL).exists() and json.loads((project/TUTORIAL).read_text())['active'] == 30,
                               'actual start of original lesson30, not a completion fixture')
                terminal.screen_shows('FROZEN SELECTED-REFERENCE')
                terminal.send(b'n')
                terminal.screen_shows('SELECT OWN SAVED ORIGINALS')
                selected_original(terminal, primary)
                terminal.send(b'\x1b[3~')
                terminal.paste('PTY frozen reference')
                terminal.send(b'\r')
                terminal.screen_shows('READY, NOT SAVED:')
                self.assertFalse((project/VOICE).exists())
                self.assertFalse(completed(project, 30))
                self.assertEqual(primary.read_bytes(), before)
                terminal.send(b's')
                terminal.until(lambda: (project/VOICE).exists(), 'explicit sufficient-material frozen reference save')
                frozen = json.loads((project/VOICE).read_text())
                self.assertEqual(len(frozen['profiles']), 1)
                self.assertGreaterEqual(frozen['profiles'][0]['metrics']['analysis_words'], 5000)
                self.assertNotIn('lexeme', (project/VOICE).read_text(), 'baseline stores numeric/reference data, not manuscript vocabulary')
                self.assertFalse(completed(project, 30), 'prepare/save/activate alone is not comparison')
                self.assertEqual(primary.read_bytes(), before)
                quit_normally(terminal)
            finally:
                terminal.close()
            with patch.dict(os.environ, {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME)}):
                restarted = TerminalProcess(project, home, 120, 40)
            try:
                restarted.shows('The Brass Key'); restarted.send(b'\x0bThe Brass Key\r')
                restarted.send(b'\x01')
                restarted.screen_shows('whole scene is selected')
                current = 'I saw the sea. We waited for her.\n\n'*60
                restarted.paste(current)
                restarted.send(b'\x13')
                restarted.until(lambda: current.encode() in primary.read_bytes(), 'actual native current-source replacement/save for comparison')
                current_bytes = primary.read_bytes()
                self.assertEqual(json.loads((project/VOICE).read_text()), frozen, 'reference never silently live-rebases')
                open_prose(restarted); run_prose(restarted)
                restarted.send(b'vc')
                restarted.until(lambda: completed(project, 30), 'lesson30 requires actual sufficient-reference/current comparison')
                restarted.screen_shows('Reviewed actual reference comparison')
                self.assertEqual(primary.read_bytes(), current_bytes)
                restarted.send(b'D')
                restarted.screen_shows('Remove frozen voice metadata')
                restarted.send(b'y')
                restarted.until(lambda: not json.loads((project/VOICE).read_text())['profiles'], 'explicit metadata-only reference deletion')
                self.assertEqual(json.loads((project/VOICE).read_text())['next_id'], 2)
                self.assertEqual(primary.read_bytes(), current_bytes)
                self.assertFalse(any(e['origin'] == 'typing' for e in records(project)))
                quit_normally(restarted)
            finally:
                restarted.close()
