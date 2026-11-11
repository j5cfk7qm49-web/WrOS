"""The real packaged Cage must commit gamma requests and report driver refusal."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CageGamma(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if os.geteuid() == 0:
            raise RuntimeError('Cage integration tests must run unprivileged')
        cls.temporary = tempfile.TemporaryDirectory(prefix='wros-cage-gamma-')
        cls.directory = Path(cls.temporary.name)
        cls.cage = cls.directory / 'cage'
        with (cls.directory / 'build.log').open('w') as log:
            # Keep compiler output bounded while retaining the failure details.
            command = ['python3', str(ROOT / 'tools/build-cage.py'), '--output', str(cls.cage)]
            result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise AssertionError((cls.directory / 'build.log').read_text()[-6000:])
        flags = subprocess.check_output(['pkg-config', '--cflags', 'wlroots-0.18'], text=True).split()
        subprocess.run(['cc', '-O2', '-Wall', '-Wextra', '-Werror', '-shared', '-fPIC',
                        *flags, str(ROOT / 'os/tests/cage-gamma-driver.c'), '-ldl',
                        '-o', str(cls.directory / 'driver.so')], check=True)
        subprocess.run(['cc', '-O2', '-Wall', '-Wextra', '-Werror',
                        str(ROOT / 'os/tests/cage-gamma-client.c'), '-lwayland-client',
                        '-o', str(cls.directory / 'client')], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_actual_compositor_applies_restores_and_reports_rejected_luts(self):
        for rejected in (False, True):
            with self.subTest(rejected=rejected):
                trace = self.directory / ('rejected.trace' if rejected else 'accepted.trace')
                runtime = self.directory / ('runtime-rejected' if rejected else 'runtime-accepted')
                runtime.mkdir(mode=0o700)
                env = os.environ | {'XDG_RUNTIME_DIR': str(runtime), 'WLR_BACKENDS': 'headless',
                    'WLR_HEADLESS_OUTPUTS': '1', 'WLR_RENDERER': 'pixman',
                    'LD_PRELOAD': str(self.directory / 'driver.so'), 'WROS_CAGE_GAMMA_TRACE': str(trace)}
                if rejected:
                    env['WROS_CAGE_GAMMA_REJECT'] = '1'
                else:
                    env.pop('WROS_CAGE_GAMMA_REJECT', None)
                command = [str(self.cage), '-s', '--', str(self.directory / 'client')]
                if rejected:
                    command.append('reject')
                result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr[-4000:])
                commits = trace.read_text().splitlines() if trace.exists() else []
                self.assertTrue(any(line.endswith('256 65535 51000 25500') for line in commits), commits)
                self.assertTrue(any(line.endswith(' 0') for line in commits), commits)
