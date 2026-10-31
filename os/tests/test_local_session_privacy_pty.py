"""Real local Git history must not transmit merely because a remote exists."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run unprivileged')
class LocalSessionPrivacyPTY(unittest.TestCase):
    def exercise(self, width, height, remote_before_open):
        with tempfile.TemporaryDirectory(prefix='wros-local-session-') as temporary:
            base = Path(temporary)
            project, home, primary = fixture(base)
            before = '---\ntitle: "The Brass Key"\ncustom: "λ original"\n---\n\nλ Original private writing. %% retained %%\n'
            primary.write_text(before, encoding='utf-8')
            sink = base/'disposable-local-remote.git'
            env = os.environ | {'HOME':str(home), 'GIT_CONFIG_NOSYSTEM':'1', 'GIT_TERMINAL_PROMPT':'0'}

            def git(*args):
                return subprocess.run(['/usr/bin/git', '-C', str(project), *args], env=env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15).stdout

            git('init', '--initial-branch=main')
            git('config', 'user.name', 'Disposable fixture author')
            git('config', 'user.email', 'fixture@example.invalid')
            git('add', '.')
            git('commit', '-m', 'Original fixture snapshot')
            subprocess.run(['/usr/bin/git', 'init', '--bare', str(sink)], env=env, check=True, capture_output=True, timeout=15)
            if remote_before_open:
                git('remote', 'add', 'origin', str(sink))
            trace = home/'git-fixture-trace.log'

            def assert_local():
                refs = subprocess.run(['/usr/bin/git', '--git-dir', str(sink), 'for-each-ref', '--format=%(refname)'], env=env, check=True, capture_output=True, timeout=15).stdout
                self.assertEqual(refs, b'', 'WrOS transmitted manuscript history to the configured fixture remote')
                if trace.exists():
                    self.assertNotRegex(trace.read_text(encoding='utf-8'), r'\bgit (?:push|fetch|pull|send-pack|receive-pack)\b')

            with patch.dict(os.environ, {'GIT_TRACE':str(trace), 'GIT_CONFIG_NOSYSTEM':'1', 'GIT_TERMINAL_PROMPT':'0'}):
                terminal = TerminalProcess(project, home, width, height)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ Original')
                end = time.monotonic()+1
                terminal.until(lambda:time.monotonic() >= end, 'initial writer event loop')
                assert_local()
                if not remote_before_open:
                    git('remote', 'add', 'origin', str(sink))
                initial = git('rev-parse', 'HEAD')
                terminal.send(b'\x1b[H'); terminal.paste('Added '); terminal.send(b'\x13')
                changed = before.replace('λ Original', 'Added λ Original')
                terminal.until(lambda:primary.read_bytes() == changed.encode(), 'ordinary original edit saved')
                terminal.send(b'\x0bSave this session now\r')
                terminal.until(lambda:git('rev-parse', 'HEAD') != initial, 'local session retained')
                terminal.send(b'\x0bWriting sessions\r'); terminal.screen_shows('WRITING SESSIONS')
                end = time.monotonic()+1
                terminal.until(lambda:time.monotonic() >= end, 'saved session event loop')
                assert_local()
                self.assertIn('Added λ Original', git('show', 'HEAD:'+primary.relative_to(project).as_posix()).decode())
                if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                    captures=ROOT/'build/verification/local-session-privacy-screens'; captures.mkdir(parents=True,exist_ok=True)
                    (captures/f'local-session-privacy-{width}x{height}.txt').write_text(terminal.screen.text+'\n',encoding='utf-8')
                terminal.send(b'\x1b'); terminal.until(lambda:'WRITING SESSIONS' not in terminal.screen.text, 'history dismissed')
                terminal.send(b'\x1a\x13'); terminal.until(lambda:primary.read_bytes() == before.encode(), 'Undo preserves original YAML and prose')
                terminal.send(b'\x11'); terminal.until(lambda:terminal.process.poll() is not None, 'normal quit'); self.assertEqual(terminal.process.returncode,0)
            finally:
                terminal.close()
            assert_local()
            self.assertIn('λ Original', git('show', 'HEAD:'+primary.relative_to(project).as_posix()).decode())
            with patch.dict(os.environ, {'GIT_TRACE':str(trace), 'GIT_CONFIG_NOSYSTEM':'1', 'GIT_TERMINAL_PROMPT':'0'}):
                terminal=TerminalProcess(project,home,width,height)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bWriting sessions\r'); terminal.screen_shows('WRITING SESSIONS')
                end=time.monotonic()+1
                terminal.until(lambda:time.monotonic() >= end,'reopened local history event loop')
                assert_local(); self.assertEqual(primary.read_bytes(),before.encode())
                terminal.send(b'\x1b'); terminal.until(lambda:'WRITING SESSIONS' not in terminal.screen.text,'restart writer')
                terminal.send(b'\x11'); terminal.until(lambda:terminal.process.poll() is not None,'restart quit'); self.assertEqual(terminal.process.returncode,0)
            finally:
                terminal.close()
            assert_local()

    def test_existing_remote_never_causes_startup_save_quit_or_restart_transmission_at_four_sizes(self):
        for width,height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width):
                self.exercise(width,height,True)

    def test_remote_added_during_writing_does_not_turn_a_local_snapshot_into_an_upload(self):
        self.exercise(80,24,False)


if __name__=='__main__':
    unittest.main()
