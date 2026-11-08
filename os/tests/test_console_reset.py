"""The independent crash helper restores line input without opening a host VT."""
import os
from pathlib import Path
import subprocess
import tempfile
import termios
import tty
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ConsoleReset(unittest.TestCase):
    def test_a_raw_console_accepts_enter_and_drops_typeahead_without_echo(self):
        with tempfile.TemporaryDirectory(prefix='wros-reset-') as directory:
            root = Path(directory)
            shim = root / 'vt.c'
            # Only the fixed console open and display ioctls are replaced. The
            # production helper's termios calls operate on a real, isolated PTY.
            shim.write_text('''#include <stdlib.h>
#include <unistd.h>
int __wrap_open(const char *path, int flags, ...) {
    (void)path; (void)flags;
    return dup(atoi(getenv("WROS_TEST_FD")));
}
int __wrap_ioctl(int fd, unsigned long request, ...) {
    (void)fd; (void)request;
    return 0;
}
''')
            binary = root / 'reset'
            subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', '-Wl,--wrap=open',
                            '-Wl,--wrap=ioctl', str(ROOT / 'os/helpers/wros-console-reset.c'),
                            str(shim), '-o', str(binary)], check=True, capture_output=True)
            master, slave = os.openpty()
            try:
                tty.setraw(slave)
                os.write(master, b'stale input')
                subprocess.run([str(binary)], env={'WROS_TEST_FD': str(slave)},
                               pass_fds=(slave,), check=True, timeout=5, capture_output=True)
                flags = termios.tcgetattr(slave)
                self.assertTrue(flags[0] & termios.ICRNL, 'Enter must become a newline for Debian askpass')
                self.assertTrue(flags[3] & termios.ICANON)
                self.assertFalse(flags[3] & (termios.ECHO | termios.ECHONL))
                os.write(master, b'fresh line\r')
                self.assertEqual(os.read(slave, 128), b'fresh line\n')
            finally:
                os.close(master)
                os.close(slave)
