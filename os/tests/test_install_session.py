"""Production encrypted-installer session supervisor unit tests.

Pure policy, preseed, process-retirement, storage-baseline, LUKS2/LVM/swap/mount
teardown, and target-verification tests using synthetic sysfs/proc trees; never
opens or modifies a host block device or terminal.
"""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import select
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
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
        self.assertNotIn('--pid', args)
        self.assertNotIn('--fork', args)
        self.assertIn('--propagation=private', args)
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
            for name, value in {'dev':'259:0', 'size':'33554432', 'diskseq':'42'}.items():
                (nvme_real/name).write_text(value+'\n')
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

            # A concurrently added mapper on another disk is outside installer
            # ownership even though that disk is writable and was not mounted.
            foreign = sysroot/'vdb'
            foreign.mkdir()
            for name, value in {'ro':'0','dev':'252:16','size':'33554432','diskseq':'77'}.items():
                (foreign/name).write_text(value+'\n')
            add_dm('dm-9', ['vdb'], 'foreign_crypt', 'CRYPT-LUKS2-foreign')
            self.assertNotIn('foreign_crypt', [m['dm_name'] for m in supervisor.target_mapper_teardown_order(baseline, sysroot)])
            (nvme_real/'diskseq').write_text('43\n')
            with self.assertRaisesRegex(ValueError, 'identity changed'):
                supervisor.is_writable_target_block('dm-1', baseline, sysroot)
            (nvme_real/'diskseq').write_text('42\n')

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

            # The real guided install lists its swap from inside the installer root, and the
            # host namespace sees the same swap relative to the unreachable devtmpfs mount.
            header = 'Filename\tType\tSize\tUsed\tPriority\n'
            for listed in ('/run/live/installer/dev/mapper/wros--vg-swap_1', '/mapper/wros--vg-swap_1',
                           '/dev/mapper/wros--vg-swap_1', '/dev/dm-2', '/dm-2'):
                with self.subTest(listed=listed):
                    found = supervisor.target_swaps(f'{header}{listed}\tpartition\t823292\t0\t-2\n',
                                                    baseline, sysroot=sysroot)
                    self.assertEqual(found, [{'path': listed, 'block': 'dm-2', 'device': '/dev/dm-2'}])
            for listed, message in (('/run/live/installer/dev/mapper/foreign_crypt', 'refusing non-target'),
                                    ('/mapper/not-a-mapper', 'unknown mapper'),
                                    ('/swapfile', 'unsupported swap path'),
                                    ('/run/live/installer/target/swapfile', 'unsupported swap path')):
                with self.subTest(listed=listed), self.assertRaisesRegex(ValueError, message):
                    supervisor.target_swaps(f'{header}{listed}\tfile\t1024\t0\t-2\n', baseline, sysroot=sysroot)

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
                    ('/sbin/swapoff', '/dev/dm-2'),
                    ('/sbin/dmsetup', 'remove', '--retry', 'wros--vg-root'),
                    ('/sbin/dmsetup', 'remove', '--retry', 'wros--vg-swap_1'),
                    ('/sbin/cryptsetup', 'close', 'nvme0n1p3_crypt'),
                ],
            )


    def test_failed_installs_keep_the_installer_logs_bounded_root_only_and_without_links(self):
        with tempfile.TemporaryDirectory(prefix='wros-inst-logs-') as directory:
            di_root, work = Path(directory) / 'di', Path(directory) / 'work'
            (di_root / 'var/log').mkdir(parents=True)
            work.mkdir(mode=0o700)
            (di_root / 'var/log/syslog').write_bytes(b'early line\n' + b'x' * 64 + b'\nlast line\n')
            (di_root / 'var/log/partman').symlink_to('/etc/shadow')
            self.assertEqual(supervisor.keep_installer_logs(work, di_root=di_root, limit=32), ['syslog'])
            kept = work / 'installer-syslog'
            self.assertEqual(kept.read_bytes(), (b'x' * 64 + b'\nlast line\n')[-32:])
            self.assertEqual(stat.S_IMODE(kept.stat().st_mode), 0o600)
            self.assertFalse((work / 'installer-partman').exists())
            # An existing file is never overwritten or followed.
            self.assertEqual(supervisor.keep_installer_logs(work, di_root=di_root, limit=32), [])

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
            # 3. Interrupted or failed with confirmed cleanup -> 3
            for state in ('interrupted', 'failed'):
                self.assertEqual(
                    run_client_with_outcome(
                        {'outcome': state, 'target_verified': False, 'worker_cleanup': {'clean': True}}
                    ),
                    3,
                )
            # 4. Unverified cleanup raises ValueError even if outcome claims completed
            with self.assertRaisesRegex(ValueError, 'cleanup was not verified'):
                run_client_with_outcome(
                    {'outcome': 'completed', 'target_verified': True, 'worker_cleanup': {'clean': False}},
                    cleanup_sets_verified=False,
                )

    def test_completion_proof_cannot_reuse_an_older_session_or_wrong_application(self):
        expected = {'grimoire':'a'*64, 'wros-greeter':'b'*64, 'wros-installer':'c'*64}
        nonce = 'ab'*16
        proof = {'session_nonce':nonce, 'binary_sha256':expected, 'boot_payload_checked':True}
        supervisor.validate_completion_identity(proof, nonce, expected)
        for changed in ({'session_nonce':'cd'*16}, {'binary_sha256':{}}, {'boot_payload_checked':False}):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                supervisor.validate_completion_identity(proof | changed, nonce, expected)

    def test_parent_loss_or_pid_reuse_refuses_automatic_return_to_writing(self):
        child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        try:
            record = {'caller_pid': child.pid, 'caller_started': supervisor.birth(child.pid)}
            self.assertTrue(supervisor.caller_still_owned(record))
            self.assertFalse(supervisor.caller_still_owned(record | {'caller_started':'wrong'}))
            child.terminate()
            child.wait(timeout=5)
            self.assertFalse(supervisor.caller_still_owned(record))
            self.assertFalse(supervisor.caller_still_owned({}))
        finally:
            if child.poll() is None:
                child.terminate()
            child.wait(timeout=5)


def sha256_of(data):
    return hashlib.sha256(data).hexdigest()


def make_capsule(root, archives=None):
    """Synthetic live medium with an embedded native-installer capsule."""
    medium = Path(root) / 'medium'
    capsule = medium / 'native-installer'
    files = {
        'initrd.gz': b'initrd-bytes',
        'vmlinuz': b'kernel-bytes',
        'Packages.gz': b'compressed-index',
        'media-Release': b'Origin: WrOS\n',
    }
    archives = archives or {
        'pool/main/a/alpha/alpha_1_all.udeb': b'alpha-archive',
        'pool/main/b/beta/beta_2_all.udeb': b'beta-archive-longer',
    }
    stanzas = []
    for name, data in sorted(archives.items()):
        stanzas.append(
            f'Package: {Path(name).name.split("_")[0]}\nVersion: 1\n'
            f'Filename: native-installer/{name}\nSize: {len(data)}\nSHA256: {sha256_of(data)}\n'
        )
    index = ('\n'.join(stanzas)).encode()
    files['Packages'] = index
    files.update(archives)
    for name, data in files.items():
        target = capsule / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    manifest = {
        'passed': True,
        'kernel_release': '6.12.0-test',
        'module_sha256': supervisor.MODULE_SHA256,
        'file_sha256': {name: sha256_of(data) for name, data in files.items()},
    }
    (capsule / 'manifest.json').write_text(json.dumps(manifest))
    # Authentication catalogs that only exist in the capsule, never in the view.
    (capsule / 'sources').mkdir()
    (capsule / 'sources' / 'InRelease').write_text('catalog')
    (medium / '.disk').mkdir()
    (medium / '.disk/info').write_bytes(b'WrOS live\n')
    (medium / 'live').mkdir()
    (medium / 'live/filesystem.squashfs').write_bytes(b'squashfs-image')
    return medium, capsule, manifest


def make_view(root, medium):
    """Stand-in for the overlay: the ordinary medium minus opaque directories."""
    view = Path(root) / 'view'
    shutil.copytree(
        medium, view, symlinks=True,
        ignore=lambda directory, names: ['native-installer'] if Path(directory) == medium else [],
    )
    (view / 'native-installer').mkdir()
    return view


class MediaViewTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='wros-inst-media-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.medium, self.capsule, self.manifest = make_capsule(self.root)
        self.info = b'WrOS live\n'
        self.squash = len(b'squashfs-image')

    def loaded(self):
        manifest = supervisor.load_capsule_manifest(self.capsule, kernel_release='6.12.0-test')
        archives = supervisor.indexed_archives((self.capsule / 'Packages').read_bytes(), manifest)
        return manifest, archives

    def staged_view(self):
        manifest, archives = self.loaded()
        view = make_view(self.root, self.medium)
        supervisor.stage_capsule_view(view, self.capsule, manifest)
        return view, manifest, archives

    def test_good_capsule_stages_and_verifies_only_declared_payload(self):
        view, manifest, archives = self.staged_view()
        self.assertEqual(len(archives), 2)
        supervisor.verify_media_view(view, manifest, archives, self.info, self.squash)
        self.assertEqual([p.name for p in (view / 'native-installer').iterdir()], ['pool'])
        self.assertFalse((view / 'native-installer/Packages').exists())
        self.assertFalse((view / 'native-installer/sources').exists())
        self.assertEqual(
            (view / 'dists/trixie/main/debian-installer/binary-amd64/Packages').read_bytes(),
            (self.capsule / 'Packages').read_bytes(),
        )

    def test_manifest_is_required_and_bound_checked(self):
        load = supervisor.load_capsule_manifest
        with self.assertRaisesRegex(ValueError, 'capsule is missing'):
            load(self.root / 'absent', kernel_release='6.12.0-test')
        with self.assertRaisesRegex(ValueError, 'manifest mismatch'):
            load(self.capsule, kernel_release='6.12.0-other')
        original = json.loads((self.capsule / 'manifest.json').read_text())
        broken = {
            'passed': original | {'passed': False},
            'module': original | {'module_sha256': 'f' * 64},
        }
        for label, record in broken.items():
            with self.subTest(label), self.assertRaisesRegex(ValueError, 'manifest mismatch'):
                (self.capsule / 'manifest.json').write_text(json.dumps(record))
                load(self.capsule, kernel_release='6.12.0-test')
        bad_hashes = {
            'bad digest': original['file_sha256'] | {'vmlinuz': 'xyz'},
            'path escape': original['file_sha256'] | {'pool/../../etc/shadow': 'a' * 64},
            'no pool': {k: v for k, v in original['file_sha256'].items() if not k.startswith('pool/')},
            'no initrd': {k: v for k, v in original['file_sha256'].items() if k != 'initrd.gz'},
        }
        for label, hashes in bad_hashes.items():
            with self.subTest(label), self.assertRaises(ValueError):
                (self.capsule / 'manifest.json').write_text(json.dumps(original | {'file_sha256': hashes}))
                load(self.capsule, kernel_release='6.12.0-test')

    def test_index_must_match_manifest_paths_and_hashes(self):
        manifest, archives = self.loaded()
        index = (self.capsule / 'Packages').read_bytes()
        hashes = manifest['file_sha256']
        with self.assertRaisesRegex(ValueError, 'differs from its manifest'):
            supervisor.indexed_archives(index + b'\n', manifest)
        # Index (consistently re-hashed in the manifest) listing a path the manifest lacks.
        rogue = index.replace(b'alpha_1_all', b'rogue_1_all')
        rogue_manifest = manifest | {'file_sha256': hashes | {'Packages': sha256_of(rogue)}}
        with self.assertRaisesRegex(ValueError, 'paths differ'):
            supervisor.indexed_archives(rogue, rogue_manifest)
        # Index SHA256 field that disagrees with the manifest's archive hash.
        wrong = index.replace(sha256_of(b'alpha-archive').encode(), b'0' * 64)
        wrong_manifest = manifest | {'file_sha256': hashes | {'Packages': sha256_of(wrong)}}
        with self.assertRaisesRegex(ValueError, 'index hash differs'):
            supervisor.indexed_archives(wrong, wrong_manifest)
        # A short index (missing an archive the manifest declares).
        first = index.split(b'\n\n')[0] + b'\n'
        short_manifest = manifest | {'file_sha256': hashes | {'Packages': sha256_of(first)}}
        with self.assertRaisesRegex(ValueError, 'paths differ'):
            supervisor.indexed_archives(first, short_manifest)
        with self.assertRaisesRegex(ValueError, 'bounded'):
            supervisor.indexed_archives(b'', manifest)
        with self.assertRaisesRegex(ValueError, 'needs one Filename'):
            supervisor.parse_package_index('Package: x\nSHA256: ' + 'a' * 64 + '\n')

    def test_capsule_pool_refuses_symlinks_extras_and_missing_archives(self):
        manifest, _ = self.loaded()
        pool = self.capsule / 'pool'
        names = supervisor.capsule_pool_files(pool)
        self.assertEqual(names, supervisor.capsule_pool_names(manifest))
        extra = pool / 'main/a/alpha/extra.udeb'
        extra.write_bytes(b'unlisted')
        view = make_view(self.root, self.medium)
        with self.assertRaisesRegex(ValueError, 'extra or missing'):
            supervisor.stage_capsule_view(view, self.capsule, manifest)
        extra.unlink()
        (pool / 'main/b/beta/beta_2_all.udeb').unlink()
        with self.assertRaisesRegex(ValueError, 'extra or missing'):
            supervisor.stage_capsule_view(view, self.capsule, manifest)
        (pool / 'main/b/beta/beta_2_all.udeb').symlink_to(pool / 'main/a/alpha/alpha_1_all.udeb')
        with self.assertRaisesRegex(ValueError, 'non-regular'):
            supervisor.capsule_pool_files(pool)
        (pool / 'main/b/beta/beta_2_all.udeb').unlink()
        (pool / 'main/b/beta/beta_2_all.udeb').write_bytes(b'beta-archive-longer')
        pool_link = self.root / 'elsewhere'
        pool_link.mkdir()
        (pool / 'link').symlink_to(pool_link, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'non-regular'):
            supervisor.capsule_pool_files(pool)

    def test_corrupt_or_replaced_capsule_bytes_fail_closed_on_copy(self):
        manifest, _ = self.loaded()
        (self.capsule / 'pool/main/a/alpha/alpha_1_all.udeb').write_bytes(b'tampered')
        with self.assertRaisesRegex(ValueError, 'hash mismatch after copy'):
            supervisor.stage_capsule_view(make_view(self.root, self.medium), self.capsule, manifest)

    def test_destination_symlinks_and_visible_catalogs_are_refused(self):
        manifest, _ = self.loaded()
        outside = self.root / 'outside'
        outside.mkdir()

        def fresh():
            shutil.rmtree(self.root / 'view', ignore_errors=True)
            return make_view(self.root, self.medium)

        view = fresh()
        (view / 'install').mkdir()
        (view / 'install/initrd.gz').symlink_to(outside / 'victim')
        with self.assertRaisesRegex(ValueError, 'regular files'):
            supervisor.stage_capsule_view(view, self.capsule, manifest)
        self.assertFalse((outside / 'victim').exists())
        view = fresh()
        (view / 'dists').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'regular files'):
            supervisor.stage_capsule_view(view, self.capsule, manifest)
        self.assertEqual(list(outside.iterdir()), [])
        # A non-opaque overlay would show the lower capsule's catalogs.
        view = fresh()
        (view / 'native-installer/Packages').write_bytes(b'catalog')
        with self.assertRaisesRegex(ValueError, 'not hidden'):
            supervisor.stage_capsule_view(view, self.capsule, manifest)

    def test_verification_catches_post_publication_drift(self):
        view, manifest, archives = self.staged_view()
        check = lambda: supervisor.verify_media_view(view, manifest, archives, self.info, self.squash)
        check()
        victim = view / 'native-installer/pool/main/a/alpha/alpha_1_all.udeb'
        original = victim.read_bytes()
        victim.write_bytes(b'x' * len(original))
        with self.assertRaisesRegex(ValueError, 'indexed package archive'):
            check()
        victim.write_bytes(original)
        victim.unlink()
        victim.symlink_to(view / 'native-installer/pool/main/b/beta/beta_2_all.udeb')
        with self.assertRaisesRegex(ValueError, 'regular files'):
            check()
        victim.unlink()
        victim.write_bytes(original)
        check()
        for name in ('Packages', 'manifest.json', 'media-Release'):
            (view / 'native-installer' / name).write_bytes(b'visible')
            with self.subTest(name), self.assertRaisesRegex(ValueError, 'catalogs are visible'):
                check()
            (view / 'native-installer' / name).unlink()
        (view / 'native-installer/pool/main/a/alpha/extra.udeb').write_bytes(b'extra')
        with self.assertRaisesRegex(ValueError, 'extra or missing'):
            check()
        (view / 'native-installer/pool/main/a/alpha/extra.udeb').unlink()
        (view / 'install/gtk').mkdir()
        (view / 'install/gtk/initrd.gz').write_bytes(b'gtk')
        with self.assertRaisesRegex(ValueError, 'GTK installer'):
            check()
        (view / 'install/gtk/initrd.gz').unlink()
        with self.assertRaisesRegex(ValueError, 'ordinary live payload'):
            supervisor.verify_media_view(view, manifest, archives, b'other', self.squash)
        with self.assertRaisesRegex(ValueError, 'ordinary live payload'):
            supervisor.verify_media_view(view, manifest, archives, self.info, self.squash + 1)
        (view / 'install/vmlinuz').write_bytes(b'different-kernel')
        with self.assertRaisesRegex(ValueError, 'matching capsule file'):
            check()

    def test_build_media_view_orders_opaque_overlay_nonrecursive_verify_then_publication(self):
        calls = []
        final = self.root / 'published'
        lower = self.medium

        def fake_command(*args):
            calls.append(args)
            if args[0] == '/bin/umount':
                shutil.rmtree(args[1])
                Path(args[1]).mkdir()
            if args[1:3] == ('-t', 'overlay'):
                options = dict(item.split('=', 1) for item in args[4].split(',') if '=' in item)
                self.assertEqual(options['lowerdir'], str(lower.resolve()))
                upper = Path(options['upperdir'])
                for opaque in ('install/gtk', 'native-installer'):
                    self.assertTrue((upper / opaque).is_dir())
                view = Path(args[-1])
                shutil.copytree(lower, view, symlinks=True, dirs_exist_ok=True,
                                ignore=lambda d, n: ['native-installer'] if Path(d) == lower else [])
                (view / 'native-installer').mkdir(exist_ok=True)

        binds = []

        def fake_bind(source, target, readonly=False, recursive=False):
            binds.append((Path(source), Path(target), readonly, recursive))
            if Path(source) == Path(target):
                return
            copy_target = final if Path(target) == lower else Path(target)
            shutil.copytree(source, copy_target, symlinks=True, dirs_exist_ok=True)
            if Path(target) == lower:
                supervisor.MEDIUM = final

        xattrs = []
        work = self.root / 'work'
        work.mkdir()
        original_medium = supervisor.MEDIUM
        try:
            supervisor.MEDIUM = lower
            with mock.patch.object(supervisor, 'command', fake_command), \
                 mock.patch.object(supervisor, 'bind', fake_bind), \
                 mock.patch.object(supervisor.os, 'setxattr',
                                   lambda path, name, value: xattrs.append((Path(path).name, name, value))), \
                 mock.patch.object(supervisor.os, 'uname',
                                   return_value=mock.Mock(release='6.12.0-test')):
                supervisor.build_media_view(work)
        finally:
            supervisor.MEDIUM = original_medium
        self.assertEqual(sorted(xattrs), [('gtk', 'trusted.overlay.opaque', b'y'),
                                          ('native-installer', 'trusted.overlay.opaque', b'y')])
        view = work / 'medium'
        self.assertEqual([(s.name, t.name, ro, rec) for s, t, ro, rec in binds],
                         [('medium', 'medium', True, False),
                          ('medium', 'media-rebound', True, False),
                          ('medium', 'medium', True, False)])
        self.assertEqual(binds[2][1], lower)
        self.assertIn(('/bin/umount', str(work / 'media-rebound')), calls)
        self.assertFalse((work / 'media-rebound').exists())
        self.assertTrue((view / 'native-installer/pool/main/a/alpha/alpha_1_all.udeb').is_file())

    def test_build_media_view_stops_before_publication_when_the_rebound_view_is_wrong(self):
        lower = self.medium
        work = self.root / 'work'
        work.mkdir()
        published = []

        def fake_command(*args):
            if args[1:3] == ('-t', 'overlay'):
                view = Path(args[-1])
                shutil.copytree(lower, view, symlinks=True, dirs_exist_ok=True)  # non-opaque
        def fake_bind(source, target, readonly=False, recursive=False):
            published.append(Path(target))

        original_medium = supervisor.MEDIUM
        try:
            supervisor.MEDIUM = lower
            with mock.patch.object(supervisor, 'command', fake_command), \
                 mock.patch.object(supervisor, 'bind', fake_bind), \
                 mock.patch.object(supervisor.os, 'setxattr', lambda *a: None), \
                 mock.patch.object(supervisor.os, 'uname',
                                   return_value=mock.Mock(release='6.12.0-test')):
                with self.assertRaisesRegex(ValueError, 'not hidden'):
                    supervisor.build_media_view(work)
        finally:
            supervisor.MEDIUM = original_medium
        self.assertEqual(published, [])


class PreflightTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='wros-inst-preflight-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def meminfo(self, kib):
        path = self.root / 'meminfo'
        path.write_text(f'MemTotal:       8000000 kB\nMemFree:          100 kB\nMemAvailable:   {kib} kB\n')
        return path

    def test_memory_floor_is_a_documented_named_constant_and_enforced(self):
        self.assertEqual(supervisor.MIN_MEM_AVAILABLE_MIB, 3072)
        floor_kib = supervisor.MIN_MEM_AVAILABLE_MIB * 1024
        self.assertEqual(supervisor.require_memory_headroom(self.meminfo(floor_kib)), 3072)
        self.assertGreater(supervisor.require_memory_headroom(self.meminfo(floor_kib * 2)), 3072)
        with self.assertRaisesRegex(ValueError, r'2047 MiB available, 3072 MiB required'):
            supervisor.require_memory_headroom(self.meminfo(2047 * 1024 + 1023))
        with self.assertRaisesRegex(ValueError, '3071 MiB available'):
            supervisor.require_memory_headroom(self.meminfo(floor_kib - 1))
        for broken in ('MemTotal: 1 kB\n', 'MemAvailable: lots kB\n', ''):
            (self.root / 'meminfo').write_text(broken)
            with self.subTest(broken=broken), self.assertRaisesRegex(ValueError, 'could not be determined'):
                supervisor.require_memory_headroom(self.root / 'meminfo')

    def launcher(self):
        launcher = self.root / 'launcher'
        (launcher / 'plugins').mkdir(parents=True)
        for name in supervisor.LAUNCHER_FILES:
            (launcher / name).write_text('#!/bin/sh\n')
            (launcher / name).chmod(0o755)
        return launcher

    def test_launcher_table_is_the_single_pinned_source(self):
        self.assertEqual(set(supervisor.LAUNCHER_FILES), {'plugins/live', 'debconf_kernel.sh'})
        for pinned in supervisor.LAUNCHER_FILES.values():
            self.assertTrue(pinned is None or len(pinned) == 64)

    def test_missing_launcher_components_hard_fail_with_actionable_message(self):
        launcher = self.launcher()
        owner = os.getuid()
        supervisor.verify_launcher(launcher, owner=owner)
        for name in supervisor.LAUNCHER_FILES:
            (launcher / name).rename(launcher / (name + '.moved'))
            with self.subTest(name), self.assertRaises(ValueError) as caught:
                supervisor.verify_launcher(launcher, owner=owner)
            message = str(caught.exception)
            self.assertIn('Debian installer launcher component missing', message)
            self.assertIn(str(launcher / name), message)
            self.assertIn('rebuild the media', message)
            (launcher / (name + '.moved')).rename(launcher / name)
        with self.assertRaisesRegex(ValueError, 'component missing'):
            supervisor.verify_launcher(self.root / 'absent', owner=owner)

    def test_launcher_components_must_be_trusted_regular_files_and_honour_pins(self):
        launcher = self.launcher()
        owner = os.getuid()
        live = launcher / 'plugins/live'
        live.chmod(0o775)
        with self.assertRaisesRegex(ValueError, 'untrusted'):
            supervisor.verify_launcher(launcher, owner=owner)
        live.chmod(0o755)
        with self.assertRaisesRegex(ValueError, 'untrusted'):
            supervisor.verify_launcher(launcher, owner=owner + 1)
        live.unlink()
        live.symlink_to(launcher / 'debconf_kernel.sh')
        with self.assertRaisesRegex(ValueError, 'untrusted'):
            supervisor.verify_launcher(launcher, owner=owner)
        live.unlink()
        live.write_text('. real\n')
        live.chmod(0o755)
        pins = {'plugins/live': sha256_of(b'. real\n'), 'debconf_kernel.sh': None}
        supervisor.verify_launcher(launcher, files=pins, owner=owner)
        with self.assertRaisesRegex(ValueError, 'pinned hash'):
            supervisor.verify_launcher(launcher, files=pins | {'plugins/live': '0' * 64}, owner=owner)

    def test_prepare_refuses_before_any_mount_when_memory_or_launcher_is_unusable(self):
        empty = self.root / 'no-launcher'
        empty.mkdir()
        mounted = mock.Mock()
        with mock.patch.object(supervisor, 'build_media_view', mounted),              mock.patch.object(supervisor, 'command', mounted),              mock.patch.object(supervisor, 'bind', mounted):
            with mock.patch.object(supervisor, 'LAUNCHER_DIR', empty),                  mock.patch.object(supervisor, 'require_memory_headroom', return_value=4096):
                with self.assertRaisesRegex(ValueError, 'launcher component missing'):
                    supervisor.prepare_media_and_backend(self.root / 'work', {})
            with mock.patch.object(supervisor, 'LAUNCHER_DIR', self.launcher()),                  mock.patch.object(supervisor, 'verify_launcher'),                  mock.patch.object(supervisor, 'require_memory_headroom',
                                   side_effect=ValueError('Not enough free memory')):
                with self.assertRaisesRegex(ValueError, 'Not enough free memory'):
                    supervisor.prepare_media_and_backend(self.root / 'work', {})
        mounted.assert_not_called()


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakeProcess:
    """Exits with `code` once the fake clock reaches `exit_at` (never when None)."""

    def __init__(self, clock, exit_at, code):
        self.clock, self.exit_at, self.code = clock, exit_at, code

    @property
    def returncode(self):
        return self.poll()

    def poll(self):
        if self.exit_at is not None and self.clock.now >= self.exit_at:
            return self.code
        return None


class EndOfInstallHandshakeTests(unittest.TestCase):
    PROOF = {'passed': True, 'session_nonce': 'ab' * 16}

    def judge(self, frontend_exit, backend_exit, proof_when_backend_done=True, parent_exits_at=None,
              grace=None, frontend_grace=None, always_proof=False, verify=None):
        clock = FakeClock()
        frontend = FakeProcess(clock, *frontend_exit)
        backend = FakeProcess(clock, *backend_exit)
        verified = []

        def default_verify(di_root, baseline):
            verified.append(clock.now)
            if always_proof or (proof_when_backend_done and backend.poll() is not None):
                return dict(self.PROOF)
            return None

        verdict = supervisor.supervise_installer(
            frontend, backend,
            lambda: parent_exits_at is not None and clock.now >= parent_exits_at,
            {'session_nonce': 'ab' * 16},
            di_root=Path('/nonexistent'),
            grace=grace, frontend_grace=frontend_grace,
            clock=clock, sleep=clock.sleep,
            verify=verify or default_verify,
        )
        return verdict, verified, clock

    def test_default_grace_is_the_documented_configurable_constant(self):
        self.assertEqual(supervisor.BACKEND_EXIT_GRACE_SECONDS, 120.0)
        self.assertEqual(supervisor.COMPLETED_BACKEND_EXIT_CODES, (0, 11))

    def test_frontend_disconnect_exit_before_a_slow_backend_finish_is_still_completed(self):
        (outcome, reason, proof), verified, clock = self.judge((1.0, 1), (90.0, 0))
        self.assertEqual((outcome, proof), ('completed', self.PROOF))
        self.assertEqual(reason, 'encrypted target installation verified')
        # The proof is only judged after the backend (and its finish hooks) ended.
        self.assertTrue(verified and min(verified) >= 90.0)
        self.assertGreaterEqual(clock.now, 90.0)

    def test_backend_exit_11_and_0_complete_but_other_exits_do_not(self):
        for code, outcome in ((0, 'completed'), (11, 'completed'), (1, 'failed'), (2, 'failed'),
                              (10, 'failed'), (-9, 'failed')):
            with self.subTest(code=code):
                (got, reason, proof), _, _ = self.judge((1.0, 1), (5.0, code))
                self.assertEqual(got, outcome)
                self.assertEqual(proof is not None, outcome == 'completed')
                if outcome == 'failed':
                    self.assertIn(f'code {code}', reason)

    def test_backend_that_never_exits_times_out_even_with_a_target_proof(self):
        (outcome, reason, proof), verified, clock = self.judge((1.0, 1), (None, None), always_proof=True)
        self.assertEqual((outcome, proof), ('failed', None))
        self.assertIn('did not exit within 120 s', reason)
        self.assertEqual(verified, [])
        self.assertAlmostEqual(clock.now, 1.0 + 120.0, delta=0.2)
        (outcome, reason, _), _, clock = self.judge((1.0, 1), (None, None), grace=7.5)
        self.assertEqual(outcome, 'failed')
        self.assertIn('7.5 s', reason)
        self.assertLess(clock.now, 10)

    def test_backend_finishing_without_a_target_is_failed_unless_the_user_cancelled(self):
        (outcome, _, proof), _, _ = self.judge((1.0, 1), (4.0, 0), proof_when_backend_done=False)
        self.assertEqual((outcome, proof), ('failed', None))
        (outcome, reason, proof), _, _ = self.judge((1.0, 2), (4.0, 0), proof_when_backend_done=False)
        self.assertEqual((outcome, proof), ('cancelled', None))
        self.assertIn('cancelled', reason)

    def test_user_cancel_does_not_wait_for_the_backend_grace(self):
        (outcome, _, proof), _, clock = self.judge((3.0, 2), (None, None), proof_when_backend_done=False)
        self.assertEqual((outcome, proof), ('cancelled', None))
        self.assertLess(clock.now, 4)

    def test_cancel_code_with_a_finished_target_still_waits_for_the_backend_exit(self):
        (outcome, _, proof), _, clock = self.judge((3.0, 2), (40.0, 11), always_proof=True)
        self.assertEqual(outcome, 'completed')
        self.assertGreaterEqual(clock.now, 40.0)

    def test_backend_finishing_first_gives_the_frontend_a_short_bounded_exit_interval(self):
        (outcome, _, _), _, clock = self.judge((3.0, 1), (1.0, 0))
        self.assertEqual(outcome, 'completed')
        self.assertGreaterEqual(clock.now, 3.0)
        (outcome, _, _), _, clock = self.judge((None, None), (1.0, 11), frontend_grace=4.0)
        self.assertEqual(outcome, 'completed')
        self.assertLess(clock.now, 7)
        (outcome, _, _), _, _ = self.judge((None, None), (1.0, 1), frontend_grace=4.0)
        self.assertEqual(outcome, 'failed')

    def test_writing_session_exit_interrupts_before_during_and_after_the_grace_wait(self):
        for exits_at in (0.0, 2.0, 60.0):
            with self.subTest(exits_at=exits_at):
                (outcome, reason, proof), _, _ = self.judge((1.0, 1), (None, None), parent_exits_at=exits_at,
                                                           always_proof=True)
                self.assertEqual((outcome, proof), ('interrupted', None))
                self.assertIn('live writing session exited', reason)

    def test_proof_verification_failures_propagate_instead_of_completing(self):
        def stale(*_):
            raise ValueError('stale installation completion proof')
        with self.assertRaisesRegex(ValueError, 'stale installation'):
            self.judge((1.0, 1), (2.0, 0), verify=stale)

    def completion_files(self, root, proof):
        (root / 'wros-target-proof').write_text(json.dumps(proof))
        (root / 'wros-target-proof').chmod(0o600)
        (root / 'wros-target-finished').write_text('completed\n')

    def real_proof(self, **changes):
        proof = {
            'passed': True, 'root_locked': True, 'native_authentication_preserved': True,
            'live_account_absent': True, 'live_artifacts_absent': True,
            'encrypted_root_chain': {'format': 'LUKS2', 'partition': 'vda2', 'root': 'dm-1', 'crypt': 'dm-0'},
            'session_nonce': 'ab' * 16,
            'binary_sha256': {name: 'c' * 64 for name in ('grimoire', 'wros-greeter', 'wros-installer')},
            'boot_payload_checked': True,
        }
        return proof | changes

    def run_real_verification(self, proof, disk_ok=True):
        directory = tempfile.TemporaryDirectory(prefix='wros-inst-proof-')
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        self.completion_files(root, proof)
        real_lstat = Path.lstat

        def as_root(path):
            info = real_lstat(path)
            values = list(info)
            values[4] = 0
            return os.stat_result(values)

        selected = []

        def writable(block, baseline):
            selected.append(block)
            return disk_ok

        with mock.patch.object(supervisor.Path, 'lstat', as_root), \
             mock.patch.object(supervisor, 'digest', return_value='c' * 64), \
             mock.patch.object(supervisor, 'is_writable_target_block', writable):
            verdict = self.judge_with_real_verify(root)
        return verdict, selected

    def judge_with_real_verify(self, root):
        clock = FakeClock()
        frontend, backend = FakeProcess(clock, 1.0, 1), FakeProcess(clock, 30.0, 0)
        return supervisor.supervise_installer(
            frontend, backend, lambda: False, {'session_nonce': 'ab' * 16}, di_root=root,
            clock=clock, sleep=clock.sleep)

    def test_completion_still_requires_fresh_nonce_and_selected_disk_ancestry(self):
        (outcome, _, proof), selected = self.run_real_verification(self.real_proof())
        self.assertEqual(outcome, 'completed')
        self.assertEqual(selected, ['vda2'])
        self.assertEqual(proof['session_nonce'], 'ab' * 16)
        with self.assertRaisesRegex(ValueError, 'stale installation completion proof'):
            self.run_real_verification(self.real_proof(session_nonce='cd' * 16))
        with self.assertRaisesRegex(ValueError, 'outside selected disk'):
            self.run_real_verification(self.real_proof(), disk_ok=False)
        with self.assertRaisesRegex(ValueError, 'failed validation'):
            self.run_real_verification(self.real_proof(passed=False))
        with self.assertRaisesRegex(ValueError, 'failed validation'):
            self.run_real_verification(self.real_proof(live_artifacts_absent=False))
        stripped = self.real_proof()
        stripped.pop('live_artifacts_absent')
        with self.assertRaisesRegex(ValueError, 'failed validation'):
            self.run_real_verification(stripped)


class TargetHygieneTests(unittest.TestCase):
    NONCE = 'ab' * 16

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='wros-inst-hygiene-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / 'target'
        for name in ('etc/greetd', 'etc/sudoers.d', 'etc/systemd/system', 'etc/live/config.conf.d',
                     'usr/lib/live/config', 'var/lib/live', 'home/alice', 'run'):
            (self.root / name).mkdir(parents=True)
        (self.root / 'lib').symlink_to('usr/lib')
        shutil.copyfile(ROOT / 'os/config/greetd.toml', self.root / 'etc/greetd/config.toml')
        (self.root / 'etc/sudoers.d/README').write_text('# see sudoers(5)\n')

    def put(self, relative, text='x\n'):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def install_live_files(self):
        self.put('etc/sudoers.d/live', 'wros-live ALL=(ALL) NOPASSWD: ALL\n')
        self.put('etc/live/config.conf.d/90-wros.conf', 'LIVE_USERNAME="wros-live"\n')
        self.put('lib/live/config/9998-wros-session', '#!/bin/sh\n')
        self.put('etc/systemd/system/greetd.service.d/90-wros-live.conf',
                 '[Service]\nEnvironment=WROS_GREETD_CONFIG=/etc/greetd/config.toml\n')
        self.put('var/lib/live/config/wros-session', '')
        wants = self.root / 'etc/systemd/system/multi-user.target.wants'
        wants.mkdir()
        (wants / 'live-config.service').symlink_to('/lib/systemd/system/live-config.service')

    def test_clean_target_passes(self):
        self.assertEqual(supervisor.verify_target_hygiene(self.root), {'live_artifacts_absent': True})
        self.assertEqual(supervisor.purge_live_artifacts(self.root), [])

    def test_every_staged_live_artifact_is_refused_then_purged_exactly_once(self):
        for relative in supervisor.live_artifact_paths():
            if relative.startswith('usr/'):
                continue
            with self.subTest(relative=relative):
                self.put(relative)
                with self.assertRaisesRegex(ValueError, 'persisted in installed target'):
                    supervisor.verify_target_hygiene(self.root)
                self.assertEqual(supervisor.purge_live_artifacts(self.root), ['/' + relative])
                supervisor.verify_target_hygiene(self.root)
        self.install_live_files()
        with self.assertRaises(ValueError):
            supervisor.verify_target_hygiene(self.root)
        removed = supervisor.purge_live_artifacts(self.root)
        self.assertIn('/etc/systemd/system/multi-user.target.wants/live-config.service', removed)
        self.assertEqual(len(removed), 6)
        supervisor.verify_target_hygiene(self.root)
        self.assertEqual(supervisor.purge_live_artifacts(self.root), [])
        # Only the named files go: the rest of the tree is untouched.
        self.assertTrue((self.root / 'etc/sudoers.d/README').is_file())
        self.assertTrue((self.root / 'etc/greetd/config.toml').is_file())
        self.assertTrue((self.root / 'etc/live/config.conf.d').is_dir())

    def test_purge_unlinks_symlinks_without_following_and_refuses_directories(self):
        outside = Path(self.directory.name) / 'outside'
        outside.write_text('keep\n')
        (self.root / 'etc/sudoers.d/live').symlink_to(outside)
        self.assertEqual(supervisor.purge_live_artifacts(self.root), ['/etc/sudoers.d/live'])
        self.assertEqual(outside.read_text(), 'keep\n')
        (self.root / 'etc/sudoers.d/live').mkdir()
        (self.root / 'etc/sudoers.d/live/inner').write_text('x')
        with self.assertRaisesRegex(ValueError, 'refusing to remove a directory'):
            supervisor.purge_live_artifacts(self.root)
        self.assertTrue((self.root / 'etc/sudoers.d/live/inner').exists())

    def test_autologin_and_live_account_effects_are_refused_even_without_the_named_files(self):
        cases = {
            'greetd initial_session': ('etc/greetd/config.toml',
                                       (ROOT / 'os/config/greetd.toml').read_text()
                                       + '\n[initial_session]\ncommand = "/usr/libexec/writingdeck-session"\nuser = "alice"\n'),
            'second greetd profile': ('etc/greetd/live.toml', '[initial_session]\ncommand = "x"\nuser = "u"\n'),
            'live greetd override': ('etc/systemd/system/greetd.service.d/zz.conf',
                                     '[Service]\nEnvironmentFile=-/run/wros-live/wros-live.env\n'),
            'vendor greetd override': ('usr/lib/systemd/system/greetd.service.d/zz.conf',
                                       '[Service]\nEnvironment=WROS_GREETD_CONFIG=/x\n'),
            'getty autologin': ('etc/systemd/system/getty@tty1.service.d/override.conf',
                                '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin wros-live tty1\n'),
            'sudoers for live user': ('etc/sudoers.d/90-anything', 'wros-live ALL=(ALL) NOPASSWD: ALL\n'),
            'live-config account': ('etc/live/config.conf.d/10-user.conf', 'LIVE_USERNAME="x"\n'),
            'live home': ('home/wros-live/.profile', 'x\n'),
            'live marker': ('run/wros-live/session.json', '{}\n'),
        }
        for label, (relative, text) in cases.items():
            with self.subTest(label):
                path = self.put(relative, text)
                with self.assertRaises(ValueError):
                    supervisor.verify_target_hygiene(self.root)
                path.unlink()
                if relative.startswith(('home/wros-live', 'run/wros-live')):
                    path.parent.rmdir()
                supervisor.verify_target_hygiene(self.root)
        (self.root / 'etc/sudoers.d/odd').symlink_to('/etc/passwd')
        with self.assertRaisesRegex(ValueError, 'non-regular'):
            supervisor.verify_target_hygiene(self.root)

    def test_native_greetd_toml_is_valid_clean_input(self):
        text = (ROOT / 'os/config/greetd.toml').read_text()
        self.assertNotIn('initial_session', tomllib.loads(text))

    def sysfs(self, root):
        sysroot = root / 'sys'
        for block, uid, slave in (('dm-1', 'LVM-root', 'dm-0'),
                                  ('dm-0', 'CRYPT-LUKS2-0123456789abcdef0123456789abcdef-vda2_crypt', 'vda2')):
            (sysroot / block / 'dm').mkdir(parents=True, exist_ok=True)
            (sysroot / block / 'dm/uuid').write_text(uid + '\n')
            (sysroot / block / 'slaves' / slave).mkdir(parents=True, exist_ok=True)
        (sysroot / 'vda2').mkdir(exist_ok=True)
        (sysroot / 'vda2/partition').write_text('2\n')
        return sysroot

    # Debian greetd 0.10.3-4's own account row, as the installed system really has it.
    GREETER = '_greetd:x:101:102::/var/lib/greetd:/usr/sbin/nologin\n'

    def verify_state(self, passwd_extra='', shadow_extra='', greeter=GREETER):
        sysroot = self.sysfs(Path(self.directory.name))
        luks = '01234567-89ab-cdef-0123-456789abcdef'
        return supervisor.verify_installed_target_state(
            passwd_text=('root:x:0:0:root:/root:/bin/bash\n'
                         + greeter +
                         'alice:x:1000:1000:Alice:/home/alice:/bin/bash\n' + passwd_extra),
            shadow_text=('root:!:19000:0:99999:7:::\n_greetd:!:19000:0:99999:7:::\n'
                         'alice:$6$rounds=656000$saltsalt$hashedpasswordvalue:19000:0:99999:7:::\n'
                         + shadow_extra),
            group_text='sudo:x:27:alice\nwritingdeck:x:999:alice\n',
            greetd_toml_text=(ROOT / 'os/config/greetd.toml').read_text(),
            crypttab_text=f'vda2_crypt UUID={luks} none luks,discard\n',
            fstab_text='/dev/mapper/wros--vg-root / ext4 errors=remount-ro 0 1\n',
            root_block_name='dm-1',
            sysroot=sysroot,
        )

    def test_account_state_rejects_live_identities_and_unlocked_installer_account(self):
        self.assertTrue(self.verify_state()['passed'])
        locked = '_wros-installer:x:996:996::/nonexistent:/usr/sbin/nologin\n'
        self.assertTrue(self.verify_state(locked, '_wros-installer:!:19000:::::::\n')['passed'])
        for name in ('wros-live', 'live', 'builder'):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'temporary live or build account'):
                self.verify_state(f'{name}:x:1001:1001::/home/{name}:/bin/bash\n')
        with self.assertRaisesRegex(ValueError, 'locked root/greeter/installer'):
            self.verify_state(locked, '_wros-installer:$6$x$y:19000:::::::\n')
        with self.assertRaisesRegex(ValueError, 'no-login identity'):
            self.verify_state('_wros-installer:x:996:996::/nonexistent:/bin/bash\n',
                              '_wros-installer:!:19000:::::::\n')
        with self.assertRaisesRegex(ValueError, 'no-login identity|unique'):
            self.verify_state('_wros-installer:x:0:0::/nonexistent:/usr/sbin/nologin\n',
                              '_wros-installer:!:19000:::::::\n')

    def test_greeter_account_must_be_an_unprivileged_nonlogin_identity(self):
        self.assertTrue(self.verify_state()['passed'])
        self.assertTrue(self.verify_state(greeter='_greetd:x:105:65534::/nonexistent:/usr/sbin/nologin\n')['passed'])
        for bad in ('_greetd:x:0:0::/var/lib/greetd:/usr/sbin/nologin\n',
                    '_greetd:x:1000:1000::/var/lib/greetd:/usr/sbin/nologin\n',
                    '_greetd:x:101:102::/var/lib/greetd:/bin/bash\n',
                    '_greetd:x:ab:102::/var/lib/greetd:/usr/sbin/nologin\n',
                    self.GREETER * 2, ''):
            # A UID 1000 or duplicated greeter is already refused by the account checks before it.
            with self.subTest(bad=bad), self.assertRaisesRegex(
                    ValueError, 'unprivileged native greeter|one normal UID 1000|unique target accounts'):
                self.verify_state(greeter=bad)

    def test_a_failed_finish_hook_marks_the_failure_and_the_supervisor_ends_with_its_reason(self):
        di_root = Path(self.directory.name) / 'di'
        supervisor.stage_completion_hooks(di_root, self.NONCE)
        for hook in sorted((di_root / 'usr/lib/finish-install.d').iterdir()):
            self.assertIn(f'printf "%s\\n" {hook.name} > /wros-target-failed\n  exit 10\n', hook.read_text())
        self.assertIsNone(supervisor.completion_failure(di_root))
        (di_root / 'var/log').mkdir(parents=True)
        (di_root / 'var/log/syslog').write_text(
            'Nov  1 08:59:03 finish-install: info: Running /usr/lib/finish-install.d/94wros-target-proof\n'
            'Nov  1 08:59:03 in-target: WrOS installer session stopped: unprivileged native greeter required in target\n')
        (di_root / 'wros-target-failed').write_text('94wros-target-proof\n')
        reason = 'installer finish step 94wros-target-proof failed: unprivileged native greeter required in target'
        self.assertEqual(supervisor.completion_failure(di_root), reason)
        clock = FakeClock()
        # Debian's main menu keeps both processes alive; the marker still ends the session.
        frontend, backend = FakeProcess(clock, 10_000.0, 1), FakeProcess(clock, 10_000.0, 0)
        verdict = supervisor.supervise_installer(frontend, backend, lambda: False, {'session_nonce': 'ab' * 16},
                                                 di_root=di_root, clock=clock, sleep=clock.sleep)
        self.assertEqual(verdict, ('failed', reason, None))

    def test_completion_hooks_run_hygiene_before_the_proof_and_refuse_pre_existing_hooks(self):
        di_root = Path(self.directory.name) / 'di'
        supervisor.stage_completion_hooks(di_root, self.NONCE)
        hooks = sorted((di_root / 'usr/lib/finish-install.d').iterdir())
        self.assertEqual([hook.name for hook in hooks],
                         ['93wros-target-hygiene', '94wros-target-proof', '97wros-target-finished'])
        hygiene = hooks[0].read_text()
        self.assertIn(f'target-hygiene {self.NONCE}', hygiene)
        self.assertIn('exit 10', hygiene)
        self.assertNotIn('--pass-stdout', hygiene)
        self.assertIn(f'target-audit {self.NONCE}', hooks[1].read_text())
        for hook in hooks:
            self.assertEqual(hook.stat().st_mode & 0o777, 0o755)
            subprocess.run(['/bin/sh', '-n', str(hook)], check=True)
        with self.assertRaisesRegex(ValueError, 'already exist'):
            supervisor.stage_completion_hooks(di_root, self.NONCE)
        (di_root / 'usr/lib/finish-install.d/94wros-target-proof').unlink()
        (di_root / 'usr/lib/finish-install.d/97wros-target-finished').unlink()
        with self.assertRaisesRegex(ValueError, 'already exist'):
            supervisor.stage_completion_hooks(di_root, self.NONCE)

    def test_hygiene_subcommand_is_reserved_for_the_target_hook_and_checks_its_nonce(self):
        with mock.patch.object(supervisor.sys, 'argv', ['x', 'target-hygiene', self.NONCE]), \
             mock.patch.dict(supervisor.os.environ, {'PKEXEC_UID': '1000'}, clear=True):
            with self.assertRaisesRegex(ValueError, 'reserved'):
                supervisor.main()
        with mock.patch.object(supervisor.os, 'geteuid', return_value=0):
            for bad in ('', 'AB' * 16, 'a' * 31, '../x'):
                with self.subTest(bad=bad), self.assertRaisesRegex(ValueError, 'session nonce'):
                    supervisor.in_target_hygiene(bad)
            with mock.patch.object(supervisor, 'purge_live_artifacts', return_value=['/etc/sudoers.d/live']) as purge, \
                 mock.patch.object(supervisor, 'verify_target_hygiene', return_value={}) as verify, \
                 mock.patch('builtins.print') as printed:
                self.assertEqual(supervisor.in_target_hygiene(self.NONCE), 0)
        purge.assert_called_once_with()
        verify.assert_called_once_with()
        self.assertIn('/etc/sudoers.d/live', printed.call_args.args[0])

    def test_in_target_audit_runs_the_hygiene_check_and_the_proof_requires_its_result(self):
        with mock.patch.object(supervisor.os, 'geteuid', return_value=0), \
             mock.patch.object(supervisor, 'verify_target_hygiene',
                               side_effect=ValueError('live session artifact persisted')), \
             mock.patch.object(supervisor, 'verify_installed_target_state') as state:
            with self.assertRaisesRegex(ValueError, 'live session artifact persisted'):
                supervisor.in_target_audit(self.NONCE)
        state.assert_not_called()


if __name__ == '__main__':
    unittest.main()
