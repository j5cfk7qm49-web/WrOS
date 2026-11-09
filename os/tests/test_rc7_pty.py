"""Run the actual writing app in a private PTY, with a local fixture locker."""
import hashlib
import os
from pathlib import Path
import secrets
import tempfile
import unittest
from unittest.mock import patch

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess

DOWN = b'\x1b[B'


def system_settings(terminal):
    terminal.shows('The Brass Key')
    terminal.send(b'\x0bThe Brass Key\r')
    terminal.screen_shows('The Brass Key')
    terminal.send(b'\x0bWrOS settings\r')
    terminal.screen_shows('WrOS · Settings')
    terminal.send(DOWN * 2 + b'\r')
    terminal.screen_shows('Settings › System')


def reader_children(pid):
    readers = set()
    for task in Path(f'/proc/{pid}/task').glob('*/children'):
        for child in task.read_text().split():
            comm = Path(f'/proc/{child}/comm')
            if comm.exists() and comm.read_text().strip() == 'wros-input':
                readers.add(child)
    return readers


@unittest.skipUnless(BINARY.is_file(), 'build the writing binary first')
@unittest.skipIf(os.geteuid() == 0, 'the PTY must run as an unprivileged writer')
class Rc7PTY(unittest.TestCase):
    @unittest.skipUnless(os.environ.get('WROS_QA_PTY') == '1', 'build with --features pty-qa')
    def test_lock_retires_the_reader_and_discards_credentials_and_typeahead(self):
        with tempfile.TemporaryDirectory(prefix='wros-lock-pty-') as tmp:
            project, home, primary = fixture(Path(tmp))
            credential = secrets.token_hex(12).encode()
            stale = secrets.token_hex(12).encode()
            locker = home / 'locker'
            locker.write_text('#!/usr/bin/python3 -I\n'
                              'import hashlib, os, time\n'
                              'os.write(1, b"\\r\\nQA locker is waiting\\r\\n")\n'
                              'value = bytearray()\n'
                              'while (byte := os.read(0, 1)) not in (b"", b"\\r", b"\\n"):\n'
                              '    value.extend(byte)\n'
                              'time.sleep(0.25)\n'
                              f'raise SystemExit(hashlib.sha256(value).hexdigest() != {hashlib.sha256(credential).hexdigest()!r})\n')
            locker.chmod(0o700)
            with patch.dict(os.environ, {'WROS_QA_LOCK_HELPER': str(locker)}):
                terminal = TerminalProcess(project, home, 100, 30)
            try:
                system_settings(terminal)
                old_reader = reader_children(terminal.process.pid)
                self.assertEqual(len(old_reader), 1)
                terminal.send(DOWN * 2 + b'\r')
                terminal.screen_shows('Lock writing session')
                terminal.screen_shows('System status refreshed locally.')
                terminal.send(b'\x1b[F')  # End
                terminal.screen_shows('▸ Lock writing session')
                terminal.send(b'\r')
                terminal.screen_shows('QA locker is waiting')
                self.assertFalse(reader_children(terminal.process.pid))
                terminal.send(credential + b'\r' + stale)
                terminal.screen_shows('Writing session unlocked')
                fresh = reader_children(terminal.process.pid)
                self.assertEqual(len(fresh), 1)
                self.assertTrue(fresh.isdisjoint(old_reader))
                terminal.send(b'\x1b')
                terminal.screen_shows('▸ Battery and power')
                terminal.send(b'\x1b')
                terminal.screen_shows('▸ System')
                terminal.send(b'\x1b')
                terminal.screen_shows('The Brass Key')
                terminal.paste('Writing after unlock. ')
                terminal.send(b'\x13')
                terminal.until(lambda: b'Writing after unlock.' in primary.read_bytes(), 'post-lock save')
                for sample in (credential, stale):
                    self.assertNotIn(sample, terminal.output)
                    for path in Path(tmp).rglob('*'):
                        if path.is_file() and not path.is_symlink():
                            self.assertNotIn(sample, path.read_bytes())
            finally:
                terminal.close()
