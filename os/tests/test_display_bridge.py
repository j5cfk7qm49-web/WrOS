"""Exercise the actual production DRM bridge against a test-only fake driver.

No privileged process or real display is used. LD_PRELOAD belongs only to this
normal-user fixture; it is never supplied to the installed root manager.
"""
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DisplayBridge(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if os.geteuid() == 0:
            raise RuntimeError('DRM bridge tests must run unprivileged')
        cls.temporary = tempfile.TemporaryDirectory(prefix='wros-drm-test-')
        base = Path(cls.temporary.name)
        cls.bridge, cls.fake = base / 'bridge', base / 'fake-drm.so'
        flags = subprocess.check_output(['pkg-config', '--cflags', '--libs', 'libdrm'], text=True).split()
        subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-D_FORTIFY_SOURCE=3', '-fstack-protector-strong',
                        str(ROOT / 'os/helpers/wros-display-hw.c'), *flags, '-Wl,-z,relro,-z,now', '-o', str(cls.bridge)], check=True)
        subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-shared', '-fPIC',
                        str(ROOT / 'os/tests/fake-display-drm.c'), *flags, '-ldl', '-o', str(cls.fake)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def run_bridge(self, operation, payload=None, scenario=None):
        environment = os.environ | {'LD_PRELOAD': str(self.fake)}
        if scenario:
            environment['WROS_TEST_DRM_SCENARIO'] = scenario
        else:
            environment.pop('WROS_TEST_DRM_SCENARIO', None)
        return subprocess.run([str(self.bridge), operation], input=payload, capture_output=True,
                              env=environment, timeout=5)

    def wanted(self):
        captured = self.run_bridge('capture')
        self.assertEqual(captured.returncode, 0, captured.stderr)
        self.assertEqual(captured.stdout[:8], b'WrOSGM1\0')
        data = bytearray(captured.stdout)
        at = 12
        self.assertEqual(struct.unpack_from('<I', data, 8)[0], 2)
        for _ in range(2):
            points = struct.unpack_from('<I', data, at + 16)[0]
            at += 28
            for channel in range(3):
                for point in range(points):
                    offset = at + (channel * points + point) * 2
                    original = struct.unpack_from('<H', data, offset)[0]
                    factor = (1.0, 0.7, 0.2)[channel]
                    struct.pack_into('<H', data, offset, round(original * factor))
            at += points * 6
        return bytes(data)

    def test_actual_bridge_refuses_unprivileged_hardware_access_without_the_fixture(self):
        environment = os.environ.copy()
        environment.pop('LD_PRELOAD', None)
        result = subprocess.run([str(self.bridge), 'capture'], env=environment,
                                capture_output=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'authorized root manager', result.stderr)
        self.assertEqual(result.stdout, b'')

    def test_valid_multi_output_application_requires_real_api_readback(self):
        result = self.run_bridge('apply', self.wanted())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b'applied-and-readback-verified\n')
        self.assertIn(b'fake-writes=2', result.stderr)
        self.assertIn(b'fake-restored=0', result.stderr)

    def test_partial_set_or_readback_failure_restores_both_original_calibrations(self):
        for scenario in ('set-failure', 'readback'):
            with self.subTest(scenario=scenario):
                result = self.run_bridge('apply', self.wanted(), scenario)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b'')
                self.assertIn(b'original ramps restored', result.stderr)
                self.assertIn(b'fake-restored=1', result.stderr)

    def test_failed_restoration_is_not_reported_as_success(self):
        result = self.run_bridge('apply', self.wanted(), 'rollback-failure')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'restoration incomplete', result.stderr)
        self.assertIn(b'fake-restored=0', result.stderr)

    def test_hotplug_during_apply_is_not_success_and_old_calibration_is_not_written_to_new_outputs(self):
        result = self.run_bridge('apply', self.wanted(), 'hotplug-after-set')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b'')
        self.assertIn(b'restoration incomplete', result.stderr)
        self.assertIn(b'fake-writes=2', result.stderr)

    def test_unsupported_disconnected_and_changed_outputs_never_receive_color_writes(self):
        for scenario in ('unsupported', 'disconnected', 'topology'):
            with self.subTest(scenario=scenario):
                result = self.run_bridge('apply', self.wanted(), scenario)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(b'fake-writes=0', result.stderr)
                self.assertIn(b'fake-restored=1', result.stderr)

    def test_invalid_duplicate_truncated_and_oversized_payloads_do_not_open_devices(self):
        valid = self.wanted()
        duplicate = bytearray(valid)
        first = valid[12:40]
        duplicate[64:92] = first  # each synthetic output is 28 + 24 bytes
        bad_points = bytearray(valid)
        struct.pack_into('<I', bad_points, 28, 0xFFFFFFFF)
        for data in (b'', b'not-a-snapshot', valid[:-1], valid + b'x', bytes(duplicate),
                     bytes(bad_points), valid + b'x' * (1024 * 1024)):
            with self.subTest(length=len(data)):
                result = self.run_bridge('apply', data)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(b'fake-drm-opens=0', result.stderr)
                self.assertIn(b'fake-writes=0', result.stderr)
