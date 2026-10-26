"""Actual non-root binary/PTY: offline Spark, originals, failures and restart.

Only disposable demo/HOME fixtures; no host service, credentials, network,
user manuscript or host keymap. The shared screen shim is not a full emulator.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


def json_file(path):
    return json.loads(path.read_text()) if path.is_file() else None


def action(terminal, index):
    terminal.send(b'a' + b'\x1b[B'*index + b'\r')


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing app must be non-root')
class SparkPTY(unittest.TestCase):
    def test_real_modes_rerolls_paste_guard_favorites_daily_saved_original_and_loaded_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-spark-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                originals = {p: p.read_bytes() for p in (project/'manuscript').rglob('*.md')}
                meta = project/'.grimoire/writingdeck/spark/state.json'

                def state():
                    return json_file(meta)

                def current():
                    return (state() or {}).get('current')

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bStory Spark\r')
                    terminal.screen_shows('1047 original plots / 84 exercises')
                    terminal.paste('1234sxafper Commands in pasted prose must never run.')
                    terminal.send(b'v')
                    terminal.screen_shows('Spark provenance')
                    self.assertFalse(meta.exists(), 'paste must not generate or write metadata')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Choose 1, 2, 3 or 4')
                    terminal.send(b'1')
                    terminal.until(lambda: current() is not None and current()['mode'] == 'plot', 'real plot-only generation')
                    plot = current()
                    self.assertIsNone(plot['exercise'])
                    terminal.send(b's')
                    terminal.until(lambda: state()['exports'] and state()['exports'][0]['phase'] == 'ready', 'real saved plot note')
                    terminal.send(b'2')
                    terminal.until(lambda: current()['mode'] == 'exercise', 'independently useful exercise')
                    self.assertIsNone(current()['plot'])
                    terminal.send(b'3')
                    terminal.until(lambda: current()['mode'] == 'combined', 'compatible combined result')
                    combined = current()
                    terminal.send(b'p')
                    terminal.until(lambda: current()['id'] != combined['id'], 'plot-only reroll')
                    p = current()
                    self.assertEqual(p['exercise'], combined['exercise'])
                    self.assertNotEqual(p['plot'], combined['plot'])
                    terminal.send(b'e')
                    terminal.until(lambda: current()['id'] != p['id'], 'exercise-only reroll')
                    e = current()
                    self.assertEqual(e['plot'], p['plot'])
                    self.assertNotEqual(e['exercise'], p['exercise'])
                    terminal.send(b'r')
                    terminal.until(lambda: current()['id'] != e['id'], 'joint reroll')
                    final = current()
                    self.assertNotEqual(final['plot'], e['plot'])
                    self.assertNotEqual(final['exercise'], e['exercise'])
                    terminal.send(b'fs')
                    saved = project/'notes/sparks'/f'{final["id"]}.md'
                    terminal.until(lambda: final['id'] in state()['favorites'] and saved.is_file() and
                                   any(r['result']['id'] == final['id'] and r['phase'] == 'ready' for r in state()['exports']),
                                   'favorite and original Markdown note for the same final result')
                    saved_bytes = saved.read_bytes()
                    # A file receipt can lead rendering. Await the exact new
                    # component IDs AND completed frame output, not stale text
                    # left on screen during a differential terminal write.
                    terminal.until(lambda: f"PLOT [{final['plot']['id']}]" in terminal.screen.text and
                                   f"EXERCISE [{final['exercise']['id']}]" in terminal.screen.text and
                                   f"Spark save · notes/sparks/{final['id']}" in terminal.screen.text and
                                   terminal.output.endswith(b'\x1b[?25l'), 'completed same-result Spark frame')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        (ROOT/'docs/evidence'/f'spark-pty-{width}x{height}.txt').write_text(terminal.screen.text+'\n')
                    terminal.send(b'4')
                    terminal.until(lambda: current()['mode'] == 'daily', 'persisted local-civil-date daily result')
                    daily = current()
                    daily_file = project/'.grimoire/writingdeck/spark/daily'/f'{daily["date"]}.json'
                    daily_bytes = daily_file.read_bytes()
                    terminal.send(b'p')
                    terminal.until(lambda: current()['mode'] == 'combined' and current()['derived_from'] == daily['id'], 'reroll branches off daily')
                    terminal.send(b'4')
                    terminal.until(lambda: current()['id'] == daily['id'], 'fixed daily restored without another draw')
                    self.assertEqual(daily_file.read_bytes(), daily_bytes)
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal Spark quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    restarted.send(b'\x0bStory Spark\r')
                    restarted.screen_shows('1047 original plots / 84 exercises')
                    self.assertEqual(current(), daily)
                    restarted.send(b'F\r')
                    restarted.until(lambda: current()['id'] == final['id'], 'recall exact favorite after separate-process restart')
                    self.assertEqual(current(), final)
                    restarted.send(b's')
                    restarted.screen_shows('Spark save · notes/sparks')
                    self.assertEqual(saved.read_bytes(), saved_bytes)
                    restarted.send(b'\x0bSaved Story Spark '+final['id'].encode()+b'\r')
                    restarted.paste('Loaded Spark original interpretation. ')
                    restarted.send(b'\x13')
                    restarted.until(lambda: b'Loaded Spark original interpretation.' in saved.read_bytes(), 'edit actual loaded saved original')
                    restarted.send(b'\x1a\x13')
                    restarted.until(lambda: saved.read_bytes() == saved_bytes, 'loaded-source undo restores exact saved original')
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal restarted Spark quit')
                    self.assertEqual(restarted.process.returncode, 0)
                    self.assertEqual(daily_file.read_bytes(), daily_bytes)
                    self.assertNotIn('Loaded Spark original interpretation.', meta.read_text())
                    for path, data in originals.items():
                        self.assertEqual(path.read_bytes(), data)
                finally:
                    restarted.close()
                env = os.environ | {'HOME': str(home), 'XDG_DATA_HOME': str(home/'data'), 'XDG_CONFIG_HOME': str(home/'config')}
                before = {p: p.read_bytes() for p in (project/'.grimoire/writingdeck').rglob('*.json')}
                checked = subprocess.run([str(BINARY), 'writingdeck', 'check-config', str(project)], env=env, capture_output=True, text=True, timeout=15)
                self.assertEqual(checked.returncode, 0, checked.stderr)
                self.assertIn('Spark schema 1 validated', checked.stdout)
                self.assertEqual({p: p.read_bytes() for p in before}, before)

    def test_prepared_export_failure_visible_status_real_retry_and_empty_independent_scene(self):
        with tempfile.TemporaryDirectory(prefix='wd-spark-export-pty-') as temporary:
            project, home, primary = fixture(Path(temporary))
            original = primary.read_bytes()
            blocker = project/'notes/sparks'
            blocker.write_text('Original blocker; never overwrite.\n')
            meta = project/'.grimoire/writingdeck/spark/state.json'
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bStory Spark\r1s')
                terminal.screen_shows('Spark action not completed')
                terminal.until(lambda: (json_file(meta) or {}).get('exports'), 'durable prepared export receipt')
                self.assertEqual(json_file(meta)['exports'][0]['phase'], 'prepared')
                self.assertEqual(blocker.read_text(), 'Original blocker; never overwrite.\n')
                self.assertEqual(primary.read_bytes(), original)
                blocker.unlink()
                terminal.send(b's')
                terminal.until(lambda: json_file(meta)['exports'][0]['phase'] == 'ready', 'real idempotent export retry')
                self.assertEqual(len(json_file(meta)['exports']), 1)
                r = json_file(meta)['current']
                note = project/'notes/sparks'/f'{r["id"]}.md'
                self.assertTrue(note.is_file())
                action(terminal, 7)
                scene = project/'manuscript'/f'98-Spark-{r["id"]}-experiment.md'
                terminal.until(scene.is_file, 'real independent experimental scene created')
                self.assertIn('compile: false', scene.read_text())
                self.assertEqual(scene.read_text().split('---\n', 2)[2].strip(), '')
                terminal.paste('Independent experimental original. ')
                terminal.send(b'\x13')
                terminal.until(lambda: b'Independent experimental original.' in scene.read_bytes(), 'experimental original editor opened and saved')
                self.assertNotIn(b'Independent experimental original.', note.read_bytes())
                self.assertEqual(primary.read_bytes(), original)
                terminal.send(b'\x11')
                terminal.until(lambda: terminal.process.poll() is not None, 'normal scene quit')
                self.assertEqual(terminal.process.returncode, 0)
            finally:
                terminal.close()

    def test_corrupt_future_state_is_displayed_and_preserved_in_ui_and_read_only_cli(self):
        for data in (b'{not-json\n', b'{"version":999,"favorites":"opaque original"}\n'):
            with self.subTest(data=data), tempfile.TemporaryDirectory(prefix='wd-spark-future-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                original = primary.read_bytes()
                meta = project/'.grimoire/writingdeck/spark/state.json'
                meta.parent.mkdir(parents=True, exist_ok=True)
                meta.write_bytes(data)
                terminal = TerminalProcess(project, home, 80, 24)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bStory Spark\r3')
                    terminal.screen_shows('Spark unavailable')
                    terminal.screen_shows('Spark action not completed')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit with preserved optional Spark state')
                    self.assertEqual(terminal.process.returncode, 0)
                    self.assertEqual(meta.read_bytes(), data)
                    self.assertEqual(primary.read_bytes(), original)
                finally:
                    terminal.close()
                env = os.environ | {'HOME': str(home), 'XDG_DATA_HOME': str(home/'data'), 'XDG_CONFIG_HOME': str(home/'config')}
                checked = subprocess.run([str(BINARY), 'writingdeck', 'check-config', str(project)], env=env, capture_output=True, text=True, timeout=15)
                self.assertNotEqual(checked.returncode, 0)
                self.assertEqual(meta.read_bytes(), data)

    def test_real_spark_linked_active_sprint_normal_quit_restart_and_metadata_publication_failure(self):
        for failure in (False, True):
            with self.subTest(external_failure=failure), tempfile.TemporaryDirectory(prefix='wd-spark-sprint-pty-') as temporary:
                project, home, primary = fixture(Path(temporary))
                original = primary.read_bytes()
                meta = project/'.grimoire/writingdeck/productivity.json'
                terminal = TerminalProcess(project, home, 80, 24)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bStory Spark\r3')
                    spark_meta = project/'.grimoire/writingdeck/spark/state.json'
                    terminal.until(lambda: (json_file(spark_meta) or {}).get('current'), 'real selected sprint source')
                    reference = json_file(spark_meta)['current']
                    action(terminal, 8)
                    terminal.until(lambda: (json_file(meta) or {}).get('sprint') is not None, 'real timed sprint metadata')
                    terminal.screen_shows('Spark source')
                    sprint = json_file(meta)['sprint']
                    self.assertEqual(sprint['kind'], 'timed')
                    self.assertEqual(sprint['spark'], reference)
                    self.assertLessEqual(sprint['target'], 30, 'suggested duration is minutes, not seconds')
                    if failure:
                        saved = meta.read_bytes()
                        external = b'{"version":999,"opaque":"retain original"}\n'
                        meta.write_bytes(external)
                        terminal.send(b'\x11')
                        terminal.screen_shows('Q again to quit anyway')
                        self.assertIsNone(terminal.process.poll())
                        self.assertEqual(meta.read_bytes(), external)
                        self.assertEqual(primary.read_bytes(), original)
                        meta.write_bytes(saved)
                        # A different actual key disarms forced quit. Restoration
                        # is not permission to bypass the genuine checkpoint.
                        terminal.send(b'\x0bAdvanced sprints\r')
                        terminal.screen_shows('Spark source')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal active-sprint checkpoint and quit')
                    self.assertEqual(terminal.process.returncode, 0)
                    saved_sprint = json_file(meta)['sprint']
                    self.assertGreater(saved_sprint['elapsed_ms'], 0)
                    self.assertEqual(saved_sprint['spark'], reference)
                    self.assertEqual(primary.read_bytes(), original)
                finally:
                    terminal.close()
                restarted = TerminalProcess(project, home, 80, 24)
                try:
                    restarted.shows('The Brass Key')
                    restarted.send(b'\x0bAdvanced sprints\r')
                    restarted.screen_shows('paused (resume explicitly)')
                    self.assertEqual(json_file(meta)['sprint']['elapsed_ms'], saved_sprint['elapsed_ms'])
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal loaded active-sprint quit without offline credit')
                    self.assertEqual(restarted.process.returncode, 0)
                    self.assertEqual(json_file(meta)['sprint']['elapsed_ms'], saved_sprint['elapsed_ms'])
                    self.assertEqual(primary.read_bytes(), original)
                finally:
                    restarted.close()
