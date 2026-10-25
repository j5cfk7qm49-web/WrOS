"""Actual unprivileged binary/PTY pin controls, source custody and restart.

No host keymaps, system services, network, real novel, or physical disks touched.
The shared harness answers only the startup terminal cursor-position query.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_secondary_pty import BINARY, TerminalProcess


def fixture(base):
    project, home = base / 'demo', base / 'home'
    home.mkdir()
    env = os.environ | {
        'HOME': str(home), 'XDG_DATA_HOME': str(home / 'data'),
        'XDG_CONFIG_HOME': str(home / 'config'),
    }
    subprocess.run(
        [str(BINARY), 'writingdeck', 'tutorial-demo', str(project)],
        env=env, capture_output=True, text=True, check=True, timeout=15,
    )
    source = project / 'notes/pin-source.md'
    source.write_text("---\ntitle: Pin Source\nunknown: 'raw: value' # unchanged\n---\n\n## Anchor\n" + ''.join(f'Original reference paragraph {i}.\n' for i in range(60)))
    primary = next(p for p in (project / 'manuscript').rglob('*.md') if 'The Brass Key' in p.read_text())
    return project, home, source, primary


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing application must not run as root')
class PinsPTY(unittest.TestCase):
    def test_real_pins_source_edit_independent_scroll_archive_paste_guard_and_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='writingdeck-pins-pty-') as temporary:
                project, home, source, primary = fixture(Path(temporary))
                original_source, original_primary = source.read_bytes(), primary.read_bytes()
                path = project / '.grimoire/writingdeck/pins.json'

                def state():
                    return json.loads(path.read_text())

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bPinboard\r')  # Ctrl-K, actual palette, Enter
                    terminal.send(b'nPin Source\r')
                    terminal.until(lambda: path.is_file() and len(state()['pins']) == 1, 'persistent first reference')
                    self.assertEqual(state()['pins'][0]['source'], 'notes/pin-source.md')
                    terminal.send(b'h\x1b[6~')  # capture real heading + terminal PageDown
                    terminal.until(lambda: state()['pins'][0]['scroll'] == 3, 'anchored card scrolling')
                    self.assertEqual(state()['pins'][0]['anchor'], {'kind': 'heading', 'text': 'Anchor'})
                    self.assertEqual(state()['pins'][0]['mode'], 'anchored')
                    self.assertEqual(source.read_bytes(), original_source)
                    self.assertEqual(primary.read_bytes(), original_primary)
                    self.assertNotIn('Original reference paragraph', path.read_text())
                    terminal.send(b'\x1b[H')  # actual terminal Home, not an app method
                    terminal.until(lambda: state()['pins'][0]['scroll'] == 0, 'return to unique heading anchor')
                    before_paste = state()
                    terminal.paste('dauJKhqfbn Commands must not run from pasted prose.')
                    # Ratatui emits differential runs, not whole title words.
                    # A subsequent real scroll is the processing barrier; if
                    # pasted commands ran, this card/collection would change.
                    terminal.send(b'\x1b[6~')
                    terminal.until(lambda: state()['pins'][0]['scroll'] == 3, 'paste ignored before next real scroll')
                    after_barrier = state()
                    after_barrier['pins'][0]['scroll'] = 0
                    self.assertEqual(after_barrier, before_paste)
                    terminal.send(b'\x1b[H')
                    terminal.until(lambda: state()['pins'][0]['scroll'] == 0, 'restore anchor after paste barrier')
                    terminal.send(b'\r')  # explicitly edit the original source beside Primary
                    terminal.paste('PTY pin source edit.\n')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'PTY pin source edit.' in source.read_bytes(), 'guarded source save')
                    self.assertIn(b"unknown: 'raw: value' # unchanged", source.read_bytes())
                    self.assertEqual(primary.read_bytes(), original_primary)
                    source_saved = source.read_bytes()
                    terminal.output = b''
                    terminal.send(b'\x0bPinboard\r')
                    terminal.send(b'nPin Source\r')
                    terminal.until(lambda: len(state()['pins']) == 2, 'second card of same source')
                    terminal.send(b'\x1b[6~')
                    terminal.until(lambda: state()['pins'][1]['scroll'] == 3, 'independent second card scroll')
                    self.assertEqual(state()['pins'][0]['scroll'], 0)
                    self.assertEqual(state()['pins'][0]['source'], state()['pins'][1]['source'])
                    terminal.send(b'a')
                    terminal.until(lambda: state()['pins'][1]['life'] == 'archived', 'archive card without moving source')
                    terminal.send(b'\td')
                    terminal.until(lambda: state()['pins'][1]['life'] == 'trashed', 'recoverable card trash')
                    terminal.send(b'\tu')
                    terminal.until(lambda: state()['pins'][1]['life'] == 'active', 'restore card metadata')
                    self.assertEqual(source.read_bytes(), source_saved)
                    persisted = state()
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal saved quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.shows('The Brass Key')
                    restarted.send(b'\x0bPinboard\r\x1b[6~')
                    restarted.until(lambda: state()['pins'][1]['scroll'] == 6, 'restart loaded the selected card and its prior scroll')
                    self.assertEqual(state()['pins'][0]['scroll'], 0)
                    restarted.send(b'\x1b[5~')
                    restarted.until(lambda: state()['pins'][1]['scroll'] == 3, 'restore persisted scroll after restart proof')
                    self.assertEqual(state(), persisted)
                    self.assertEqual(source.read_bytes(), source_saved)
                    self.assertEqual(primary.read_bytes(), original_primary)
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal restart quit')
                    self.assertEqual(restarted.process.returncode, 0)
                finally:
                    restarted.close()

    def test_corrupt_metadata_is_reported_and_never_reset_by_actual_picker(self):
        with tempfile.TemporaryDirectory(prefix='writingdeck-pins-bad-pty-') as temporary:
            project, home, source, primary = fixture(Path(temporary))
            original_source, original_primary = source.read_bytes(), primary.read_bytes()
            path = project / '.grimoire/writingdeck/pins.json'
            path.parent.mkdir(parents=True, exist_ok=True)
            original = b'{"version":999,"future_data":"must survive"}\n'
            path.write_bytes(original)
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bPinboard\r')
                terminal.shows('Pins metadata preserved')
                terminal.send(b'nPin Source\r')
                # Graceful quit is the causal processing barrier for the
                # preceding picker attempt, not a raw differential-title match.
                terminal.send(b'\x11')
                terminal.until(lambda: terminal.process.poll() is not None, 'normal quit after refused metadata overwrite')
                self.assertEqual(terminal.process.returncode, 0)
                self.assertEqual(path.read_bytes(), original)
                self.assertEqual(source.read_bytes(), original_source)
                self.assertEqual(primary.read_bytes(), original_primary)
            finally:
                terminal.close()


if __name__ == '__main__':
    unittest.main()
