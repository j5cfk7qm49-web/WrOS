import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
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

    @staticmethod
    def make_openpgp_key(uid="WrOS Production Signer <release@wros.org>", is_secret=False, is_fixture=False):
        dummy_mpi = (1024).to_bytes(2, 'big') + b'\x01' * 128 + (17).to_bytes(2, 'big') + b'\x01\x00\x01'
        body = b'\x04\x00\x00\x00\x00\x01' + dummy_mpi
        tag = 5 if is_secret else 6
        key_hdr = bytes([0xC0 | tag, len(body)])
        uid_str = "WrOS Fixture Key <test@example.invalid>" if is_fixture else uid
        uid_bytes = uid_str.encode('utf-8')
        uid_hdr = bytes([0xC0 | 13, len(uid_bytes)])
        keyring_bytes = key_hdr + body + uid_hdr + uid_bytes
        fpr = hashlib.sha1(b'\x99' + len(body).to_bytes(2, 'big') + body).hexdigest().upper()
        return keyring_bytes, fpr

    def test_verify_update_trust_requires_pinned_fingerprint_in_clean_keyring(self):
        keyring_bytes, fpr = self.make_openpgp_key()
        with tempfile.TemporaryDirectory(prefix='wros-trust-') as tmp:
            tmp_path = Path(tmp)
            keyring_file = tmp_path / 'wros-archive-keyring.gpg'
            signers_file = tmp_path / 'update-signers'
            keyring_file.write_bytes(keyring_bytes)
            keyring_file.chmod(0o644)
            signers_file.write_text(f"# Pinned release signer\n{fpr}\n")
            signers_file.chmod(0o644)

            # Valid keyring and signers
            res = module.verify_update_trust(keyring_file, signers_file, required=True)
            self.assertTrue(res['checked'])
            self.assertIn(fpr, res['pinned_fingerprints'])
            self.assertFalse(res['secret_key_material'])

            # Reject secret key material
            sec_bytes, sec_fpr = self.make_openpgp_key(is_secret=True)
            keyring_file.write_bytes(sec_bytes)
            signers_file.write_text(f"{sec_fpr}\n")
            with self.assertRaisesRegex(ValueError, 'contains PRIVATE key material'):
                module.verify_update_trust(keyring_file, signers_file, required=True)

            # Reject fixture key in production update trust
            fix_bytes, fix_fpr = self.make_openpgp_key(is_fixture=True)
            keyring_file.write_bytes(fix_bytes)
            signers_file.write_text(f"{fix_fpr}\n")
            with self.assertRaisesRegex(ValueError, 'test/fixture key in production update trust'):
                module.verify_update_trust(keyring_file, signers_file, required=True)

            # Reject missing signer from keyring
            keyring_file.write_bytes(keyring_bytes)
            other_fpr = '0' * 40
            signers_file.write_text(f"{other_fpr}\n")
            with self.assertRaisesRegex(ValueError, 'pinned update signer is absent'):
                module.verify_update_trust(keyring_file, signers_file, required=True)

            # Reject group/world writable
            signers_file.write_text(f"{fpr}\n")
            keyring_file.chmod(0o666)
            with self.assertRaisesRegex(ValueError, 'group/world writable'):
                module.verify_update_trust(keyring_file, signers_file, required=True)
            keyring_file.chmod(0o644)

            # Check check_update_trust wrapper against staged rootfs
            rootfs = tmp_path / 'rootfs'
            dest_key = rootfs / 'usr/share/keyrings/wros-archive-keyring.gpg'
            dest_signers = rootfs / 'usr/share/wros/update-signers'
            dest_key.parent.mkdir(parents=True, exist_ok=True)
            dest_signers.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(keyring_file, dest_key); dest_key.chmod(0o644)
            shutil.copyfile(signers_file, dest_signers); dest_signers.chmod(0o644)
            trust_res = module.check_update_trust(rootfs, required=True)
            self.assertTrue(trust_res['checked'])

    def test_boot_structure_validation_and_shape_comparison(self):
        sample_report = """
El Torito boot img :   1  BIOS  y  none  0x0000  0x00  4  2345
El Torito img path :   1  /isolinux/isolinux.bin
El Torito img opts :   1  boot-info-table isohybrid-suitable
El Torito boot img :   2  UEFI  y  none  0x0000  0x00  5760  2400
El Torito img path :   2  /boot/grub/efi.img
System area summary: MBR isohybrid cyl-align-on GPT APM
GPT start and size : 1 2048 100000
GPT start and size : 2 102048 5760
GPT partition path : 2 /boot/grub/efi.img
"""
        parsed = module.parse_boot_report(sample_report)
        self.assertEqual(len(parsed['el_torito']), 2)
        self.assertEqual(parsed['gpt_partitions'], 2)
        self.assertIn('MBR', parsed['system_area'])
        self.assertIn('GPT', parsed['system_area'])

        with mock.patch.object(module, 'xorriso') as mock_xorriso:
            mock_proc = mock.MagicMock()
            mock_proc.stdout = sample_report.encode()
            mock_proc.stderr = b""
            mock_xorriso.return_value = mock_proc

            iso_path = Path('/fake/test.iso')
            structure = module.boot_structure(iso_path)
            self.assertEqual(len(structure['el_torito']), 2)

            # Compare against self baseline
            module.same_boot_structure(structure, structure)

            # Altered baseline partition count raises
            altered = dict(structure, gpt_partitions=1)
            with self.assertRaisesRegex(ValueError, 'Re-mastered ISO boot records differ'):
                module.same_boot_structure(structure, altered)

        # Missing BIOS entry raises
        no_bios = """
El Torito boot img :   1  UEFI  y  none  0x0000  0x00  5760  2400
El Torito img path :   1  /boot/grub/efi.img
System area summary: MBR isohybrid cyl-align-on GPT
GPT start and size : 1 2048 100000
GPT start and size : 2 102048 5760
"""
        with mock.patch.object(module, 'xorriso') as mock_xorriso:
            mock_proc = mock.MagicMock()
            mock_proc.stdout = no_bios.encode(); mock_proc.stderr = b""
            mock_xorriso.return_value = mock_proc
            with self.assertRaisesRegex(ValueError, 'no BIOS El Torito boot image'):
                module.boot_structure(iso_path)

    def test_squash_kernel_abi_detection_and_matching(self):
        valid_listing = {
            'boot/vmlinuz-6.12.113+deb13-amd64': ('-', None),
            'usr/lib/modules/6.12.113+deb13-amd64': ('d', None),
            'etc/debian_version': ('-', None),
        }
        self.assertEqual(module.squash_kernel_abi(valid_listing), '6.12.113+deb13-amd64')

        # Multiple kernels rejected
        multi_kernel = dict(valid_listing, **{'boot/vmlinuz-old': ('-', None)})
        with self.assertRaisesRegex(ValueError, 'must contain exactly one kernel image'):
            module.squash_kernel_abi(multi_kernel)

        # Module mismatch rejected
        mismatch_mod = {
            'boot/vmlinuz-6.12.113+deb13-amd64': ('-', None),
            'usr/lib/modules/other-kernel': ('d', None),
        }
        with self.assertRaisesRegex(ValueError, 'kernel modules .* do not match its kernel'):
            module.squash_kernel_abi(mismatch_mod)

    def test_squash_mode_reads_setuid_and_owner_from_the_real_squashfs(self):
        with tempfile.TemporaryDirectory(prefix='wros-squash-mode-') as tmp:
            source = Path(tmp) / 'root'
            (source / 'usr/sbin').mkdir(parents=True)
            locker = source / 'usr/sbin/vlock-main'
            locker.write_bytes(b'\x7fELF synthetic locker\n')
            for mode, expected in ((0o4755, '-rwsr-xr-x'), (0o755, '-rwxr-xr-x')):
                locker.chmod(mode)
                image = Path(tmp) / f'{mode:o}.squashfs'
                subprocess.run(['mksquashfs', str(source), str(image), '-all-root', '-quiet', '-no-progress'],
                               check=True, stdout=subprocess.DEVNULL)
                with self.subTest(mode=oct(mode)):
                    self.assertEqual(module.squash_mode(image, 'usr/sbin/vlock-main'), (expected, 'root/root'))
            with self.assertRaisesRegex(ValueError, 'no entry for usr/sbin/missing'):
                module.squash_mode(image, 'usr/sbin/missing')

    def test_deny_list_rejects_qa_files_autologin_and_insecure_auth(self):
        with tempfile.TemporaryDirectory(prefix='wros-deny-') as tmp:
            rootfs = Path(tmp)
            (rootfs / 'etc').mkdir(parents=True, exist_ok=True)
            (rootfs / 'etc/shadow').write_text('root:!:1:0:99999:7:::\n_greetd:!:1:0:99999:7:::\n')
            (rootfs / 'etc/passwd').write_text('root:x:0:0:root:/root:/bin/bash\n_greetd:x:999:999::/nonexistent:/usr/sbin/nologin\n')
            (rootfs / 'etc/sudoers').write_text('root ALL=(ALL:ALL) ALL\n')

            listing = {'etc/debian_version': ('-', None)}
            boot_configs = {'boot/grub/grub.cfg': 'linux /live/vmlinuz boot=live components wros.live=1\n'}

            # Clean baseline passes
            module.deny_list(rootfs, listing, boot_configs)

            # QA artifact in listing fails
            qa_listing = dict(listing, **{'etc/writingdeck/VM-DEMO-ONLY': ('-', None)})
            with self.assertRaisesRegex(ValueError, 'QA/test file shipped'):
                module.deny_list(rootfs, qa_listing, boot_configs)

            # live sudoers file in listing fails
            live_sudo = dict(listing, **{'etc/sudoers.d/live': ('-', None)})
            with self.assertRaisesRegex(ValueError, 'live NOPASSWD sudoers file shipped'):
                module.deny_list(rootfs, live_sudo, boot_configs)

            # NOPASSWD in sudoers fails
            (rootfs / 'etc/sudoers').write_text('writer ALL=(ALL) NOPASSWD: ALL\n')
            with self.assertRaisesRegex(ValueError, 'passwordless sudo configured'):
                module.deny_list(rootfs, listing, boot_configs)
            (rootfs / 'etc/sudoers').write_text('root ALL=(ALL:ALL) ALL\n')

            # autologin in boot configuration fails
            autologin_boot = {'boot/grub/grub.cfg': 'linux /live/vmlinuz boot=live autologin\n'}
            with self.assertRaisesRegex(ValueError, 'autologin requested in boot configuration'):
                module.deny_list(rootfs, listing, autologin_boot)

            # shell escape in boot configuration fails
            shell_boot = {'boot/grub/grub.cfg': 'linux /live/vmlinuz append init=/bin/sh\n'}
            with self.assertRaisesRegex(ValueError, 'shell-escape kernel parameter'):
                module.deny_list(rootfs, listing, shell_boot)

            # A package autologin generator fails unless /etc masks it with /dev/null
            generators = rootfs / 'usr/lib/systemd/system-generators'
            generators.mkdir(parents=True)
            generator = generators / 'live-config-getty-generator'
            generator.write_text('#!/bin/sh\nExecStart=-/sbin/agetty --autologin user\n')
            with self.assertRaisesRegex(ValueError, 'autologin/initial-session configured outside'):
                module.deny_list(rootfs, listing, boot_configs)
            etc_generators = rootfs / 'etc/systemd/system-generators'
            etc_generators.mkdir(parents=True)
            mask = etc_generators / 'live-config-getty-generator'
            mask.symlink_to('/tmp/not-a-mask')
            with self.assertRaisesRegex(ValueError, 'autologin/initial-session configured outside'):
                module.deny_list(rootfs, listing, boot_configs)
            mask.unlink()
            mask.symlink_to('/dev/null')
            module.deny_list(rootfs, listing, boot_configs)
            # A mask never hides QA text, and only covers the top-level unit it names
            generator.write_text('#!/bin/sh\nwros-qa- --autologin user\n')
            with self.assertRaisesRegex(ValueError, 'QA marker or fixture credential text'):
                module.deny_list(rootfs, listing, boot_configs)
            generator.write_text('#!/bin/sh\nExecStart=-/sbin/agetty --autologin user\n')
            nested = generators / 'live-config-getty-generator.d'
            nested.mkdir()
            (nested / 'override').write_text('ExecStart=-/sbin/agetty --autologin user\n')
            with self.assertRaisesRegex(ValueError, 'autologin/initial-session configured outside'):
                module.deny_list(rootfs, listing, boot_configs)
            shutil.rmtree(rootfs / 'usr')
            shutil.rmtree(rootfs / 'etc/systemd')

            # Unlocked root fails
            (rootfs / 'etc/shadow').write_text('root:$6$hash:1:0:99999:7:::\n')
            with self.assertRaisesRegex(ValueError, 'root account is not locked'):
                module.deny_list(rootfs, listing, boot_configs)
            (rootfs / 'etc/shadow').write_text('root:!:1:0:99999:7:::\n')

            # Demo account in passwd fails
            (rootfs / 'etc/passwd').write_text('writer:x:1000:1000:Writer:/home/writer:/bin/bash\n')
            with self.assertRaisesRegex(ValueError, 'builder/demo/runtime account shipped'):
                module.deny_list(rootfs, listing, boot_configs)

    def test_compare_helpers_matches_repo_scripts(self):
        with tempfile.TemporaryDirectory(prefix='wros-helpers-') as tmp:
            rootfs = Path(tmp)
            libexec = rootfs / 'usr/libexec'
            libexec.mkdir(parents=True, exist_ok=True)

            wanted = list(module.HELPERS_REQUIRED) + [n for n in module.HELPERS_IF_SOURCE if (ROOT / 'os/scripts' / n).is_file()]
            for name in wanted:
                source = ROOT / 'os/scripts' / name
                dest = libexec / name
                shutil.copyfile(source, dest)
                dest.chmod(0o755)

            # All match passes
            res = module.compare_helpers(rootfs)
            self.assertEqual(len(res), len(wanted))

            # Missing helper raises
            first = libexec / wanted[0]
            first.unlink()
            with self.assertRaisesRegex(ValueError, 'helper missing from the image'):
                module.compare_helpers(rootfs)

            # Non-executable helper raises
            shutil.copyfile(ROOT / 'os/scripts' / wanted[0], first)
            first.chmod(0o644)
            with self.assertRaisesRegex(ValueError, 'not executable or is group/world writable'):
                module.compare_helpers(rootfs)

            # Modified helper raises
            first.chmod(0o755)
            first.write_bytes(b'#!/bin/sh\necho tampered\n')
            with self.assertRaisesRegex(ValueError, 'helper differs from its source'):
                module.compare_helpers(rootfs)

    def test_build_release_iso_driver_invariants_and_step_ordering(self):
        driver = ROOT / 'os/scripts/build-release-iso.sh'
        self.assertTrue(driver.is_file(), 'build-release-iso.sh exists')
        self.assertTrue(os.access(driver, os.X_OK), 'build-release-iso.sh is executable')

        # Check syntax with bash -n
        subprocess.run(['bash', '-n', str(driver)], check=True)

        # Check --plan step list
        output = subprocess.check_output(['bash', str(driver), '--plan'], text=True)
        steps = output.strip().splitlines()
        expected = ['binaries', 'prose-runtime', 'inputs', 'installer-frontend',
                    'live-backend', 'live-image', 'native-pool', 'native-capsule',
                    'remaster', 'audit', 'smoke-boot', 'publish']
        self.assertEqual(steps, expected)

        # Check publication integrity: atomic rename and receipt creation
        script_text = driver.read_text()
        self.assertIn('audit_media', script_text)
        self.assertIn('publish_release', script_text)
        self.assertIn('mv -- "$final.receipt.json.partial" "$final.receipt.json"', script_text)

    def test_tree_gate_runs_as_root_and_its_result_is_bound_to_the_build_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp) / 'live'
            base.mkdir()
            with mock.patch.object(module.os, 'geteuid', return_value=1000):
                with self.assertRaisesRegex(ValueError, 'run --tree-only as root'):
                    module.tree_gate(base)
            passed = {'passed': True, 'tree_gate_uid': 0, 'build_tree': str(base.resolve())}
            result = Path(tmp) / 'tree-audit.json'
            result.write_text(json.dumps(passed))
            self.assertEqual(module.load_tree_result(result, base)['build_tree'], str(base.resolve()))
            for name, value in {'failed': dict(passed, passed=False),
                                'unprivileged': dict(passed, tree_gate_uid=1000),
                                'other tree': dict(passed, build_tree='/elsewhere')}.items():
                with self.subTest(name=name):
                    result.write_text(json.dumps(value))
                    with self.assertRaises(ValueError):
                        module.load_tree_result(result, base)
            link = Path(tmp) / 'link.json'
            link.symlink_to(result)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                module.load_tree_result(link, base)
            with self.assertRaisesRegex(ValueError, 'missing'):
                module.load_tree_result(Path(tmp) / 'absent.json', base)

    def test_driver_runs_the_tree_gate_as_root_before_the_unprivileged_iso_audit(self):
        script = (ROOT / 'os/scripts/build-release-iso.sh').read_text()
        body = script[script.index('audit_media() {'):script.index('step_audit() {')]
        tree = body.index('--tree-only')
        self.assertNotIn('as_builder', body[body.rindex('\n', 0, tree):tree])
        self.assertLess(tree, body.index('as_builder python3 -I "$repo/tools/audit-installer-media.py"'))
        self.assertIn('--tree-result "$build/tree-audit.json"', body)
        self.assertIn('return 1', body[tree:body.index('as_builder python3')])

    def test_audit_capsule_structure_and_integrity(self):
        sys.path.insert(0, str(ROOT / 'tools'))
        import media_constants
        abi = media_constants.qualified_kernel_abi()
        fake_module_bytes = b'fake passthrough module content\n'
        fake_mod_hash = hashlib.sha256(fake_module_bytes).hexdigest()

        with tempfile.TemporaryDirectory(prefix='wros-capsule-') as tmp:
            tmp_path = Path(tmp)
            capsule = tmp_path / 'capsule'
            capsule.mkdir(parents=True)

            # Prepare initrd.gz containing passthrough.so
            initrd_tree = tmp_path / 'initrd-tree'
            so_file = initrd_tree / 'usr/lib/cdebconf/frontend/passthrough.so'
            so_file.parent.mkdir(parents=True)
            so_file.write_bytes(fake_module_bytes)
            raw_cpio = subprocess.check_output(
                ['cpio', '--create', '--format=newc', '--quiet'],
                cwd=initrd_tree,
                input=b'usr/lib/cdebconf/frontend/passthrough.so\n'
            )
            initrd_gz = gzip.compress(raw_cpio, mtime=0)
            initrd_hash = hashlib.sha256(initrd_gz).hexdigest()
            (capsule / 'initrd.gz').write_bytes(initrd_gz)

            # Prepare vmlinuz
            vmlinuz_bytes = b'synthetic test vmlinuz kernel image\n'
            vmlinuz_hash = hashlib.sha256(vmlinuz_bytes).hexdigest()
            (capsule / 'vmlinuz').write_bytes(vmlinuz_bytes)

            # Prepare pool deb
            deb_bytes = b'synthetic pool package archive\n'
            deb_hash = hashlib.sha256(deb_bytes).hexdigest()
            deb_file = capsule / 'pool/main/test.deb'
            deb_file.parent.mkdir(parents=True)
            deb_file.write_bytes(deb_bytes)

            # Prepare Packages and Packages.gz
            pkg_text = 'Package: test\nFilename: native-installer/pool/main/test.deb\n\n'
            pkg_bytes = pkg_text.encode('utf-8')
            pkg_hash = hashlib.sha256(pkg_bytes).hexdigest()
            pkg_gz_bytes = gzip.compress(pkg_bytes, mtime=0)
            pkg_gz_hash = hashlib.sha256(pkg_gz_bytes).hexdigest()
            (capsule / 'Packages').write_bytes(pkg_bytes)
            (capsule / 'Packages.gz').write_bytes(pkg_gz_bytes)

            # Prepare media-Release
            index_path = 'main/debian-installer/binary-amd64/Packages'
            release_text = (
                f"Origin: WrOS\nLabel: WrOS\nSuite: trixie\nCodename: trixie\nComponents: main\n"
                f"SHA256:\n"
                f" {pkg_hash} {len(pkg_bytes)} {index_path}\n"
                f" {pkg_gz_hash} {len(pkg_gz_bytes)} {index_path}.gz\n"
            )
            release_bytes = release_text.encode('utf-8')
            release_hash = hashlib.sha256(release_bytes).hexdigest()
            (capsule / 'media-Release').write_bytes(release_bytes)

            # Prepare backend-manifest.json
            backend_manifest = {
                'passed': True,
                'architecture': 'amd64',
                'kernel_version': abi,
                'module_sha256': fake_mod_hash,
                'kernel_sha256': vmlinuz_hash,
                'initrd_sha256': initrd_hash
            }
            backend_mf_bytes = json.dumps(backend_manifest).encode('utf-8')
            backend_mf_hash = hashlib.sha256(backend_mf_bytes).hexdigest()
            (capsule / 'backend-manifest.json').write_bytes(backend_mf_bytes)

            # Prepare pool-manifest.json
            pool_manifest = {
                'passed': True,
                'kernel_abi': abi,
                'package_count': 1,
                'archives': [{'file': 'pool/main/test.deb', 'sha256': deb_hash, 'bytes': len(deb_bytes)}],
                'generated_packages_sha256': pkg_hash,
                'generated_packages_gz_sha256': pkg_gz_hash
            }
            pool_mf_bytes = json.dumps(pool_manifest).encode('utf-8')
            pool_mf_hash = hashlib.sha256(pool_mf_bytes).hexdigest()
            (capsule / 'pool-manifest.json').write_bytes(pool_mf_bytes)

            # Prepare manifest.json
            file_hashes = {
                'vmlinuz': vmlinuz_hash,
                'initrd.gz': initrd_hash,
                'Packages': pkg_hash,
                'Packages.gz': pkg_gz_hash,
                'media-Release': release_hash,
                'backend-manifest.json': backend_mf_hash,
                'pool-manifest.json': pool_mf_hash,
                'pool/main/test.deb': deb_hash
            }
            manifest = {
                'passed': True,
                'architecture': 'amd64',
                'media_subdirectory': 'native-installer',
                'kernel_release': abi,
                'module_sha256': fake_mod_hash,
                'module_observed_in_compressed_initrd': True,
                'backend_kernel_matches_live_medium': True,
                'pool_independently_revalidated': True,
                'generated_media_release_debian_signed': False,
                'file_sha256': file_hashes
            }
            (capsule / 'manifest.json').write_text(json.dumps(manifest))

            # Audit with mocked module_sha256
            with mock.patch.object(media_constants, 'module_sha256', return_value=fake_mod_hash):
                res = module.audit_capsule(capsule, vmlinuz_hash, abi)
                self.assertEqual(res['kernel_release'], abi)
                self.assertEqual(res['package_count'], 1)

                # Altered kernel raises
                with self.assertRaisesRegex(ValueError, 'kernel differs from the live medium kernel'):
                    module.audit_capsule(capsule, '0' * 64, abi)

                # Altered manifest kernel_release raises
                bad_manifest = dict(manifest, kernel_release='different-abi')
                (capsule / 'manifest.json').write_text(json.dumps(bad_manifest))
                with self.assertRaisesRegex(ValueError, 'differs from the squashfs kernel ABI'):
                    module.audit_capsule(capsule, vmlinuz_hash, abi)
                (capsule / 'manifest.json').write_text(json.dumps(manifest))

                # Altered deb package raises
                deb_file.write_bytes(b'tampered package bytes\n')
                with self.assertRaisesRegex(ValueError, 'capsule file hash differs'):
                    module.audit_capsule(capsule, vmlinuz_hash, abi)
                deb_file.write_bytes(deb_bytes)
