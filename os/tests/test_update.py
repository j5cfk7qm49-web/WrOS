"""Signed update verification, freshness, anti-downgrade, dependency, and dpkg lock tests.

Uses real /usr/bin/gpg, /usr/bin/gpgv, /usr/bin/dpkg --compare-versions, and
POSIX fcntl record locks against disposable temporary keyrings and directories;
no host system packages or root paths are modified.
"""
from datetime import datetime, timezone
import hashlib
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader('wros_update', str(ROOT / 'os/scripts/wros-update'))
spec = importlib.util.spec_from_loader(loader.name, loader)
update = importlib.util.module_from_spec(spec)
loader.exec_module(update)


class SignedUpdateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg_temp = tempfile.TemporaryDirectory(prefix='wros-update-gpg-')
        cls.gnupghome = Path(cls.gpg_temp.name) / 'gnupg'
        cls.gnupghome.mkdir(mode=0o700)
        key_params = (
            '%no-protection\n'
            'Key-Type: RSA\n'
            'Key-Length: 2048\n'
            'Name-Real: WrOS Test Archive\n'
            'Name-Email: archive@wros.invalid\n'
            'Expire-Date: 0\n'
            '%commit\n'
        )
        env = {'GNUPGHOME': str(cls.gnupghome), 'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}
        subprocess.run(
            ['/usr/bin/gpg', '--batch', '--generate-key'],
            input=key_params,
            text=True,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        cls.keyring = Path(cls.gpg_temp.name) / 'trusted.gpg'
        subprocess.run(
            ['/usr/bin/gpg', '--batch', '--output', str(cls.keyring), '--export', 'archive@wros.invalid'],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        cls.gpg_env = env

    @classmethod
    def tearDownClass(cls):
        cls.gpg_temp.cleanup()

    def build_signed_bundle(
        self,
        bundle_dir: Path,
        detached=False,
        version='0.7.6',
        extra_pkg_fields='',
        release_headers='',
    ):
        pool_dir = bundle_dir / 'pool'
        pool_dir.mkdir(parents=True, exist_ok=True)
        deb_bytes = b'!<arch>\ndebian-binary   0           0     0     644     4         `\n2.0\n'
        deb_rel = f'pool/grimoire_{version}_amd64.deb'
        deb_path = bundle_dir / deb_rel
        deb_path.write_bytes(deb_bytes)
        deb_sha = hashlib.sha256(deb_bytes).hexdigest()
        packages_text = (
            f'Package: grimoire\n'
            f'Version: {version}\n'
            f'Architecture: amd64\n'
            f'{extra_pkg_fields}'
            f'Filename: {deb_rel}\n'
            f'Size: {len(deb_bytes)}\n'
            f'SHA256: {deb_sha}\n'
        )
        packages_bytes = packages_text.encode('utf-8')
        (bundle_dir / 'Packages').write_bytes(packages_bytes)
        pkg_sha = hashlib.sha256(packages_bytes).hexdigest()
        release_text = (
            'Origin: WrOS\n'
            'Suite: trixie-updates\n'
            'Codename: trixie\n'
            f'{release_headers}'
            'SHA256:\n'
            f' {pkg_sha} {len(packages_bytes)} Packages\n'
        )
        if detached:
            release_path = bundle_dir / 'Release'
            release_path.write_bytes(release_text.encode('utf-8'))
            subprocess.run(
                [
                    '/usr/bin/gpg',
                    '--batch',
                    '--yes',
                    '--detach-sign',
                    '--output',
                    str(bundle_dir / 'Release.gpg'),
                    str(release_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=self.gpg_env,
            )
        else:
            inrelease_path = bundle_dir / 'InRelease'
            subprocess.run(
                [
                    '/usr/bin/gpg',
                    '--batch',
                    '--yes',
                    '--clearsign',
                    '--output',
                    str(inrelease_path),
                ],
                input=release_text,
                text=True,
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=self.gpg_env,
            )

    def test_verify_bundle_accepts_valid_inrelease_and_detached_release_signatures(self):
        for detached in (False, True):
            with self.subTest(detached=detached), tempfile.TemporaryDirectory(prefix='wros-bundle-') as tmp:
                bundle = Path(tmp) / 'bundle'
                self.build_signed_bundle(bundle, detached=detached)
                verified = update.verify_bundle(bundle, keyring=self.keyring)
                self.assertTrue(verified['verified'])
                self.assertEqual(verified['suite'], 'trixie-updates')
                self.assertEqual(verified['package_count'], 1)
                self.assertEqual(verified['packages'][0]['name'], 'grimoire')
                self.assertEqual(verified['packages'][0]['version'], '0.7.6')

    def test_tampered_deb_packages_signature_or_unindexed_deb_fails_closed(self):
        with tempfile.TemporaryDirectory(prefix='wros-bundle-tamper-') as tmp:
            root = Path(tmp)
            # 1. Tampered .deb archive bytes
            b1 = root / 'tampered-deb'
            self.build_signed_bundle(b1)
            (b1 / 'pool/grimoire_0.7.6_amd64.deb').write_bytes(b'corrupted-deb-archive')
            with self.assertRaisesRegex(ValueError, 'failed SHA-256 or size check'):
                update.verify_bundle(b1, keyring=self.keyring)

            # 2. Tampered Packages index bytes
            b2 = root / 'tampered-packages'
            self.build_signed_bundle(b2)
            with (b2 / 'Packages').open('ab') as f:
                f.write(b'\n')
            with self.assertRaisesRegex(ValueError, 'Packages index SHA-256 or byte size'):
                update.verify_bundle(b2, keyring=self.keyring)

            # 3. Unindexed extra .deb file in bundle
            b3 = root / 'extra-deb'
            self.build_signed_bundle(b3)
            (b3 / 'pool/rogue_1.0_amd64.deb').write_bytes(b'rogue')
            with self.assertRaisesRegex(ValueError, 'unindexed .deb archive refused'):
                update.verify_bundle(b3, keyring=self.keyring)

            # 4. Unsigned / wrong keyring
            b4 = root / 'unsigned'
            self.build_signed_bundle(b4)
            (b4 / 'InRelease').write_text('-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nSuite: bad\n-----BEGIN PGP SIGNATURE-----\n-----END PGP SIGNATURE-----\n')
            with self.assertRaisesRegex(ValueError, 'GPG signature verification failed'):
                update.verify_bundle(b4, keyring=self.keyring)

    def test_recovery_status_respects_real_filesystem_layout_and_never_claims_ext4_snapper_rollback(self):
        with tempfile.TemporaryDirectory(prefix='wros-recovery-') as tmp:
            root = Path(tmp)
            dpkg_dir = root / 'dpkg'
            dpkg_dir.mkdir()
            (dpkg_dir / 'status').write_text(
                'Package: grimoire\nStatus: install ok installed\nVersion: 0.7.5\n\n'
            )
            state_dir = root / 'state'
            trigger = root / 'system-update'
            ext4_mountinfo = (
                '29 1 253:0 / / rw,relatime shared:1 - ext4 /dev/mapper/wros--vg-root rw,errors=remount-ro\n'
            )
            info = update.inspect_recovery(
                mountinfo_text=ext4_mountinfo,
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                root_dir=root,
            )
            self.assertEqual(info['root_filesystem'], 'ext4')
            self.assertFalse(info['rollback_capable'])
            self.assertIn('installing Snapper on ext4 does not provide rollback', info['rollback_strategy'])
            self.assertFalse(info['interrupted_dpkg'])
            self.assertFalse(info['staged_update'])

            # btrfs with separate /home and /.snapshots still reports rollback_capable=False
            (root / '.snapshots').mkdir()
            btrfs_mountinfo = (
                '29 1 0:25 /@ / rw,relatime shared:1 - btrfs /dev/mapper/wros-crypt rw,subvol=/@\n'
                '30 29 0:25 /@home /home rw,relatime shared:2 - btrfs /dev/mapper/wros-crypt rw,subvol=/@home\n'
                '31 29 0:25 /@snapshots /.snapshots rw,relatime shared:3 - btrfs /dev/mapper/wros-crypt rw,subvol=/@snapshots\n'
            )
            btrfs_info = update.inspect_recovery(
                mountinfo_text=btrfs_mountinfo,
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                root_dir=root,
            )
            self.assertEqual(btrfs_info['root_filesystem'], 'btrfs')
            self.assertTrue(btrfs_info['separate_home'])
            self.assertFalse(btrfs_info['rollback_capable'])

    def test_prototype_mutations_and_environment_overrides_cannot_change_system_state(self):
        with tempfile.TemporaryDirectory(prefix='wros-update-readonly-') as tmp:
            marker = Path(tmp)/'system-update'
            with mock.patch.dict(os.environ, {'WROS_UPDATE_TRIGGER': str(marker),
                                               'WROS_UPDATE_KEYRING': str(self.keyring)}):
                for argv in (['stage', tmp], ['cancel-staged'], ['recover-dpkg']):
                    with self.assertRaisesRegex(ValueError, 'unavailable; no system changes'):
                        update.main(argv)
            self.assertFalse(marker.exists())

    def test_metadata_verified_is_the_same_private_copy_that_was_parsed(self):
        with tempfile.TemporaryDirectory(prefix='wros-update-race-') as tmp:
            bundle = Path(tmp)/'bundle'
            self.build_signed_bundle(bundle)
            original_run = subprocess.run
            def replace_original(cmd, **kwargs):
                self.assertNotEqual(Path(kwargs['cwd']), bundle)
                (bundle/'InRelease').write_text('UNSIGNED REPLACEMENT')
                return original_run(cmd, **kwargs)
            with mock.patch.object(update.subprocess, 'run', side_effect=replace_original):
                verified = update.verify_bundle(bundle, keyring=self.keyring)
            self.assertTrue(verified['verified'])

    def test_freshness_anti_downgrade_dependency_and_posix_dpkg_lock_checks(self):
        now = datetime(2026, 10, 31, 12, 0, 0, tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory(prefix='wros-update-policy-') as tmp:
            root = Path(tmp)
            dpkg_dir = root / 'dpkg'
            dpkg_dir.mkdir()
            (dpkg_dir / 'status').write_text(
                'Package: grimoire\nStatus: install ok installed\nVersion: 0.7.5\n\n'
                'Package: libc6\nStatus: install ok installed\nVersion: 2.40-1\nProvides: glibc-abi\n\n'
            )

            # 1. Valid upgrade with satisfied Depends and fresh Date / Valid-Until
            b_ok = root / 'valid-upgrade'
            self.build_signed_bundle(
                b_ok,
                version='0.7.6',
                extra_pkg_fields='Depends: libc6 (>= 2.38), glibc-abi\n',
                release_headers=(
                    'Date: Sat, 31 Oct 2026 11:00:00 UTC\n'
                    'Valid-Until: Sat, 07 Nov 2026 11:00:00 UTC\n'
                ),
            )
            verified = update.verify_bundle(b_ok, keyring=self.keyring, dpkg_dir=dpkg_dir, now=now)
            self.assertEqual(verified['packages'][0]['action'], 'upgrade')

            # 2. Downgrade is refused
            b_down = root / 'downgrade'
            self.build_signed_bundle(b_down, version='0.7.4')
            with self.assertRaisesRegex(ValueError, 'refusing package downgrade'):
                update.verify_bundle(b_down, keyring=self.keyring, dpkg_dir=dpkg_dir, now=now)

            # 3. Unsatisfied dependency is refused
            b_dep = root / 'missing-dep'
            self.build_signed_bundle(
                b_dep,
                version='0.7.6',
                extra_pkg_fields='Depends: nonexistent-pkg (>= 1.0)\n',
            )
            with self.assertRaisesRegex(ValueError, 'unsatisfied dependency'):
                update.verify_bundle(b_dep, keyring=self.keyring, dpkg_dir=dpkg_dir, now=now)

            # 4. Expired Valid-Until is refused
            b_exp = root / 'expired'
            self.build_signed_bundle(
                b_exp,
                version='0.7.6',
                release_headers='Valid-Until: Fri, 30 Oct 2026 11:00:00 UTC\n',
            )
            with self.assertRaisesRegex(ValueError, 'expired'):
                update.verify_bundle(b_exp, keyring=self.keyring, dpkg_dir=dpkg_dir, now=now)

            # 5. POSIX F_GETLK detects active dpkg write lock held by another process
            lock_file = dpkg_dir / 'lock-frontend'
            lock_file.write_bytes(b'')
            self.assertEqual(update.posix_dpkg_lock_active(lock_file), (False, None))
            holder = subprocess.Popen(
                [
                    sys.executable,
                    '-c',
                    'import fcntl, sys, time; fd = open(sys.argv[1], "r+b"); '
                    'fcntl.lockf(fd, fcntl.LOCK_EX | fcntl.LOCK_NB); '
                    'sys.stdout.write("LOCKED\\n"); sys.stdout.flush(); time.sleep(30)',
                    str(lock_file),
                ],
                stdout=subprocess.PIPE,
                text=True,
            )
            try:
                self.assertEqual(holder.stdout.readline().strip(), 'LOCKED')
                locked, pid = update.posix_dpkg_lock_active(lock_file)
                self.assertTrue(locked)
                self.assertEqual(pid, holder.pid)
            finally:
                if holder.stdout is not None:
                    holder.stdout.close()
                holder.terminate()
                holder.wait(timeout=5)
            time.sleep(0.05)
            self.assertEqual(update.posix_dpkg_lock_active(lock_file), (False, None))



    def test_offline_boot_transaction_removes_trigger_early_and_records_durable_journal(self):
        now = datetime(2026, 10, 31, 12, 0, 0, tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory(prefix='wros-update-offline-') as tmp:
            root = Path(tmp)
            dpkg_dir = root / 'dpkg'
            dpkg_dir.mkdir()
            status_file = dpkg_dir / 'status'
            status_file.write_text(
                'Package: grimoire\nStatus: install ok installed\nVersion: 0.7.5\n\n'
            )
            state_dir = root / 'state'
            trigger = root / 'system-update'
            bundle = root / 'bundle'
            self.build_signed_bundle(
                bundle,
                version='0.7.6',
                release_headers=(
                    'Date: Sat, 31 Oct 2026 11:00:00 UTC\n'
                    'Valid-Until: Sat, 07 Nov 2026 11:00:00 UTC\n'
                ),
            )

            # 1. Active POSIX lock on lock-frontend blocks prepare_offline_transaction
            lock_file = dpkg_dir / 'lock-frontend'
            lock_file.write_bytes(b'')
            holder = subprocess.Popen(
                [
                    sys.executable,
                    '-c',
                    'import fcntl, sys, time; fd = open(sys.argv[1], "r+b"); '
                    'fcntl.lockf(fd, fcntl.LOCK_EX | fcntl.LOCK_NB); '
                    'sys.stdout.write("LOCKED\\n"); sys.stdout.flush(); time.sleep(30)',
                    str(lock_file),
                ],
                stdout=subprocess.PIPE,
                text=True,
            )
            try:
                self.assertEqual(holder.stdout.readline().strip(), 'LOCKED')
                with self.assertRaisesRegex(ValueError, 'dpkg POSIX lock lock-frontend is held'):
                    update.prepare_offline_transaction(
                        bundle,
                        keyring=self.keyring,
                        dpkg_dir=dpkg_dir,
                        state_dir=state_dir,
                        trigger_path=trigger,
                        now=now,
                    )
            finally:
                if holder.stdout is not None:
                    holder.stdout.close()
                holder.terminate()
                holder.wait(timeout=5)
            time.sleep(0.05)

            # 2. Prepare offline transaction when lock is free
            manifest = update.prepare_offline_transaction(
                bundle,
                keyring=self.keyring,
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                now=now,
            )
            self.assertTrue(manifest['verified'])
            self.assertTrue(trigger.is_symlink())

            # 3. Execute offline boot transaction: trigger must be unlinked BEFORE runner runs
            trigger_states_during_apt = []

            def fake_apt_ok(cmd):
                trigger_states_during_apt.append(trigger.exists() or trigger.is_symlink())
                self.assertEqual(cmd[0], '/usr/bin/apt-get')
                self.assertIn('--no-download', cmd)
                status_file.write_text(
                    'Package: grimoire\nStatus: install ok installed\nVersion: 0.7.6\n\n'
                )
                return subprocess.CompletedProcess(cmd, 0, stdout='installed\n', stderr='')

            journal = update.execute_offline_boot_transaction(
                state_dir=state_dir,
                trigger_path=trigger,
                dpkg_dir=dpkg_dir,
                runner=fake_apt_ok,
                now=now,
            )
            self.assertEqual(trigger_states_during_apt, [False])
            self.assertEqual(journal['status'], 'applied')
            self.assertFalse(trigger.is_symlink())
            rec_ok = update.inspect_recovery(
                mountinfo_text='29 1 253:0 / / rw - ext4 /dev/mapper/root rw\n',
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                root_dir=root,
            )
            self.assertIn('last offline update: applied', rec_ok['status'])

            # 4. Failed/interrupted offline boot transaction still removes /system-update early
            # and records failure + interrupted dpkg packages in journal.json
            b_next = root / 'bundle-next'
            self.build_signed_bundle(b_next, version='0.7.7')
            update.prepare_offline_transaction(
                b_next,
                keyring=self.keyring,
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                now=now,
            )
            self.assertTrue(trigger.is_symlink())

            def fake_apt_interrupted(cmd):
                self.assertFalse(trigger.exists() or trigger.is_symlink())
                status_file.write_text(
                    'Package: grimoire\nStatus: install ok half-configured\nVersion: 0.7.7\n\n'
                )
                return subprocess.CompletedProcess(cmd, 100, stdout='', stderr='dpkg interrupted\n')

            with self.assertRaisesRegex(ValueError, 'offline apt-get transaction failed'):
                update.execute_offline_boot_transaction(
                    state_dir=state_dir,
                    trigger_path=trigger,
                    dpkg_dir=dpkg_dir,
                    runner=fake_apt_interrupted,
                    now=now,
                )
            self.assertFalse(trigger.is_symlink())
            rec_fail = update.inspect_recovery(
                mountinfo_text='29 1 253:0 / / rw - ext4 /dev/mapper/root rw\n',
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                root_dir=root,
            )
            self.assertTrue(rec_fail['interrupted_dpkg'])
            self.assertEqual(rec_fail['unconfigured_packages'], ['grimoire'])
            self.assertIn('last offline update: failed', rec_fail['status'])

            # 5. run-offline-boot CLI role is refused unless invoked by systemd PID 1
            for parent, env in ((42, {'WROS_SYSTEMD_OFFLINE_UPDATE': '1'}), (1, {})):
                with mock.patch.object(update.os, 'geteuid', return_value=0), \
                     mock.patch.object(update.os, 'getppid', return_value=parent), \
                     mock.patch.dict(update.os.environ, env, clear=True):
                    with self.assertRaisesRegex(ValueError, 'unavailable; no system changes'):
                        update.main(['run-offline-boot'])


if __name__ == '__main__':
    unittest.main()
