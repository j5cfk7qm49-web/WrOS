"""Debian parser/graph and private archive fixtures; no devices or package installation.

GPG is mocked only in Release-policy unit tests. The real pool recipe separately
verifies actual Debian signatures and archives with the system archive keyring.
"""
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from debian.deb822 import Packages

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('live_installer_pool', ROOT/'tools/build-live-installer-pool.py')
pool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pool)
NOW = datetime(2026, 10, 28, 12, tzinfo=timezone.utc)


def record(name, version='1', **extra):
    values = {'Package': name, 'Version': version, 'Architecture': 'amd64',
              'Filename': 'pool/main/f/fixture/'+name+'_1_amd64.udeb',
              'SHA256': hashlib.sha256(name.encode()).hexdigest(), 'Size': '1'}
    values.update(extra)
    return Packages(values)


def kernels():
    return [record(family+'-'+abi+'-di') for abi in (pool.ABI, pool.RESCUE_ABI)
            for family in ('kernel-image', 'crypto-modules', 'crypto-dm-modules', 'ext4-modules')]


class LiveInstallerPool(unittest.TestCase):
    def signed_fixture(self, root, paragraphs=None, *, expired=False, future=False, suite='trixie'):
        index = root/'Packages'
        index.write_text('\n'.join(p.dump() for p in (paragraphs or [record('fixture')]))+'\n')
        release = (f'Origin: Debian\nCodename: {suite}\n'
                   f'Date: {format_datetime(NOW+timedelta(days=1) if future else NOW-timedelta(hours=1))}\n'
                   f'Valid-Until: {format_datetime(NOW-timedelta(hours=1) if expired else NOW+timedelta(days=1))}\n'
                   f'SHA256:\n {pool.digest(index)} {index.stat().st_size} {pool.INDEX}\n')
        signed = root/'InRelease'; signed.write_bytes(b'unit-test GPG stand-in; not a signed Debian release\n')

        def gpg(arguments, **_):
            self.assertEqual(arguments[0], 'gpgv')
            Path(arguments[arguments.index('--output')+1]).write_text(release)
            return subprocess.CompletedProcess(arguments, 0)
        return signed, index, gpg

    def test_release_policy_accepts_only_authenticated_exact_index_bytes(self):
        with tempfile.TemporaryDirectory(prefix='wros-pool-policy-') as directory:
            root = Path(directory); signed, index, gpg = self.signed_fixture(root)
            with mock.patch.object(pool.subprocess, 'run', side_effect=gpg):
                rows, _, receipt = pool.load_index(signed, index, 'trixie', root/'fixture-keyring', NOW)
                self.assertEqual([r['Package'] for r in rows], ['fixture'])
                self.assertEqual(receipt['packages_sha256'], pool.digest(index))
                index.write_bytes(index.read_bytes()+b'changed')
                with self.assertRaisesRegex(ValueError, 'index hash mismatch'):
                    pool.load_index(signed, index, 'trixie', root/'fixture-keyring', NOW)
            with mock.patch.object(pool.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, ['gpgv'])):
                with self.assertRaises(subprocess.CalledProcessError):
                    pool.load_index(signed, index, 'trixie', root/'fixture-keyring', NOW)

    def test_expired_future_wrong_suite_and_duplicate_records_are_rejected(self):
        for values, message in (({'expired': True}, 'expired'), ({'future': True}, 'future'),
                                ({'suite': 'sid'}, 'identity'),
                                ({'paragraphs': [record('fixture'), record('fixture')]}, 'Duplicate')):
            with self.subTest(values=values), tempfile.TemporaryDirectory(prefix='wros-pool-policy-') as directory:
                root = Path(directory); signed, index, gpg = self.signed_fixture(root, **values)
                with mock.patch.object(pool.subprocess, 'run', side_effect=gpg):
                    with self.assertRaisesRegex(ValueError, message):
                        pool.load_index(signed, index, 'trixie', root/'fixture-keyring', NOW)

    def test_parsed_bytes_are_the_verified_snapshot_and_changed_inputs_cannot_be_copied(self):
        with tempfile.TemporaryDirectory(prefix='wros-pool-snapshot-') as directory:
            root = Path(directory); signed, index, gpg = self.signed_fixture(root)
            original = index.read_bytes()

            def change_during_signature(arguments, **options):
                reply = gpg(arguments, **options)
                index.write_text(index.read_text().replace('Package: fixture', 'Package: different'))
                return reply

            with mock.patch.object(pool.subprocess, 'run', side_effect=change_during_signature):
                rows, _, receipt = pool.load_index(signed, index, 'trixie', root/'fixture-keyring', NOW)
            self.assertEqual([r['Package'] for r in rows], ['fixture'])
            self.assertEqual(receipt['packages_sha256'], hashlib.sha256(original).hexdigest())
            with self.assertRaisesRegex(ValueError, 'input bytes changed'):
                pool.copy_index(index, root/'not-published', receipt['packages_sha256'])
            self.assertFalse((root/'not-published').exists())

    def test_archive_paths_cannot_escape_or_reinterpret_a_regular_udeb(self):
        for value in ('/pool/fixture.udeb', 'pool/../fixture.udeb', 'pool//fixture.udeb',
                      'pool/./fixture.udeb', 'pool/fixture.deb', '../fixture.udeb'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                pool.archive_path(value)
        self.assertEqual(str(pool.archive_path('pool/main/f/fixture.udeb')), 'pool/main/f/fixture.udeb')

    def test_debian_epochs_and_same_version_hash_conflicts_use_maintained_comparison(self):
        indexes = {'main': kernels()+[record('fixture', '1:1')], 'security': [record('fixture', '2')]}
        self.assertEqual(pool.select_packages(indexes)['fixture'][1]['Version'], '1:1')
        indexes['security'] = [record('fixture', '2:0')]
        self.assertEqual(pool.select_packages(indexes)['fixture'][0], 'security')
        indexes['security'] = [record('fixture', '1:1', SHA256='0'*64)]
        with self.assertRaisesRegex(ValueError, 'same package version'):
            pool.select_packages(indexes)

    def test_versioned_virtual_dependencies_do_not_borrow_a_providers_own_version(self):
        selected = {'provider': ('main', record('provider', '999', Provides='virtual')),
                    'client': ('main', record('client', Depends='virtual (>= 2) | alternate'))}
        self.assertEqual(len(pool.dependency_gaps(selected)), 1)
        selected['provider'][1]['Provides'] = 'virtual (= 2)'
        self.assertEqual(pool.dependency_gaps(selected), [])
        selected['provider'][1]['Provides'] = 'virtual'
        selected['alternate'] = 'main', record('alternate')
        self.assertEqual(pool.dependency_gaps(selected), [])

    def test_optional_omissions_cannot_remove_a_required_engine_or_kernel(self):
        selected = {'engine': ('main', record('engine')),
                    'optional': ('main', record('optional', Depends='not-in-installer-pool')),
                    'dependent': ('main', record('dependent', Depends='optional'))}
        closed, omitted = pool.native_catalogue(selected, {'engine'})
        self.assertEqual(set(closed), {'engine'})
        self.assertEqual({p['package'] for p in omitted}, {'optional', 'dependent'})
        with self.assertRaisesRegex(ValueError, 'Required native installer dependency'):
            pool.native_catalogue(selected, {'engine', 'optional'})
        kernel = 'kernel-image-'+pool.ABI+'-di'
        selected[kernel] = 'security', record(kernel, Depends='absent-kernel-dependency')
        with self.assertRaisesRegex(ValueError, 'Required native installer dependency'):
            pool.native_catalogue(selected, {'engine'})

    def test_real_archive_identity_copy_and_existing_symlink_preservation(self):
        with tempfile.TemporaryDirectory(prefix='wros-pool-archive-') as directory:
            root = Path(directory); package = root/'package'; (package/'DEBIAN').mkdir(parents=True)
            (package/'DEBIAN/control').write_text(
                'Package: fixture\nVersion: 1\nArchitecture: amd64\n'
                'Maintainer: Fixture <fixture@example.invalid>\nDescription: private archive fixture\n')
            archive = root/'fixture.udeb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(package), str(archive)],
                           check=True, stdout=subprocess.DEVNULL)
            metadata = record('fixture', SHA256=pool.digest(archive), Size=str(archive.stat().st_size))
            selected, found = {'fixture': ('main', metadata)}, {metadata['SHA256']: archive}
            output = root/'output'; output.mkdir()
            rows = pool.write_pool(output, selected, found)
            self.assertEqual(rows[0]['sha256'], pool.digest(archive))
            self.assertIn('Filename: native-installer/pool/', (output/'Packages').read_text())
            second = root/'second'; second.mkdir()
            target = second/metadata['Filename']; target.parent.mkdir(parents=True)
            protected = root/'never-created'; target.symlink_to(protected)
            with self.assertRaisesRegex(ValueError, 'collision'):
                pool.write_pool(second, selected, found)
            self.assertFalse(protected.exists())
            wrong = Packages(metadata); wrong['Package'] = 'different'
            with self.assertRaisesRegex(ValueError, 'control identity'):
                pool.write_pool(root/'third', {'different': ('main', wrong)}, found)

    def test_ordinary_package_cannot_overwrite_the_qualified_transport(self):
        with tempfile.TemporaryDirectory(prefix='wros-pool-transport-') as directory:
            root = Path(directory); package = root/'package'; (package/'DEBIAN').mkdir(parents=True)
            (package/'DEBIAN/control').write_text(
                'Package: fixture\nVersion: 1\nArchitecture: amd64\n'
                'Maintainer: Fixture <fixture@example.invalid>\nDescription: private transport collision\n')
            module = package/'usr/lib/cdebconf/frontend/passthrough.so'
            module.parent.mkdir(parents=True); module.write_bytes(b'synthetic unqualified module')
            archive = root/'fixture.udeb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(package), str(archive)],
                           check=True, stdout=subprocess.DEVNULL)
            metadata = record('fixture', SHA256=pool.digest(archive), Size=str(archive.stat().st_size))
            with self.assertRaisesRegex(ValueError, 'qualified native passthrough'):
                pool.write_pool(root/'output', {'fixture': ('main', metadata)}, {metadata['SHA256']: archive})
            self.assertFalse((root/'output/pool').exists())
