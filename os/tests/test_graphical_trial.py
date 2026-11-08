"""The trial isolates writing and counts only its private process group."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('graphical_trial', ROOT / 'tools/graphical_trial.py')
trial = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trial)


class Trial(unittest.TestCase):
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
                                             (102, 1, 102, 900), (103, 101, 103, 11)]:
                path = proc / str(pid)
                path.mkdir()
                fields = ['S', str(parent), str(group)] + ['0'] * 19
                fields[21] = str(rss)
                (path / 'stat').write_text(f'{pid} (app with spaces) ' + ' '.join(fields))
                (path / 'smaps_rollup').write_text('Pss: 10 kB\n')
            result = trial.process_memory(100, proc)
            # foot's editor owns a new PTY session/process group and still counts.
            self.assertEqual(len(result['processes']), 3)
            self.assertEqual(result['pss_kib'], 30)
            self.assertEqual(result['rss_kib'], 23 * trial.os.sysconf('SC_PAGE_SIZE') // 1024)
