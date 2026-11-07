"""The session launcher with real cage and foot, on wlroots' headless backend.

Skipped unless cage and foot are installed. With WROS_HEADLESS_GRIMOIRE set to a built grimoire,
the real app runs in foot too and must report its first screen. Needs a writable /dev/shm.
"""
import importlib.machinery
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
session = importlib.machinery.SourceFileLoader('wros_session_headless', str(ROOT / 'os/scripts/wros-session')).load_module()
GRIMOIRE = os.environ.get('WROS_HEADLESS_GRIMOIRE')


class ToConsole(Exception):
    pass


@unittest.skipUnless(shutil.which('cage') and shutil.which('foot'), 'cage and foot are not installed')
class HeadlessSession(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.temp = Path(temp.name)
        self.home, self.run = self.temp / 'home', self.temp / 'run'
        self.run.mkdir(mode=0o700)
        (self.home / '.config/wros').mkdir(parents=True)
        (self.home / '.config/wros/session.json').write_text('{"session": "graphical"}')
        self.keyboard = self.temp / 'keyboard.json'
        self.keyboard.write_text('{"version": 1, "layout": "colemak", "generation": "test"}')
        self.saved = (session.GRIMOIRE, session.FOOT_CONFIG, session.CAGE, session.FOOT)
        self.addCleanup(self.restore)
        session.FOOT_CONFIG = str(ROOT / 'os/config/foot.ini')
        session.CAGE, session.FOOT = shutil.which('cage'), shutil.which('foot')

    def restore(self):
        session.GRIMOIRE, session.FOOT_CONFIG, session.CAGE, session.FOOT = self.saved

    def script(self, name, body):
        path = self.temp / name
        path.write_text('#!/bin/sh\n' + body)
        path.chmod(0o755)
        return str(path)

    def start(self, app):
        session.GRIMOIRE = app
        env = {'HOME': str(self.home), 'PATH': '/usr/bin:/bin', 'XDG_RUNTIME_DIR': str(self.run),
               'WLR_BACKENDS': 'headless', 'WLR_RENDERER': 'pixman', 'WLR_LIBINPUT_NO_DEVICES': '1'}

        def to_console(path, argv, env):
            raise ToConsole(env.get('WROS_SESSION_NOTICE'))
        launcher = session.Launcher(env, self.home, os.getuid(), self.run, execve=to_console,
                                    cmdline=self.temp / 'no-cmdline', console_flag=self.temp / 'no-flag',
                                    keyboard=self.keyboard, debian_keyboard=self.temp / 'no-keyboard')
        try:
            return launcher.start()
        except ToConsole as notice:
            return str(notice)

    def failures(self):
        return json.loads((self.home / '.local/state/wros/session.json').read_text())['failures']

    def test_an_app_that_draws_runs_in_foot_with_the_writers_layout(self):
        seen = self.temp / 'seen'
        app = self.script('app', f'touch "$WROS_READY_FILE"\necho "$TERM $XKB_DEFAULT_VARIANT $WROS_SESSION" > {seen}\n')
        self.assertEqual(self.start(app), 0)
        self.assertEqual(seen.read_text().split(), ['foot', 'colemak', 'graphical'])
        self.assertEqual(self.failures(), 0)

    def test_an_app_that_never_draws_falls_back_to_the_console(self):
        self.assertEqual(self.start(self.script('broken', 'exit 3\n')), session.FAILED_ONCE)
        self.assertEqual(self.failures(), 1)

    @unittest.skipUnless(GRIMOIRE, 'WROS_HEADLESS_GRIMOIRE is not set')
    def test_the_writing_app_reports_its_first_screen_from_foot(self):
        # The app waits for keys forever, so a wrapper ends it once it reported its first screen.
        # A background job's input would be /dev/null; the app reads keys from the terminal.
        app = self.script('grimoire', f'{GRIMOIRE} < /dev/tty & pid=$!\n'
                          'for i in $(seq 1 150); do [ -e "$WROS_READY_FILE" ] && break; sleep 0.2; done\n'
                          'kill $pid\n')
        self.assertEqual(self.start(app), 0)
        self.assertEqual(self.failures(), 0)


if __name__ == '__main__':
    unittest.main()
