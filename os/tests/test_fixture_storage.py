"""Synthetic sysfs trees only; no real disk is accessed."""
from pathlib import Path
import base64
import tempfile
import unittest

from fixture_storage import TARGET_BYTES, belongs_to_target, fixture_serial, inspect_target

TOKEN = '0123456789abcdef0123456789abcdef'
DESCRIPTION = {'bytes': TARGET_BYTES, 'serial': fixture_serial(TOKEN)}


class FixtureStorage(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='wros-owned-target-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sysroot = self.root/'class'; self.sysroot.mkdir()
        self.real = self.root/'devices/block/vda'; self.real.mkdir(parents=True)
        (self.sysroot/'vda').symlink_to(self.real, target_is_directory=True)
        for name, value in {'ro': '0', 'size': str(TARGET_BYTES//512),
                            'serial': DESCRIPTION['serial'], 'dev': '252:0'}.items():
            (self.real/name).write_text(value+'\n')

    def target(self):
        return inspect_target(DESCRIPTION, TOKEN, self.sysroot)

    def test_full_nonce_fits_the_virtio_identifier_without_truncation_or_option_separators(self):
        for token in (TOKEN, '00'*16, 'ff'*16):
            serial = fixture_serial(token)
            self.assertEqual(len(serial), 20)
            self.assertEqual(base64.b85decode(serial), bytes.fromhex(token))
            self.assertNotIn(',', serial)
            self.assertFalse(any(c.isspace() or c == '\0' for c in serial))

    def mapper(self, name, slaves):
        path = self.sysroot/name; (path/'slaves').mkdir(parents=True)
        for slave in slaves:
            (path/'slaves'/slave).symlink_to(self.sysroot/slave, target_is_directory=True)

    def test_exact_nonce_capacity_and_device_identity_are_required(self):
        self.assertEqual(self.target()['major'], 252)
        for changed in ({'bytes': TARGET_BYTES, 'serial': 'another target'},
                        DESCRIPTION | {'path': '/dev/sda'},
                        DESCRIPTION | {'bytes': 1}):
            with self.subTest(description=changed), self.assertRaises(ValueError):
                inspect_target(changed, TOKEN, self.sysroot)
        (self.real/'serial').write_text('WROS-QA-'+'f'*32)
        with self.assertRaisesRegex(ValueError, 'serial differs'):
            self.target()

    def test_truncation_missing_metadata_and_a_readonly_target_fail_closed(self):
        (self.real/'size').write_text('1\n')
        with self.assertRaisesRegex(ValueError, 'capacity differs'):
            self.target()
        (self.real/'size').write_text(str(TARGET_BYTES//512)+'\n')
        (self.real/'ro').write_text('1\n')
        with self.assertRaisesRegex(ValueError, 'not writable'):
            self.target()
        (self.real/'ro').unlink()
        with self.assertRaises(OSError):
            self.target()

    def test_real_partition_ancestry_and_exclusive_mapper_chains(self):
        identity = self.target()
        partition = self.real/'vda3'; partition.mkdir()
        (partition/'partition').write_text('3\n')
        (self.sysroot/'vda3').symlink_to(partition, target_is_directory=True)
        self.mapper('dm-0', ['vda3']); self.mapper('dm-1', ['dm-0'])
        for name in ('vda', 'vda3', 'dm-0', 'dm-1'):
            self.assertTrue(belongs_to_target(name, identity, self.sysroot), name)
        foreign = self.sysroot/'vdb'; foreign.mkdir()
        self.mapper('dm-2', ['dm-0', 'vdb'])
        self.assertFalse(belongs_to_target('dm-2', identity, self.sysroot))
        self.assertFalse(belongs_to_target('vdb', identity, self.sysroot))

    def test_names_cycles_and_foreign_partition_paths_cannot_claim_the_target(self):
        identity = self.target()
        fake = self.sysroot/'vda9'; fake.mkdir(); (fake/'partition').write_text('9\n')
        self.assertFalse(belongs_to_target('vda9', identity, self.sysroot))
        for name in ('.', '..', '../vda', '/dev/vda', 'vda\n'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                belongs_to_target(name, identity, self.sysroot)
        self.mapper('dm-0', ['dm-1']); self.mapper('dm-1', ['dm-0'])
        with self.assertRaisesRegex(ValueError, 'Cyclic'):
            belongs_to_target('dm-0', identity, self.sysroot)

    def test_changed_kernel_device_identity_cannot_reuse_an_old_receipt(self):
        identity = self.target()
        (self.real/'dev').write_text('252:16\n')
        with self.assertRaisesRegex(ValueError, 'identity changed'):
            belongs_to_target('vda', identity, self.sysroot)
        (self.real/'dev').write_text('252:0\n')
        (self.real/'size').write_text('1\n')
        with self.assertRaisesRegex(ValueError, 'capacity changed'):
            belongs_to_target('vda', identity, self.sysroot)
