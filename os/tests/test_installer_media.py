"""Actual Debian package and authentication-policy checks; no disk installation."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import test_wifi_policy

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('installer_audit', ROOT / 'tools/audit-installer-media.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
trust_spec = importlib.util.spec_from_file_location('installer_trust', ROOT / 'tools/verify-debian-installer.py')
trust = importlib.util.module_from_spec(trust_spec)
trust_spec.loader.exec_module(trust)


class InstallerMedia(unittest.TestCase):
    def fixture(self, base):
        root = base / 'chroot'
        test_wifi_policy.WifiImagePolicy().fixture(root)
        (root / 'etc/systemd/system').mkdir(parents=True)
        (root / 'etc/shadow').write_text('root:*:1:0:99999:7:::\n')
        (root / 'etc/passwd').write_text('root:x:0:0:root:/root:/bin/bash\n')
        for relative in ('binary/live/filesystem.squashfs', 'binary/install/vmlinuz',
                         'binary/install/initrd.gz', 'chroot/usr/bin/grimoire'):
            path = base / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'synthetic media fixture; not bootable')
        pool = base / 'binary/pool/non-free-firmware'
        pool.mkdir(parents=True)
        # Build actual Debian archives, so the gate reads their Package field
        # instead of trusting filenames or a requested package list.
        for name in ('firmware-iwlwifi', 'firmware-realtek', 'firmware-atheros',
                     'firmware-ath9k-htc', 'firmware-brcm80211', 'firmware-mediatek'):
            package = base / 'packages' / name
            (package / 'DEBIAN').mkdir(parents=True)
            (package / 'DEBIAN/control').write_text(
                f'Package: {name}\nVersion: 1\nArchitecture: all\nMaintainer: Fixture <fixture@example.invalid>\nDescription: synthetic firmware policy fixture\n')
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(package), str(pool / (name + '.deb'))],
                           check=True, stdout=subprocess.DEVNULL)

    def test_installed_firmware_is_insufficient_without_archives_on_installation_media(self):
        with tempfile.TemporaryDirectory(prefix='wros-media-') as tmp:
            base = Path(tmp)
            self.fixture(base)
            result = module.audit(base)
            self.assertTrue(result['passed'])
            self.assertFalse(result['installation_verified'])
            self.assertEqual(len(result['installer_firmware_packages']), 6)
            archive = base / 'binary/pool/non-free-firmware/firmware-iwlwifi.deb'
            archive.unlink()
            with self.assertRaisesRegex(ValueError, 'missing offline firmware.*firmware-iwlwifi'):
                module.audit(base)

    def test_autologin_demo_accounts_and_unlocked_root_are_rejected(self):
        with tempfile.TemporaryDirectory(prefix='wros-media-auth-') as tmp:
            base = Path(tmp)
            self.fixture(base)
            root = base / 'chroot'
            unit = root / 'etc/systemd/system/other.service'
            unit.write_text('[Service]\nExecStart=/sbin/agetty --autologin writer\n')
            with self.assertRaisesRegex(ValueError, 'autologin'):
                module.audit(base)
            unit.unlink()
            (root / 'etc/shadow').write_text('root::1:0:99999:7:::\n')
            with self.assertRaisesRegex(ValueError, 'root account must be locked'):
                module.audit(base)
            (root / 'etc/shadow').write_text('root:!:1:0:99999:7:::\n')
            with (root / 'etc/passwd').open('a') as users:
                users.write('writer:x:1000:1000:demo:/home/writer:/bin/bash\n')
            with self.assertRaisesRegex(ValueError, 'build/demo account'):
                module.audit(base)

    def test_shipping_preseed_keeps_every_disk_and_password_question_interactive(self):
        selections = [line.split(None, 3) for line in (ROOT / 'os/config/installer.preseed').read_text().splitlines()
                      if line.strip() and not line.startswith('#')]
        self.assertEqual({row[1] for row in selections}, {
            'passwd/root-login', 'passwd/make-user', 'passwd/user-default-groups', 'live-installer/enable'})
        values = {row[1]: row[3] for row in selections}
        self.assertEqual(values['passwd/root-login'], 'false')
        self.assertEqual(values['passwd/make-user'], 'true')
        self.assertIn('writingdeck', values['passwd/user-default-groups'].split())

    def test_hash_chain_rejects_tampering_and_uses_only_signed_sha256_section(self):
        digest = 'a' * 64
        release = f'MD5Sum:\n deadbeef 3 wrong\nSHA256:\n {digest} 12 expected\nSHA512:\n ignored 6 wrong\n'
        self.assertEqual(trust.release_hashes(release), {'expected': (digest, 12)})
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            trust.release_hashes(f'SHA256:\n {digest} 12 x\n {digest} 12 x\n')
        with self.assertRaisesRegex(ValueError, 'hash/size mismatch'):
            trust.check(b'tampered installer image', (digest, 12))

    def test_real_signature_verifier_refuses_unsigned_release_without_a_trust_record(self):
        with tempfile.TemporaryDirectory(prefix='wros-unsigned-installer-') as tmp:
            cache = Path(tmp)
            with mock.patch.object(trust, 'fetch', return_value=b'Origin: Debian\nCodename: trixie\n'):
                with self.assertRaises(subprocess.CalledProcessError):
                    trust.verify(cache)
            self.assertFalse((cache / 'verified-InRelease').exists())
