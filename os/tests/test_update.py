"""Signed update verification, freshness, anti-downgrade, dependency, and dpkg lock tests.

Uses real /usr/bin/gpg, /usr/bin/gpgv, /usr/bin/dpkg --compare-versions, and
POSIX fcntl record locks against disposable temporary keyrings and directories;
no host system packages or root paths are modified.
"""
from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_update_support as support  # noqa: E402

ROOT = support.ROOT
update = support.load_engine()
FIXED_NOW = datetime(2026, 10, 31, 12, 0, 0, tzinfo=timezone.utc)
FIXED_HEADERS = (
    'Date: Sat, 31 Oct 2026 11:00:00 UTC\n'
    'Valid-Until: Sat, 07 Nov 2026 11:00:00 UTC\n'
)


class SignedUpdateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()
        cls.system = support.SystemFixture(cls.gpg)
        cls.keyring = cls.system.keyring
        cls.signers = cls.system.signers

    @classmethod
    def tearDownClass(cls):
        cls.system.cleanup()

    def verify(self, bundle, **kwargs):
        kwargs.setdefault('host_arch', 'amd64')
        return update.verify_bundle(bundle, keyring=self.keyring, signers=self.signers, **kwargs)

    def build_signed_bundle(
        self,
        bundle_dir: Path,
        detached=False,
        version='0.7.6',
        extra_pkg_fields='',
        release_headers='',
        signer='archive',
        subkey=True,
    ):
        pool_dir = bundle_dir / 'pool'
        pool_dir.mkdir(parents=True, exist_ok=True)
        deb_bytes = b'!<arch>\ndebian-binary   0           0     0     644     4         `\n2.0\n'
        deb_rel = f'pool/grimoire_{version.split(":")[-1]}_amd64.deb'
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
            'Suite: wros-updates\n'
            'Codename: trixie\n'
            f'{release_headers or support.fresh_headers()}'
            'SHA256:\n'
            f' {pkg_sha} {len(packages_bytes)} Packages\n'
        )
        if detached:
            (bundle_dir / 'Release').write_bytes(release_text.encode('utf-8'))
            (bundle_dir / 'Release.gpg').write_bytes(self.gpg.sign_detached(release_text.encode('utf-8'), signer, subkey=subkey))
        else:
            (bundle_dir / 'InRelease').write_text(self.gpg.sign_clear(release_text, signer, subkey=subkey))

    def test_verify_bundle_accepts_valid_inrelease_and_detached_release_signatures(self):
        for detached in (False, True):
            with self.subTest(detached=detached), tempfile.TemporaryDirectory(prefix='wros-bundle-') as tmp:
                bundle = Path(tmp) / 'bundle'
                self.build_signed_bundle(bundle, detached=detached)
                verified = self.verify(bundle)
                self.assertTrue(verified['verified'])
                self.assertEqual(verified['suite'], 'wros-updates')
                self.assertEqual(verified['package_count'], 1)
                self.assertEqual(verified['packages'][0]['name'], 'grimoire')
                self.assertEqual(verified['packages'][0]['version'], '0.7.6')
                self.assertEqual(verified['signer_fingerprint'], self.gpg.keys['archive']['primary'].upper())

    def test_bundle_signed_directly_by_primary_key_without_subkey(self):
        solo_sys = support.SystemFixture(
            self.gpg,
            keyring_names=('archive', 'solo'),
            signers=[self.gpg.keys['solo']['primary']],
        )
        self.addCleanup(solo_sys.cleanup)
        for detached in (False, True):
            with self.subTest(detached=detached), tempfile.TemporaryDirectory(prefix='wros-bundle-solo-') as tmp:
                bundle = Path(tmp) / 'bundle'
                self.build_signed_bundle(bundle, detached=detached, signer='solo', subkey=False)
                verified = update.verify_bundle(
                    bundle,
                    keyring=solo_sys.keyring,
                    signers=solo_sys.signers,
                    host_arch='amd64',
                )
                self.assertTrue(verified['verified'])
                self.assertEqual(verified['signer_fingerprint'], self.gpg.keys['solo']['primary'].upper())

                # Unpinned primary key signature is refused
                with self.assertRaisesRegex(ValueError, 'update signature is not from a pinned WrOS signing key'):
                    update.verify_bundle(
                        bundle,
                        keyring=solo_sys.keyring,
                        signers=self.signers,
                        host_arch='amd64',
                    )

    def test_tampered_deb_packages_signature_or_unindexed_deb_fails_closed(self):
        with tempfile.TemporaryDirectory(prefix='wros-bundle-tamper-') as tmp:
            root = Path(tmp)
            # 1. Tampered .deb archive bytes
            b1 = root / 'tampered-deb'
            self.build_signed_bundle(b1)
            (b1 / 'pool/grimoire_0.7.6_amd64.deb').write_bytes(b'corrupted-deb-archive')
            with self.assertRaisesRegex(ValueError, 'failed SHA-256 or size check'):
                self.verify(b1)

            # 2. Tampered Packages index bytes
            b2 = root / 'tampered-packages'
            self.build_signed_bundle(b2)
            with (b2 / 'Packages').open('ab') as f:
                f.write(b'\n')
            with self.assertRaisesRegex(ValueError, 'Packages index SHA-256 or byte size'):
                self.verify(b2)

            # 3. Unindexed extra .deb file in bundle
            b3 = root / 'extra-deb'
            self.build_signed_bundle(b3)
            (b3 / 'pool/rogue_1.0_amd64.deb').write_bytes(b'rogue')
            with self.assertRaisesRegex(ValueError, 'unindexed .deb archive refused'):
                self.verify(b3)

            # 4. Unsigned / wrong keyring
            b4 = root / 'unsigned'
            self.build_signed_bundle(b4)
            (b4 / 'InRelease').write_text('-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nSuite: bad\n-----BEGIN PGP SIGNATURE-----\n-----END PGP SIGNATURE-----\n')
            with self.assertRaisesRegex(ValueError, 'GPG signature verification failed'):
                self.verify(b4)

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
            self.assertEqual(info['update_state'], 'idle')

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
                for argv in (['stage', tmp], ['cancel-staged'], ['recover-dpkg'], ['cancel'], ['repair'],
                             ['mount'], ['rescue-repair']):
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
                verified = self.verify(bundle)
            self.assertTrue(verified['verified'])

    def test_freshness_anti_downgrade_dependency_and_posix_dpkg_lock_checks(self):
        now = FIXED_NOW
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
                release_headers=FIXED_HEADERS,
            )
            verified = self.verify(b_ok, dpkg_dir=dpkg_dir, now=now)
            self.assertEqual(verified['packages'][0]['action'], 'upgrade')

            # 2. Downgrade is refused
            b_down = root / 'downgrade'
            self.build_signed_bundle(b_down, version='0.7.4', release_headers=FIXED_HEADERS)
            with self.assertRaisesRegex(ValueError, 'refusing package downgrade'):
                self.verify(b_down, dpkg_dir=dpkg_dir, now=now)

            # 3. Unsatisfied dependency is refused
            b_dep = root / 'missing-dep'
            self.build_signed_bundle(
                b_dep,
                version='0.7.6',
                extra_pkg_fields='Depends: nonexistent-pkg (>= 1.0)\n',
                release_headers=FIXED_HEADERS,
            )
            with self.assertRaisesRegex(ValueError, 'unsatisfied dependency'):
                self.verify(b_dep, dpkg_dir=dpkg_dir, now=now)

            # 4. Expired Valid-Until is refused
            b_exp = root / 'expired'
            self.build_signed_bundle(
                b_exp,
                version='0.7.6',
                release_headers=('Date: Thu, 29 Oct 2026 11:00:00 UTC\n'
                                 'Valid-Until: Fri, 30 Oct 2026 11:00:00 UTC\n'),
            )
            with self.assertRaisesRegex(ValueError, 'expired'):
                self.verify(b_exp, dpkg_dir=dpkg_dir, now=now)

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

    def stage(self, system, bundle, **extra):
        review_id = self.verify(bundle, dpkg_dir=system.dpkg, now=FIXED_NOW)['review_id']
        kwargs = system.kwargs() | {'now': FIXED_NOW, 'allowed_prefixes': None} | extra
        return update.stage_update(bundle, review_id, **kwargs)

    @mock.patch.object(update, 'plan_with_apt', return_value={})
    def test_offline_boot_transaction_removes_trigger_early_and_records_durable_journal(self, planner):
        now = FIXED_NOW
        system = support.SystemFixture(self.gpg, status='Package: grimoire\nStatus: install ok installed\nVersion: 0.7.5\n\n')
        self.addCleanup(system.cleanup)
        with tempfile.TemporaryDirectory(prefix='wros-update-offline-') as tmp:
            root = Path(tmp)
            dpkg_dir = system.dpkg
            status_file = system.status
            state_dir = system.state
            trigger = system.trigger
            bundle = root / 'bundle'
            self.build_signed_bundle(bundle, version='1:0.7.6', release_headers=FIXED_HEADERS)

            # 1. Active POSIX lock on lock-frontend blocks staging
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
                    self.stage(system, bundle)
            finally:
                if holder.stdout is not None:
                    holder.stdout.close()
                holder.terminate()
                holder.wait(timeout=5)
            time.sleep(0.05)
            self.assertFalse(trigger.is_symlink())
            self.assertEqual(list(state_dir.glob('staged*')), [])

            # 2. Stage when the lock is free
            staged = self.stage(system, bundle)
            self.assertEqual(staged['status'], 'staged')
            self.assertTrue(trigger.is_symlink())

            # 3. Boot apply: 'applying' is durable and the trigger is gone BEFORE the runner runs
            states_during_apt = []

            def fake_apt_ok(cmd):
                states_during_apt.append((trigger.exists() or trigger.is_symlink(),
                                         [r['state'] for r in update.Store(state_dir).records()]))
                self.assertEqual(cmd[0], '/usr/bin/apt-get')
                self.assertIn('--no-download', cmd)
                status_file.write_text(
                    'Package: grimoire\nStatus: install ok installed\nVersion: 1:0.7.6\n\n'
                )
                return subprocess.CompletedProcess(cmd, 0, stdout='installed\n', stderr='')

            journal = update.execute_offline_boot_transaction(
                state_dir=state_dir,
                trigger_path=trigger,
                dpkg_dir=dpkg_dir,
                runner=fake_apt_ok,
                now=now,
                keyring=system.keyring,
                signers=system.signers,
                host_arch='amd64',
            )
            self.assertEqual(states_during_apt, [(False, ['staged', 'applying'])])
            self.assertEqual(journal['status'], 'completed')
            self.assertFalse(trigger.is_symlink())
            rec_ok = update.inspect_recovery(
                mountinfo_text='29 1 253:0 / / rw - ext4 /dev/mapper/root rw\n',
                dpkg_dir=dpkg_dir,
                state_dir=state_dir,
                trigger_path=trigger,
                root_dir=root,
            )
            self.assertIn('last offline update: completed', rec_ok['status'])
            self.assertEqual(rec_ok['update_state'], 'completed')

            # 4. Failed/interrupted offline boot transaction still removes /system-update early
            # and records failure + interrupted dpkg packages in the journal
            b_next = root / 'bundle-next'
            self.build_signed_bundle(b_next, version='1:0.7.7', release_headers=(
                'Date: Sat, 31 Oct 2026 11:30:00 UTC\nValid-Until: Sat, 07 Nov 2026 11:30:00 UTC\n'))
            self.stage(system, b_next)
            self.assertTrue(trigger.is_symlink())

            def fake_apt_interrupted(cmd):
                self.assertFalse(trigger.exists() or trigger.is_symlink())
                status_file.write_text(
                    'Package: grimoire\nStatus: install ok half-configured\nVersion: 1:0.7.7\n\n'
                )
                return subprocess.CompletedProcess(cmd, 100, stdout='', stderr='dpkg interrupted\n')

            with self.assertRaisesRegex(ValueError, 'offline apt-get transaction failed'):
                update.execute_offline_boot_transaction(
                    state_dir=state_dir,
                    trigger_path=trigger,
                    dpkg_dir=dpkg_dir,
                    runner=fake_apt_interrupted,
                    now=now,
                    keyring=system.keyring,
                    signers=system.signers,
                    host_arch='amd64',
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
            self.assertEqual(rec_fail['update_state'], 'failed')

            # 5. run-offline-boot CLI role is refused unless invoked by systemd PID 1
            for parent, env in ((42, {'WROS_SYSTEMD_OFFLINE_UPDATE': '1'}), (1, {})):
                with mock.patch.object(update.os, 'geteuid', return_value=0), \
                     mock.patch.object(update.os, 'getppid', return_value=parent), \
                     mock.patch.dict(update.os.environ, env, clear=True):
                    with self.assertRaisesRegex(ValueError, 'unavailable; no system changes'):
                        update.main(['run-offline-boot'])

    def test_foreign_update_and_custom_application_paths_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix='wros-update-ownership-') as tmp:
            root = Path(tmp)
            stage = root/'state/staged'
            stage.mkdir(parents=True)
            archive = stage/'keep.deb'
            archive.write_bytes(b'original rescue data')
            trigger = root/'system-update'
            trigger.symlink_to(root/'some-other-updater')
            result = update.execute_offline_boot_transaction(
                state_dir=root/'state', trigger_path=trigger, dpkg_dir=root/'dpkg',
                runner=lambda cmd: self.fail('foreign trigger must not execute APT'))
            self.assertEqual(result['status'], 'not-owned')
            self.assertEqual(os.readlink(trigger), str(root/'some-other-updater'))
            self.assertEqual(archive.read_bytes(), b'original rescue data')
            trigger.unlink()
            trigger.symlink_to(stage)
            with self.assertRaisesRegex(ValueError, 'fixed system paths'):
                update.execute_offline_boot_transaction(
                    state_dir=root/'state', trigger_path=trigger, dpkg_dir=root/'dpkg')
            self.assertTrue(trigger.is_symlink())

    def test_real_apt_solver_refuses_missing_and_reverse_dependencies_without_host_changes(self):
        with tempfile.TemporaryDirectory(prefix='wros-apt-plan-') as tmp:
            root = Path(tmp)
            dpkg_dir = root/'dpkg'
            dpkg_dir.mkdir()
            installed = dpkg_dir/'status'
            installed.write_text('Package: wros-lib-fixture\nStatus: install ok installed\nArchitecture: all\nVersion: 1.0\nDescription: QA fixture\n\n')
            stage = root/'stage'
            stage.mkdir()
            def package(name, version, depends=''):
                tree = root/(name+version)
                (tree/'DEBIAN').mkdir(parents=True)
                (tree/'DEBIAN/control').write_text(
                    f'Package: {name}\nVersion: {version}\nArchitecture: all\n'
                    'Maintainer: QA <qa@wros.invalid>\nDescription: disposable offline solver fixture\n'
                    + (f'Depends: {depends}\n' if depends else ''))
                path = root/(name+'_'+version+'_all.deb')
                subprocess.run(['/usr/bin/dpkg-deb', '--build', '--root-owner-group', str(tree), str(path)],
                               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                return path, {'name': name, 'version': version}
            app, app_meta = package('wros-app-fixture','1.0','wros-lib-fixture (>= 2.0)')
            library, lib_meta = package('wros-lib-fixture','1:2.0')
            before = installed.read_bytes()
            with self.assertRaisesRegex(ValueError, 'APT could not resolve'):
                update.plan_with_apt([app], [app_meta], dpkg_dir, stage)
            plan = update.plan_with_apt([app, library], [app_meta, lib_meta], dpkg_dir, stage)
            self.assertEqual(plan, {'wros-app-fixture':'1.0','wros-lib-fixture':'1:2.0'})
            self.assertEqual(installed.read_bytes(), before)
            installed.write_bytes(before + b'Package: wros-consumer-fixture\nStatus: install ok installed\nArchitecture: all\nVersion: 1.0\nDepends: wros-lib-fixture (= 1.0)\nDescription: QA reverse dependency\n\n')
            with self.assertRaisesRegex(ValueError, 'APT could not resolve|removals|complete authenticated bundle'):
                update.plan_with_apt([library], [lib_meta], dpkg_dir, stage)


class UpdateMediaTests(unittest.TestCase):
    """find_update_media runs inside the pkexec launchers, which set umask 077."""

    def find(self, media_root, fstype):
        calls = []

        def runner(command):
            calls.append(command)
            if command[0] == '/usr/bin/lsblk':
                return subprocess.CompletedProcess(command, 0, f'/dev/sdb1 {fstype} WROS-UPDATE part\n', '')
            return subprocess.CompletedProcess(command, 0, '', '')

        previous = os.umask(0o077)
        try:
            result = update.find_update_media(runner=runner, media_root=media_root, mountinfo_text='')
        finally:
            os.umask(previous)
        return result, calls[-1]

    def test_the_writer_can_traverse_to_mounted_media_despite_the_launcher_umask(self):
        with tempfile.TemporaryDirectory(prefix='wros-media-') as tmp:
            media_root = Path(tmp) / 'run/wros-update/media'
            result, mount = self.find(media_root, 'ext4')
            self.assertEqual(result, {'status': 'mounted', 'path': str(media_root / '0')})
            for directory in (media_root.parent, media_root, media_root / '0'):
                self.assertEqual(directory.stat().st_mode & 0o777, 0o755, directory)
            self.assertEqual(mount[:5], ['/usr/bin/mount', '-t', 'ext4', '-o', 'ro,nodev,nosuid,noexec,noload'])
            # A second medium in the same boot reuses the root and still gets a 0755 mountpoint.
            media_root.chmod(0o700)
            result, _ = self.find(media_root, 'iso9660')
            self.assertEqual(result['path'], str(media_root / '1'))
            self.assertEqual(media_root.stat().st_mode & 0o777, 0o755)
            self.assertEqual((media_root / '1').stat().st_mode & 0o777, 0o755)
            self.assertEqual(Path(tmp).stat().st_mode & 0o777, 0o700)  # pre-existing parents untouched

    def test_vfat_media_is_readable_without_inheriting_the_umask(self):
        with tempfile.TemporaryDirectory(prefix='wros-media-') as tmp:
            _, mount = self.find(Path(tmp) / 'media', 'vfat')
            self.assertEqual(mount[4], 'ro,nodev,nosuid,noexec,dmask=0022,fmask=0133')


if __name__ == '__main__':
    unittest.main()
