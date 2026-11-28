"""tools/build-nixos-update-bundle.py: cutting a full bundle down to a delta.

rc15 first went out with an archive missing: sudo.pam (new in rc15) and passwd.pam (already in
rc14) have identical contents, so they share one .nar.zst, which the manifest credited to
passwd.pam; the delta dropped it, and the laptop's update failed at the restart that installs it.
"""
import hashlib
import importlib.util
import json
import lzma
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('bundle_tool', ROOT / 'tools/build-nixos-update-bundle.py')
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

TOP = '/nix/store/' + 'a' * 32 + '-nixos-system-unnamed-0.1.0-rc15'
OLD = '/nix/store/' + 'b' * 32 + '-passwd.pam'
NEW = '/nix/store/' + 'c' * 32 + '-sudo.pam'
GONE = '/nix/store/' + 'd' * 32 + '-in-the-baseline'


def bundle(root):
    """A full bundle: four store paths, two of them (OLD, NEW) sharing one archive."""
    cache = Path(root) / 'cache'
    (cache / 'nar').mkdir(parents=True)
    files = [{'path': 'cache/nix-cache-info', 'store_path': None}]
    (cache / 'nix-cache-info').write_text('StoreDir: /nix/store\n')
    for path, nar in ((TOP, 'top'), (OLD, 'shared'), (NEW, 'shared'), (GONE, 'gone')):
        name = Path(path).name.split('-', 1)[0] + '.narinfo'
        (cache / name).write_text(f'StorePath: {path}\nURL: nar/{nar}.nar.zst\n')
        files.append({'path': f'cache/{name}', 'store_path': path})
        if not (cache / 'nar' / f'{nar}.nar.zst').exists():
            (cache / 'nar' / f'{nar}.nar.zst').write_bytes(nar.encode())
            # The manifest credits a shared archive to the first path that names it.
            files.append({'path': f'cache/nar/{nar}.nar.zst', 'store_path': path})
    return files


class Delta(unittest.TestCase):
    def test_a_shared_archive_stays_while_a_kept_package_needs_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            kept = tool.prune_to_delta(tmp, files, {OLD, GONE}, TOP)
            paths = {item['path']: item['store_path'] for item in kept}
            self.assertIn('cache/nar/shared.nar.zst', paths)
            # Credited to the package the machine lacks, so it is downloaded when needed.
            self.assertEqual(paths['cache/nar/shared.nar.zst'], NEW)
            self.assertNotIn('cache/nar/gone.nar.zst', paths)
            self.assertFalse((Path(tmp) / 'cache/nar/gone.nar.zst').exists())
            self.assertNotIn(f'cache/{"b" * 32}.narinfo', paths)
            tool.check_archives(tmp, kept)

    def test_a_delta_archive_shared_by_two_kept_packages_has_no_store_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            kept = tool.prune_to_delta(tmp, files, {GONE}, TOP)
            paths = {item['path']: item['store_path'] for item in kept}
            self.assertEqual(paths[f'cache/{"b" * 32}.narinfo'], OLD)
            self.assertEqual(paths[f'cache/{"c" * 32}.narinfo'], NEW)
            self.assertIsNone(paths['cache/nar/shared.nar.zst'])
            tool.check_archives(tmp, kept)

    def test_a_bundle_missing_an_archive_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            (Path(tmp) / 'cache/nar/shared.nar.zst').unlink()
            broken = [item for item in files if item['path'] != 'cache/nar/shared.nar.zst']
            with self.assertRaisesRegex(tool.BuildError, 'lacks'):
                tool.check_archives(tmp, broken)

    def test_the_old_rule_would_have_dropped_it(self):
        # What rc15's first bundle did: prune by each file's own credit.
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            old = [item for item in files
                   if not (item['store_path'] in {OLD, GONE} and item['store_path'] != TOP)]
            with self.assertRaises(tool.BuildError):
                tool.check_archives(tmp, old)


def nix32(data):
    """Nix's base-32, as in NarHash (the reverse of tool.nix32_decode)."""
    out = []
    length = (len(data) * 8 - 1) // 5 + 1
    for n in range(length - 1, -1, -1):
        b = n * 5
        i, j = b // 8, b % 8
        c = (data[i] >> j) | ((data[i + 1] << (8 - j)) if i + 1 < len(data) else 0)
        out.append(tool.NIX32[c & 0x1f])
    return ''.join(out)


class Thin(unittest.TestCase):
    """--thin (rc23, #169): packages cache.nixos.org has byte for byte are listed, not shipped."""

    def setUp(self):
        import hashlib
        import lzma
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.staging, self.cache, self.keep = root / 'bundle', root / 'cache.nixos.org', root / 'keep'
        (self.staging / 'cache/nar').mkdir(parents=True)
        (self.cache / 'nar').mkdir(parents=True)
        self.files = [{'path': 'cache/nix-cache-info', 'store_path': None}]
        (self.staging / 'cache/nix-cache-info').write_text('StoreDir: /nix/store\n')
        self.paths = {
            'top': '/nix/store/' + 'a' * 32 + '-nixos-system-unnamed-0.1.0-rc23',
            'mpv': '/nix/store/' + 'b' * 32 + '-mpv-0.41.0',          # on the cache, same NAR
            'grimoire': '/nix/store/' + 'c' * 32 + '-grimoire-0.1.0',  # WrOS's own
            'patched': '/nix/store/' + 'd' * 32 + '-greetd-0.10.3',    # on the cache, different NAR
        }
        for key, path in self.paths.items():
            nar = f'{key} nar'.encode()
            nar_hash = 'sha256:' + nix32(hashlib.sha256(nar).digest())
            name = Path(path).name.split('-', 1)[0]
            (self.staging / f'cache/{name}.narinfo').write_text(
                f'StorePath: {path}\nURL: nar/{key}.nar.zst\nNarHash: {nar_hash}\n')
            (self.staging / f'cache/nar/{key}.nar.zst').write_bytes(nar)
            self.files += [{'path': f'cache/{name}.narinfo', 'store_path': path},
                           {'path': f'cache/nar/{key}.nar.zst', 'store_path': path}]
            if key in ('mpv', 'patched'):
                served = nar if key == 'mpv' else b'their build'
                remote_hash = 'sha256:' + nix32(hashlib.sha256(served).digest())
                archive = 'nar/' + ('q' if key == 'mpv' else 'r') * 52 + '.nar.xz'
                (self.cache / archive).write_bytes(lzma.compress(served))
                (self.cache / f'{name}.narinfo').write_text(
                    f'StorePath: {path}\nURL: {archive}\nCompression: xz\nNarHash: {remote_hash}\n')

    def tearDown(self):
        self.tmp.cleanup()

    def thin(self):
        return tool.thin_out(self.staging, self.files, self.cache.as_uri() + '/', self.paths['top'],
                             self.keep)

    def test_nix32_round_trips(self):
        import hashlib
        digest = hashlib.sha256(b'x').digest()
        self.assertEqual(tool.nix32_decode(nix32(digest)), digest)

    def test_thin_bundle_lists_upstream_files_with_their_cache_address(self):
        kept, thin = self.thin()
        self.assertTrue(thin)
        by_path = {item['path']: item for item in kept}
        mpv = 'b' * 32 + '.narinfo'
        self.assertEqual(by_path[f'cache/{mpv}']['url'], 'https://cache.nixos.org/' + mpv)
        nar = 'nar/' + 'q' * 52 + '.nar.xz'
        self.assertEqual(by_path['cache/' + nar]['url'], 'https://cache.nixos.org/' + nar)
        self.assertEqual(by_path['cache/' + nar]['store_path'], self.paths['mpv'])
        self.assertEqual(by_path['cache/' + nar]['sha256'],
                         __import__('hashlib').sha256((self.cache / nar).read_bytes()).hexdigest())
        self.assertNotIn('cache/nar/mpv.nar.zst', by_path, "WrOS's copy of mpv is left out")
        self.assertFalse((self.staging / 'cache/nar/mpv.nar.zst').exists())
        self.assertFalse((self.staging / f'cache/{mpv}').exists(), 'the laptop fetches it')
        self.assertTrue((self.keep / 'cache' / nar).is_file(), 'kept for the self-check')
        tool.check_archives(self.staging, kept)

    def test_a_differing_nar_hash_stays_in_the_bundle(self):
        kept, _ = self.thin()
        by_path = {item['path']: item for item in kept}
        for key in ('grimoire', 'patched', 'top'):
            self.assertIn(f'cache/nar/{key}.nar.zst', by_path, key)
            self.assertFalse(by_path[f'cache/nar/{key}.nar.zst'].get('url'), key)

    def add_mpv_twin(self, nar=b'mpv nar', *, cached_nar=b'mpv nar'):
        import hashlib
        twin = '/nix/store/' + 'e' * 32 + '-mpv-twin'
        nar_hash = 'sha256:' + nix32(hashlib.sha256(nar).digest())
        name = Path(twin).name.split('-', 1)[0]
        (self.staging / f'cache/{name}.narinfo').write_text(
            f'StorePath: {twin}\nURL: nar/mpv.nar.zst\nNarHash: {nar_hash}\n')
        self.files.append({'path': f'cache/{name}.narinfo', 'store_path': twin})
        archive = 'nar/' + 'q' * 52 + '.nar.xz'
        cached_hash = 'sha256:' + nix32(hashlib.sha256(cached_nar).digest())
        (self.cache / f'{name}.narinfo').write_text(
            f'StorePath: {twin}\nURL: {archive}\nCompression: xz\nNarHash: {cached_hash}\n')
        return name, archive

    def test_two_packages_with_one_archive_on_the_cache_list_it_once(self):
        # rc32: two store paths with byte-identical contents share one cache.nixos.org archive;
        # listing it twice made the engine refuse the manifest (duplicate cache file).
        name, archive = self.add_mpv_twin()
        kept, thin = self.thin()
        self.assertTrue(thin)
        paths = [item['path'] for item in kept]
        self.assertEqual(paths.count('cache/' + archive), 1, paths)
        self.assertEqual(len(paths), len(set(paths)), paths)
        self.assertIn(f'cache/{name}.narinfo', paths)
        entries = {item['path']: item for item in kept}
        self.assertIsNone(entries['cache/' + archive]['store_path'])

    def test_reusing_an_archive_with_a_different_nar_hash_is_refused(self):
        name, _ = self.add_mpv_twin(b'a different nar', cached_nar=b'a different nar')
        with self.assertRaisesRegex(tool.BuildError, rf'{name}.*NAR hash'):
            self.thin()

    def test_a_conflicting_cached_nar_hash_is_refused_even_if_it_differs_from_the_local_hash(self):
        name, _ = self.add_mpv_twin(cached_nar=b'a different nar')
        with self.assertRaisesRegex(tool.BuildError, rf'{name}.*NAR hash'):
            self.thin()

    def test_thinning_one_package_leaves_the_local_archive_credited_to_its_twin(self):
        name, archive = self.add_mpv_twin()
        (self.cache / f'{name}.narinfo').unlink()
        kept, thin = self.thin()
        self.assertTrue(thin)
        paths = {item['path']: item['store_path'] for item in kept}
        self.assertEqual(paths['cache/nar/mpv.nar.zst'], paths[f'cache/{name}.narinfo'])
        self.assertEqual(paths['cache/' + archive], self.paths['mpv'])

    def test_nothing_on_the_cache_leaves_the_bundle_whole(self):
        for f in self.cache.glob('*.narinfo'):
            f.unlink()
        kept, thin = self.thin()
        self.assertFalse(thin)
        self.assertEqual(kept, self.files)


class Fetch(unittest.TestCase):
    """rc32: signing fetches hundreds of archives from cache.nixos.org, and a connection that
    drops mid-download made urllib hand back a short read without an error."""

    BODY = b'the whole archive, every byte of it'

    def serve(self, short_answers):
        import http.server
        import threading
        body, answered = self.BODY, []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                answered.append(self.path)
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                if len(answered) <= short_answers:
                    self.wfile.write(body[:7])  # then the connection closes
                    self.close_connection = True
                else:
                    self.wfile.write(body)

            def log_message(self, *args):
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        self.retry_pause, tool.RETRY_PAUSE = getattr(tool, 'RETRY_PAUSE', None), 0
        self.addCleanup(setattr, tool, 'RETRY_PAUSE', self.retry_pause)
        return f'http://127.0.0.1:{server.server_address[1]}/nar/x.nar.zst', answered

    def test_a_download_cut_short_is_fetched_again(self):
        url, answered = self.serve(short_answers=2)
        self.assertEqual(tool.fetch(url, 1024), self.BODY)
        self.assertEqual(len(answered), 3)

    def test_a_download_always_cut_short_is_refused(self):
        url, _ = self.serve(short_answers=99)
        with self.assertRaisesRegex(tool.BuildError, 'cut short'):
            tool.fetch(url, 1024)


class Firmware(unittest.TestCase):
    DRIVER = '/nix/store/' + 'g' * 32 + '-wros-firmware-ath9k_htc'
    SEED = '/nix/store/' + 'f' * 32 + '-wros-firmware-rtw88'
    PACKAGES = {'ath9k_htc': DRIVER, 'rtw88': SEED}

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.staging = self.root / 'unsigned'
        bundle(self.staging)
        self.driver_files = self.add_package(self.DRIVER, 'ath9k')
        self.seed_files = self.add_package(self.SEED, 'seed')

    def add_package(self, store_path, archive):
        narinfo = f'cache/{Path(store_path).name.split("-")[0]}.narinfo'
        nar = f'cache/nar/{archive}.nar.zst'
        (self.staging / narinfo).write_text(f'StorePath: {store_path}\nURL: nar/{archive}.nar.zst\n')
        (self.staging / nar).write_bytes(archive.encode())
        return {narinfo, nar}

    def collect(self, baseline=(), baseline_toplevel=None):
        return tool.collect_cache_manifest(
            self.staging, TOP, '0.1.0~rc33', {TOP, OLD, NEW, GONE, self.SEED},
            self.PACKAGES, baseline=baseline, baseline_toplevel=baseline_toplevel)

    def test_the_bundle_lists_every_firmware_package_with_its_files(self):
        manifest = self.collect()
        by_path = {entry['path']: entry for entry in manifest['cache_files']}
        self.assertTrue(self.seed_files <= by_path.keys())
        self.assertTrue(self.driver_files.isdisjoint(by_path))
        self.assertEqual(set(manifest['firmware']), {'ath9k-htc', 'rtw88'})
        self.assertEqual(manifest['firmware']['rtw88'], {'store_path': self.SEED, 'files': []})
        driver = manifest['firmware']['ath9k-htc']
        self.assertEqual(driver['store_path'], self.DRIVER)
        self.assertEqual({f['path'] for f in driver['files']}, self.driver_files)
        for entry in driver['files']:
            data = (self.staging / entry['path']).read_bytes()
            self.assertEqual(entry['sha256'], hashlib.sha256(data).hexdigest())
            self.assertEqual(entry['size'], len(data))
            self.assertEqual(entry['store_path'], self.DRIVER)
        closure = tool.load_engine().parse_closure_manifest(json.dumps(manifest).encode())
        tool.check_firmware_files(self.staging, closure)
        (self.staging / 'cache/nar/ath9k.nar.zst').write_bytes(b'wrong')
        with self.assertRaisesRegex(tool.BuildError, 'ath9k.*manifest hash'):
            tool.check_firmware_files(self.staging, closure)

    def test_firmware_files_survive_the_delta_and_the_thin_pass(self):
        normal_archive = self.staging / 'cache/nar/shared.nar.zst'
        nar = normal_archive.read_bytes()
        nar_hash = 'sha256:' + nix32(hashlib.sha256(nar).digest())
        narinfo = 'c' * 32 + '.narinfo'
        remote_nar = 'nar/' + 'q' * 52 + '.nar.xz'
        remote = {
            tool.UPSTREAM + narinfo: f'StorePath: {NEW}\nURL: {remote_nar}\nNarHash: {nar_hash}\n'.encode(),
            tool.UPSTREAM + remote_nar: lzma.compress(nar),
        }
        with (self.staging / 'cache' / narinfo).open('a') as stream:
            stream.write(f'NarHash: {nar_hash}\n')
        # Exercise pruning both when collecting the cache and when preparing to sign it.
        manifest = self.collect(baseline={self.DRIVER, self.SEED}, baseline_toplevel=OLD)
        self.assertTrue(all((self.staging / rel).is_file() for rel in self.driver_files | self.seed_files))
        baseline = self.root / 'baseline'
        baseline.write_text('\n'.join([OLD, GONE, self.DRIVER, self.SEED]))
        keys = ROOT / 'nixos/tests/keys'
        fingerprint = (keys / 'update-signers').read_text().strip()
        home = self.root / 'gnupg'
        home.mkdir(mode=0o700)
        subprocess.run(['gpg', '--batch', '--homedir', str(home), '--import',
                        str(keys / 'test-signer.sec.asc')], check=True, capture_output=True)
        output = self.root / 'output'
        args = tool.parse_args([
            str(self.staging), '--out', str(output), '--gnupg-home', str(home),
            '--key-id', fingerprint, '--expect-fingerprint', fingerprint, '--thin',
            '--baseline-toplevel', OLD, '--baseline-store-paths', str(baseline),
        ])
        manifest_path = self.staging / 'closure-manifest.json'
        manifest_path.write_text(json.dumps(manifest))
        manifest_path.chmod(0o444)
        for directory in (self.staging / 'cache/nar', self.staging / 'cache', self.staging):
            directory.chmod(0o555)
        with (mock.patch.object(tool, 'fetch', side_effect=lambda url, _limit: remote[url]) as fetch,
              mock.patch.dict(tool.ENV, PATH=os.environ['PATH'])):
            tool.build(args)
        self.assertEqual([call.args[0] for call in fetch.call_args_list], list(remote))
        self.assertFalse((output / 'cache/nar/gone.nar.zst').exists())
        self.assertFalse((output / 'cache/nar/shared.nar.zst').exists())
        self.assertTrue(all((output / rel).is_file() for rel in self.driver_files | self.seed_files))
        signed = json.loads((output / 'closure-manifest.json').read_text())
        self.assertTrue(signed['thin'])
        self.assertEqual(signed['firmware'], manifest['firmware'])
        self.assertTrue(self.seed_files <= {f['path'] for f in signed['cache_files']})

    def test_a_firmware_file_over_95_mb_stops_the_build(self):
        archive = self.staging / 'cache/nar/ath9k.nar.zst'
        with archive.open('r+b') as stream:
            stream.truncate(95_000_001)
        with self.assertRaisesRegex(tool.BuildError, 'ath9k.*95 MB'):
            self.collect()


if __name__ == '__main__':
    unittest.main()
