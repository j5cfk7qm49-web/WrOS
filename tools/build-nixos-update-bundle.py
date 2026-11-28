#!/usr/bin/env python3
"""Build a signed WrOS NixOS offline/online binary-cache update bundle.

Takes an unsigned closure directory (produced by nix build .#wros-update-bundle or
tools/collect-nixos-update-runtime.py) containing closure-manifest.json and cache/
(nix-cache-info, *.narinfo, nar/*.nar.zst), optionally prunes paths present in
--baseline-store-paths for a delta bundle (U5), signs Release into InRelease and
Release.gpg using the GnuPG key whose primary fingerprint equals --expect-fingerprint,
and proves the finished bundle with nixos/scripts/wros-update-engine.py.
"""
import argparse
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parents[1]
GPG = 'gpg'
ENV = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C'}
FINGERPRINT_RE = re.compile(r'^[0-9A-F]{40}$')
FIRMWARE_FILE_LIMIT = 95_000_000


class BuildError(Exception):
    pass


def narinfo_url(path):
    """The cache-relative archive (cache/nar/....nar.zst) a .narinfo points at."""
    for line in Path(path).read_text(encoding='utf-8').splitlines():
        if line.startswith('URL: '):
            return 'cache/' + line[len('URL: '):].strip()
    raise BuildError(f'{Path(path).name} names no archive')


def archive_credits(narinfos):
    """A shared archive must be fetched even if one of its store paths is present."""
    credits = {}
    for archive, store_path in narinfos:
        credits[archive] = None if archive in credits else store_path
    return credits


def credit_archives(cache_files, credits):
    return [
        {**item, 'store_path': credits[item['path']]}
        if item['path'].startswith('cache/nar/') and item['path'] in credits else item
        for item in cache_files
    ]


def prune_to_delta(staging, cache_files, baseline, toplevel, *, keep_store_paths=()):
    """Keep what a machine running `baseline` lacks. Keep an archive whenever a kept .narinfo
    points at it, with a store-path credit only if exactly one kept .narinfo needs it."""
    staging = Path(staging)

    def dropped(item):
        sp = item.get('store_path')
        return bool(sp) and sp in baseline and sp != toplevel and sp not in keep_store_paths

    needed = archive_credits((narinfo_url(staging / item['path']), item.get('store_path'))
                             for item in cache_files
                             if item['path'].endswith('.narinfo') and not dropped(item))
    kept = []
    for item in cache_files:
        if item['path'].startswith('cache/nar/'):
            if item['path'] in needed:
                kept.append(item)
                continue
        elif not dropped(item):
            kept.append(item)
            continue
        (staging / item['path']).unlink(missing_ok=True)
    return credit_archives(kept, needed)


UPSTREAM = 'https://cache.nixos.org/'
NIX32 = '0123456789abcdfghijklmnpqrsvwxyz'
NAR_REL_RE = re.compile(r'^nar/[0-9a-df-np-sv-z]{52}\.nar(?:\.xz|\.zst)?$')


def nix32_decode(text):
    """Nix's base-32 (as in NarHash: sha256:…) to bytes."""
    size = len(text) * 5 // 8
    out = bytearray(size)
    for n, char in enumerate(reversed(text)):
        digit = NIX32.index(char)
        bit = n * 5
        i, j = bit // 8, bit % 8
        out[i] |= (digit << j) & 0xff
        if i + 1 < size:
            out[i + 1] |= digit >> (8 - j)
    return bytes(out)


def narinfo_fields(text):
    return dict(line.split(': ', 1) for line in text.splitlines() if ': ' in line)


def store_narinfo_path(store_path):
    return f'cache/{Path(store_path).name.split("-", 1)[0]}.narinfo'


def check_firmware_size(path, size):
    if size >= FIRMWARE_FILE_LIMIT:
        raise BuildError(f'firmware file {path} must be under 95 MB (got {size} bytes)')


def cache_file_entry(staging, rel, store_path):
    path = staging / rel
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        size = os.fstat(stream.fileno()).st_size
    return {'path': rel, 'sha256': digest, 'size': size, 'store_path': store_path}


def collect_cache_manifest(staging, toplevel, version, system_paths, firmware_paths,
                           *, baseline=(), baseline_toplevel=None):
    """Index the cache from nix copy. Firmware already in the system closure stays in cache_files
    so older engines still import it; its firmware entry needs no additional files."""
    staging = Path(staging)
    narinfos = {}
    for path in sorted((staging / 'cache').glob('*.narinfo')):
        fields = narinfo_fields(path.read_text(encoding='utf-8'))
        url = fields.get('URL', '')
        if re.fullmatch(r'nar/[A-Za-z0-9._-]+\.nar(?:\.zst|\.xz)?', url) is None:
            raise BuildError(f'{path.name} names an invalid archive')
        narinfos[path.relative_to(staging).as_posix()] = (fields['StorePath'], 'cache/' + url)
    credits = archive_credits((archive, sp) for sp, archive in narinfos.values())
    firmware = {}
    extra_files = set()
    for driver, store_path in sorted(firmware_paths.items()):
        name = driver.replace('_', '-')
        if name in firmware:
            raise BuildError(f'more than one firmware package is named {name}')
        narinfo = store_narinfo_path(store_path)
        if narinfo not in narinfos or narinfos[narinfo][0] != store_path:
            raise BuildError(f'the cache has no .narinfo for firmware package {name}')
        _, archive = narinfos[narinfo]
        for rel in (narinfo, archive):
            check_firmware_size(rel, (staging / rel).stat().st_size)
        files = []
        if store_path not in system_paths:
            if credits[archive] is None:
                raise BuildError(f'firmware package {name} shares archive {archive} with another store path')
            files = [cache_file_entry(staging, rel, store_path) for rel in (narinfo, archive)]
            extra_files.update((narinfo, archive))
        firmware[name] = {'store_path': store_path, 'files': files}

    cache_files = []
    for path in sorted((staging / 'cache').rglob('*')):
        rel = path.relative_to(staging).as_posix()
        if path.is_file() and rel not in extra_files:
            sp = narinfos[rel][0] if rel in narinfos else credits.get(rel)
            cache_files.append(cache_file_entry(staging, rel, sp))
    if baseline:
        cache_files = prune_to_delta(staging, cache_files, baseline, toplevel,
                                     keep_store_paths=firmware_paths.values())
    return {
        'schema': 2,
        'format': 'wros-nixos-cache-v2',
        'version': version,
        'system': 'x86_64-linux',
        'toplevel': toplevel,
        'baseline': baseline_toplevel,
        'app_only': False,
        'cache_files': cache_files,
        'firmware': firmware,
    }


FETCH_TRIES = 4
RETRY_PAUSE = 5  # seconds, doubled after each failed try


def fetch(url, limit):
    """GET a cache file, or None if the cache doesn't have it.

    A connection that drops mid-download makes urllib return a short read with no error, so
    the length is checked against Content-Length and a download cut short is tried again."""
    import http.client
    import time
    import urllib.error
    import urllib.request
    pause = RETRY_PAUSE
    for attempt in range(1, FETCH_TRIES + 1):
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                expected = response.headers.get('Content-Length')
                data = response.read(limit + 1)
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            raise BuildError(f'{url}: HTTP {exc.code}') from None
        except urllib.error.URLError as exc:
            if isinstance(exc.reason, FileNotFoundError):  # a file:// copy without it
                return None
            problem = str(exc.reason)
        except (OSError, http.client.HTTPException) as exc:
            problem = str(exc) or type(exc).__name__
        else:
            if len(data) > limit:
                raise BuildError(f'{url} is larger than {limit} bytes')
            if expected is None or len(data) == int(expected):
                return data
            problem = f'download cut short: {len(data)} of {expected} bytes'
        if attempt == FETCH_TRIES:
            raise BuildError(f'{url}: {problem} (tried {FETCH_TRIES} times)')
        time.sleep(pause)
        pause *= 2
    raise AssertionError('unreachable')


def decompress(name, data):
    if name.endswith('.xz'):
        import lzma
        return lzma.decompress(data)
    if name.endswith('.zst'):
        res = subprocess.run(['zstd', '-dc'], input=data, capture_output=True, check=False)
        if res.returncode != 0:
            raise BuildError(f'could not decompress {name}')
        return res.stdout
    return data


def thin_out(staging, cache_files, upstream, toplevel, keep, *, keep_store_paths=()):
    """Leave out of the bundle every package cache.nixos.org already has byte for byte (#169): list
    its .narinfo and archive under their cache.nixos.org address instead, with the SHA-256 and size
    of cache.nixos.org's own files, so the laptop fetches them from there and checks them against
    this signed manifest. A package whose NAR hash differs (WrOS built or patched it) stays. The
    fetched files are written to `keep` for the bundle's self-check, not to the bundle."""
    staging = Path(staging)
    narinfo_items = [i for i in cache_files if i['path'].endswith('.narinfo')]
    local_urls = {i['path']: narinfo_url(staging / i['path']) for i in narinfo_items}
    final_urls = local_urls.copy()
    moved = {}
    # Byte-identical packages share one archive on cache.nixos.org: fetch it once.
    archives = {}
    for item in narinfo_items:
        store_path = item.get('store_path')
        if not store_path or store_path == toplevel or store_path in keep_store_paths:
            continue
        local = narinfo_fields((staging / item['path']).read_text(encoding='utf-8'))
        name = Path(item['path']).name
        remote_bytes = fetch(upstream + name, 64 * 1024)
        if remote_bytes is None:
            continue
        remote = narinfo_fields(remote_bytes.decode('utf-8'))
        nar_rel = remote.get('URL', '')
        nar_hash = remote.get('NarHash')
        if not nar_hash or not NAR_REL_RE.fullmatch(nar_rel):
            continue
        verified = archives.get(nar_rel)
        if verified is not None and verified[0] != nar_hash:
            raise BuildError(f"{name} declares a different NAR hash for cache.nixos.org's {nar_rel}")
        if nar_hash != local.get('NarHash'):
            continue
        if verified is not None:
            _, archive = verified
        else:
            archive = fetch(upstream + nar_rel, 1536 * 1024 * 1024)
            if archive is None:
                continue
            algo, _, digest = nar_hash.partition(':')
            if algo != 'sha256' or hashlib.sha256(decompress(nar_rel, archive)).digest() != nix32_decode(digest):
                raise BuildError(f'cache.nixos.org\'s archive for {name} does not unpack to its NAR hash')
            archives[nar_rel] = (nar_hash, archive)
        moved[item['path']] = (store_path, name, remote_bytes, nar_rel, archive)
        final_urls[item['path']] = 'cache/' + nar_rel
    credits = archive_credits((final_urls[i['path']], i.get('store_path')) for i in narinfo_items)
    if not moved:
        return credit_archives(cache_files, credits), False
    # Local archives still needed by a package that stays.
    still_needed = {url for path, url in local_urls.items() if path not in moved}
    replaced = {local_urls[path] for path in moved} - still_needed
    kept = []
    for item in cache_files:
        if item['path'] in moved:
            continue
        if item['path'].startswith('cache/nar/') and item['path'] in replaced:
            (staging / item['path']).unlink(missing_ok=True)
            continue
        kept.append(item)
    # A shared archive is listed once, even if several .narinfo files name it.
    listed = {item['path'] for item in kept}
    for rel, (store_path, name, remote_bytes, nar_rel, archive) in moved.items():
        (staging / rel).unlink(missing_ok=True)
        for path, data, url in ((rel, remote_bytes, UPSTREAM + name),
                                ('cache/' + nar_rel, archive, UPSTREAM + nar_rel)):
            if path in listed:
                continue
            listed.add(path)
            target = Path(keep) / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            kept.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest(),
                         'size': len(data), 'store_path': store_path, 'url': url})
    return credit_archives(kept, credits), True


def check_archives(root, cache_files):
    """Every .narinfo in the bundle has its archive in the bundle (or both come from cache.nixos.org)."""
    listed = {item['path'] for item in cache_files}
    for item in cache_files:
        if (item.get('url') or '').startswith(UPSTREAM):
            continue
        if item['path'].endswith('.narinfo'):
            url = narinfo_url(Path(root) / item['path'])
            if url not in listed or not (Path(root) / url).is_file():
                raise BuildError(f'{item["path"]} points at {url}, which the bundle lacks')


def check_firmware_files(staging, closure):
    """Verify the firmware files the engine will only download in a later update step."""
    system_files = {item['path']: item for item in closure['cache_files']}
    for name, package in closure['firmware'].items():
        files = package['files']
        listed = {item['path']: item for item in files} if files else system_files
        narinfo = store_narinfo_path(package['store_path'])
        if narinfo not in listed or listed[narinfo]['url'] is not None:
            raise BuildError(f'firmware package {name} lacks its local .narinfo')
        info = narinfo_fields((staging / narinfo).read_text(encoding='utf-8'))
        archive = 'cache/' + info.get('URL', '')
        if info.get('StorePath') != package['store_path'] or archive not in listed:
            raise BuildError(f'firmware package {name} lacks its archive or has the wrong store path')
        for item in files or (listed[narinfo], listed[archive]):
            rel = item['path']
            if item['url'] is not None:
                raise BuildError(f'firmware file {rel} must be hosted in the bundle')
            check_firmware_size(rel, (staging / rel).stat().st_size)
            if files:
                actual = cache_file_entry(staging, rel, item['store_path'])
                if actual['sha256'] != item['sha256'] or actual['size'] != item['size']:
                    raise BuildError(f'firmware file {rel} does not match its manifest hash or size')


def load_engine():
    path = REPO / 'nixos/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_nixos_update_engine_for_tool', str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def run(cmd, **kwargs):
    kwargs.setdefault('env', ENV)
    kwargs.setdefault('stdin', subprocess.DEVNULL)
    kwargs.setdefault('stderr', subprocess.PIPE)
    kwargs.setdefault('stdout', subprocess.PIPE)
    result = subprocess.run(cmd, **kwargs)
    if result.returncode != 0:
        detail = (result.stderr or b'').decode('utf-8', 'replace').strip().splitlines()[-1:] or ['no detail']
        raise BuildError(f'{Path(cmd[0]).name} failed: {detail[0]}')
    return result


def normalize_fingerprint(value):
    cleaned = re.sub(r'\s+', '', value).upper()
    if FINGERPRINT_RE.fullmatch(cleaned) is None:
        raise BuildError('--expect-fingerprint must be a 40-hex primary key fingerprint')
    return cleaned


def primary_fingerprint(gnupg_home, key_id):
    listing = run([GPG, '--homedir', str(gnupg_home), '--batch', '--with-colons', '--fingerprint',
                   '--list-secret-keys', '--', key_id]).stdout.decode('utf-8', 'replace')
    primaries = []
    take_next = False
    for line in listing.splitlines():
        fields = line.split(':')
        if fields[0] == 'sec':
            take_next = True
            if len(fields) > 11 and fields[1] in ('e', 'r', 'd', 'i'):
                raise BuildError('the signing key is expired, revoked or disabled')
        elif fields[0] == 'ssb':
            take_next = False
        elif fields[0] == 'fpr' and take_next:
            primaries.append(fields[9].upper())
            take_next = False
    if len(primaries) != 1:
        raise BuildError(f'--key-id must match exactly one secret key (found {len(primaries)})')
    return primaries[0]


def release_moment(args):
    if args.date:
        moment = datetime.fromisoformat(args.date)
        if moment.tzinfo is None:
            moment = moment.replace(tzinfo=timezone.utc)
    elif os.environ.get('SOURCE_DATE_EPOCH', '').isdigit():
        moment = datetime.fromtimestamp(int(os.environ['SOURCE_DATE_EPOCH']), timezone.utc)
    else:
        moment = datetime.now(timezone.utc)
    return moment.astimezone(timezone.utc).replace(microsecond=0)


def rfc2822(moment):
    return format_datetime(moment, usegmt=True).replace('GMT', 'UTC')


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('closure_dir', help='unsigned closure directory containing closure-manifest.json and cache/')
    parser.add_argument('--out', required=True, help='new output directory (must not exist or be empty)')
    parser.add_argument('--gnupg-home', required=True, help='GNUPGHOME holding the secret signing key (outside the repo)')
    parser.add_argument('--key-id', required=True, help='key id, fingerprint or user id of the signing key')
    parser.add_argument('--expect-fingerprint', required=True, help='40-hex PRIMARY fingerprint that must own --key-id')
    parser.add_argument('--baseline-toplevel', help='optional baseline system toplevel store path for delta bundle')
    parser.add_argument('--baseline-store-paths', type=Path, help='optional file listing store paths in baseline')
    parser.add_argument('--suite', default='wros-nixos-updates')
    parser.add_argument('--origin', default='WrOS')
    parser.add_argument('--date', help='Release Date as ISO 8601 UTC')
    parser.add_argument('--valid-days', type=int, default=14)
    parser.add_argument('--use-primary', action='store_true')
    parser.add_argument('--thin', action='store_true',
                        help='leave out packages cache.nixos.org has; laptops fetch them from there (#169)')
    parser.add_argument('--upstream-url', default=UPSTREAM,
                        help='where to look them up (tests: a file:// copy); the manifest always names cache.nixos.org')
    return parser.parse_args(argv)


def prepare_manifest(staging, args, upstream_files):
    manifest = json.loads((staging / 'closure-manifest.json').read_text(encoding='utf-8'))
    firmware_paths = {package['store_path'] for package in manifest.get('firmware', {}).values()}
    if args.baseline_store_paths is not None and args.baseline_toplevel is not None:
        baseline = {line.strip() for line in args.baseline_store_paths.read_text(encoding='utf-8').splitlines()
                    if line.strip()}
        manifest['cache_files'] = prune_to_delta(staging, manifest['cache_files'], baseline, manifest['toplevel'],
                                                keep_store_paths=firmware_paths)
        manifest['baseline'] = args.baseline_toplevel
    if args.thin:
        manifest['cache_files'], manifest['thin'] = thin_out(
            staging, manifest['cache_files'], args.upstream_url, manifest['toplevel'],
            upstream_files, keep_store_paths=firmware_paths)
    check_archives(staging, manifest['cache_files'])
    return manifest


def build(args):
    engine = load_engine()
    if not 1 <= args.valid_days <= 30:
        raise BuildError('--valid-days must be between 1 and 30')
    gnupg_home = Path(args.gnupg_home).resolve()
    if not gnupg_home.is_dir() or gnupg_home == REPO or REPO in gnupg_home.parents:
        raise BuildError('invalid or in-repo --gnupg-home')
    expected = normalize_fingerprint(args.expect_fingerprint)
    actual = primary_fingerprint(gnupg_home, args.key_id)
    if actual != expected:
        raise BuildError(f'refusing to sign: key {args.key_id} has primary fingerprint {actual}, not {expected}')
    src = Path(args.closure_dir).resolve()
    out = Path(args.out).absolute()
    if out.is_symlink() or (out.exists() and (not out.is_dir() or any(out.iterdir()))):
        raise BuildError('--out must not exist or must be an empty directory')
    moment = release_moment(args)
    until = moment + timedelta(days=args.valid_days)
    out.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='.wros-nixos-bundle-', dir=out.parent))
    upstream_files = None
    try:
        shutil.copytree(src, staging, dirs_exist_ok=True)
        manifest_path = staging / 'closure-manifest.json'
        # Nix bundle outputs are read-only. Make the copied files and directories we edit writable.
        for path in (staging, staging / 'cache', staging / 'cache/nar', manifest_path):
            if path.exists():
                path.chmod(path.stat().st_mode | stat.S_IWUSR)
        # Validate paths and firmware metadata before pruning or thinning any files.
        engine.parse_closure_manifest(manifest_path.read_bytes())
        upstream_files = Path(tempfile.mkdtemp(prefix='wros-nixos-upstream-'))
        manifest_data = prepare_manifest(staging, args, upstream_files)
        if args.thin or (args.baseline_store_paths is not None and args.baseline_toplevel is not None):
            manifest_path.write_text(json.dumps(manifest_data, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        manifest_bytes = manifest_path.read_bytes()
        closure = engine.parse_closure_manifest(manifest_bytes)
        check_firmware_files(staging, closure)
        release = (
            f'Origin: {args.origin}\nLabel: WrOS NixOS updates\nSuite: {args.suite}\nCodename: nixos-26.05\n'
            f'Date: {rfc2822(moment)}\nValid-Until: {rfc2822(until)}\nArchitectures: amd64\n'
            f'SHA256:\n {hashlib.sha256(manifest_bytes).hexdigest()} {len(manifest_bytes)} closure-manifest.json\n'
        )
        (staging / 'Release').write_text(release, encoding='utf-8')
        signer = actual + '!' if args.use_primary else actual
        signing = [GPG, '--homedir', str(gnupg_home), '--batch', '--yes', '--local-user', signer]
        run([*signing, '--clearsign', '--output', str(staging / 'InRelease'), str(staging / 'Release')])
        run([*signing, '--detach-sign', '--output', str(staging / 'Release.gpg'), str(staging / 'Release')])

        with tempfile.TemporaryDirectory(prefix='wros-nixos-selfcheck-') as check:
            keyring, signers = Path(check) / 'keyring.gpg', Path(check) / 'update-signers'
            run([GPG, '--homedir', str(gnupg_home), '--batch', '--output', str(keyring), '--export', actual])
            signers.write_text(actual + '\n', encoding='utf-8')
            fake_store = Path(check) / 'store'
            fake_store.mkdir()
            if closure.get('baseline'):
                (fake_store / Path(closure['baseline']).name).mkdir()
            engine.TRUSTED_UID = os.geteuid()
            engine._base.TRUSTED_UID = os.geteuid()
            # Check the cache a laptop fetches for the system update, including upstream files.
            # The optional firmware files were checked separately before signing.
            optional_firmware = {staging / item['path'] for package in closure['firmware'].values()
                                 for item in package['files']}
            whole = Path(check) / 'whole'
            shutil.copytree(staging, whole, ignore=lambda directory, names: [
                name for name in names if Path(directory) / name in optional_firmware
            ])
            shutil.copytree(upstream_files, whole, dirs_exist_ok=True)
            verified = engine.verify_bundle(
                whole, keyring=keyring, signers=signers, suites=(args.suite,),
                now=moment + timedelta(minutes=1), version_file=Path('/nonexistent'),
                current_system=Path('/nonexistent'), store_dir=fake_store)
        if out.exists():
            out.rmdir()
        staging.rename(out)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        if upstream_files is not None:
            shutil.rmtree(upstream_files, ignore_errors=True)
    return {
        'out': str(out),
        'signer': actual,
        'suite': args.suite,
        'date': moment.isoformat(),
        'valid_until': until.isoformat(),
        'version': closure['version'],
        'toplevel': closure['toplevel'],
        'baseline': closure.get('baseline'),
        'cache_files': len(closure['cache_files']),
        'review_id': verified['review_id'],
    }


def main(argv=None):
    try:
        summary = build(parse_args(sys.argv[1:] if argv is None else argv))
    except (BuildError, ValueError, OSError) as exc:
        print(f'build-nixos-update-bundle: {exc}', file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
