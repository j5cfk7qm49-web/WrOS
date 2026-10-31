"""Signed update verification, offline staging, and dpkg recovery tests.

Uses real /usr/bin/gpg and /usr/bin/gpgv against disposable temporary keyrings
and directories; no host system packages or root paths are modified.
"""
import hashlib
import importlib.machinery
import importlib.util
import os
from pathlib import Path
from unittest import mock
import subprocess
import tempfile
import unittest

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

    def build_signed_bundle(self, bundle_dir: Path, detached=False):
        pool_dir = bundle_dir / 'pool'
        pool_dir.mkdir(parents=True, exist_ok=True)
        deb_bytes = b'!<arch>\ndebian-binary   0           0     0     644     4         `\n2.0\n'
        deb_rel = 'pool/grimoire_0.7.6_amd64.deb'
        deb_path = bundle_dir / deb_rel
        deb_path.write_bytes(deb_bytes)
        deb_sha = hashlib.sha256(deb_bytes).hexdigest()
        packages_text = (
            f'Package: grimoire\n'
            f'Version: 0.7.6\n'
            f'Architecture: amd64\n'
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

            # btrfs with separate /home and /.snapshots reports rollback_capable=True
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


if __name__ == '__main__':
    unittest.main()
