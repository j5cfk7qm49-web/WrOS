"""Normal-user transaction tests with real state files and a fake display/timer."""
import importlib.machinery
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
module = importlib.machinery.SourceFileLoader('wros_display', str(ROOT / 'os/scripts/wros-display')).load_module()


def snapshot():
    values = (0, 10000, 30000, 65535, 0, 11000, 32000, 60000, 0, 12000, 33000, 58000)
    return b'WrOSGM1\0' + struct.pack('<I7I12H', 1, 0, 100, 640, 480, 4, 11, 12, *values)


class FakeHardware:
    def __init__(self, events):
        self.events, self.current = events, snapshot()
        self.failed_apply = self.unavailable = False

    def capture(self):
        self.events.append('capture')
        if self.unavailable:
            raise RuntimeError('display driver unsupported')
        return self.current

    def apply(self, data):
        module.decode(data)
        self.events.append('apply')
        if self.failed_apply:
            self.failed_apply = False
            self.current = data
            raise RuntimeError('fixture incomplete hardware restoration')
        self.current = data


class DisplayTransactions(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='wros-display-state-')
        self.root = Path(self.temporary.name)
        self.events, self.now = [], 100.0
        self.hardware = FakeHardware(self.events)
        self.fail_start = self.fail_stop = False

        def runner(arguments):
            verb = arguments[1]
            self.events.append(verb)
            if (verb == 'restart' and self.fail_start) or (verb == 'stop' and self.fail_stop):
                raise subprocess.CalledProcessError(1, arguments)

        self.manager = module.Manager(self.root / 'etc/display.json', self.root / 'run',
                                      self.hardware, runner, lambda: self.now, owner=42)

    def tearDown(self):
        self.temporary.cleanup()

    def preview(self, kelvin=2200, strength=100, enabled=True):
        return self.manager.preview(enabled, kelvin, strength)['token']

    def confirm(self, token):
        return self.manager.resolve(token, True)

    def test_timer_is_armed_before_real_state_changes_and_cancel_restores_exact_calibration(self):
        before = self.hardware.current
        token = self.preview()
        self.assertLess(self.events.index('restart'), self.events.index('apply'))
        self.assertNotEqual(self.hardware.current, before)
        self.assertFalse(self.manager.config.exists())
        self.manager.resolve(token, False)
        self.assertEqual(self.hardware.current, before)
        self.assertFalse(self.manager.pending.exists())

    def test_temperature_strength_and_disable_preserve_original_curves_without_compounding(self):
        before = self.hardware.current
        self.assertEqual(module.transform(before, module.preference(True, 6500, 100)), before)
        self.assertEqual(module.transform(before, module.preference(True, 1000, 0)), before)
        self.assertEqual(module.transform(before, module.preference(False, 1000, 100)), before)
        red, green, blue = module.factors(2200, 100)
        self.assertEqual(red, 1)
        self.assertLess(blue, green)
        self.assertLess(green, 1)
        self.confirm(self.preview(2200))
        self.confirm(self.preview(1800))
        self.assertEqual(self.hardware.current, module.transform(before, module.preference(True, 1800, 100)))
        self.assertEqual(module.decode(self.hardware.current)[0][1][:4], module.decode(before)[0][1][:4])

    def test_expired_and_wrong_user_confirmations_cannot_commit(self):
        token = self.preview()
        self.manager.owner = 43
        with self.assertRaises(PermissionError):
            self.confirm(token)
        self.assertTrue(self.manager.pending.exists())
        self.manager.owner = 42
        self.now = 116
        with self.assertRaisesRegex(ValueError, 'expired'):
            self.confirm(token)
        self.assertEqual(self.hardware.current, snapshot())
        self.assertFalse(self.manager.config.exists())

    def test_confirm_is_durable_and_a_late_timer_never_reverts_it(self):
        token = self.preview()
        pending = self.manager.pending.read_bytes()
        restore = self.manager.restore.read_bytes()
        self.confirm(token)
        wanted = self.hardware.current
        # Crash boundary after durable config rename but before pending unlink.
        module.atomic(self.manager.pending, pending)
        module.atomic(self.manager.restore, restore)
        applied = self.events.count('apply')
        self.manager.rollback()
        self.assertEqual(self.events.count('apply'), applied)
        self.assertEqual(self.hardware.current, wanted)
        self.assertEqual(self.manager.saved()['generation'], token)

    def test_failed_timer_arm_never_changes_colors(self):
        self.fail_start = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.preview()
        self.assertNotIn('apply', self.events)
        self.assertEqual(self.hardware.current, snapshot())
        self.assertFalse(self.manager.pending.exists())

    def test_incomplete_hardware_failure_keeps_independent_recovery_state(self):
        self.hardware.failed_apply = True
        with self.assertRaisesRegex(RuntimeError, 'incomplete'):
            self.preview()
        self.assertTrue(self.manager.pending.exists())
        self.assertTrue(self.manager.restore.exists())
        self.assertIn('restart', self.events)
        self.manager.rollback()
        self.assertEqual(self.hardware.current, snapshot())

    def test_missing_driver_is_unavailable_and_creates_no_preview(self):
        self.hardware.unavailable = True
        self.assertFalse(self.manager.status()['available'])
        with self.assertRaisesRegex(RuntimeError, 'unsupported'):
            self.preview()
        self.assertFalse(self.manager.pending.exists())
        self.assertNotIn('apply', self.events)

    def test_off_preview_cancel_and_commit_restore_the_exact_original(self):
        self.confirm(self.preview())
        warm = self.hardware.current
        token = self.preview(enabled=False)
        self.assertEqual(self.hardware.current, snapshot())
        self.manager.resolve(token, False)
        self.assertEqual(self.hardware.current, warm)
        self.confirm(self.preview(enabled=False))
        self.assertEqual(self.hardware.current, snapshot())
        self.assertFalse(self.manager.saved()['enabled'])
        self.assertFalse(self.manager.baseline.exists())

    def test_future_config_is_preserved_and_does_not_block_physical_preview_rollback(self):
        future = b'{"version":99,"future":"preserved"}\n'
        module.atomic(self.manager.config, future)
        with self.assertRaises(ValueError):
            self.preview()
        self.assertEqual(self.events, [])
        self.manager.config.unlink()
        token = self.preview()
        module.atomic(self.manager.config, future)
        with self.assertRaisesRegex(ValueError, 'changed externally'):
            self.confirm(token)
        self.manager.rollback()
        self.assertEqual(self.hardware.current, snapshot())
        self.assertEqual(self.manager.config.read_bytes(), future)

    def test_external_color_changes_and_preference_changes_are_not_overwritten(self):
        self.confirm(self.preview())
        external = module.transform(snapshot(), module.preference(True, 3500, 30))
        self.hardware.current = external
        before = self.events.count('apply')
        with self.assertRaisesRegex(ValueError, 'changed externally'):
            self.preview(1800)
        self.assertEqual(self.hardware.current, external)
        self.assertEqual(self.events.count('apply'), before)

    def test_invalid_choices_and_state_are_rejected_before_color_io(self):
        for args in ((True, 999, 100), (True, 6501, 100), (True, 2200, -1),
                     (True, 2200, 101), (True, 2200.0, 50), ('on', 2200, 50), (True, 2200, True)):
            with self.subTest(args=args), self.assertRaises(ValueError):
                self.manager.preview(*args)
        self.assertEqual(self.events, [])
        self.manager.runtime.mkdir()
        self.manager.baseline.symlink_to(self.root / 'missing')
        with self.assertRaises(ValueError):
            self.preview()
        self.assertNotIn('apply', self.events)

    def test_boot_reapplies_only_confirmed_preferences_and_retains_recovery_on_failure(self):
        saved = {'version': 1, **module.preference(True, 2200, 100), 'generation': 'a' * 32}
        module.json_write(self.manager.config, saved)
        self.hardware.failed_apply = True
        with self.assertRaises(RuntimeError):
            self.manager.boot()
        self.assertTrue(self.manager.pending.exists())
        self.manager.rollback()
        self.assertEqual(self.hardware.current, snapshot())
        self.assertEqual(self.manager.saved(), saved)
        self.assertFalse(self.manager.status()['applied'])
        self.manager.boot()
        self.assertEqual(self.hardware.current, module.transform(snapshot(), saved))
        self.assertFalse(self.manager.pending.exists())

    def test_timer_stop_failure_cannot_undo_a_durable_commit(self):
        token = self.preview()
        self.fail_stop = True
        self.confirm(token)
        before = self.hardware.current
        self.manager.rollback()
        self.assertEqual(self.hardware.current, before)
        self.assertEqual(self.manager.saved()['generation'], token)
