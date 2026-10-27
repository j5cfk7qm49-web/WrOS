"""Real editor/reader lifetime and rescue in fresh unprivileged PTYs only."""
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess


def reader_pid(editor):
    path = Path(f'/proc/{editor}/task/{editor}/children')
    candidates = []
    for value in path.read_text().split():
        comm = Path(f'/proc/{value}/comm')
        if comm.is_file() and comm.read_text().strip() == 'wros-input':
            candidates.append(int(value))
    assert len(candidates) == 1
    return candidates[0]


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'the editor/reader must run unprivileged')
class InputOwnershipPTY(unittest.TestCase):
    def test_reader_death_rescues_both_actual_dirty_document_buffers(self):
        with tempfile.TemporaryDirectory(prefix='wros-input-editor-') as temporary:
            project, home, primary = fixture(Path(temporary))
            secondary = next(p for p in (project / 'manuscript').rglob('*.md')
                             if 'The Other Door' in p.read_text())
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x1b[23~Other Door\r')
                terminal.screen_shows('Secondary')
                terminal.paste('Uncommitted reader fixture secondary. ')
                terminal.screen_shows('Uncommitted reader fixture secondary.')
                terminal.send(b'\x1b[Z')
                terminal.paste('Uncommitted reader fixture primary. ')
                terminal.screen_shows('Uncommitted reader fixture primary.')
                pid = reader_pid(terminal.process.pid)
                status = Path(f'/proc/{pid}/status').read_text()
                self.assertIn(f'\nUid:\t{os.geteuid()}\t', status)
                self.assertIn(b'--wros-input-reader', Path(f'/proc/{pid}/cmdline').read_bytes())
                os.kill(pid, signal.SIGKILL)
                terminal.until(lambda: terminal.process.poll() is not None, 'reader death must end editing and save')
                self.assertEqual(terminal.process.returncode, 1)
                self.assertIn('Uncommitted reader fixture primary.', primary.read_text())
                self.assertIn('Uncommitted reader fixture secondary.', secondary.read_text())
                self.assertFalse(Path(f'/proc/{pid}').exists())
            finally:
                terminal.close()

    def test_editor_sigkill_cannot_leave_an_input_reader_running(self):
        with tempfile.TemporaryDirectory(prefix='wros-input-parent-') as temporary:
            project, home, _ = fixture(Path(temporary))
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                pid = reader_pid(terminal.process.pid)
                terminal.process.kill()
                terminal.process.wait(timeout=4)
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    path = Path(f'/proc/{pid}/stat')
                    if not path.exists() or path.read_text().split(') ', 1)[1].startswith('Z '):
                        break
                    time.sleep(0.02)
                else:
                    self.fail('orphan input reader remains executable after editor SIGKILL')
            finally:
                terminal.close()

    def test_private_reader_mode_rejects_bad_context_without_creating_a_project(self):
        with tempfile.TemporaryDirectory(prefix='wros-input-context-') as temporary:
            home = Path(temporary)
            env = os.environ | {'HOME': str(home), 'XDG_DATA_HOME': str(home / 'data'),
                                'XDG_CONFIG_HOME': str(home / 'config')}
            result = subprocess.run([str(BINARY), '--wros-input-reader', str(os.getpid())],
                                    stdin=subprocess.DEVNULL, capture_output=True, env=env,
                                    cwd=home, timeout=4)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, b'')
            self.assertEqual(result.stderr.strip(), b'WrOS input reader unavailable')
            self.assertEqual(list(home.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
