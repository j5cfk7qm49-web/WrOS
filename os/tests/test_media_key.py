"""Unit tests for the brightness and volume key handler (os/scripts/wros-media-key, #80)."""
import contextlib
import fcntl
import importlib.machinery
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'os/scripts/wros-media-key'
keys = importlib.machinery.SourceFileLoader('wros_media_key', str(SCRIPT)).load_module()


class Recorder:
    def __init__(self, fail=False):
        self.calls, self.fail = [], fail

    def __call__(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        if self.fail:
            raise subprocess.CalledProcessError(1, argv)
        return subprocess.CompletedProcess(argv, 0)


class MediaKeyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.sys = Path(self.tmp.name, 'sys')
        self.state = Path(self.tmp.name, 'run')
        self.state.mkdir()

    def backlight(self, name, now, top):
        path = self.sys / 'class/backlight' / name
        path.mkdir(parents=True)
        (path / 'brightness').write_text(f'{now}\n')
        (path / 'max_brightness').write_text(f'{top}\n')

    def press(self, action, run=None):
        run = run or Recorder()
        with contextlib.redirect_stderr(io.StringIO()):
            code = keys.main([action], state=self.state, sys_root=self.sys, run=run)
        return code, run

    def test_brightness_moves_one_step_on_the_screens_backlight_and_tells_the_app(self):
        self.backlight('intel_backlight', 9600, 19200)
        code, run = self.press('brightness-up')
        self.assertEqual(code, 0)
        argv, kwargs = run.calls[0]
        self.assertEqual(argv, ['/usr/bin/brightnessctl', '--quiet', '--class=backlight',
                                '--device=intel_backlight', 'set', '10560'])
        self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8'})
        self.assertTrue((self.state / 'changed').exists())
        _, run = self.press('brightness-down')
        self.assertEqual(run.calls[0][0][-1], '8640')

    def test_brightness_never_goes_dark_or_past_the_top(self):
        self.backlight('acpi_video0', 1, 100)
        _, run = self.press('brightness-down')
        self.assertEqual(run.calls[0][0][-1], '5')
        (self.sys / 'class/backlight/acpi_video0/brightness').write_text('98\n')
        _, run = self.press('brightness-up')
        self.assertEqual(run.calls[0][0][-1], '100')

    def test_at_the_limit_nothing_runs_and_the_app_is_not_told(self):
        self.backlight('acpi_video0', 100, 100)
        code, run = self.press('brightness-up')
        self.assertEqual((code, run.calls), (0, []))
        self.assertFalse((self.state / 'changed').exists())

    def test_a_coarse_backlight_still_moves_one_hardware_step(self):
        self.assertEqual(keys.brightness_target(4, 7, True), 5)
        self.assertEqual(keys.brightness_target(4, 7, False), 3)
        self.assertEqual(keys.brightness_target(1, 7, False), 1)

    def test_no_backlight_is_not_an_error(self):
        code, run = self.press('brightness-up')
        self.assertEqual((code, run.calls), (0, []))

    def test_unreadable_backlights_are_skipped_for_the_next(self):
        self.backlight('a_broken', 5, 0)
        self.backlight('b_good', 50, 100)
        _, run = self.press('brightness-up')
        self.assertIn('--device=b_good', run.calls[0][0])

    def test_volume_keys_go_to_the_sound_helper(self):
        for action, word in (('volume-up', 'up'), ('volume-down', 'down'), ('mute', 'toggle')):
            with self.subTest(action=action):
                code, run = self.press(action)
                self.assertEqual(code, 0)
                self.assertEqual(run.calls[0][0], ['/usr/libexec/wros-sound', word])

    def test_a_press_while_the_last_still_runs_is_dropped(self):
        fd = os.open(self.state / 'lock', os.O_RDWR | os.O_CREAT, 0o600)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX)
        self.backlight('intel_backlight', 50, 100)
        code, run = self.press('brightness-up')
        self.assertEqual((code, run.calls), (0, []))

    def test_a_failed_change_reports_failure_and_does_not_tell_the_app(self):
        self.backlight('intel_backlight', 50, 100)
        code, _ = self.press('brightness-up', Recorder(fail=True))
        self.assertEqual(code, 1)
        self.assertFalse((self.state / 'changed').exists())

    def test_without_its_state_directory_it_still_works(self):
        self.backlight('intel_backlight', 50, 100)
        run = Recorder()
        code = keys.main(['volume-up'], state=Path(self.tmp.name, 'missing'), sys_root=self.sys, run=run)
        self.assertEqual((code, len(run.calls)), (0, 1))

    def test_refuses_anything_else(self):
        for bad in ([], ['brightness'], ['volume-up', 'extra'], ['set', '50'], ['--help']):
            run, err = Recorder(), io.StringIO()
            with self.subTest(bad=bad), contextlib.redirect_stderr(err):
                self.assertEqual(keys.main(bad, state=self.state, sys_root=self.sys, run=run), 2)
                self.assertEqual(run.calls, [])
                self.assertIn('usage', err.getvalue())


class KeyConfigurationTests(unittest.TestCase):
    def test_every_trigger_runs_the_handler_with_a_known_action(self):
        lines = [line.split(None, 2) for line in
                 (ROOT / 'os/config/wros-keys-triggers.conf').read_text().splitlines()
                 if line and not line.startswith('#')]
        seen = set()
        for event, value, command in lines:
            program, action = command.split()
            self.assertEqual(program, '/usr/libexec/wros-media-key')
            self.assertIn(action, keys.ACTIONS)
            self.assertIn(value, ('1', '2'))
            seen.add((event, value))
        for event in ('KEY_BRIGHTNESSUP', 'KEY_BRIGHTNESSDOWN', 'KEY_VOLUMEUP', 'KEY_VOLUMEDOWN'):
            self.assertIn((event, '1'), seen)
            self.assertIn((event, '2'), seen)
        self.assertIn(('KEY_MUTE', '1'), seen)
        self.assertNotIn(('KEY_MUTE', '2'), seen)

    def test_triggerhappy_runs_only_wros_triggers_as_the_keys_account(self):
        text = (ROOT / 'os/config/triggerhappy-wros.conf').read_text()
        starts = [line for line in text.splitlines() if line.startswith('ExecStart=')]
        self.assertEqual(starts[0], 'ExecStart=')
        self.assertEqual(starts[1].split(), [
            'ExecStart=/usr/sbin/thd', '--triggers', '/usr/lib/wros/keys/', '--socket', '/run/thd.socket',
            '--user', 'wros-keys', '--deviceglob', '/dev/input/event*'])

    def test_the_keys_account_may_only_use_the_backlight_and_the_sound_card(self):
        lines = [line.split() for line in (ROOT / 'os/config/wros-keys-sysusers.conf').read_text().splitlines()
                 if line and not line.startswith('#')]
        self.assertEqual(lines[0][:2], ['u', 'wros-keys'])
        self.assertEqual(lines[0][-1], '/usr/sbin/nologin')
        self.assertEqual(sorted(line[2] for line in lines[1:]), ['audio', 'video'])

    def test_the_kernel_does_not_also_change_brightness_on_the_keys(self):
        modprobe = (ROOT / 'os/config/60-wros-brightness-keys.conf').read_text()
        self.assertIn('options video brightness_switch_enabled=0', modprobe.splitlines())
        tmpfiles = (ROOT / 'os/config/wros-keys-tmpfiles.conf').read_text().splitlines()
        self.assertIn('d /run/wros-keys 0755 wros-keys wros-keys -', tmpfiles)
        self.assertIn('w /sys/module/video/parameters/brightness_switch_enabled - - - - N', tmpfiles)


if __name__ == '__main__':
    unittest.main()
