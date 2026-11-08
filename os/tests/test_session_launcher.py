"""The login session launcher (os/scripts/wros-session): console or graphical, and the fallback.

Nothing here starts cage, foot or grimoire: running and exec'ing go through recording fakes.
"""
import importlib.machinery
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
session = importlib.machinery.SourceFileLoader('wros_session', str(ROOT / 'os/scripts/wros-session')).load_module()


class Exec(Exception):
    """The fake execve: the launcher never comes back from it."""


class Machine:
    """A writer's home, /run and /etc under one temporary folder, with recording fakes."""

    def __init__(self, temp, *, installed=True, choice='graphical', draws=True):
        self.temp = Path(temp)
        self.home = self.temp / 'home'
        self.runtime = self.temp / 'run/user/1000'
        self.runtime.mkdir(parents=True)
        self.etc = self.temp / 'etc'
        self.etc.mkdir()
        self.cmdline = self.temp / 'cmdline'
        self.cmdline.write_text('BOOT_IMAGE=/vmlinuz root=UUID=x ro quiet\n')
        self.flag = self.temp / 'run/wros-session/console'
        self.installed, self.draws = installed, draws
        self.runs, self.execs = [], []
        if choice:
            self.write(self.home / '.config/wros/session.json', {'version': 1, 'session': choice, 'backend': 'foot'})

    @staticmethod
    def write(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value) if not isinstance(value, str) else value)

    def exists(self, path):
        if path in (session.CAGE, session.FOOT):
            return self.installed
        return Path(path).exists()

    def run(self, argv, *, env, stdin, check):
        self.runs.append((list(argv), dict(env)))
        if self.draws:
            Path(env['WROS_READY_FILE']).touch()

    def execve(self, path, argv, env):
        self.execs.append((path, list(argv), dict(env)))
        raise Exec

    def launcher(self, environ=None):
        return session.Launcher(environ or {'HOME': str(self.home), 'PATH': '/usr/bin'}, self.home, 1000,
                                self.runtime, exists=self.exists, run=self.run, execve=self.execve,
                                cmdline=self.cmdline, console_flag=self.flag,
                                keyboard=self.etc / 'wros/keyboard.json',
                                debian_keyboard=self.etc / 'default/keyboard')

    def start(self, environ=None):
        try:
            return self.launcher(environ).start()
        except Exec:
            return 'exec'

    def failures(self):
        path = self.home / '.local/state/wros/session.json'
        return json.loads(path.read_text())['failures'] if path.exists() else 0

    def notice(self):
        return self.execs[-1][2].get('WROS_SESSION_NOTICE')


class Launcher(unittest.TestCase):
    def test_wgpu_runs_directly_under_cage_without_foot(self):
        m = self.machine()
        m.write(m.home / '.config/wros/session.json', {'session': 'graphical', 'backend': 'wgpu'})
        m.write(m.etc / 'wros/keyboard.json', {'layout': 'colemak', 'generation': 'chosen'})
        m.exists = lambda path: path == session.CAGE or (path != session.FOOT and Path(path).exists())
        self.assertEqual(m.start(), 0)
        argv, env = m.runs[-1]
        self.assertEqual(argv, [session.CAGE, '-s', '--', session.GRIMOIRE])
        self.assertEqual(env['WROS_SESSION'], 'wgpu')
        self.assertEqual(env['XKB_DEFAULT_VARIANT'], 'colemak')

    def test_failure_counts_belong_to_the_selected_compositor_backend(self):
        m = self.machine(draws=False)
        m.start()
        m.start()
        self.assertEqual(m.failures(), 2)
        m.write(m.home / '.config/wros/session.json', {'session': 'graphical', 'backend': 'wgpu'})
        m.draws = True
        self.assertEqual(m.start(), 0)
        self.assertEqual(m.runs[-1][1]['WROS_SESSION'], 'wgpu')

    def machine(self, **kwargs):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        return Machine(temp.name, **kwargs)

    def test_an_explicit_console_choice_is_respected(self):
        for kwargs in ({'choice': 'console'},):
            with self.subTest(**kwargs):
                m = self.machine(**kwargs)
                self.assertEqual(m.start(), 'exec')
                self.assertEqual(m.runs, [])
                path, argv, env = m.execs[-1]
                self.assertEqual((path, argv), ('/usr/bin/grimoire', ['/usr/bin/grimoire']))
                self.assertEqual((env['TERM'], env['WROS_SESSION'], env['WROS_OS']), ('linux', 'console', '1'))
                self.assertIsNone(m.notice())

    def test_a_new_home_defaults_to_native_graphics(self):
        m = self.machine(choice=None, installed=False)
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.execs[-1][2]['WROS_SESSION'], 'drm')

    def test_the_graphical_session_runs_cage_foot_and_the_app(self):
        m = self.machine()
        self.assertEqual(m.start(), 0)
        self.assertEqual(m.execs, [])
        argv, env = m.runs[0]
        self.assertEqual(argv, ['/usr/bin/cage', '-s', '--', '/usr/bin/foot',
                                '--config=/usr/share/wros/foot.ini', '--', '/usr/bin/grimoire'])
        self.assertEqual(env['WROS_SESSION'], 'graphical')
        self.assertEqual(env['WROS_READY_FILE'], str(m.runtime / 'wros-session-ready'))
        self.assertNotIn('TERM', env)  # foot sets its own
        self.assertEqual(m.failures(), 0)

    def test_text_start_forces_text_writing_even_without_a_saved_session_choice(self):
        m = self.machine(choice=None)
        m.cmdline.write_text('root=UUID=x wros.unlock=text\n')
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.execs[-1][2]['WROS_SESSION'], 'console')
        self.assertEqual(m.runs, [])

    def test_a_graphical_session_that_never_draws_falls_back_to_the_console_at_once(self):
        m = self.machine(draws=False)
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(len(m.runs), 1)
        self.assertEqual(m.execs[-1][2]['WROS_SESSION'], 'console')
        self.assertEqual(m.notice(), session.FAILED_ONCE)
        self.assertEqual(m.failures(), 1)
        # The second failure in a row says so, and the next login stays on the console.
        m.start()
        self.assertEqual(m.notice(), session.FAILED_TWICE)
        self.assertEqual(m.failures(), 2)
        m.runs.clear()
        m.start()
        self.assertEqual(m.runs, [])
        self.assertEqual(m.notice(), session.FAILED_TWICE)

    def test_a_session_that_drew_clears_the_failure_count(self):
        m = self.machine(draws=False)
        m.start()
        m.draws = True
        self.assertEqual(m.start(), 0)
        self.assertEqual(m.failures(), 0)

    def test_a_stale_ready_file_from_an_earlier_login_does_not_count(self):
        m = self.machine(draws=False)
        (m.runtime / 'wros-session-ready').touch()
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.failures(), 1)

    def test_cage_that_cannot_start_falls_back_too(self):
        m = self.machine()

        def broken(argv, **kwargs):
            raise FileNotFoundError(argv[0])
        m.run = broken
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.notice(), session.FAILED_ONCE)

    def test_recovery_or_the_boot_line_can_ask_for_the_console(self):
        m = self.machine()
        m.flag.parent.mkdir(parents=True)
        m.flag.touch()
        self.assertEqual(m.start(), 'exec')
        self.assertEqual((m.runs, m.notice()), ([], session.RECOVERY_CHOSE))
        m.flag.unlink()
        m.cmdline.write_text('BOOT_IMAGE=/vmlinuz ro quiet wros.session=console\n')
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.runs, [])
        # Choosing the console does not count as the graphical session failing.
        self.assertEqual(m.failures(), 0)

    def test_the_login_screen_can_ask_for_the_console_for_one_login(self):
        m = self.machine()
        launcher = session.Launcher({'HOME': str(m.home)}, m.home, 1000, m.runtime, exists=m.exists, run=m.run,
                                    execve=m.execve, cmdline=m.cmdline, console_flag=m.flag, login_console=True)
        with self.assertRaises(Exec):
            launcher.start()
        self.assertEqual((m.runs, m.notice(), m.failures()), ([], None, 0))
        # The writer's own choice is unchanged, so the next login is graphical again.
        self.assertEqual(m.start(), 0)

    def test_the_live_session_uses_native_graphics_without_a_compositor(self):
        m = self.machine()
        self.assertEqual(m.start({'HOME': str(m.home), 'WROS_LIVE_SESSION': '1'}), 'exec')
        self.assertEqual(m.runs, [])
        self.assertEqual(m.execs[-1][2]['WROS_LIVE_SESSION'], '1')
        self.assertEqual(m.execs[-1][2]['WROS_SESSION'], 'drm')

    def test_no_runtime_folder_means_the_console(self):
        m = self.machine()
        launcher = session.Launcher({'HOME': str(m.home)}, m.home, 1000, None, exists=m.exists, run=m.run,
                                    execve=m.execve, cmdline=m.cmdline, console_flag=m.flag)
        with self.assertRaises(Exec):
            launcher.start()
        self.assertEqual(m.runs, [])

    def test_unreadable_choice_or_count_files_mean_the_console_and_no_count(self):
        m = self.machine(choice=None)
        m.write(m.home / '.config/wros/session.json', '{not json')
        self.assertEqual(m.start(), 'exec')
        self.assertEqual(m.runs, [])
        self.assertEqual(m.execs[-1][2]['WROS_SESSION'], 'console')
        m = self.machine(draws=False)
        m.write(m.home / '.local/state/wros/session.json', {'failures': 'lots'})
        m.start()
        self.assertEqual(m.failures(), 1)


class Keyboard(unittest.TestCase):
    def xkb(self, keyboard=None, debian=None):
        with tempfile.TemporaryDirectory() as temp:
            k, d = Path(temp, 'keyboard.json'), Path(temp, 'keyboard')
            if keyboard is not None:
                k.write_text(json.dumps(keyboard))
            if debian is not None:
                d.write_text(debian)
            return session.xkb_environment(k, d)

    def test_the_layout_chosen_in_wros_is_the_one_cage_uses(self):
        for layout, variant in (('colemak', 'colemak'), ('colemak-dh-ansi', 'colemak_dh'),
                                ('colemak-dh-iso', 'colemak_dh_iso')):
            env = self.xkb({'version': 1, 'layout': layout, 'generation': 'abc'})
            self.assertEqual(env, {'XKB_DEFAULT_LAYOUT': 'us', 'XKB_DEFAULT_VARIANT': variant})
        self.assertEqual(self.xkb({'version': 1, 'layout': 'qwerty', 'generation': 'installer'}),
                         {'XKB_DEFAULT_LAYOUT': 'us'})

    def test_before_a_choice_debians_installer_layout_is_kept(self):
        debian = 'XKBMODEL="pc105"\nXKBLAYOUT="de"\nXKBVARIANT="nodeadkeys"\nXKBOPTIONS=""\nBACKSPACE="guess"\n'
        env = self.xkb({'version': 1, 'layout': 'qwerty', 'generation': 'initial'}, debian)
        self.assertEqual(env, {'XKB_DEFAULT_MODEL': 'pc105', 'XKB_DEFAULT_LAYOUT': 'de',
                               'XKB_DEFAULT_VARIANT': 'nodeadkeys'})
        self.assertEqual(self.xkb(None, 'XKBLAYOUT=us\nXKBVARIANT=colemak\n'),
                         {'XKB_DEFAULT_LAYOUT': 'us', 'XKB_DEFAULT_VARIANT': 'colemak'})

    def test_odd_debian_values_fall_back_to_us(self):
        for debian in ('XKBLAYOUT="us;reboot"\n', 'XKBLAYOUT=us\nXKBLAYOUT=de\n', 'XKBVARIANT=colemak\n', ''):
            with self.subTest(debian=debian):
                self.assertEqual(self.xkb(None, debian), {'XKB_DEFAULT_LAYOUT': 'us'})

    def test_the_graphical_session_gets_the_layout(self):
        with tempfile.TemporaryDirectory() as temp:
            m = Machine(temp)
            m.write(m.etc / 'wros/keyboard.json', {'version': 1, 'layout': 'colemak', 'generation': 'x'})
            m.start()
            env = m.runs[0][1]
            self.assertEqual((env['XKB_DEFAULT_LAYOUT'], env['XKB_DEFAULT_VARIANT']), ('us', 'colemak'))


if __name__ == '__main__':
    unittest.main()
