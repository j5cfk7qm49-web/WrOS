"""Disk guard policy fixtures; no block device is opened or modified."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('native_guest', Path(__file__).with_name('native_installer_guest.py'))
guest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guest)


class DisklessNativeGuest(unittest.TestCase):
    def test_only_readonly_media_or_unconfigured_zero_length_loop_slots_are_allowed(self):
        with tempfile.TemporaryDirectory(prefix='wros-disk-guard-') as directory:
            root = Path(directory)
            for name, readonly, size, expected in [('sr0', '1', '999', True),
                    ('vda', '0', '999', False), ('sda', '0', '0', False),
                    ('sr1', '0', '0', False), ('loop1', '0', '0', True),
                    ('loop2', '0', '999', False), ('loophole', '0', '0', False)]:
                path = root/name; path.mkdir()
                (path/'ro').write_text(readonly+'\n'); (path/'size').write_text(size+'\n')
                with self.subTest(device=name):
                    self.assertEqual(guest.block_is_readonly_or_unconfigured_loop(path), expected)
            (root/'loop1/loop').mkdir()
            (root/'loop1/loop/backing_file').write_text('/private/backing-image\n')
            self.assertFalse(guest.block_is_readonly_or_unconfigured_loop(root/'loop1'))

    def test_unreadable_device_metadata_fails_closed(self):
        with tempfile.TemporaryDirectory(prefix='wros-disk-guard-missing-') as directory:
            with self.assertRaises(OSError):
                guest.block_is_readonly_or_unconfigured_loop(Path(directory)/'missing')
