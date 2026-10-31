"""Disk guard policy fixtures; no block device is opened or modified."""
import importlib.util
import os
from pathlib import Path
import select
import shutil
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

    def test_full_install_mode_still_refuses_write_seeds_plaintext_keys_and_boot_bypasses(self):
        defaults = (b'd-i partman-auto/disk string /dev/vda\n'
                    b'd-i partman-auto/method string crypto\n'
                    b'd-i passwd/user-password-crypted password $6$fixture$'+b'A'*86+b'\n'
                    b'd-i apt-setup/services-select multiselect\n'
                    b'tasksel tasksel/first multiselect\n'
                    b'd-i partman-auto-crypto/erase_disks boolean false\n')
        guest.validate_review_preseed(defaults, installing=True)
        for value in (defaults+b'd-i partman/confirm boolean true\n',
                defaults+b'd-i partman-crypto/passphrase password generated\n',
                defaults+b'd-i preseed/late_command string /bin/true\n',
                defaults+b'd-i debian-installer/add-kernel-opts string init=/bin/sh\n',
                defaults.replace(b'erase_disks boolean false', b'erase_disks boolean true'),
                defaults.replace(b'$6$fixture$'+b'A'*86, b'plaintext-fixture')):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                guest.validate_review_preseed(value, installing=True)
        with self.assertRaises(RuntimeError):
            guest.validate_review_preseed(defaults)

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

    def test_cleanup_target_stack_unmounts_deepest_first_disables_swap_and_closes_lvm_before_luks(self):
        token = '0123456789abcdef0123456789abcdef'
        description = {'bytes': guest.storage.TARGET_BYTES, 'serial': guest.storage.fixture_serial(token)}
        with tempfile.TemporaryDirectory(prefix='wros-installer-cleanup-') as directory:
            root = Path(directory)
            sysroot = root/'sys'; sysroot.mkdir()
            real = root/'devices/block/vda'; real.mkdir(parents=True)
            (sysroot/'vda').symlink_to(real, target_is_directory=True)
            for name, value in {'ro': '0', 'size': str(guest.storage.TARGET_BYTES//512),
                                'serial': description['serial'], 'dev': '252:0'}.items():
                (real/name).write_text(value+'\n')
            part = real/'vda3'; part.mkdir()
            (part/'partition').write_text('3\n')
            (sysroot/'vda3').symlink_to(part, target_is_directory=True)

            def add_dm(block, slaves, dm_name, dm_uuid):
                p = sysroot/block
                (p/'slaves').mkdir(parents=True)
                for s in slaves:
                    (p/'slaves'/s).symlink_to(sysroot/s, target_is_directory=True)
                (p/'dm').mkdir()
                (p/'dm/name').write_text(dm_name+'\n')
                (p/'dm/uuid').write_text(dm_uuid+'\n')

            add_dm('dm-0', ['vda3'], 'vda3_crypt', 'CRYPT-LUKS2-0123-vda3_crypt')
            add_dm('dm-1', ['dm-0'], 'wros--vg-root', 'LVM-abcd-root')
            add_dm('dm-2', ['dm-0'], 'wros--vg-swap_1', 'LVM-abcd-swap')

            identity = guest.storage.inspect_target(description, token, sysroot)
            mountinfo = root/'mountinfo'
            mountinfo.write_text(
                '40 25 253:1 / /run/live/installer/target rw - ext4 /dev/mapper/wros--vg-root rw\n'
                '41 40 252:2 / /run/live/installer/target/boot rw - ext2 /dev/vda2 rw\n'
                '42 41 252:1 / /run/live/installer/target/boot/efi rw - vfat /dev/vda1 rw\n'
            )
            swaps = root/'swaps'
            swaps.write_text(
                'Filename\tType\tSize\tUsed\tPriority\n'
                '/dev/mapper/wros--vg-swap_1\tpartition\t999932\t0\t-2\n'
            )
            commands = []

            def fake_runner(*args):
                commands.append(args)
                if args[0] == '/bin/umount':
                    target_mp = args[1]
                    lines = [
                        line for line in mountinfo.read_text().splitlines()
                        if line.split()[4] != target_mp
                    ]
                    mountinfo.write_text('\n'.join(lines) + ('\n' if lines else ''))
                elif args[0] == '/sbin/swapoff':
                    swaps.write_text('Filename\tType\tSize\tUsed\tPriority\n')
                elif args[0] in ('/sbin/dmsetup', '/sbin/cryptsetup'):
                    dm_target = args[-1]
                    for dm_dir in list(sysroot.glob('dm-*')):
                        if (dm_dir/'dm/name').read_text().strip() == dm_target:
                            shutil.rmtree(dm_dir)

            report = guest.cleanup_target_stack(
                identity,
                target_root=Path('/run/live/installer/target'),
                sysroot=sysroot,
                mountinfo_path=mountinfo,
                swaps_path=swaps,
                runner=fake_runner,
            )
            self.assertTrue(report['clean'])
            self.assertEqual(
                commands,
                [
                    ('/bin/umount', '/run/live/installer/target/boot/efi'),
                    ('/bin/umount', '/run/live/installer/target/boot'),
                    ('/bin/umount', '/run/live/installer/target'),
                    ('/sbin/swapoff', '/dev/mapper/wros--vg-swap_1'),
                    ('/sbin/dmsetup', 'remove', '--retry', 'wros--vg-root'),
                    ('/sbin/dmsetup', 'remove', '--retry', 'wros--vg-swap_1'),
                    ('/sbin/cryptsetup', 'close', 'vda3_crypt'),
                ],
            )

            # Verify handoff_to_writing_session switches to tty1 and starts greetd.service
            with patch.object(guest, 'installer_readers_retired', return_value=True):
                handoff_cmds = []
                guest.handoff_to_writing_session(
                    identity=identity,
                    target_root=Path('/run/live/installer/target'),
                    sysroot=sysroot,
                    mountinfo_path=mountinfo,
                    swaps_path=swaps,
                    runner=lambda *a: handoff_cmds.append(a),
                )
                self.assertEqual(
                    handoff_cmds,
                    [
                        ('/bin/chvt', '1'),
                        ('/usr/bin/systemctl', 'start', 'greetd.service'),
                    ],
                )
