"""Real unprivileged binary + PTY, not a simulated editor or screenshot.

Only fresh tutorial fixtures and an isolated HOME are touched. These tests do
not set a host keymap, start networking, or launch the application as root.
"""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unittest

from terminal_screen import Screen

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get('GRIMOIRE_BINARY', ROOT / 'target/debug/grimoire'))


class TerminalProcess:
    def __init__(self, project, home, width, height):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        self.output = b''
        self.query_tail = b''
        self.screen = Screen(width, height)
        env = os.environ | {
            'HOME': str(home), 'XDG_DATA_HOME': str(home / 'data'),
            'XDG_CONFIG_HOME': str(home / 'config'), 'TERM': 'xterm-256color',
            'LANG': 'C.UTF-8',
        }
        env.pop('WRITINGDECK_OS', None)

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        try:
            self.process = subprocess.Popen(
                [str(BINARY), str(project)], stdin=slave, stdout=slave, stderr=slave,
                env=env, cwd=home, preexec_fn=controlling_terminal,
            )
        finally:
            os.close(slave)

    def close(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                self.process.wait(timeout=4)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=4)
        os.close(self.master)

    def send(self, data):
        os.write(self.master, data)

    def paste(self, text):
        self.send(b'\x1b[200~' + text.encode() + b'\x1b[201~')

    def until(self, condition, description, timeout=12):
        deadline = time.monotonic() + timeout
        while not condition():
            if time.monotonic() >= deadline:
                raise AssertionError(description + '\n' + repr(self.output[-2500:]))
            if select.select([self.master], [], [], min(0.1, deadline - time.monotonic()))[0]:
                try:
                    chunk = os.read(self.master, 65536)
                except OSError as exc:
                    if exc.errno != errno.EIO:
                        raise
                    chunk = b''
                # A PTY is not a terminal emulator. Answer the startup
                # cursor-position query at the initial (1,1), including a
                # query split over read() boundaries.
                queries = self.query_tail + chunk
                while b'\x1b[6n' in queries:
                    _, queries = queries.split(b'\x1b[6n', 1)
                    self.send(b'\x1b[1;1R')
                self.query_tail = queries[-3:]
                self.screen.feed(chunk)
                self.output = (self.output + chunk)[-262144:]
                if not chunk and self.process.poll() is not None and not condition():
                    raise AssertionError('application exited before ' + description + '\n' + repr(self.output[-2500:]))

    def screen_shows(self, text):
        self.until(lambda: text in self.screen.text, 'terminal screen did not show ' + repr(text))

    def shows(self, text):
        needle = text.encode()
        self.until(lambda: needle in self.output, 'terminal did not emit ' + repr(text))


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing application must not run as root')
class IndependentEditorsPTY(unittest.TestCase):
    def test_real_f11_paste_save_undo_redo_and_quit_at_all_required_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='writingdeck-pty-') as temporary:
                base = Path(temporary)
                project, home = base / 'demo', base / 'home'
                home.mkdir()
                env = os.environ | {'HOME': str(home), 'XDG_DATA_HOME': str(home / 'data'), 'XDG_CONFIG_HOME': str(home / 'config')}
                subprocess.run([str(BINARY), 'writingdeck', 'tutorial-demo', str(project)], env=env, capture_output=True, text=True, check=True, timeout=15)
                scenes = list((project / 'manuscript').rglob('*.md'))
                primary = next(p for p in scenes if 'The Brass Key' in p.read_text())
                secondary = next(p for p in scenes if 'The Other Door' in p.read_text())
                original_primary, original_secondary = primary.read_bytes(), secondary.read_bytes()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x1b[23~')  # actual terminal F11 escape sequence
                    terminal.send(b'Other Door\r')
                    terminal.shows('Secondary')
                    terminal.paste('PTY secondary edits. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'PTY secondary edits.' in secondary.read_bytes(), 'secondary save')
                    self.assertEqual(primary.read_bytes(), original_primary)
                    terminal.send(b'\x1b[Z')  # Shift-Tab: secondary -> primary
                    terminal.paste('PTY primary edits. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: b'PTY primary edits.' in primary.read_bytes(), 'primary save')
                    terminal.send(b'\x1a\x13')  # Ctrl-Z + save: primary only
                    terminal.until(lambda: primary.read_bytes() == original_primary, 'primary undo and save')
                    self.assertIn(b'PTY secondary edits.', secondary.read_bytes())
                    terminal.send(b'\x19\x13')  # Ctrl-Y + save
                    terminal.until(lambda: b'PTY primary edits.' in primary.read_bytes(), 'primary redo and save')
                    terminal.send(b'\t\x1a\x13')  # Tab, Ctrl-Z, save: secondary only
                    terminal.until(lambda: secondary.read_bytes() == original_secondary, 'secondary undo and save')
                    self.assertIn(b'PTY primary edits.', primary.read_bytes())
                    terminal.send(b'\x19\x13')
                    terminal.until(lambda: b'PTY secondary edits.' in secondary.read_bytes(), 'secondary redo and save')
                    terminal.send(b'\x1b[23~\x1b[23~\x11')  # hide, restore, normal Ctrl-Q
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                    state = json.loads((project / '.grimoire/writingdeck/layout.json').read_text())
                    workspace = state['workspaces'][state['selected']]
                    self.assertEqual(state['version'], 3)
                    self.assertIsNone(workspace['pin_ids'])
                    self.assertEqual(project / workspace['primary']['document'], primary)
                    self.assertEqual(project / workspace['secondary']['document'], secondary)
                    self.assertEqual(workspace['active_editor'], 'secondary')
                    self.assertIn(b'PTY primary edits.', primary.read_bytes())
                    self.assertIn(b'PTY secondary edits.', secondary.read_bytes())
                finally:
                    terminal.close()


if __name__ == '__main__':
    unittest.main()
