"""Disk guard policy fixtures; no block device is opened or modified."""
import importlib.util
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('native_guest', Path(__file__).with_name('native_installer_guest.py'))
guest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guest)


class DisklessNativeGuest(unittest.TestCase):
    def test_live_reader_blocks_handoff_but_kernel_confirmed_exit_does_not(self):
        with tempfile.TemporaryDirectory(prefix='wros-reader-retirement-') as directory:
            child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
            descriptor = os.pidfd_open(child.pid)
            try:
                process = Path(directory)/str(child.pid); process.mkdir()
                (process/'comm').write_text('wros-input\n')
                with patch.object(guest, 'Path') as root:
                    root.return_value.glob.return_value = [process]
                    self.assertFalse(guest.writing_readers_retired())
                    child.terminate()
                    self.assertTrue(select.select([descriptor], [], [], 5)[0])
                    # Do not reap it yet: an exited zombie cannot read input.
                    self.assertTrue(guest.writing_readers_retired())
            finally:
                os.close(descriptor)
                if child.poll() is None:
                    child.terminate()
                child.wait(timeout=5)

    def test_review_defaults_cannot_preapprove_writes_inject_commands_or_supply_unlock_keys(self):
        defaults = (b'd-i partman-auto/disk string /dev/vda\n'
                    b'd-i partman-auto/method string crypto\n')
        guest.validate_review_preseed(defaults)
        for value in (defaults.replace(b'/dev/vda', b'/dev/sda'),
                defaults+b'd-i partman/confirm boolean true\n',
                defaults+b'd-i preseed/early_command string /bin/true\n',
                defaults+b'd-i partman-crypto/passphrase password fixture\n',
                defaults+b'd-i passwd/user-password-crypted password unexpected\n',
                defaults+b'd-i ethdetect/module_select select unexpected\n',
                defaults+b'd-i partman-auto/disk string /dev/vda\n'):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                guest.validate_review_preseed(value)

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
