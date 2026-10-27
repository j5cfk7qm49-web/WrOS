"""Real shell navigation/paste/undo/restart contracts in isolated demo books."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'the writing shell must never run as root')
class UnifiedShellPTY(unittest.TestCase):
    def test_native_settings_preserve_actual_cursor_undo_sources_and_user_preferences_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-shell-pty-') as temporary:
                project, user_dir, primary = fixture(Path(temporary))
                terminal = TerminalProcess(project, user_dir, width, height)
                prefix, suffix = 'Before λ ', 'After Ω '
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r')
                    terminal.paste(prefix)
                    terminal.send(b'\x13')
                    terminal.until(lambda: prefix.encode() in primary.read_bytes(), 'initial original save')
                    before = primary.read_bytes()
                    terminal.send(b'\x0bWrOS settings\r')
                    terminal.screen_shows('WrOS · Settings')
                    terminal.send(b'\x1b[B\x1b[B\r')
                    terminal.screen_shows('Settings › System')
                    terminal.send(b'\r')
                    # This row exists only after entering Network. Its title
                    # also appears in the parent menu and is not a barrier.
                    terminal.screen_shows('Enable Wi-Fi radio')
                    terminal.paste('Pasted settings commands must never reach prose.')
                    terminal.send(b'\x18\x1a')  # cut/undo must belong to the system page
                    terminal.send(b'\x1b')
                    terminal.screen_shows('▸ Wi-Fi and networking')
                    terminal.send(b'\x1b[B\r')
                    terminal.screen_shows('Brightness -10%')
                    terminal.send(b'\x1b[F\r')
                    terminal.screen_shows('Check display support and saved settings')
                    terminal.screen_shows('1000–6500 K')
                    terminal.paste('Display settings paste must never reach prose.')
                    terminal.send(b'\x18\x1a')
                    self.assertEqual(primary.read_bytes(), before)
                    terminal.send(b'\x1b')
                    terminal.screen_shows('Warm screen · blue light filter')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('▸ Display')
                    terminal.send(b'\x1b')
                    terminal.screen_shows('▸ System')
                    terminal.send(b'\x1b')
                    terminal.screen_shows(prefix)
                    terminal.paste(suffix)
                    terminal.send(b'\x13')
                    expected = before.replace(prefix.encode(), (prefix + suffix).encode(), 1)
                    terminal.until(lambda: suffix.encode() in primary.read_bytes(), 'return to the same caret and save')
                    self.assertEqual(primary.read_bytes(), expected)
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda: primary.read_bytes() == before, 'retained original undo stack')
                    terminal.send(b'\x19\x13')
                    terminal.until(lambda: primary.read_bytes() == expected, 'retained redo stack')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal shell quit')
                    self.assertEqual(terminal.process.returncode, 0)
                    preferences = list((user_dir / 'data').rglob('wros/ui.json'))
                    self.assertEqual(len(preferences), 1)
                    state = json.loads(preferences[0].read_text())
                    self.assertEqual(state['version'], 1)
                    self.assertFalse((project / '.grimoire/writingdeck/shell.json').exists())
                finally:
                    terminal.close()
                reopened = TerminalProcess(project, user_dir, width, height)
                try:
                    reopened.shows('The Brass Key')
                    reopened.send(b'\x0bWrOS settings\r')
                    reopened.screen_shows('WrOS · Settings')
                    reopened.send(b'\r')  # the persisted root selection is System
                    reopened.screen_shows('Settings › System')
                    reopened.send(b'\x11')
                    reopened.until(lambda: reopened.process.poll() is not None, 'normal restarted quit')
                    self.assertEqual(reopened.process.returncode, 0)
                    self.assertEqual(primary.read_bytes(), expected)
                finally:
                    reopened.close()
