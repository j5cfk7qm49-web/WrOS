"""Actual non-root native TUI + private socket + fresh PTYs; no disk backend."""
import os
from pathlib import Path
import socket
import select
import tempfile
import time
import unittest

from test_secondary_pty import TerminalProcess

BINARY=Path(os.environ.get('WROS_INSTALLER_BINARY',Path(os.environ.get('CARGO_TARGET_DIR','target'))/'debug/wros-installer')).resolve()

class InstallerTerminal(TerminalProcess):
    def __init__(self,home,width,height):
        self.peer,frontend=socket.socketpair()
        self.peer.settimeout(8)
        try:
            super().__init__(home,home,width,height,command=[str(BINARY),'--frontend-fd',str(frontend.fileno())],pass_fds=(frontend.fileno(),))
        finally:
            frontend.close()
        self.wire=self.peer.makefile('rwb',buffering=0)
        self.screen_shows('Preparing installation')

    def close(self):
        self.wire.close()
        self.peer.close()
        super().close()

    def command(self,line,reply=b'0\n'):
        self.wire.write(line.encode()+b'\n')
        if reply is not None:
            self.expect(reply)

    def expect(self,expected):
        actual=bytearray()
        def received():
            if select.select([self.peer],[],[],0)[0]:
                chunk=self.peer.recv(65538-len(actual))
                if not chunk: raise AssertionError('installer channel closed')
                actual.extend(chunk)
                if len(actual)>65536: raise AssertionError('installer reply too large')
            return b'\n' in actual
        self.until(received,'installer reply',timeout=8)
        if actual!=expected: raise AssertionError('installer reply status differed')

@unittest.skipUnless(BINARY.is_file(),'build the native installer first')
@unittest.skipIf(os.geteuid()==0,'native frontend must be unprivileged')
class InstallerPTY(unittest.TestCase):
    def test_installer_uses_the_existing_saved_grimoire_custom_palette(self):
        with tempfile.TemporaryDirectory(prefix='wros-installer-theme-') as directory:
            home=Path(directory)
            config=home/'.config/grimoire';config.mkdir(parents=True)
            (config/'theme.toml').write_text('name = "Custom"\naccent = "#12ab34"\n')
            terminal=InstallerTerminal(home,80,24)
            try:
                terminal.until(lambda:b'\x1b[38;2;18;171;52;' in terminal.output or b'\x1b[38;2;18;171;52m' in terminal.output,'saved palette rendered')
            finally: terminal.close()

    def test_actual_fields_help_secret_back_and_explicit_no_at_four_sizes(self):
        for width,height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-installer-pty-') as directory:
                terminal=InstallerTerminal(Path(directory),width,height)
                secret=('fixture-password-'+str(os.getpid())).encode()
                try:
                    terminal.command('CAPB backup',b'0 backup\n')
                    for line in ['DATA fixture/password type password','DATA fixture/password description Fixture password',
                                 'INPUT critical fixture/password']:
                        terminal.command(line)
                    terminal.command('GO',None)
                    terminal.screen_shows('Fixture password')
                    terminal.send(b'\x1bOP')
                    terminal.screen_shows('Installation help')
                    terminal.send(b'\r')
                    time.sleep(.1)
                    self.assertIn('Installation help',terminal.screen.text)
                    terminal.send(b'\x1bOP')
                    terminal.screen_shows('Fixture password')
                    terminal.send(secret)
                    terminal.screen_shows('[hidden]')
                    terminal.send(b'\r')
                    terminal.expect(b'0\n')
                    terminal.command('GET fixture/password',b'0 '+secret+b'\n')
                    self.assertNotIn(secret,terminal.output)
                    terminal.command('GET fixture/password',b'0 \n')
                    for line in ['DATA fixture/confirm type boolean','DATA fixture/confirm description Write fixture changes?',
                                 'SET fixture/confirm true','INPUT critical fixture/confirm']:
                        terminal.command(line)
                    terminal.command('GO',None)
                    terminal.screen_shows('Write fixture changes?')
                    terminal.screen_shows('▸ No')
                    terminal.send(b'\r')
                    terminal.expect(b'0\n')
                    terminal.command('GET fixture/confirm',b'0 false\n')
                    terminal.command('INPUT high fixture/password')
                    terminal.command('GO',None)
                    terminal.screen_shows('Fixture password')
                    terminal.send(b'\x1b')
                    terminal.expect(b'30\n')
                    terminal.command('INPUT high fixture/password')
                    terminal.command('GO',None)
                    terminal.screen_shows('Fixture password')
                    terminal.send(b'\r')
                    terminal.expect(b'0\n')
                    terminal.command('GET fixture/password',b'0 \n')
                finally:
                    terminal.close()

    def test_backend_disconnection_stops_without_claiming_success(self):
        with tempfile.TemporaryDirectory(prefix='wros-installer-eof-') as directory:
            terminal=InstallerTerminal(Path(directory),80,24)
            try:
                terminal.peer.shutdown(socket.SHUT_RDWR)
                terminal.until(lambda:terminal.process.poll() is not None,'frontend termination')
                self.assertEqual(terminal.process.returncode,1)
                self.assertNotIn(b'Installation complete',terminal.output)
            finally:
                terminal.close()
