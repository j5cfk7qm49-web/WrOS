"""Compositor preferences never grant the session direct DRM access."""
import importlib.machinery
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
display = importlib.machinery.SourceFileLoader('compositor_display', str(ROOT / 'os/scripts/wros-display')).load_module()


class CompositorPreferences(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='wros-compositor-preferences-')
        self.root = Path(self.temporary.name)
        self.manager = display.Manager(self.root / 'etc/display.json', self.root / 'run', owner=42)
        self.manager.runtime.mkdir()
        # Any hardware call here is a test failure: only preferences may change.
        self.manager.hardware = object()

    def tearDown(self):
        self.temporary.cleanup()

    def save(self, stamp='missing', token='a' * 32):
        return self.manager.compositor_save(True, 2200, 75, token, stamp)

    def test_keep_is_durable_and_checks_the_reviewed_configuration(self):
        self.manager.config.parent.mkdir()
        previous = os.umask(0o077)
        try:
            self.assertEqual(self.save()['status'], 'committed')
        finally:
            os.umask(previous)
        saved = self.manager.saved()
        self.assertEqual(saved, {'version': 1, 'enabled': True, 'kelvin': 2200,
                                'strength': 75, 'generation': 'a' * 32})
        self.assertEqual(self.manager.config.stat().st_mode & 0o777, 0o644)
        original = self.manager.config.read_bytes()
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.save()
        self.assertEqual(self.manager.config.read_bytes(), original)
        self.save(self.manager.config_stamp(), 'b' * 32)
        self.assertEqual(self.manager.saved()['generation'], 'b' * 32)

    def test_unknown_preferences_and_native_transactions_are_preserved(self):
        self.manager.config.parent.mkdir()
        self.manager.config.write_text('{"version":99}')
        self.manager.config.chmod(0o600)
        with self.assertRaises(ValueError):
            self.save()
        with self.assertRaises(ValueError):
            self.manager.prepare_compositor()
        self.assertEqual(self.manager.config.read_text(), '{"version":99}')
        self.assertEqual(self.manager.config.stat().st_mode & 0o777, 0o600)
        self.manager.config.unlink()
        for owned in (self.manager.pending, self.manager.baseline):
            owned.write_bytes(b'owned native state')
            with self.assertRaisesRegex(ValueError, 'native'):
                self.save()
            self.assertFalse(self.manager.config.exists())
            self.assertEqual(owned.read_bytes(), b'owned native state')
            owned.unlink()

    def test_invalid_bounds_token_and_stamp_never_write_preferences(self):
        for arguments in [(True, 999, 75, 'a' * 32, 'missing'),
                          (True, 2200, 101, 'a' * 32, 'missing'),
                          (True, 2200, 75, ';bad', 'missing'),
                          (True, 2200, 75, 'a' * 32, '../other')]:
            with self.assertRaises(ValueError):
                self.manager.compositor_save(*arguments)
            self.assertFalse(self.manager.config.exists())

    def test_prepare_restores_the_native_baseline_before_cage_can_start(self):
        from test_display_manager import FakeHardware
        events = []
        hardware = FakeHardware(events)
        self.manager.hardware = hardware
        baseline = hardware.current
        warm = display.transform(baseline, display.preference(True, 2200, 100))
        self.manager.baseline.write_bytes(baseline)
        self.manager.current.write_bytes(warm)
        hardware.current = warm
        self.manager.run = lambda args: events.append(args[1])
        self.manager.prepare_compositor()
        self.assertEqual(hardware.current, baseline)
        self.assertFalse(self.manager.baseline.exists())
        self.assertFalse(self.manager.current.exists())
        self.assertEqual(events, ['capture', 'apply', 'stop'])
        self.assertFalse(self.manager.config.exists())

    def test_prepare_makes_old_root_only_preferences_readable(self):
        self.save()
        original = self.manager.config.read_bytes()
        self.manager.config.chmod(0o600)
        self.manager.prepare_compositor()
        self.assertEqual(self.manager.config.stat().st_mode & 0o777, 0o644)
        self.assertEqual(self.manager.config.read_bytes(), original)

    def test_prepare_does_not_change_permissions_through_a_link(self):
        self.save()
        self.manager.config.chmod(0o600)
        linked = self.root / 'other.json'
        os.link(self.manager.config, linked)
        with self.assertRaisesRegex(ValueError, 'linked display preferences'):
            self.manager.prepare_compositor()
        self.assertEqual(linked.stat().st_mode & 0o777, 0o600)

    def test_prepare_preserves_a_display_changed_by_another_owner(self):
        from test_display_manager import FakeHardware
        self.manager.hardware = FakeHardware([])
        self.manager.baseline.write_bytes(self.manager.hardware.current)
        self.manager.current.write_bytes(b'expect a different curve')
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.manager.prepare_compositor()
        self.assertTrue(self.manager.baseline.exists())


if __name__ == '__main__':
    unittest.main()
