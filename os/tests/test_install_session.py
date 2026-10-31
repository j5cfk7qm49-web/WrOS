"""Production encrypted-installer session supervisor unit tests.

Pure policy, preseed, process-retirement, storage-baseline, LUKS2/LVM/swap/mount
teardown, and target-verification tests using synthetic sysfs/proc trees; never
opens or modifies a host block device or terminal.
"""
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader(
    'wros_install_session',
    str(ROOT / 'os/scripts/wros-install-session'),
)
spec = importlib.util.spec_from_loader(loader.name, loader)
supervisor = importlib.util.module_from_spec(spec)
loader.exec_module(supervisor)


class InstallSessionSupervisorTests(unittest.TestCase):
    def test_session_must_match_the_live_callers_active_local_seat_and_tty1(self):
        good = {
            'User': '1000',
            'Active': 'yes',
            'Remote': 'no',
            'TTY': 'tty1',
            'VTNr': '1',
            'Seat': 'seat0',
            'Type': 'tty',
            'Class': 'user',
        }
        self.assertTrue(supervisor.session_allowed(good))
        for key, value in {
            'User': '1001',
            'Active': 'no',
            'Remote': 'yes',
            'TTY': 'tty2',
            'VTNr': '7',
            'Seat': 'seat1',
            'Type': 'x11',
            'Class': 'greeter',
        }.items():
            with self.subTest(key=key):
                self.assertFalse(supervisor.session_allowed(good | {key: value}))

    def test_shipping_preseed_matches_repo_and_refuses_disk_writes_or_credentials(self):
        shipping = (ROOT / 'os/config/installer.preseed').read_bytes()
        self.assertTrue(supervisor.validate_shipping_preseed(shipping))
        for bad in (
            shipping + b'd-i partman-auto/disk string /dev/sda\n',
            shipping + b'd-i partman/confirm boolean true\n',
            shipping + b'd-i partman-crypto/passphrase password secret\n',
            shipping + b'd-i passwd/user-password-crypted password $6$rounds=1000$saltsalt$hash\n',
            shipping + b'd-i preseed/early_command string /bin/sh\n',
            shipping + b'd-i preseed/late_command string /bin/sh\n',
            shipping.replace(b'use_mirror boolean false', b'use_mirror boolean true'),
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                supervisor.validate_shipping_preseed(bad)

    def test_systemd_command_contains_namespace_isolation_and_execstoppost_cleanup(self):
        args = supervisor.systemd_command(1000, 42, '987654')
        self.assertEqual(args[0], '/usr/bin/systemd-run')
        for flag in (
            '--wait',
            '--collect',
            '--service-type=exec',
            '--unit=wros-installer-session',
            '--property=KillMode=control-group',
            '--property=ExecStopPost=/usr/libexec/wros-install-session cleanup',
        ):
            self.assertIn(flag, args)
        self.assertIn('/usr/bin/unshare', args)
        self.assertEqual(
            args[-5:],
            ['/usr/libexec/wros-install-session', 'worker', '1000', '42', '987654'],
        )
        for bad in ((0, 42, '987654'), (1001, 42, '987654'), (1000, 1, '987654'), (1000, 42, 'bad\n')):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                supervisor.systemd_command(*bad)

    def test_service_roles_cannot_be_called_from_pkexec_or_ordinary_parent(self):
        for argv in (
            ['wros-install-session', 'worker', '1000', '42', '987654'],
            ['wros-install-session', 'cleanup'],
        ):
            for parent, env in ((42, {}), (1, {'PKEXEC_UID': '1000'})):
                with self.subTest(argv=argv, parent=parent), \
                     mock.patch.object(supervisor.sys, 'argv', argv), \
                     mock.patch.object(supervisor.os, 'geteuid', return_value=0), \
                     mock.patch.object(supervisor.os, 'getppid', return_value=parent), \
                     mock.patch.dict(supervisor.os.environ, env, clear=True):
                    with self.assertRaises(ValueError):
                        supervisor.main()

    def test_live_input_reader_blocks_handoff_until_retired(self):
        with tempfile.TemporaryDirectory(prefix='wros-inst-reader-') as directory:
            proc_root = Path(directory)
            child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
            descriptor = os.pidfd_open(child.pid)
            try:
                proc_dir = proc_root / str(child.pid)
                task_dir = proc_dir / 'task' / str(child.pid)
                task_dir.mkdir(parents=True)
                (task_dir / 'comm').write_text('wros-input\n')
                self.assertFalse(supervisor.input_readers_retired(proc_root))
                child.terminate()
                self.assertTrue(select.select([descriptor], [], [], 5)[0])
                self.assertTrue(supervisor.input_readers_retired(proc_root))
            finally:
                os.close(descriptor)
                if child.poll() is None:
                    child.terminate()
                child.wait(timeout=5)

    def test_storage_baseline_protects_live_media_and_cleans_target_lvm_luks_stack(self):
        with tempfile.TemporaryDirectory(prefix='wros-inst-stack-') as directory:
            root = Path(directory)
            sysroot = root / 'sys'
            sysroot.mkdir()
            # Live read-only ISO device sr0 and loop0
            sr0 = sysroot / 'sr0'
            sr0.mkdir()
            (sr0 / 'ro').write_text('1\n')
            loop0 = sysroot / 'loop0'
            loop0.mkdir()
            (loop0 / 'ro').write_text('1\n')
            # Writable target disk nvme0n1 + partition nvme0n1p3
            nvme_real = root / 'devices/nvme0n1'
            nvme_real.mkdir(parents=True)
            (nvme_real / 'ro').write_text('0\n')
            (sysroot / 'nvme0n1').symlink_to(nvme_real, target_is_directory=True)
            part_real = nvme_real / 'nvme0n1p3'
            part_real.mkdir()
            (part_real / 'partition').write_text('3\n')
            (sysroot / 'nvme0n1p3').symlink_to(part_real, target_is_directory=True)

            mountinfo = root / 'mountinfo'
            mountinfo.write_text(
                '20 1 0:20 / /run/live/medium ro - iso9660 /dev/sr0 ro\n'
                '21 1 7:0 / /run/live/rootfs/filesystem.squashfs ro - squashfs /dev/loop0 ro\n'
            )
            swaps = root / 'swaps'
            swaps.write_text('Filename\tType\tSize\tUsed\tPriority\n')

            baseline = supervisor.capture_storage_baseline(
                sysroot=sysroot,
                mountinfo_path=mountinfo,
                swaps_path=swaps,
            )
            self.assertEqual(baseline['protected_disks'], ['loop0', 'sr0'])

            # Simulate Debian Installer creating LUKS2 dm-0 and LVM LVs dm-1 (root) & dm-2 (swap)
            def add_dm(block, slaves, dm_name, dm_uuid):
                p = sysroot / block
                (p / 'slaves').mkdir(parents=True)
                for s in slaves:
                    (p / 'slaves' / s).symlink_to(sysroot / s, target_is_directory=True)
                (p / 'dm').mkdir()
                (p / 'dm/name').write_text(dm_name + '\n')
                (p / 'dm/uuid').write_text(dm_uuid + '\n')

            luks_uuid = '01234567-89ab-cdef-0123-456789abcdef'
            add_dm(
                'dm-0',
                ['nvme0n1p3'],
                'nvme0n1p3_crypt',
                f'CRYPT-LUKS2-{luks_uuid.replace("-", "")}-nvme0n1p3_crypt',
            )
            add_dm('dm-1', ['dm-0'], 'wros--vg-root', 'LVM-abcd-root')
            add_dm('dm-2', ['dm-0'], 'wros--vg-swap_1', 'LVM-abcd-swap')

            # Verify installed target state against the LUKS2 -> LVM chain
            proof = supervisor.verify_installed_target_state(
                passwd_text=(
                    'root:x:0:0:root:/root:/bin/bash\n'
                    '_greetd:x:105:65534::/nonexistent:/usr/sbin/nologin\n'
                    'alice:x:1000:1000:Alice:/home/alice:/bin/bash\n'
                ),
                shadow_text=(
                    'root:!:19000:0:99999:7:::\n'
                    '_greetd:!:19000:0:99999:7:::\n'
                    'alice:$6$rounds=656000$saltsalt$hashedpasswordvalue:19000:0:99999:7:::\n'
                ),
                group_text='sudo:x:27:alice\nwritingdeck:x:999:alice\n',
                greetd_toml_text=(ROOT / 'os/config/greetd.toml').read_text(),
                crypttab_text=f'nvme0n1p3_crypt UUID={luks_uuid} none luks,discard\n',
                fstab_text='/dev/mapper/wros--vg-root / ext4 errors=remount-ro 0 1\n',
                root_block_name='dm-1',
                sysroot=sysroot,
            )
            self.assertTrue(proof['passed'])
            self.assertEqual(proof['installed_user'], 'alice')
            self.assertEqual(proof['encrypted_root_chain']['format'], 'LUKS2')

            # Simulate active target mounts and swap during/after installation
            mountinfo.write_text(
                mountinfo.read_text()
                + '40 20 253:1 / /run/live/installer/target rw - ext4 /dev/mapper/wros--vg-root rw\n'
                + '41 40 259:2 / /run/live/installer/target/boot rw - ext2 /dev/nvme0n1p2 rw\n'
                + '42 41 259:1 / /run/live/installer/target/boot/efi rw - vfat /dev/nvme0n1p1 rw\n'
            )
            swaps.write_text(
                'Filename\tType\tSize\tUsed\tPriority\n'
                '/dev/mapper/wros--vg-swap_1\tpartition\t999932\t0\t-2\n'
            )
            commands = []

            def fake_runner(*args):
                commands.append(args)
                if args[0] == '/bin/umount':
                    mp = args[1]
                    kept = [
                        line for line in mountinfo.read_text().splitlines()
                        if line.split()[4] != mp
                    ]
                    mountinfo.write_text('\n'.join(kept) + '\n')
                elif args[0] == '/sbin/swapoff':
                    swaps.write_text('Filename\tType\tSize\tUsed\tPriority\n')
                elif args[0] in ('/sbin/dmsetup', '/sbin/cryptsetup'):
                    dm_target = args[-1]
                    for dm_dir in list(sysroot.glob('dm-*')):
                        if (dm_dir / 'dm/name').read_text().strip() == dm_target:
                            shutil.rmtree(dm_dir)

            report = supervisor.cleanup_target_stack(
                baseline,
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
                    ('/sbin/cryptsetup', 'close', 'nvme0n1p3_crypt'),
                ],
            )


    def test_client_and_cleanup_outcomes_completed_cancelled_interrupted_and_unverified_refusal(self):
        import json
        with tempfile.TemporaryDirectory(prefix='wros-inst-outcomes-') as directory:
            run_dir = Path(directory) / 'run'
            run_dir.mkdir(mode=0o700)
            baseline = {'protected_disks': ['sr0'], 'baseline_dm': [], 'baseline_swaps': []}
            (run_dir / 'baseline.json').write_text(json.dumps(baseline) + '\n')

            def run_client_with_outcome(outcome_record, cleanup_sets_verified=True):
                def fake_systemd_run(args, **kwargs):
                    (run_dir / 'outcome.json').write_text(
                        json.dumps(
                            outcome_record | {'host_cleanup_verified': cleanup_sets_verified}
                        )
                        + '\n'
                    )
                    return subprocess.CompletedProcess(args, 0)

                with mock.patch.object(supervisor, 'RUN_DIR', run_dir), \
                     mock.patch.object(supervisor, 'validate_caller', return_value=42), \
                     mock.patch.object(supervisor, 'secure_process'), \
                     mock.patch.object(supervisor, 'console_state', return_value=(1, 0, 2)), \
                     mock.patch.object(supervisor, 'capture_storage_snapshot', return_value=baseline), \
                     mock.patch.object(supervisor, 'birth', return_value='123456'), \
                     mock.patch.object(supervisor.subprocess, 'run', side_effect=fake_systemd_run):
                    return supervisor.client(owner=os.geteuid())

            # 1. Completed + target_verified + host_cleanup_verified -> 0
            self.assertEqual(
                run_client_with_outcome(
                    {'outcome': 'completed', 'target_verified': True, 'worker_cleanup': {'clean': True}}
                ),
                0,
            )
            # 2. Cancelled + host_cleanup_verified -> 2
            self.assertEqual(
                run_client_with_outcome(
                    {'outcome': 'cancelled', 'target_verified': False, 'worker_cleanup': {'clean': True}}
                ),
                2,
            )
            # 3. Interrupted or failed -> 1
            for state in ('interrupted', 'failed'):
                self.assertEqual(
                    run_client_with_outcome(
                        {'outcome': state, 'target_verified': False, 'worker_cleanup': {'clean': True}}
                    ),
                    1,
                )
            # 4. Unverified cleanup raises ValueError even if outcome claims completed
            with self.assertRaisesRegex(ValueError, 'cleanup was not verified'):
                run_client_with_outcome(
                    {'outcome': 'completed', 'target_verified': True, 'worker_cleanup': {'clean': False}},
                    cleanup_sets_verified=False,
                )


if __name__ == '__main__':
    unittest.main()
