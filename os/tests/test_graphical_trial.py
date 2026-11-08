"""The trial isolates writing and counts only its private process group."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('graphical_trial', ROOT / 'tools/graphical_trial.py')
trial = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trial)


class Trial(unittest.TestCase):
    def test_each_key_and_capture_observation_is_retained_with_the_actual_frame_size(self):
        from PIL import Image
        def png(colour):
            buffer = io.BytesIO()
            Image.new('RGB', (16, 48), colour).save(buffer, format='PNG')
            return buffer.getvalue()
        unchanged, changed = png('black'), png('white')
        child = SimpleNamespace(stdin=io.StringIO(), wait=lambda **_: 0)
        with mock.patch.object(trial, 'keyboard', return_value=(child, {})), \
             mock.patch.object(trial, 'key'), \
             mock.patch.object(trial.time, 'sleep'), \
             mock.patch.object(trial.time, 'monotonic', side_effect=[0, .010, .100, .125]), \
             mock.patch.object(trial.subprocess, 'run', side_effect=[
                 SimpleNamespace(stdout=data) for data in (unchanged, unchanged, changed)]):
            measured = trial.key_latency(Path('/private-fixture'), {}, 1)
        self.assertEqual(measured['samples_ms'], [25.0])
        self.assertEqual(measured['capture_samples_ms'], [10.0])
        self.assertEqual(measured['median_ms'], 25.0)
        self.assertEqual(measured['frame_pixels'], [16, 48])

    def test_only_the_headless_compositor_fixture_can_skip_the_native_colour_handoff(self):
        for headless, backend in ((True, 'wgpu'), (True, 'graphical'), (False, 'wgpu'), (False, 'drm')):
            with self.subTest(headless=headless, backend=backend), tempfile.TemporaryDirectory() as temp:
                env = trial.environment(Path(temp), headless, backend)
                if headless and backend in ('wgpu', 'graphical'):
                    self.assertEqual(env.get('WROS_COMPOSITOR_WARMTH'), 'ready')
                else:
                    self.assertNotIn('WROS_COMPOSITOR_WARMTH', env)

    def test_the_trial_never_inherits_the_writers_home_or_credentials(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            env = trial.environment(path, True, 'wgpu')
            self.assertEqual(env['HOME'], str(path / 'home'))
            self.assertEqual(env['XDG_STATE_HOME'], str(path / 'home/.local/state'))
            self.assertEqual(env['WLR_BACKENDS'], 'headless')
            self.assertNotIn('SSH_AUTH_SOCK', env)
            self.assertNotIn('WROS_TEST_DRIVER_SOCKET', env)
            self.assertNotIn('WROS_OS', env)

    def test_compositor_commands_do_not_need_a_shell(self):
        binary, book = Path('/trial app/grimoire'), Path('/trial book')
        self.assertEqual(trial.command('wgpu', binary, book), ['cage', '-s', '--', str(binary), str(book)])
        self.assertEqual(trial.command('drm', binary, book), [str(binary), str(book)])
        self.assertIn('--config=' + str(ROOT / 'os/config/foot.ini'), trial.command('foot', binary, book))

    def test_memory_excludes_another_writing_sessions_process_group(self):
        with tempfile.TemporaryDirectory() as temp:
            proc = Path(temp)
            for pid, parent, group, rss in [(100, 1, 100, 5), (101, 100, 100, 7),
                                             (102, 1, 102, 900), (103, 101, 103, 11), (104, 103, 103, 3)]:
                path = proc / str(pid)
                path.mkdir()
                fields = ['S', str(parent), str(group)] + ['0'] * 19
                fields[21] = str(rss)
                (path / 'stat').write_text(f'{pid} (app with spaces) ' + ' '.join(fields))
                (path / 'smaps_rollup').write_text('Pss: 10 kB\n')
            result = trial.process_memory(100, proc)
            # foot's editor owns a new PTY group; its colour child counts too.
            self.assertEqual(len(result['processes']), 4)
            self.assertEqual(result['pss_kib'], 40)
            self.assertEqual(result['rss_kib'], 26 * trial.os.sysconf('SC_PAGE_SIZE') // 1024)
