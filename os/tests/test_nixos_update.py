"""Unit tests for the NixOS generation-based update engine (nixos/scripts/wros-update-engine.py)."""
import base64
from collections import Counter
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import http.server
import io
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
from unittest import mock
import urllib.error


REPO = Path(__file__).resolve().parents[2]
ENGINE_PATH = REPO / 'nixos/scripts/wros-update-engine.py'


def load_nixos_engine():
    loader = importlib.machinery.SourceFileLoader('wros_nixos_update_engine', str(ENGINE_PATH))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def load_nixos_publisher():
    spec = importlib.util.spec_from_file_location(
        'publish_nixos_updates', REPO / 'tools/publish-nixos-updates.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class NixosUpdateEngineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.engine = load_nixos_engine()
        cls.engine.TRUSTED_UID = os.geteuid()
        cls.engine._base.TRUSTED_UID = os.geteuid()

    def test_constants_match_debian_engine(self):
        base = self.engine._base
        for name in (
            'MAX_VALIDITY',
            'MAX_STAGED_AGE',
            'CLOCK_SKEW',
            'BOOT_CLOCK_SKEW',
            'PROBATION_FAILED_LOGINS',
        ):
            self.assertEqual(getattr(self.engine, name), getattr(base, name), name)
        self.assertEqual(self.engine.LAUNCHER_VERBS, (*base.LAUNCHER_VERBS, 'firmware'))
        self.assertEqual(base.ONLINE_URL, 'https://j5cfk7qm49-web.github.io/WrOS/updates/')
        self.assertEqual(self.engine.ONLINE_URL, 'https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/')
        self.assertEqual(self.engine.PINNED_SUITES, ('wros-nixos-updates',))

    def test_tilde_version_comparison_orders_rc_before_release(self):
        cmp_v = self.engine.compare_versions
        self.assertTrue(cmp_v('0.1.0~rc13', 'lt', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'gt', '0.1.0~rc13'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'lt', '0.1.0'))
        self.assertTrue(cmp_v('0.1.0', 'gt', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'eq', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0', 'lt', '0.1.0+hotfix1'))

    def _closure_manifest(self):
        toplevel = '/nix/store/' + 'a' * 32 + '-nixos-system-wros'
        return {
            'schema': 2, 'format': 'wros-nixos-cache-v2', 'version': '0.1.0~rc33',
            'system': 'x86_64-linux', 'toplevel': toplevel,
            'cache_files': [
                {'path': 'cache/nix-cache-info', 'sha256': 'a' * 64, 'size': 1},
                {'path': f'cache/{"a" * 32}.narinfo', 'sha256': 'a' * 64, 'size': 1,
                 'store_path': toplevel},
            ],
        }

    def test_a_manifest_without_firmware_parses_as_before(self):
        manifest = self._closure_manifest()
        closure = self.engine.parse_closure_manifest(json.dumps(manifest).encode())
        self.assertEqual(closure['firmware'], {})
        self.assertEqual(closure['toplevel'], manifest['toplevel'])
        self.assertEqual([f['path'] for f in closure['cache_files']],
                         [f['path'] for f in manifest['cache_files']])

    def test_a_firmware_section_with_a_shared_path_is_refused(self):
        manifest = self._closure_manifest()
        package = '/nix/store/' + 'b' * 32 + '-firmware'
        shared = {'path': 'cache/nar/firmware.nar.zst', 'sha256': 'b' * 64, 'size': 2,
                  'store_path': package}
        manifest['firmware'] = {'rtw88': {'store_path': package, 'files': [shared]}}
        closure = self.engine.parse_closure_manifest(json.dumps(manifest).encode())
        self.assertEqual(closure['firmware']['rtw88']['files'], [{**shared, 'url': None}])
        with self.subTest('also in cache_files'):
            manifest['cache_files'].append(shared)
            with self.assertRaisesRegex(ValueError, 'duplicate cache file'):
                self.engine.parse_closure_manifest(json.dumps(manifest).encode())
            manifest['cache_files'].pop()
        with self.subTest('also in another firmware package'):
            other = '/nix/store/' + 'c' * 32 + '-other-firmware'
            manifest['firmware']['rtw89'] = {'store_path': other, 'files': [shared]}
            with self.assertRaisesRegex(ValueError, 'duplicate cache file'):
                self.engine.parse_closure_manifest(json.dumps(manifest).encode())

    def _make_signed_bundle(self, root, *, version='0.1.0~rc14',
                            toplevel='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14',
                            moment=None, tamper_nar=False, baseline=None, orphan_narinfo=False,
                            upstream=False, firmware=()):
        moment = moment or datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc)
        until = moment + timedelta(days=14)
        gnupg = root / 'gnupg'
        gnupg.mkdir(mode=0o700)
        batch = (
            '%no-protection\n'
            'Key-Type: RSA\n'
            'Key-Length: 2048\n'
            'Name-Real: WrOS Test Signer\n'
            'Name-Email: test@wros.invalid\n'
            'Expire-Date: 0\n'
            '%commit\n'
        )
        # The caller's PATH first: the Nix check sandbox has gpg, but no /usr/bin.
        env = {'PATH': os.environ.get('PATH', '') + ':/usr/sbin:/usr/bin:/sbin:/bin',
               'LC_ALL': 'C', 'GNUPGHOME': str(gnupg)}
        subprocess.run(['gpg', '--batch', '--generate-key'], input=batch.encode(), env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        colons = subprocess.check_output(
            ['gpg', '--batch', '--with-colons', '--fingerprint', '--list-secret-keys'],
            env=env, text=True)
        fpr = next(line.split(':')[9] for line in colons.splitlines() if line.startswith('fpr:'))
        keyring = root / 'keyring.gpg'
        signers = root / 'update-signers'
        subprocess.run(['gpg', '--batch', '--output', str(keyring), '--export', fpr], env=env, check=True)
        signers.write_text(fpr + '\n', encoding='utf-8')
        os.chmod(keyring, 0o644)
        os.chmod(signers, 0o644)

        bundle = root / 'bundle'
        (bundle / 'cache/nar').mkdir(parents=True)
        cache_info = b'StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30\n'
        (bundle / 'cache/nix-cache-info').write_bytes(cache_info)
        top_hash = Path(toplevel).name.split('-', 1)[0]
        nar_rel = f'cache/nar/{top_hash}.nar.zst'
        nar_bytes = b'fake-compressed-nar-payload-' + version.encode()
        (bundle / nar_rel).write_bytes(nar_bytes)
        narinfo_rel = f'cache/{top_hash}.narinfo'
        narinfo_bytes = (
            f'StorePath: {toplevel}\n'
            f'URL: nar/{top_hash}.nar.zst\n'
            f'Compression: zstd\n'
            f'FileHash: sha256:{hashlib.sha256(nar_bytes).hexdigest()}\n'
            f'FileSize: {len(nar_bytes)}\n'
        ).encode()
        (bundle / narinfo_rel).write_bytes(narinfo_bytes)

        closure = {
            'schema': 2,
            'format': 'wros-nixos-cache-v2',
            'version': version,
            'system': 'x86_64-linux',
            'toplevel': toplevel,
            'baseline': baseline,
            'app_only': False,
            'cache_files': [
                {
                    'path': 'cache/nix-cache-info',
                    'sha256': hashlib.sha256(cache_info).hexdigest(),
                    'size': len(cache_info),
                    'store_path': None,
                },
                {
                    'path': narinfo_rel,
                    'sha256': hashlib.sha256(narinfo_bytes).hexdigest(),
                    'size': len(narinfo_bytes),
                    'store_path': toplevel,
                },
                {
                    'path': nar_rel,
                    'sha256': hashlib.sha256(nar_bytes).hexdigest(),
                    'size': len(nar_bytes),
                    'store_path': toplevel,
                },
            ],
        }
        if orphan_narinfo:
            # A package whose archive the bundle lacks (rc15's first sudo.pam).
            orphan = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-sudo.pam'
            orphan_rel = 'cache/abcdfghijklmnpqrsvwxyz0123456789.narinfo'
            orphan_bytes = f'StorePath: {orphan}\nURL: nar/missing.nar.zst\n'.encode()
            (bundle / orphan_rel).write_bytes(orphan_bytes)
            closure['cache_files'].append({
                'path': orphan_rel, 'sha256': hashlib.sha256(orphan_bytes).hexdigest(),
                'size': len(orphan_bytes), 'store_path': orphan})
        if upstream:
            # A thin bundle (rc23, #169): one package comes from cache.nixos.org, its files kept
            # beside the bundle in root/upstream as the cache would serve them.
            pkg = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-mpv-0.41.0'
            pkg_hash = Path(pkg).name.split('-', 1)[0]
            nar_name = 'nar/' + 'q' * 52 + '.nar.xz'
            pkg_nar = b'upstream-nar-payload'
            pkg_info = (f'StorePath: {pkg}\nURL: {nar_name}\nCompression: xz\n').encode()
            served = root / 'upstream'
            for rel, data in ((f'{pkg_hash}.narinfo', pkg_info), (nar_name, pkg_nar)):
                (served / rel).parent.mkdir(parents=True, exist_ok=True)
                (served / rel).write_bytes(data)
                closure['cache_files'].append({
                    'path': 'cache/' + rel, 'sha256': hashlib.sha256(data).hexdigest(),
                    'size': len(data), 'store_path': pkg,
                    'url': 'https://cache.nixos.org/' + rel})
            closure['thin'] = True
        if firmware:
            closure['firmware'] = {}
            for digit, name in zip('bcd', firmware, strict=True):
                store_path = '/nix/store/' + digit * 32 + '-' + name
                archive = f'nar/{name}.nar.zst'
                files = []
                for rel, data in ((digit * 32 + '.narinfo', f'StorePath: {store_path}\nURL: {archive}\n'.encode()),
                                  (archive, name.encode())):
                    rel = 'cache/' + rel
                    (bundle / rel).write_bytes(data)
                    files.append({'path': rel, 'sha256': hashlib.sha256(data).hexdigest(),
                                  'size': len(data), 'store_path': store_path})
                closure['firmware'][name] = {'store_path': store_path, 'files': files}
        closure_raw = (json.dumps(closure, indent=2, sort_keys=True) + '\n').encode()
        (bundle / 'closure-manifest.json').write_bytes(closure_raw)
        if tamper_nar:
            (bundle / nar_rel).write_bytes(nar_bytes + b'-corrupted')

        rfc_now = format_datetime(moment, usegmt=True).replace('GMT', 'UTC')
        rfc_until = format_datetime(until, usegmt=True).replace('GMT', 'UTC')
        release = (
            f'Origin: WrOS\nLabel: WrOS NixOS updates\nSuite: wros-nixos-updates\n'
            f'Date: {rfc_now}\nValid-Until: {rfc_until}\nArchitectures: amd64\nSHA256:\n'
            f' {hashlib.sha256(closure_raw).hexdigest()} {len(closure_raw)} closure-manifest.json\n'
        )
        (bundle / 'Release').write_text(release, encoding='utf-8')
        subprocess.run(['gpg', '--batch', '--yes', '--local-user', fpr, '--clearsign',
                        '--output', str(bundle / 'InRelease'), str(bundle / 'Release')],
                       env=env, check=True)
        return bundle, keyring, signers, moment

    def _online_bundle(self, root, baseline):
        bundle, keyring, signers, moment = self._make_signed_bundle(
            root, version='0.1.0~rc18', baseline=baseline,
            toplevel='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc18')
        version_file = root / 'version'
        version_file.write_text('0.1.0~rc16\n', encoding='utf-8')
        store_dir = root / 'store'
        store_dir.mkdir()
        paths = dict(keyring=keyring, signers=signers, now=moment + timedelta(minutes=1),
                     version_file=version_file, current_system=root / 'current-system',
                     store_dir=store_dir)

        def get_metadata(url, _limit, _opener, **_kwargs):
            return (bundle / url.removeprefix(self.engine.ONLINE_URL)).read_bytes()

        return bundle, paths, mock.patch.object(self.engine, 'http_get', side_effect=get_metadata)

    def _firmware_update(self, root, *, present=(), running=(), detected=('rtw88',), target_missing=()):
        bundle, keyring, signers, moment = self._make_signed_bundle(
            root, firmware=('rtw88', 'ath9k-htc', 'rtw89'))
        closure = self.engine.parse_closure_manifest((bundle / 'closure-manifest.json').read_bytes())
        packages = closure['firmware']
        store_dir = root / 'store'
        current = store_dir / ('f' * 32 + '-current')
        current.mkdir(parents=True)
        (current / 'wros-firmware.json').touch()
        current_system = root / 'current-system'
        current_system.symlink_to(current)
        sets = root / 'firmware'
        running_set = sets / current.name
        running_set.mkdir(parents=True)
        (running_set / 'packages.json').write_text(json.dumps({'packages': running}), encoding='utf-8')
        for name in present:
            (store_dir / Path(packages[name]['store_path']).name).mkdir()
        paths = dict(keyring=keyring, signers=signers, now=moment + timedelta(minutes=1),
                     version_file=root / 'version', current_system=current_system, store_dir=store_dir)
        asked, commands = [], []

        class Opener:
            def open(self, url, timeout=None):
                rel = url.removeprefix('http://channel.test/')
                asked.append(rel)
                return io.BytesIO((bundle / rel).read_bytes())

        def run(cmd):
            commands.append(cmd)
            reply = ''
            if cmd[0] == 'nix' and 'copy' in cmd:
                for arg in cmd:
                    if arg.startswith('/nix/store/'):
                        (store_dir / Path(arg).name).mkdir(exist_ok=True)
            elif cmd[0] == str(self.engine.FIRMWARE_TOOL):
                if cmd[1] == 'detect':
                    self.assertEqual(cmd[2:], ['--system', str(current_system)])
                    reply = json.dumps({'packages': detected, 'unknown': []})
                elif cmd[1] == 'ensure':
                    self.assertEqual(cmd[2:], [closure['toplevel']])
                    missing = []
                    for name in target_missing:
                        package = packages.get(name.replace('_', '-'))
                        if package is None or not (store_dir / Path(package['store_path']).name).exists():
                            missing.append(name)
                    reply = json.dumps({'packages': [], 'missing': missing, 'unknown': []})
            return subprocess.CompletedProcess(cmd, 0, reply, '')

        cache = root / 'online'
        online = dict(base_url='http://channel.test/', opener=Opener(), firmware_sets_dir=sets, runner=run)
        got = self.engine.fetch_online(cache=cache, **online, **paths)
        return got, paths, online, cache, commands, asked, closure

    def test_fetch_downloads_only_the_detected_firmware(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-fetch-') as tmp:
            got, _, _, _, _, asked, closure = self._firmware_update(Path(tmp))
            needed = {f['path'] for f in closure['firmware']['rtw88']['files']}
            extra = set(asked) - {f['path'] for f in closure['cache_files']} - {
                'InRelease', 'closure-manifest.json'}
            self.assertEqual(extra, needed)
            self.assertTrue(all((Path(got['path']) / path).is_file() for path in needed))

    def test_fetch_skips_firmware_already_in_the_store(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-present-') as tmp:
            _, _, _, _, _, asked, closure = self._firmware_update(Path(tmp), present=('rtw88',))
            self.assertTrue(all(f['path'] not in asked for f in closure['firmware']['rtw88']['files']))

    def test_fetch_includes_the_running_sets_packages(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-running-') as tmp:
            _, _, _, _, _, asked, closure = self._firmware_update(Path(tmp), running=('ath9k_htc',))
            self.assertTrue(all(f['path'] in asked for f in closure['firmware']['ath9k-htc']['files']))
            self.assertTrue(all(f['path'] not in asked for f in closure['firmware']['rtw89']['files']))

    def _device_firmware(self, root, *, detected=('rtw88',), installed=(), present=(), different=None,
                         offline=False, stick=False, drop_download=False):
        bundle, keyring, signers, moment = self._make_signed_bundle(
            root, firmware=('rtw88', 'ath9k-htc', 'rtw89'))
        manifest = json.loads((bundle / 'closure-manifest.json').read_bytes())
        packages = manifest['firmware']
        table = {name.replace('-', '_'): {'store_path': package['store_path']}
                 for name, package in packages.items()}
        table['unlisted'] = {'store_path': '/nix/store/' + 'e' * 32 + '-unlisted'}
        if different is not None:
            table[different]['store_path'] = '/nix/store/' + 'e' * 32 + '-previous-firmware'
        store = root / 'store'
        if stick:
            for name in ('rtw88', 'ath9k_htc'):
                tree = root / 'stick-store' / Path(table[name]['store_path']).name
                firmware = tree / 'lib/firmware/example.bin'
                firmware.parent.mkdir(parents=True)
                firmware.write_bytes(b'test firmware for ' + name.encode())
                (firmware.parent / 'alias.bin').symlink_to(firmware.name)
                nar = subprocess.check_output(['nix-store', '--dump', str(tree)])
                table[name].update(nar_sha256=hashlib.sha256(nar).hexdigest(), nar_size=len(nar))
        current = store / ('f' * 32 + '-current')
        current.mkdir(parents=True)
        (current / 'wros-firmware.json').write_text(json.dumps({'format': 1, 'packages': table}))
        current_system = root / 'current-system'
        current_system.symlink_to(current)
        profile = root / 'system'
        profile.symlink_to(current)
        sets = root / 'sets'
        running_set = sets / current.name
        running_set.mkdir(parents=True)
        metadata = running_set / 'packages.json'
        metadata.write_text(json.dumps({'packages': installed}))
        for name in (*installed, *present):
            (store / Path(table[name]['store_path']).name).mkdir()
        asked, commands = [], []
        boot = mock.Mock(return_value=0)

        class Opener:
            def open(self, url, timeout=None):
                rel = url.removeprefix('http://channel.test/')
                asked.append(rel)
                if offline or (drop_download and rel.startswith('cache/')):
                    raise urllib.error.URLError('no connection')
                return io.BytesIO((bundle / rel).read_bytes())

        def run(cmd, **streams):
            commands.append(cmd)
            executable = Path(cmd[0]).name
            if executable == 'lsblk':
                output = '/dev/sdb iso9660 WROS_LIVE disk\n' if stick else ''
                return subprocess.CompletedProcess(cmd, 0, output, '')
            if executable == 'mount':
                target = Path(cmd[-1])
                if 'squashfs' in cmd:
                    shutil.copytree(root / 'stick-store', target, dirs_exist_ok=True, symlinks=True)
                else:
                    (target / 'nix-store.squashfs').touch()
            if executable == 'umount':
                for entry in Path(cmd[-1]).iterdir():
                    if entry.is_dir() and not entry.is_symlink():
                        shutil.rmtree(entry)
                    else:
                        entry.unlink()
            if cmd[0] == 'prlimit':
                return subprocess.run(cmd, stderr=subprocess.PIPE, text=True, **streams)
            if cmd == ['nix-store', '--import']:
                result = subprocess.run(
                    [*cmd, '--store', f'local?root={root / "import-root"}'],
                    capture_output=True, text=True, **streams)
                if result.returncode == 0:
                    for path in result.stdout.splitlines():
                        (store / Path(path).name).mkdir()
                return result
            if cmd[1:2] == ['detect']:
                return subprocess.CompletedProcess(cmd, 0, json.dumps({'packages': detected}), '')
            if cmd[0] == 'nix':
                for arg in cmd:
                    if arg.startswith('/nix/store/'):
                        (store / Path(arg).name).mkdir(exist_ok=True)
            if cmd[1:2] == ['ensure']:
                names = sorted(set(cmd[4:]) | set(json.loads(metadata.read_text())['packages']))
                reply = json.dumps({'packages': names, 'missing': []})
                metadata.write_text(reply)
                return subprocess.CompletedProcess(cmd, 0, reply, '')
            if cmd[1:] == ['boot']:
                return subprocess.CompletedProcess(cmd, boot(), '', 'boot refresh failed')
            return subprocess.CompletedProcess(cmd, 0, '', '')

        def invoke():
            # Preserve the test process's umask across launcher_main.
            previous = os.umask(0o022)
            try:
                return self.engine.launcher_main(
                    'firmware', [], '1000', euid=0, runner=run,
                    base_url='http://channel.test/', opener=Opener(), cache=root / 'firmware-download',
                    keyring=keyring, signers=signers, now=moment + timedelta(minutes=1),
                    state_dir=root / 'state', current_system=current_system, profile=profile,
                    firmware_sets_dir=sets, store_dir=store, media_root=root / 'media')
            finally:
                os.umask(previous)

        return invoke, asked, commands, packages, boot

    def test_a_stick_package_matching_the_table_is_added(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, asked, commands, packages, _ = self._device_firmware(root, offline=True, stick=True)
            self.assertEqual(invoke(), {'status': 'added', 'packages': ['rtw88']})
            self.assertEqual(asked, ['InRelease'])
            self.assertEqual([Path(cmd[0]).name for cmd in commands[1:-2]],
                             ['lsblk', 'mount', 'mount', 'prlimit', 'nix-store', 'umount', 'umount'])
            self.assertEqual([cmd[1] for cmd in commands[-2:]], ['ensure', 'boot'])
            self.assertEqual(list((root / 'media').iterdir()), [])
            self.assertEqual((root / 'media').stat().st_mode & 0o777, 0o755)
            self.assertEqual(list((root / 'sets').glob('.firmware-*')), [])
            store_path = packages['rtw88']['store_path']
            rows = subprocess.check_output([
                'nix', '--offline', '--extra-experimental-features', 'nix-command',
                'path-info', '--json', '--json-format', '1',
                '--store', f'local?root={root / "import-root"}', store_path,
            ])
            info = json.loads(rows)[store_path]
            table, = (root / 'store').glob('*/wros-firmware.json')
            entry = json.loads(table.read_text())['packages']['rtw88']
            self.assertEqual(info['narHash'], 'sha256-' + base64.b64encode(
                bytes.fromhex(entry['nar_sha256'])).decode())
            self.assertEqual(info['narSize'], entry['nar_size'])
            restored = root / 'import-root/nix/store' / Path(store_path).name
            self.assertEqual(os.readlink(restored / 'lib/firmware/alias.bin'), 'example.bin')
            self.assertFalse((root / 'import-root/nix/store' / Path(packages['ath9k-htc']['store_path']).name).exists())

    def test_a_stick_package_with_another_hash_is_refused(self):
        for extra_bytes in (0, 4096):
            with self.subTest(extra_bytes=extra_bytes), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                invoke, _, commands, packages, _ = self._device_firmware(root, offline=True, stick=True)
                firmware = root / 'stick-store' / Path(packages['rtw88']['store_path']).name / 'lib/firmware/example.bin'
                firmware.write_bytes(b'X' * (firmware.stat().st_size + extra_bytes))
                with self.assertRaisesRegex(ValueError, 'failed SHA-256 or size check'):
                    invoke()
                self.assertFalse(any(cmd[1] in ('--import', 'ensure', 'boot') for cmd in commands))
                self.assertEqual(list((root / 'media').iterdir()), [])

    def test_stick_requires_the_exact_package_path_and_refuses_a_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            invoke, _, commands, packages, _ = self._device_firmware(root, offline=True, stick=True)
            package = root / 'stick-store' / Path(packages['rtw88']['store_path']).name
            other = package.rename(package.with_name('e' * 32 + '-rtw88'))
            self.assertEqual(invoke(), {'status': 'not-on-stick', 'packages': ['rtw88']})
            package.symlink_to(other.name)
            with self.assertRaisesRegex(ValueError, 'wrong type'):
                invoke()
            self.assertFalse(any(cmd[1] in ('--import', 'ensure', 'boot') for cmd in commands))
            self.assertEqual(list((root / 'media').iterdir()), [])

    def test_no_stick_and_no_network_says_so(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            invoke, asked, commands, _, _ = self._device_firmware(Path(tmp), offline=True)
            self.assertEqual(invoke(), {'status': 'media-needed', 'packages': ['rtw88']})
            self.assertEqual(asked, ['InRelease'])
            self.assertEqual([cmd[1] for cmd in commands], ['detect', '-rn'])

    def test_firmware_refuses_a_symlinked_media_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            invoke, _, commands, _, _ = self._device_firmware(root, offline=True)
            other = root / 'other'
            other.mkdir()
            (root / 'media').symlink_to(other)
            with self.assertRaisesRegex(ValueError, 'wrong type: media'):
                invoke()
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])
            self.assertEqual(list(other.iterdir()), [])

    def test_firmware_does_not_try_media_when_the_channel_manifest_fails_verification(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            invoke, asked, commands, _, _ = self._device_firmware(root)
            (root / 'bundle/closure-manifest.json').write_bytes(b'{}')
            with self.assertRaisesRegex(ValueError, 'does not match signed Release metadata'):
                invoke()
            self.assertEqual(asked, ['InRelease', 'closure-manifest.json'])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])

    def test_firmware_can_use_a_stick_if_the_channel_download_drops(self):
        with tempfile.TemporaryDirectory() as tmp:
            invoke, asked, commands, _, _ = self._device_firmware(
                Path(tmp), stick=True, drop_download=True, detected=('rtw88', 'ath9k_htc'))
            with mock.patch.object(self.engine, 'RETRY_PAUSE', 0):
                self.assertEqual(invoke(), {'status': 'added', 'packages': ['ath9k_htc', 'rtw88']})
            self.assertEqual(asked[:2], ['InRelease', 'closure-manifest.json'])
            self.assertIn('cache/nix-cache-info', asked)
            self.assertEqual(commands.count(['nix-store', '--import']), 1)

    def test_firmware_verb_adds_only_missing_packages(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, asked, commands, packages, _ = self._device_firmware(
                root, detected=('rtw88', 'ath9k_htc', 'rtw89'), installed=('rtw88',), present=('rtw89',))
            self.assertEqual(invoke(), {'status': 'added', 'packages': ['ath9k_htc', 'rtw89']})
            package = packages['ath9k-htc']
            self.assertCountEqual(asked, ['InRelease', 'closure-manifest.json', 'cache/nix-cache-info',
                                          *(f['path'] for f in package['files'])])
            detect, imported, ensure, boot = commands
            current = root / 'current-system'
            self.assertEqual(detect, [str(self.engine.FIRMWARE_TOOL), 'detect', '--system', str(current)])
            self.assertEqual([path for path in imported if path.startswith('/nix/store/')],
                             [package['store_path']])
            self.assertEqual(ensure, [str(self.engine.FIRMWARE_TOOL), 'ensure', str(current),
                                      '--add', 'ath9k_htc', 'rtw89'])
            self.assertEqual(boot, [str(current / 'bin/switch-to-configuration'), 'boot'])

    def test_firmware_verb_with_nothing_new_changes_nothing(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, asked, commands, _, _ = self._device_firmware(root, installed=('rtw88',))
            self.assertEqual(invoke(), {'status': 'nothing-needed', 'packages': []})
            self.assertEqual(asked, [])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])
            self.assertFalse((root / 'firmware-download').exists())

    def test_firmware_verb_returns_update_first_for_a_different_store_path(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            invoke, asked, commands, _, _ = self._device_firmware(
                Path(tmp), detected=('rtw88', 'ath9k_htc'), different='rtw88')
            self.assertEqual(invoke(), {'status': 'update-first', 'packages': ['rtw88']})
            self.assertEqual(asked, ['InRelease', 'closure-manifest.json'])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])

    def test_firmware_verb_reports_packages_absent_from_the_channel(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            invoke, asked, commands, _, _ = self._device_firmware(Path(tmp), detected=('unlisted',))
            self.assertEqual(invoke(), {'status': 'not-in-channel', 'packages': ['unlisted']})
            self.assertEqual(asked, ['InRelease', 'closure-manifest.json'])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])

    def test_firmware_verb_refuses_a_bad_signed_archive_hash(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, _, commands, packages, _ = self._device_firmware(root)
            archive = root / 'bundle' / packages['rtw88']['files'][1]['path']
            archive.write_bytes(b'X' * archive.stat().st_size)
            with mock.patch.object(self.engine, 'RETRY_PAUSE', 0):
                with self.assertRaisesRegex(ValueError, 'failed SHA-256'):
                    invoke()
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])

    def test_firmware_verb_preserves_the_default_boot_generation(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, asked, commands, _, _ = self._device_firmware(root)
            default = root / 'store' / ('a' * 32 + '-default')
            default.mkdir()
            (root / 'system').unlink()
            (root / 'system').symlink_to(default)
            with self.assertRaisesRegex(ValueError, 'default WrOS version'):
                invoke()
            self.assertEqual(asked, ['InRelease', 'closure-manifest.json'])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])

    def test_firmware_verb_refuses_a_symlinked_download_directory(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            root = Path(tmp)
            invoke, asked, commands, _, _ = self._device_firmware(root)
            other = root / 'other'
            other.mkdir()
            (root / 'firmware-download').symlink_to(other)
            with self.assertRaisesRegex(ValueError, 'wrong type: firmware-download'):
                invoke()
            self.assertEqual(asked, [])
            self.assertEqual([cmd[1] for cmd in commands], ['detect'])
            self.assertEqual(list(other.iterdir()), [])

    def test_firmware_verb_can_retry_a_failed_boot_refresh(self):
        with tempfile.TemporaryDirectory(prefix='wros-device-firmware-') as tmp:
            invoke, asked, commands, _, boot = self._device_firmware(Path(tmp))
            boot.side_effect = [1, 0]
            with self.assertRaisesRegex(ValueError, 'boot refresh failed'):
                invoke()
            asked.clear()
            commands.clear()
            self.assertEqual(invoke(), {'status': 'added', 'packages': ['rtw88']})
            self.assertEqual(asked, [])
            self.assertEqual([cmd[-1] for cmd in commands if cmd[1:] == ['boot']], ['boot'])
            self.assertFalse(any(cmd[0] == 'nix' for cmd in commands))

    def _stage_firmware_update(self, root, *, target_missing, tamper=False):
        got, paths, online, cache, commands, asked, closure = self._firmware_update(
            root, target_missing=target_missing)
        before_staging = list(asked)
        state = dict(state_dir=root / 'state', trigger_path=root / 'system-update', gcroots_dir=root / 'gcroots')
        review = self.engine.review_bundle(got['path'], state_dir=state['state_dir'], **paths)

        def stage():
            return self.engine.stage_update(got['path'], review['review_id'], allowed_prefixes=(str(root),),
                                            download_cache=cache, **online, **paths, **state)

        if tamper:
            archive = root / 'bundle' / closure['firmware']['ath9k-htc']['files'][1]['path']
            archive.write_bytes(b'X' * archive.stat().st_size)
            with mock.patch.object(self.engine, 'RETRY_PAUSE', 0):
                with self.assertRaisesRegex(ValueError, 'failed SHA-256'):
                    stage()
            self.assertFalse(state['trigger_path'].exists())
            package = closure['firmware']['ath9k-htc']['store_path']
            self.assertFalse(any(package in cmd for cmd in commands))
            return
        staged = stage()
        self.assertEqual(staged['status'], 'staged')
        self.assertFalse(any(cmd[-1] == 'boot' for cmd in commands))
        self.engine.prune_old_generations_and_gc(
            self.engine.Store(state['state_dir']), profile=root / 'profile',
            current_system=paths['current_system'], run=online['runner'],
            gcroots_dir=state['gcroots_dir'], trigger_path=state['trigger_path'])
        self.assertFalse(any(cmd[1:2] == ['prune'] for cmd in commands))
        applied = self.engine.execute_offline_boot_transaction(
            profile=root / 'profile', runner=online['runner'], **paths, **state)
        self.assertEqual(applied['status'], 'completed')
        return staged, commands, asked[len(before_staging):], closure

    def test_staging_imports_the_firmware_and_writes_the_new_generations_set_before_boot(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-stage-') as tmp:
            staged, commands, late, closure = self._stage_firmware_update(Path(tmp), target_missing=('unlisted',))
            self.assertEqual(staged['firmware_missing'], ['unlisted'])
            self.assertEqual(late, [])
            firmware = closure['firmware']['rtw88']['store_path']
            imported = next(i for i, cmd in enumerate(commands) if cmd[0] == 'nix' and firmware in cmd)
            ensured = commands.index([str(self.engine.FIRMWARE_TOOL), 'ensure', closure['toplevel']])
            boot = commands.index([closure['toplevel'] + '/bin/switch-to-configuration', 'boot'])
            self.assertLess(imported, ensured)
            self.assertLess(ensured, boot)

    def test_staging_fetches_firmware_detected_only_by_the_target_before_boot(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-target-') as tmp:
            staged, commands, late, closure = self._stage_firmware_update(Path(tmp), target_missing=('ath9k_htc',))
            self.assertEqual(staged['firmware_missing'], [])
            package = closure['firmware']['ath9k-htc']
            self.assertCountEqual(late, [f['path'] for f in package['files']])
            imported = next(i for i, cmd in enumerate(commands) if cmd[0] == 'nix' and package['store_path'] in cmd)
            ensures = [i for i, cmd in enumerate(commands)
                       if cmd == [str(self.engine.FIRMWARE_TOOL), 'ensure', closure['toplevel']]]
            boot = commands.index([closure['toplevel'] + '/bin/switch-to-configuration', 'boot'])
            self.assertEqual(len(ensures), 2)
            self.assertLess(ensures[0], imported)
            self.assertLess(imported, ensures[1])
            self.assertLess(ensures[1], boot)

    def test_staging_refuses_target_firmware_with_a_bad_signed_hash(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-bad-hash-') as tmp:
            self._stage_firmware_update(Path(tmp), target_missing=('ath9k_htc',), tamper=True)

    def test_old_generation_folders_are_pruned_after_generations_are_deleted(self):
        with tempfile.TemporaryDirectory(prefix='wros-firmware-prune-') as tmp:
            root = Path(tmp)
            _, commands, _, closure = self._stage_firmware_update(root, target_missing=())
            profile = root / 'profile'

            def run(cmd):
                commands.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, '', '')

            self.engine.prune_old_generations_and_gc(
                self.engine.Store(root / 'state'), profile=profile, gcroots_dir=root / 'gcroots',
                current_system=root / 'current-system', trigger_path=root / 'system-update', run=run)
            set_profile = ['nix-env', '-p', str(profile), '--set', closure['toplevel']]
            delete = ['nix-env', '-p', str(profile), '--delete-generations', '+2']
            prune = [str(self.engine.FIRMWARE_TOOL), 'prune', '--profile', str(profile)]
            self.assertLess(commands.index(set_profile), commands.index(delete))
            self.assertLess(commands.index(delete), commands.index(prune))
            self.assertLess(commands.index(prune), commands.index(['nix-collect-garbage']))

    # --- Thin bundles (rc23, #169) ---

    def _thin_fetch(self, root, *, served=None, fail_with=None):
        """fetch_online against a thin bundle: the channel from root/bundle, cache.nixos.org's
        files from root/upstream, through a mirror base named in the system configuration."""
        bundle, keyring, signers, moment = self._make_signed_bundle(
            root, version='0.1.0~rc18', upstream=True,
            toplevel='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc18')
        (root / 'version').write_text('0.1.0~rc16\n', encoding='utf-8')
        (root / 'store').mkdir()
        mirror_file = root / 'update-upstream-url'
        mirror_file.write_text('http://mirror.test/\n', encoding='utf-8')
        asked = []
        online = self.engine.ONLINE_URL

        class Response(io.BytesIO):
            status = 200

        class Opener:
            def open(self, url, timeout=None):
                asked.append(url)
                if url.startswith('http://mirror.test/'):
                    if fail_with is not None:
                        raise fail_with
                    rel = url.removeprefix('http://mirror.test/')
                    data = (served or {}).get(rel) or (root / 'upstream' / rel).read_bytes()
                    return Response(data)
                return Response((bundle / url.removeprefix(online)).read_bytes())

        def get_metadata(url, _limit, _opener, **_kwargs):
            return (bundle / url.removeprefix(self.engine.ONLINE_URL)).read_bytes()

        cache = root / 'cache'
        cache.mkdir()
        with mock.patch.object(self.engine, 'http_get', side_effect=get_metadata), \
                mock.patch.object(self.engine, 'RETRY_PAUSE', 0):
            try:
                result = self.engine.fetch_online(
                    cache=cache, keyring=keyring, signers=signers,
                    now=moment + timedelta(minutes=1), version_file=root / 'version',
                    current_system=root / 'current-system', store_dir=root / 'store',
                    opener=Opener(), upstream_file=mirror_file)
            finally:
                staged = [p.name for p in cache.iterdir() if p.name != 'progress.json']
        return result, asked, staged

    def test_fetch_downloads_upstream_files_and_imports_like_hosted_ones(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-thin-') as tmp:
            result, asked, _ = self._thin_fetch(Path(tmp))
            self.assertEqual(result['status'], 'fetched')
            mirrored = [u for u in asked if u.startswith('http://mirror.test/')]
            self.assertEqual(sorted(mirrored), [
                'http://mirror.test/abcdfghijklmnpqrsvwxyz0123456789.narinfo',
                'http://mirror.test/nar/' + 'q' * 52 + '.nar.xz'])
            fetched = Path(result['path'])
            self.assertTrue((fetched / 'cache/abcdfghijklmnpqrsvwxyz0123456789.narinfo').is_file())
            self.assertTrue((fetched / 'cache/nar' / ('q' * 52 + '.nar.xz')).is_file())

    def test_a_tampered_upstream_file_is_refused(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-thin-') as tmp:
            bad = {'nar/' + 'q' * 52 + '.nar.xz': b'upstream-nar-paylo4d'}
            with self.assertRaisesRegex(ValueError, 'SHA-256'):
                self._thin_fetch(Path(tmp), served=bad)
            self._kept_partial(Path(tmp))

    def test_fetch_reports_an_unreachable_upstream_and_stages_nothing(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-thin-') as tmp:
            with self.assertRaisesRegex(ValueError, "^Couldn't reach cache.nixos.org for .*mpv"):
                self._thin_fetch(Path(tmp), fail_with=urllib.error.URLError('timed out'))
            self._kept_partial(Path(tmp))

    def test_a_missing_upstream_file_is_named_and_stages_nothing(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-thin-') as tmp:
            gone = urllib.error.HTTPError('http://mirror.test/x', 404, 'Not Found', {}, None)
            with self.assertRaisesRegex(ValueError, '^cache.nixos.org no longer has .*mpv-0.41.0'):
                self._thin_fetch(Path(tmp), fail_with=gone)
            self._kept_partial(Path(tmp))

    def _kept_partial(self, root):
        digest = hashlib.sha256((root / 'bundle/InRelease').read_bytes()).hexdigest()[:16]
        partial = root / 'cache' / ('.partial-' + digest)
        left = [p for p in (root / 'cache').iterdir() if p.name != self.engine.FETCH_LOCK_NAME]
        self.assertEqual(left, [partial], 'nothing was staged')
        self.assertTrue(partial.is_dir())
        return partial

    def test_an_offline_thin_bundle_is_refused_with_a_reason(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-thin-') as tmp:
            root = Path(tmp)
            bundle, keyring, signers, moment = self._make_signed_bundle(
                root, version='0.1.0~rc18', upstream=True,
                toplevel='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc18')
            (root / 'store').mkdir()
            args = dict(keyring=keyring, signers=signers, now=moment + timedelta(minutes=1),
                        version_file=Path('/nonexistent'), current_system=Path('/nonexistent'),
                        store_dir=root / 'store')
            with self.assertRaisesRegex(ValueError, '^This update needs the internet'):
                self.engine.verify_bundle(bundle, **args)
            # The Pages channel of a thin bundle is checked without them.
            self.assertTrue(self.engine.verify_bundle(bundle, allow_upstream_missing=True, **args)['verified'])

    def test_upstream_urls_must_be_cache_nixos_org(self):
        good = 'https://cache.nixos.org/nar/' + 'q' * 52 + '.nar.xz'
        self.assertTrue(self.engine.UPSTREAM_URL_RE.fullmatch(good))
        for bad in ('https://evil.example/nar/' + 'q' * 52 + '.nar.xz',
                    'http://cache.nixos.org/nar/' + 'q' * 52 + '.nar.xz',
                    'https://cache.nixos.org/nar/../' + 'q' * 49 + '.nar.xz',
                    'https://cache.nixos.org/nar/' + 'Q' * 52 + '.nar.xz',
                    'https://cache.nixos.org/' + 'e' * 32 + '.narinfo',
                    'https://cache.nixos.org.evil/' + 'a' * 32 + '.narinfo'):
            self.assertIsNone(self.engine.UPSTREAM_URL_RE.fullmatch(bad), bad)
        with tempfile.TemporaryDirectory() as tmp:
            conf = Path(tmp) / 'update-upstream-url'
            self.assertEqual(self.engine.upstream_base(conf), 'https://cache.nixos.org/')
            conf.write_text('http://10.0.2.2:8080/cache/\n', encoding='utf-8')
            self.assertEqual(self.engine.upstream_base(conf), 'http://10.0.2.2:8080/cache/')
            conf.write_text('file:///etc/shadow\n', encoding='utf-8')
            with self.assertRaises(ValueError):
                self.engine.upstream_base(conf)

    def test_check_online_requires_baseline_only_for_a_newer_delta(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-check-') as tmp:
            root = Path(tmp)
            delta = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-nixos-system-writing-room-0.1.0-rc17'
            for name, baseline in (('delta', delta), ('full', None)):
                work = root / name
                work.mkdir()
                _, paths, serve = self._online_bundle(work, baseline)
                with self.subTest(bundle=name), serve:
                    result = self.engine.check_online(**paths)
                    self.assertEqual(result['version'], '0.1.0~rc18')
                    if baseline is None:
                        self.assertEqual(result['status'], 'available')
                        continue
                    self.assertEqual(result['status'], 'needs-earlier-update')
                    self.assertEqual(result['needs'], '0.1.0~rc17')

                    baseline_dir = paths['store_dir'] / Path(baseline).name
                    baseline_dir.mkdir()
                    self.assertEqual(self.engine.check_online(**paths)['status'], 'available')
                    baseline_dir.rmdir()
                    for installed in ('0.1.0~rc18', '0.1.0~rc19'):
                        paths['version_file'].write_text(installed + '\n', encoding='utf-8')
                        result = self.engine.check_online(**paths)
                        self.assertEqual(result['status'], 'current')
                        self.assertNotIn('needs', result)

    def test_missing_delta_baseline_names_version_before_downloading_archives(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-fetch-') as tmp:
            root = Path(tmp)
            baseline = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-nixos-system-wros-0.1.0-rc17'
            bundle, paths, serve = self._online_bundle(root, baseline)
            message = r'^This update needs WrOS 0\.1\.0~rc17 first$'
            with self.assertRaisesRegex(ValueError, message):
                self.engine.verify_bundle(bundle, **paths)
            with serve, mock.patch.object(self.engine, 'http_stream_to_file') as download:
                with self.assertRaisesRegex(ValueError, message):
                    self.engine.fetch_online(cache=root / 'cache', **paths)
                download.assert_not_called()

            fallback = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-custom_system'
            self.assertEqual(self.engine.missing_baseline({'baseline': fallback}, paths['store_dir']),
                             Path(fallback).name)

    def test_fetch_online_reports_download_progress_and_clears_it(self):
        """Settings shows how far the download got from cache/progress.json (#167)."""
        import itertools
        with tempfile.TemporaryDirectory(prefix='wros-nixos-progress-') as tmp:
            root = Path(tmp)
            bundle, paths, serve = self._online_bundle(root, None)
            cache = root / 'cache'
            cache.mkdir()
            (cache / 'progress.json').write_text('{"step": "download", "done": 9}', encoding='utf-8')
            seen = []
            stale = []
            check_baseline = self.engine.require_baseline

            def baseline(*args, **kwargs):
                stale.append((cache / 'progress.json').exists())
                return check_baseline(*args, **kwargs)

            def download(url, dest, size, sha, opener=None, timeout=120, on_chunk=None):
                seen.append(json.loads((cache / 'progress.json').read_text(encoding='utf-8')))
                data = (bundle / url.removeprefix(self.engine.ONLINE_URL)).read_bytes()
                Path(dest).parent.mkdir(parents=True, exist_ok=True)
                Path(dest).write_bytes(data)
                on_chunk(len(data))

            clock = itertools.count(0, 1.0)
            with serve, mock.patch.object(self.engine, 'http_stream_to_file', side_effect=download), \
                    mock.patch.object(self.engine.time, 'monotonic', side_effect=lambda: next(clock)), \
                    mock.patch.object(self.engine, 'require_baseline', side_effect=baseline), \
                    mock.patch.object(self.engine, 'verify_bundle',
                                      side_effect=lambda *a, **k: seen.append(json.loads(
                                          (cache / 'progress.json').read_text(encoding='utf-8')))):
                result = self.engine.fetch_online(cache=cache, **paths)
            self.assertEqual(result['status'], 'fetched')
            self.assertEqual(stale, [False], 'a stale file from a killed fetch is cleared first')
            self.assertEqual(seen[0], {'step': 'download', 'version': '0.1.0~rc18',
                                       'done': 0, 'total': seen[0]['total']})
            self.assertGreater(seen[0]['total'], 0)
            self.assertEqual(seen[-1]['step'], 'verify')
            self.assertEqual(seen[-1]['done'], seen[-1]['total'])
            self.assertFalse((cache / 'progress.json').exists(), 'cleared when done')
            self.assertEqual([p.name for p in cache.iterdir() if p.name.startswith('.progress')], [])

    def _serve_downloads(self, files, *, short=None, ignore_range=False):
        asked = []
        short = {} if short is None else short

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                wanted = self.headers.get('Range')
                asked.append((self.path, wanted))
                if self.path not in files:
                    self.send_error(404)
                    return
                body = files[self.path]
                start = 0
                if wanted and not ignore_range:
                    start = int(wanted.removeprefix('bytes=').removesuffix('-'))
                self.send_response(206 if start else 200)
                if start:
                    self.send_header('Content-Range', f'bytes {start}-{len(body) - 1}/{len(body)}')
                self.send_header('Content-Length', str(len(body) - start))
                self.end_headers()
                cuts = short.get(self.path)
                end = cuts.pop(0) if cuts else len(body)
                self.wfile.write(body[start:end])
                self.close_connection = True

            def log_message(self, *args):
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        self.enterContext(mock.patch.object(self.engine, 'RETRY_PAUSE', 0))
        return f'http://127.0.0.1:{server.server_address[1]}', asked

    def _download_cut_short(self, *, ignore_range):
        body = b'the whole archive, every byte of it'
        url, asked = self._serve_downloads({'/archive': body}, short={'/archive': [7]},
                                           ignore_range=ignore_range)
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / 'archive'
            chunks = []
            self.engine.http_stream_to_file(url + '/archive', dest, len(body),
                                            hashlib.sha256(body).hexdigest(), on_chunk=chunks.append)
            self.assertEqual(dest.read_bytes(), body)
            self.assertEqual(list(Path(tmp).iterdir()), [dest])
            self.assertEqual(sum(chunks), len(body))
        self.assertEqual(asked, [('/archive', None), ('/archive', 'bytes=7-')])

    def test_a_download_cut_short_resumes_with_a_range_request(self):
        self._download_cut_short(ignore_range=False)

    def test_a_server_that_ignores_range_starts_the_file_again(self):
        self._download_cut_short(ignore_range=True)

    def test_a_download_always_cut_short_fails_after_four_tries(self):
        body = b'the whole archive, every byte of it'
        url, asked = self._serve_downloads({'/archive': body}, short={'/archive': [7] * 4})
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / 'archive'
            with self.assertRaisesRegex(ValueError, 'SHA-256 or size check.*files are kept'):
                self.engine.http_stream_to_file(url + '/archive', dest, len(body),
                                                hashlib.sha256(body).hexdigest())
            self.assertFalse(dest.exists())
            self.assertEqual((Path(tmp) / 'archive.part').read_bytes(), body[:7])
        self.assertEqual(asked, [('/archive', None)] + [('/archive', 'bytes=7-')] * 3)

    def test_a_tls_connection_closed_early_is_retried(self):
        import ssl
        body = b'the whole archive, every byte of it'
        url, asked = self._serve_downloads({'/archive': body})
        download = self.engine._stream_download
        calls = []

        def drop_first(*args):
            calls.append(args[2])
            if len(calls) == 1:
                raise ssl.SSLEOFError(8, 'EOF occurred in violation of protocol')
            return download(*args)

        with tempfile.TemporaryDirectory() as tmp,                 mock.patch.object(self.engine, '_stream_download', side_effect=drop_first):
            dest = Path(tmp) / 'archive'
            self.engine.http_stream_to_file(url + '/archive', dest, len(body),
                                            hashlib.sha256(body).hexdigest())
            self.assertEqual(dest.read_bytes(), body)
        self.assertEqual(len(calls), 2)

    def test_a_second_fetch_while_one_runs_is_refused(self):
        import fcntl
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths, files, _ = self._signed_download_channel(root)
            url, asked = self._serve_downloads(files)
            cache = root / 'cache'
            cache.mkdir()
            (cache / 'progress.json').write_text('{"step": "download"}')
            with open(cache / self.engine.FETCH_LOCK_NAME, 'w') as held:
                fcntl.flock(held, fcntl.LOCK_EX)
                with self.assertRaisesRegex(ValueError, 'already running'):
                    self.engine.fetch_online(**{**paths, 'base_url': url + '/', 'cache': cache})
            self.assertEqual(asked, [], 'nothing was downloaded')
            self.assertTrue((cache / 'progress.json').exists(), "the running fetch's progress stays")

    def test_a_404_is_not_retried(self):
        url, asked = self._serve_downloads({})
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(urllib.error.HTTPError) as caught:
                self.engine.http_stream_to_file(url + '/archive', Path(tmp) / 'archive', 1, '0' * 64)
            self.assertEqual(caught.exception.code, 404)
        self.assertEqual(asked, [('/archive', None)])

    def _signed_download_channel(self, root):
        bundle, paths, _ = self._online_bundle(root, None)
        manifest = json.loads((bundle / 'closure-manifest.json').read_bytes())
        archives = ['/' + item['path'] for item in manifest['cache_files']]
        files = {'/' + p.relative_to(bundle).as_posix(): p.read_bytes()
                 for p in bundle.rglob('*') if p.is_file()}
        return paths, files, archives

    def _fetch_twice(self, *, change_kept_file):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths, files, archives = self._signed_download_channel(root)
            archive = archives[2]
            url, asked = self._serve_downloads(files, short={archive: [7] * 4})
            cache = root / 'cache'
            paths.update(base_url=url + '/', cache=cache)
            with self.assertRaisesRegex(ValueError, 'files are kept'):
                self.engine.fetch_online(**paths)
            partial = self._kept_partial(root)
            for name in archives[:2]:
                self.assertEqual((partial / name[1:]).read_bytes(), files[name])
            self.assertEqual((partial / (archive[1:] + '.part')).read_bytes(), files[archive][:7])
            if change_kept_file:
                kept = partial / archives[0][1:]
                data = kept.read_bytes()
                kept.write_bytes(bytes([data[0] ^ 1]) + data[1:])

            progress = []
            download = self.engine.http_stream_to_file

            def read_progress(*args, **kwargs):
                progress.append(json.loads((cache / 'progress.json').read_bytes()))
                return download(*args, **kwargs)

            asked.clear()
            with mock.patch.object(self.engine, 'require_free_space', wraps=self.engine.require_free_space) as space, \
                    mock.patch.object(self.engine, 'http_stream_to_file', side_effect=read_progress):
                result = self.engine.fetch_online(**paths)
            remaining = len(files[archive]) - 7
            needed = ['/InRelease', '/closure-manifest.json', archive]
            if change_kept_file:
                remaining += len(files[archives[0]])
                needed.append(archives[0])
            self.assertEqual(Counter(path for path, _ in asked), Counter(needed))
            self.assertIn((archive, 'bytes=7-'), asked)
            space.assert_called_once_with(cache, remaining)
            total = sum(len(files[path]) for path in archives)
            self.assertEqual((progress[0]['done'], progress[0]['total']), (total - remaining, total))
            self.assertFalse(partial.exists())
            for name in archives:
                self.assertEqual((Path(result['path']) / name[1:]).read_bytes(), files[name])

    def test_a_second_fetch_reuses_files_already_verified(self):
        self._fetch_twice(change_kept_file=False)

    def test_a_kept_file_that_was_changed_is_fetched_again(self):
        self._fetch_twice(change_kept_file=True)

    def test_a_partial_folder_for_another_release_is_removed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths, files, archives = self._signed_download_channel(root)
            url, _ = self._serve_downloads(files, short={archives[2]: [7] * 4})
            stale = root / 'cache/.partial-0000000000000000'
            stale.mkdir(parents=True)
            (stale / 'archive.part').write_bytes(b'incomplete')
            with self.assertRaisesRegex(ValueError, 'files are kept'):
                self.engine.fetch_online(base_url=url + '/', cache=stale.parent, **paths)
            self.assertFalse(stale.exists())
            self._kept_partial(root)

    def test_the_partial_folder_must_not_be_a_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths, files, _ = self._signed_download_channel(root)
            url, asked = self._serve_downloads(files)
            digest = hashlib.sha256(files['/InRelease']).hexdigest()[:16]
            cache = root / 'cache'
            cache.mkdir()
            elsewhere = root / 'elsewhere'
            elsewhere.mkdir()
            partial = cache / ('.partial-' + digest)
            partial.symlink_to(elsewhere, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'root-owned directory'):
                self.engine.fetch_online(base_url=url + '/', cache=cache, **paths)
            self.assertTrue(partial.is_symlink())
            self.assertEqual(list(elsewhere.iterdir()), [])
            self.assertEqual(asked, [('/InRelease', None), ('/closure-manifest.json', None)])

    def test_ci_accepts_a_signed_delta_without_a_nix_store(self):
        """The Pages workflow checks a release's delta bundle on a runner with no Nix store;
        the release it updates from only has to exist on the machines being updated."""
        import tarfile
        publish = load_nixos_publisher()
        with tempfile.TemporaryDirectory(prefix='wros-nixos-publish-') as tmp:
            root = Path(tmp)
            baseline = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-nixos-system-unnamed-0.1.0-rc14'
            now = datetime.now(timezone.utc) - timedelta(minutes=2)
            for name, tamper, orphan in (('good', False, False), ('bad', True, False),
                                         ('orphan', False, True)):
                work = root / name
                work.mkdir()
                bundle, keyring, signers, _ = self._make_signed_bundle(
                    work, version='0.1.0~rc15', moment=now, baseline=baseline, tamper_nar=tamper,
                    orphan_narinfo=orphan)
                tar = work / 'wros-nixos-updates.tar'
                with tarfile.open(tar, 'w') as archive:
                    for item in sorted(bundle.rglob('*')):
                        archive.add(item, arcname=item.relative_to(bundle).as_posix(), recursive=False)
                check = lambda: publish.check_bundle(tar, work / 'site/nixos-updates', '0.1.0~rc15',
                                                     keyring=keyring, pin=signers)
                if tamper:
                    with self.assertRaisesRegex(ValueError, 'SHA-256'):
                        check()
                elif orphan:
                    with self.assertRaisesRegex(ValueError, 'archive the bundle lacks'):
                        check()
                else:
                    self.assertEqual(check()['version'], '0.1.0~rc15')
                    self.assertTrue((work / 'site/nixos-updates/InRelease').is_file())

    def test_ci_counts_whole_days_until_channel_expiry(self):
        publish = load_nixos_publisher()
        now = datetime(2026, 11, 19, 12, tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory() as tmp:
            inrelease = Path(tmp) / 'InRelease'
            for remaining, expected in ((timedelta(days=10), 10),
                                        (timedelta(days=10, seconds=-1), 9),
                                        (timedelta(seconds=-1), -1)):
                with self.subTest(remaining=remaining):
                    until = now + remaining
                    header = format_datetime(until, usegmt=True).replace('GMT', 'UTC')
                    inrelease.write_text(
                        '-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nOrigin: WrOS\n'
                        f'Valid-Until: {header}\n-----BEGIN PGP SIGNATURE-----\n',
                        encoding='utf-8')
                    self.assertEqual(publish.channel_expiry(inrelease, now=now),
                                     (expected, until.date().isoformat()))

    def test_ci_rejects_missing_or_malformed_channel_expiry(self):
        publish = load_nixos_publisher()
        with tempfile.TemporaryDirectory() as tmp:
            inrelease = Path(tmp) / 'InRelease'
            for header, error in (('', 'missing'), ('Valid-Until: not a date\n', 'malformed')):
                with self.subTest(error=error):
                    inrelease.write_text(f'Origin: WrOS\n{header}', encoding='utf-8')
                    with self.assertRaisesRegex(ValueError, f'{error} Valid-Until'):
                        publish.channel_expiry(inrelease)

    def test_downgrade_and_equal_version_refused_on_fresh_install(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-ver-') as tmp:
            root = Path(tmp)
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            cur_sys = root / 'current-system'
            store_dir = root / 'nix/store'
            store_dir.mkdir(parents=True)

            # Equal version refused
            eq_root = root / 'eq'
            eq_root.mkdir()
            b_eq, kr, sig, moment = self._make_signed_bundle(eq_root, version='0.1.0~rc14')
            with self.assertRaisesRegex(ValueError, 'already installed'):
                self.engine.verify_bundle(
                    b_eq, keyring=kr, signers=sig, now=moment + timedelta(minutes=1),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir)

            # Older version (downgrade) refused on fresh install without replay.json
            old_root = root / 'old'
            old_root.mkdir()
            b_old, kr2, sig2, moment2 = self._make_signed_bundle(old_root, version='0.1.0~rc13')
            with self.assertRaisesRegex(ValueError, 'downgrade from 0.1.0~rc14 to 0.1.0~rc13 refused'):
                self.engine.verify_bundle(
                    b_old, keyring=kr2, signers=sig2, now=moment2 + timedelta(minutes=1),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir)

    def test_nar_hash_checked_before_import_and_stage_apply_rollback_gc(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-upd-') as tmp:
            root = Path(tmp)
            bad_root = root / 'bad'
            bad_root.mkdir()
            bad_bundle, bad_kr, bad_sig, moment = self._make_signed_bundle(bad_root, tamper_nar=True)
            with self.assertRaisesRegex(ValueError, 'failed SHA-256 or size check'):
                self.engine.verify_bundle(
                    bad_bundle, keyring=bad_kr, signers=bad_sig, now=moment + timedelta(minutes=1),
                    version_file=root / 'nonexistent')

            # Valid bundle v1 -> stage -> apply -> v2 -> stage -> apply -> healthy GC -> rollback to v1
            v1_root = root / 'v1'
            v1_root.mkdir()
            top1 = '/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13'
            b1, kr, sig, t1 = self._make_signed_bundle(
                v1_root, version='0.1.0~rc13', toplevel=top1,
                moment=datetime(2026, 11, 14, 10, 0, tzinfo=timezone.utc))

            state_dir = root / 'state'
            trigger = root / 'system-update'
            store_dir = root / 'nix/store'
            store_dir.mkdir(parents=True)
            gcroots = root / 'gcroots'
            profile = root / 'profile'
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc12\n', encoding='utf-8')
            cur_sys = root / 'current-system'

            rev1 = self.engine.review_bundle(
                b1, keyring=kr, signers=sig, state_dir=state_dir,
                now=t1 + timedelta(minutes=1), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir)
            self.assertEqual(rev1['status'], 'reviewed')
            self.assertEqual(rev1['packages'][0]['to_version'], '0.1.0~rc13')

            staged1 = self.engine.stage_update(
                b1, rev1['review_id'], keyring=kr, signers=sig, state_dir=state_dir,
                trigger_path=trigger, now=t1 + timedelta(minutes=2),
                allowed_prefixes=(str(root),), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots)
            self.assertEqual(staged1['status'], 'staged')
            self.assertEqual(os.readlink(gcroots / 'staged'), top1)

            commands = []
            def fake_runner(cmd):
                commands.append(cmd)
                if len(cmd) >= 4 and cmd[0] == 'nix' and 'copy' in cmd:
                    # The engine copies the staged paths, not a closure: --no-recursive.
                    self.assertIn('--no-recursive', cmd)
                    for arg in cmd:
                        if arg.startswith('/nix/store/'):
                            (store_dir / Path(arg).name).mkdir(parents=True, exist_ok=True)
                return subprocess.CompletedProcess(cmd, 0, b'', b'')

            applied1 = self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, keyring=kr, signers=sig,
                store_dir=store_dir, profile=profile, current_system=cur_sys,
                gcroots_dir=gcroots, runner=fake_runner, now=t1 + timedelta(minutes=5))
            self.assertEqual(applied1['status'], 'completed')
            # Verify staged/cache was removed before keeping installed/ so no NAR duplicates remain (U4)
            self.assertFalse((state_dir / 'installed/cache').exists())
            ver_file.write_text('0.1.0~rc13\n', encoding='utf-8')

            # Stage and apply v2 (0.1.0~rc14)
            v2_root = root / 'v2'
            v2_root.mkdir()
            top2 = '/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14'
            b2, kr2, sig2, t2 = self._make_signed_bundle(
                v2_root, version='0.1.0~rc14', toplevel=top2,
                moment=datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc))
            rev2 = self.engine.review_bundle(
                b2, keyring=kr2, signers=sig2, state_dir=state_dir,
                now=t2 + timedelta(minutes=1), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir)
            self.engine.stage_update(
                b2, rev2['review_id'], keyring=kr2, signers=sig2, state_dir=state_dir,
                trigger_path=trigger, now=t2 + timedelta(minutes=2),
                allowed_prefixes=(str(root),), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots)
            self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, keyring=kr2, signers=sig2,
                store_dir=store_dir, profile=profile, current_system=cur_sys,
                gcroots_dir=gcroots, runner=fake_runner, now=t2 + timedelta(minutes=5))
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            self.assertEqual(os.readlink(gcroots / 'previous'), top1)
            self.assertFalse((state_dir / 'previous/cache').exists())

            # Stage rollback to v1 (signed with kr/sig)
            rb = self.engine.stage_rollback(
                keyring=kr, signers=sig, state_dir=state_dir, trigger_path=trigger,
                now=t2 + timedelta(minutes=10), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots, profile=profile)
            self.assertTrue(rb['rollback'])
            self.assertEqual(rb['toplevel'], top1)

    def test_going_back_uses_previous_system_generation(self):
        """NixOS keeps the previous generation, so even the first update can go back, by
        nix-env --switch-generation, with no kept update tree."""
        with tempfile.TemporaryDirectory(prefix='wros-nixos-gen-') as tmp:
            root = Path(tmp)
            store_dir = root / 'nix/store'
            top1 = '/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13'
            top2 = '/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14'
            for top, ver in ((top1, '0.1.0~rc13'), (top2, '0.1.0~rc14')):
                etc = store_dir / Path(top).name / 'etc/wros'
                etc.mkdir(parents=True)
                (etc / 'version').write_text(ver + '\n', encoding='utf-8')
            profiles = root / 'profiles'
            profiles.mkdir()
            os.symlink(top1, profiles / 'system-1-link')
            os.symlink(top2, profiles / 'system-2-link')
            os.symlink('system-2-link', profiles / 'system')
            profile = profiles / 'system'
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            state_dir, trigger = root / 'state', root / 'system-update'
            gcroots, cur_sys = root / 'gcroots', root / 'current-system'
            moment = datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc)

            rb = self.engine.stage_rollback(
                state_dir=state_dir, trigger_path=trigger, now=moment, version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots, profile=profile)
            self.assertEqual((rb['status'], rb['rollback'], rb['toplevel']), ('staged', True, top1))
            self.assertTrue(trigger.is_symlink())

            commands = []

            def fake_runner(cmd):
                commands.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, '', '')

            applied = self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, store_dir=store_dir, profile=profile,
                current_system=cur_sys, gcroots_dir=gcroots, runner=fake_runner,
                now=moment + timedelta(minutes=5))
            self.assertEqual(applied['status'], 'completed')
            self.assertIn(['nix-env', '-p', str(profile), '--switch-generation', '1'], commands)
            self.assertFalse(any('--set' in cmd for cmd in commands))
            self.assertFalse(any(cmd[0] == 'nix' for cmd in commands))  # nothing to import

            # Back on generation 1 there is nothing older: going back again is refused.
            os.unlink(profile)
            os.symlink('system-1-link', profile)
            ver_file.write_text('0.1.0~rc13\n', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'no previous version'):
                self.engine.stage_rollback(
                    state_dir=state_dir, trigger_path=trigger, now=moment + timedelta(minutes=10),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir,
                    gcroots_dir=gcroots, profile=profile)

    def test_same_version_generation_is_not_a_previous_version_and_repair_adds_none(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-gen2-') as tmp:
            root = Path(tmp)
            store_dir = root / 'nix/store'
            tops = {
                1: ('/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13', '0.1.0~rc13'),
                2: ('/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14', '0.1.0~rc14'),
                3: ('/nix/store/2123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14', '0.1.0~rc14'),
            }
            profiles = root / 'profiles'
            profiles.mkdir()
            for number, (top, ver) in tops.items():
                etc = store_dir / Path(top).name / 'etc/wros'
                etc.mkdir(parents=True)
                (etc / 'version').write_text(ver + '\n', encoding='utf-8')
                os.symlink(top, profiles / f'system-{number}-link')
            os.symlink('system-3-link', profiles / 'system')
            profile = profiles / 'system'
            # Generation 2 carries the running version; going back skips it to generation 1.
            self.assertEqual(self.engine.previous_generation(profile, store_dir=store_dir),
                             (1, tops[1][0], '0.1.0~rc13'))

            commands = []

            def fake_runner(cmd):
                commands.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, '', '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=fake_runner)
            self.assertEqual(res['status'], 'recovered', res)
            self.assertIn([tops[3][0] + '/bin/switch-to-configuration', 'boot'], commands)
            self.assertFalse(any(cmd[:1] == ['nix-env'] for cmd in commands), commands)

            # The contents checked are the closure of what will boot, not the whole store: a path
            # only an older generation keeps must not fail the repair.
            commands.clear()
            closure = [tops[3][0], '/nix/store/3123456789abcdfghijklmnpqrsvwxyz-wros-helpers']

            def closure_runner(cmd):
                commands.append(cmd)
                out = '\n'.join(closure) + '\n' if cmd[:3] == ['nix-store', '--query', '--requisites'] else ''
                return subprocess.CompletedProcess(cmd, 0, out, '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=closure_runner)
            self.assertEqual(res['status'], 'recovered', res)
            self.assertIn(['nix-store', '--verify-path', *closure], commands)
            self.assertNotIn(['nix-store', '--verify', '--check-contents'], commands)

            def failing_runner(cmd):
                failed = cmd[:2] == ['nix-store', '--verify-path']
                return subprocess.CompletedProcess(cmd, 1 if failed else 0, '',
                                                   "path '/nix/store/x' was modified!" if failed else '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=failing_runner)
            self.assertEqual(res['status'], 'failed', res)
            self.assertIn('was modified', res['message'])


if __name__ == '__main__':
    unittest.main()
