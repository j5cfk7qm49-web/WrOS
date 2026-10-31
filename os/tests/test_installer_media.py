"""Actual Debian package and authentication-policy checks; no disk installation."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import test_wifi_policy
import test_native_image

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('installer_audit', ROOT / 'tools/audit-installer-media.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
trust_spec = importlib.util.spec_from_file_location('installer_trust', ROOT / 'tools/verify-debian-installer.py')
trust = importlib.util.module_from_spec(trust_spec)
trust_spec.loader.exec_module(trust)


class InstallerMedia(unittest.TestCase):
    def archive(self, base, name, version='1', payload=None, section='main'):
        package = base/'packages'/name
        (package/'DEBIAN').mkdir(parents=True, exist_ok=True)
        (package/'DEBIAN/control').write_text(
            f'Package: {name}\nVersion: {version}\nArchitecture: all\n'
            'Maintainer: Fixture <fixture@example.invalid>\nDescription: synthetic policy fixture, not bootable\n')
        for relative, contents in (payload or {}).items():
            path = package/relative; path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
        pool = base/'binary/pool'/section; pool.mkdir(parents=True, exist_ok=True)
        subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(package), str(pool/(name+'.deb'))],
                       check=True, stdout=subprocess.DEVNULL)

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
        # Build actual Debian archives, so the gate reads their Package field
        # instead of trusting filenames or a requested package list.
        for name in ('firmware-iwlwifi', 'firmware-realtek', 'firmware-atheros',
                     'firmware-ath9k-htc', 'firmware-brcm80211', 'firmware-mediatek'):
            self.archive(base, name, section='non-free-firmware')
        for name, paths in module.ENCRYPTION_PAYLOAD.items():
            with (root/'var/lib/dpkg/status').open('a') as output:
                output.write(f'\n\nPackage: {name}\nStatus: install ok installed\nArchitecture: all\n'
                             'Version: 1\nMaintainer: Fixture <fixture@example.invalid>\n'
                             'Description: synthetic encryption policy fixture, not bootable\n')
            payload = {}
            for relative, executable in paths:
                contents = ('synthetic '+name+' policy fixture; not an encryption implementation\n').encode()
                path = root/relative; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(contents); path.chmod(0o755 if executable else 0o644)
                payload[relative] = contents
            self.archive(base, name, payload=payload)
        test_native_image.fixture(root)

    def test_encrypted_target_needs_matching_offline_archive_not_just_root_install(self):
        with tempfile.TemporaryDirectory(prefix='wros-offline-encryption-') as directory:
            base = Path(directory); self.fixture(base)
            result = module.audit(base)
            self.assertEqual(set(result['encrypted_target_packages']), set(module.ENCRYPTION_PAYLOAD))
            archive = base/'binary/pool/main/systemd-cryptsetup.deb'
            renamed = archive.with_name('unrelated-filename.deb'); archive.rename(renamed)
            self.assertIn('systemd-cryptsetup', module.audit(base)['offline_encryption_archives'])
            renamed.unlink()
            with self.assertRaisesRegex(ValueError, 'missing matching offline encryption.*systemd-cryptsetup'):
                module.audit(base)
            outside = base/'packages/systemd-cryptsetup-external.deb'
            self.archive(base, 'systemd-cryptsetup')
            archive.rename(outside); archive.symlink_to(outside)
            with self.assertRaisesRegex(ValueError, 'offline package must be a regular archive'):
                module.audit(base)

    def test_encryption_archive_version_and_real_payload_must_match_staged_root(self):
        for mutation in ('version', 'staged payload', 'archive payload'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory(prefix='wros-encryption-match-') as directory:
                base = Path(directory); self.fixture(base)
                if mutation == 'version':
                    self.archive(base, 'lvm2', version='2')
                    expected = 'missing matching offline encryption.*lvm2'
                elif mutation == 'staged payload':
                    (base/'chroot/usr/sbin/cryptsetup').write_bytes(b'altered staged payload')
                    expected = 'payload differs from offline archive'
                else:
                    self.archive(base, 'cryptsetup-bin', payload={'usr/sbin/cryptsetup': b'altered archive payload'})
                    expected = 'payload differs from offline archive'
                with self.assertRaisesRegex(ValueError, expected):
                    module.audit(base)

    def test_encryption_installed_state_and_executable_roles_are_required(self):
        for mutation in ('state', 'missing hook', 'not executable'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory(prefix='wros-encryption-runtime-') as directory:
                base = Path(directory); self.fixture(base)
                if mutation == 'state':
                    path = base/'chroot/var/lib/dpkg/status'
                    path.write_text(path.read_text().replace(
                        'Package: systemd-cryptsetup\nStatus: install ok installed',
                        'Package: systemd-cryptsetup\nStatus: deinstall ok config-files'))
                    expected = 'encrypted-target package is not installed'
                elif mutation == 'missing hook':
                    (base/'chroot/usr/share/initramfs-tools/scripts/local-top/cryptroot').unlink()
                    expected = 'runtime missing or not executable'
                else:
                    (base/'chroot/usr/lib/systemd/systemd-cryptsetup').chmod(0o644)
                    expected = 'runtime missing or not executable'
                with self.assertRaisesRegex(ValueError, expected):
                    module.audit(base)

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
            (root / 'etc/shadow').write_text('root::1:0:99999:7:::\n_greetd:!:1:0:99999:7:::\n')
            with self.assertRaisesRegex(ValueError, 'root account must be locked'):
                module.audit(base)
            (root / 'etc/shadow').write_text('root:!:1:0:99999:7:::\n_greetd:!:1:0:99999:7:::\n')
            with (root / 'etc/passwd').open('a') as users:
                users.write('writer:x:1000:1000:demo:/home/writer:/bin/bash\n')
            with self.assertRaisesRegex(ValueError, 'build/demo account'):
                module.audit(base)

    def test_shipping_preseed_keeps_every_disk_and_password_question_interactive(self):
        selections = [line.split(None, 3) for line in (ROOT / 'os/config/installer.preseed').read_text().splitlines()
                      if line.strip() and not line.startswith('#')]
        self.assertEqual({row[1] for row in selections}, {
            'passwd/root-login', 'passwd/make-user', 'passwd/user-default-groups', 'live-installer/enable',
            'clock-setup/ntp', 'apt-setup/use_mirror', 'apt-setup/services-select',
            'popularity-contest/participate'})
        for row in selections:
            self.assertFalse(row[1].startswith(('partman', 'grub-installer', 'netcfg/wireless')), row)
            self.assertNotIn('password', row[1].replace('user-default', ''), row)
            self.assertNotIn('confirm', row[1], row)
        values = {row[1]: row[3] if len(row) > 3 else '' for row in selections}
        # Nothing contacts a time server, mirror or package survey unprompted.
        self.assertEqual(values['clock-setup/ntp'], 'false')
        self.assertEqual(values['apt-setup/use_mirror'], 'false')
        self.assertEqual(values['apt-setup/services-select'], '')
        self.assertEqual(values['popularity-contest/participate'], 'false')
        self.assertEqual(values['passwd/root-login'], 'false')
        self.assertEqual(values['passwd/make-user'], 'true')
        self.assertIn('writingdeck', values['passwd/user-default-groups'].split())

    def test_build_publishes_iso_and_audit_only_after_the_audit_passes(self):
        script = (ROOT / 'os/scripts/build-installer.sh').read_text()
        tail = script[script.index('\nlb binary'):]
        audit = tail.index('audit-installer-media.py')
        # The release names appear only after the audit, never as its input or output.
        self.assertLess(tail.index('mv "$iso" "$iso.unaudited"'), audit)
        self.assertIn('> media-audit.json.partial', tail)
        self.assertNotIn('> media-audit.json\n', tail)
        self.assertLess(audit, tail.index('mv media-audit.json.partial media-audit.json'))
        self.assertLess(tail.index('mv media-audit.json.partial media-audit.json'),
                        tail.index('mv "$iso.unaudited" "$iso"'))
        self.assertLess(tail.index('mv "$iso.unaudited" "$iso"'), tail.index('sha256sum "$iso" > SHA256SUMS'))
        self.assertIn('exit 1', tail[audit:tail.index('mv media-audit.json.partial media-audit.json')])

    def test_live_build_refuses_a_kernel_that_drifted_from_the_qualified_abi(self):
        script = (ROOT / 'os/scripts/build-installer.sh').read_text()
        pool = (ROOT / 'tools/build-live-installer-pool.py').read_text()
        self.assertRegex(pool, r"(?m)^ABI = '[0-9.]+\+deb13-amd64'$")
        self.assertIn('tools/build-live-installer-pool.py', script)
        self.assertLess(script.index('lb chroot'), script.index('Live kernel drifted'))
        self.assertLess(script.index('Live kernel drifted'), script.index('stage-live-session.sh'))

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
