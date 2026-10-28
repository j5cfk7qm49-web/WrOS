"""Private regular-file capsule fixtures; no real kernel, device or root grant.

Synthetic transport digests test receipt/CPIO boundaries. Actual staging
separately verifies the qualified Debian component and signed package pool.
"""
from datetime import datetime, timezone
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from debian.deb822 import Release

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('native_capsule', ROOT/'tools/stage-native-installer.py')
capsule = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capsule)
NOW = datetime(2026, 10, 28, 12, tzinfo=timezone.utc)
MODULE = b'Synthetic fixture module; never executed or accepted by the production pin.\n'
MODULE_SHA = hashlib.sha256(MODULE).hexdigest()


def fixture(root, versions=None, module=MODULE):
    backend, medium, tree = root/'backend', root/'medium', root/'tree'
    backend.mkdir(); (medium/'live').mkdir(parents=True)
    path = tree/'usr/lib/cdebconf/frontend/passthrough.so'; path.parent.mkdir(parents=True)
    path.write_bytes(module)
    for version in versions or [capsule.pool.ABI]:
        path = tree/'usr/lib/modules'/version/'modules.order'
        path.parent.mkdir(parents=True); path.write_bytes(b'fixture\n')
    names = [str(p.relative_to(tree)) for p in sorted(tree.rglob('*'))]
    raw = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'], cwd=tree,
                                  input=('\n'.join(names)+'\n').encode())
    (backend/'initrd.gz').write_bytes(gzip.compress(raw, mtime=0))
    (backend/'vmlinuz').write_bytes(b'Synthetic kernel fixture; never booted.\n')
    (medium/('live/vmlinuz-'+capsule.pool.ABI)).write_bytes((backend/'vmlinuz').read_bytes())
    record = {'passed': True, 'architecture': 'amd64', 'kernel_version': capsule.pool.ABI,
              'kernel_bytes_match_signed_debian_package': True,
              'module_observed_in_compressed_initrd': True, 'module_sha256': MODULE_SHA,
              'kernel_sha256': capsule.digest(backend/'vmlinuz'),
              'initrd_sha256': capsule.digest(backend/'initrd.gz'), 'synthetic_fixture_only': True}
    (backend/'manifest.json').write_text(json.dumps(record))
    return backend, medium


class NativeInstallerCapsule(unittest.TestCase):
    def test_generated_release_preserves_binary_pool_and_labels_custom_metadata(self):
        binary = 'main/binary-amd64/Packages'
        original = (f'Origin: Debian\nLabel: Debian\nCodename: trixie\nSuite: trixie\n'
                    f'Components: main non-free-firmware\nSHA256:\n'
                    f' {"a"*64} 7 {binary}\n {"b"*64} 9 {capsule.INDEX}\n'
                    f' {"c"*64} 2 Release\n').encode()
        package_index = b'new native package index\n'
        compressed = gzip.compress(package_index, mtime=0)
        updated = Release(capsule.media_release(original, package_index, compressed, NOW).decode())
        self.assertEqual((updated['Origin'], updated['Label']), ('WrOS', 'WrOS'))
        self.assertEqual(updated['Components'], 'main non-free-firmware')
        hashes = {row['name']: row for row in updated['SHA256']}
        self.assertEqual(hashes[binary]['sha256'], 'a'*64)
        self.assertNotIn('Release', hashes)
        self.assertEqual(hashes[capsule.INDEX]['sha256'], hashlib.sha256(package_index).hexdigest())
        self.assertEqual(hashes[capsule.INDEX+'.gz']['sha256'], hashlib.sha256(compressed).hexdigest())
        self.assertTrue(original.startswith(b'Origin: Debian\n'), 'Base media input remains intact')

    def test_wrong_media_suite_cannot_be_relabelled_as_the_qualified_view(self):
        with self.assertRaisesRegex(ValueError, 'Debian 13'):
            capsule.media_release(b'Codename: sid\nComponents: main\n', b'index', b'compressed', NOW)

    def test_copy_checks_bytes_and_preserves_existing_files_and_symlinks(self):
        with tempfile.TemporaryDirectory(prefix='wros-capsule-copy-') as directory:
            root = Path(directory); source = root/'source'; source.write_bytes(b'expected component')
            target = root/'target'; target.write_bytes(b'existing personal fixture')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                capsule.copy_verified(source, target, capsule.digest(source))
            self.assertEqual(target.read_bytes(), b'existing personal fixture')
            link = root/'link'; link.symlink_to(root/'never-created')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                capsule.copy_verified(source, link, capsule.digest(source))
            self.assertFalse((root/'never-created').exists())
            with self.assertRaisesRegex(ValueError, 'receipt'):
                capsule.copy_verified(source, root/'bad', '0'*64)
            self.assertFalse((root/'bad').exists())

    def test_real_cpio_receipt_and_live_kernel_binding(self):
        with tempfile.TemporaryDirectory(prefix='wros-capsule-cpio-') as directory:
            backend, medium = fixture(Path(directory))
            with mock.patch.object(capsule, 'MODULE_SHA256', MODULE_SHA):
                self.assertEqual(capsule.verify_backend(backend, medium)['kernel_version'], capsule.pool.ABI)
                (medium/('live/vmlinuz-'+capsule.pool.ABI)).write_bytes(b'different kernel')
                with self.assertRaisesRegex(ValueError, 'kernel/image bytes differ'):
                    capsule.verify_backend(backend, medium)

    def test_extra_kernel_modules_are_rejected_even_with_matching_kernel_bytes(self):
        with tempfile.TemporaryDirectory(prefix='wros-capsule-abi-') as directory:
            backend, medium = fixture(Path(directory), versions=[capsule.pool.ABI, capsule.pool.RESCUE_ABI])
            with mock.patch.object(capsule, 'MODULE_SHA256', MODULE_SHA):
                with self.assertRaisesRegex(ValueError, 'different kernel'):
                    capsule.verify_backend(backend, medium)

    def test_compressed_transport_cannot_be_replaced_using_an_edited_container_receipt(self):
        with tempfile.TemporaryDirectory(prefix='wros-capsule-module-') as directory:
            backend, medium = fixture(Path(directory), module=b'different synthetic module')
            with mock.patch.object(capsule, 'MODULE_SHA256', MODULE_SHA):
                with self.assertRaisesRegex(ValueError, 'transport differs'):
                    capsule.verify_backend(backend, medium)

    def test_existing_output_and_root_invocation_fail_before_staging(self):
        with tempfile.TemporaryDirectory(prefix='wros-capsule-preserve-') as directory:
            root = Path(directory)
            for name in ('backend', 'pool', 'medium', 'output'):
                (root/name).mkdir()
            protected = root/'output/original'; protected.write_bytes(b'keep this fixture')
            with self.assertRaisesRegex(ValueError, 'new absolute'):
                capsule.stage(root/'backend', root/'pool', root/'medium', root/'output')
            self.assertEqual(protected.read_bytes(), b'keep this fixture')
            with mock.patch.object(capsule.os, 'geteuid', return_value=0):
                with self.assertRaisesRegex(ValueError, 'normal user'):
                    capsule.stage(root/'backend', root/'pool', root/'medium', root/'new')
            self.assertFalse((root/'new').exists())
