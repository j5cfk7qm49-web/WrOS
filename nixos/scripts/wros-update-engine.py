"""WrOS NixOS update engine: authenticated binary-cache bundles, crash-consistent staging, generation switch and rollback.

Imports the unchanged OpenPGP trust, append-only journal, probation, media mount, and
public status mirror from os/scripts/wros-update-engine.py, while replacing APT/dpkg
package solving with verified Nix binary-cache import (`nix --extra-experimental-features
nix-command copy --from file://<cache> --no-check-sigs <toplevel>`), system profile
generation switching, boot-loader installation, explicit GC roots for running, previous,
and staged closures, and generation pruning + garbage collection once a new version is
marked healthy.
"""
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request


def _load_debian_engine():
    here = Path(__file__).resolve().parent
    candidates = [
        here / 'debian_engine.py',
        here.parents[1] / 'os/scripts/wros-update-engine.py',
        Path('/usr/lib/wros-update/debian_engine.py'),
    ]
    for candidate in candidates:
        if candidate.is_file():
            loader = importlib.machinery.SourceFileLoader('wros_debian_update_engine', str(candidate))
            spec = importlib.util.spec_from_loader(loader.name, loader)
            module = importlib.util.module_from_spec(spec)
            loader.exec_module(module)
            return module
    raise RuntimeError('could not locate base WrOS update engine module')


_base = _load_debian_engine()

LIMIT = _base.LIMIT
ARCHIVE_LIMIT = 1536 * 1024 * 1024
DEFAULT_KEYRING = Path('/etc/wros/update-trust/wros-archive-keyring.gpg')
DEFAULT_SIGNERS = Path('/etc/wros/update-trust/update-signers')
PACKAGE_KEYRING = Path('/etc/wros/update-trust/wros-archive-keyring.gpg')
KEYRING_LIMIT = _base.KEYRING_LIMIT
RETAINED_KEYRING = _base.RETAINED_KEYRING
DEFAULT_STATE_DIR = _base.DEFAULT_STATE_DIR
DEFAULT_TRIGGER = _base.DEFAULT_TRIGGER
DEFAULT_MEDIA_ROOT = _base.DEFAULT_MEDIA_ROOT
MEDIA_LABEL = _base.MEDIA_LABEL
MEDIA_FILESYSTEMS = _base.MEDIA_FILESYSTEMS
DEBIAN_ONLINE_URL = _base.ONLINE_URL
ONLINE_URL = 'https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/'
ONLINE_CACHE = _base.ONLINE_CACHE
ONLINE_TIMEOUT = _base.ONLINE_TIMEOUT
ALLOWED_MEDIA_PREFIXES = _base.ALLOWED_MEDIA_PREFIXES
PINNED_ORIGIN = _base.PINNED_ORIGIN
PINNED_SUITES = ('wros-nixos-updates',)
ACCEPTED_ARCHITECTURES = ('x86_64-linux', 'amd64')
MAX_VALIDITY = timedelta(days=30)
MAX_STAGED_AGE = timedelta(days=30)
CLOCK_SKEW = timedelta(minutes=5)
BOOT_CLOCK_SKEW = timedelta(hours=1)
PROBATION_FAILED_LOGINS = 2
LAUNCHER_VERBS = ('stage', 'cancel', 'repair', 'media', 'fetch', 'apply', 'rollback', 'healthy')
TRUSTED_UID = _base.TRUSTED_UID
SAFE_PATH = '/usr/sbin:/usr/bin:/sbin:/bin'
MANIFEST_VERSION = 2
MIN_FREE_BYTES = 512 * 1024 * 1024
UNAVAILABLE = _base.UNAVAILABLE

DEFAULT_PROFILE = Path('/nix/var/nix/profiles/system')
DEFAULT_CURRENT_SYSTEM = Path('/run/current-system')
DEFAULT_GCROOTS_DIR = Path('/nix/var/nix/gcroots/wros')
DEFAULT_VERSION_FILE = Path('/etc/wros/version')
CLOSURE_MANIFEST_NAME = 'closure-manifest.json'
STORE_PATH_RE = re.compile(r'^/nix/store/[0-9a-df-np-sv-z]{32}-[A-Za-z0-9._+?=-]+$')
CACHE_REL_RE = re.compile(
    r'^cache/(?:nix-cache-info|[0-9a-df-np-sv-z]{32}\.narinfo|nar/[0-9a-df-np-sv-z0-9A-Za-z._-]+\.nar(?:\.zst|\.xz)?)$'
)
RELEASE_ASSET_URL_RE = re.compile(
    r'^https://github\.com/j5cfk7qm49-web/WrOS/releases/download/v[0-9A-Za-z.+~-]+/[A-Za-z0-9._-]+$'
)
HEX64_RE = _base.HEX64_RE
HEX32_RE = _base.HEX32_RE
PKEXEC_UID_RE = _base.PKEXEC_UID_RE

fail = _base.fail
StateBusy = _base.StateBusy
clock = _base.clock
clean_text = _base.clean_text
safe_env = _base.safe_env
fsync_dir = _base.fsync_dir
atomic_write = _base.atomic_write
json_bytes = _base.json_bytes
json_write = _base.json_write
regular_stream = _base.regular_stream
read_regular_bytes = _base.read_regular_bytes
sha256_file = _base.sha256_file
require_private_entry = _base.require_private_entry
require_trusted_file = _base.require_trusted_file
load_pinned_signers = _base.load_pinned_signers
keyring_primary_fingerprints = _base.keyring_primary_fingerprints
verify_signed_metadata = _base.verify_signed_metadata
parse_release_time = _base.parse_release_time
parse_release = _base.parse_release
parse_iso = _base.parse_iso
read_boot_id = _base.read_boot_id
Store = _base.Store
summarize = _base.summarize
read_public_status = _base.read_public_status
clear_partials = _base.clear_partials
remove_staged = _base.remove_staged
keep_installed = _base.keep_installed
keep_gone_back = _base.keep_gone_back
start_probation = _base.start_probation
end_probation = _base.end_probation
read_probation = _base.read_probation
finish_keeping = _base.finish_keeping
owned_trigger = _base.owned_trigger
unlink_trigger = _base.unlink_trigger
find_update_media = _base.find_update_media
parse_mountinfo = _base.parse_mountinfo
http_get = _base.http_get
tls_clock_failure = _base.tls_clock_failure
server_agrees = _base.server_agrees
CLOCK_FAILURES = _base.CLOCK_FAILURES
CLOCK_FLOOR = _base.CLOCK_FLOOR
continue_boot_after_failure = _base.continue_boot_after_failure
reserved_role = _base.reserved_role


def resolve_trust_anchor(path):
    """Resolve a store-backed /etc/wros/update-trust symlink to its immutable /nix/store regular file."""
    p = Path(path)
    try:
        if p.is_symlink():
            resolved = p.resolve(strict=True)
            if str(resolved).startswith('/nix/store/'):
                return resolved
    except OSError:
        pass
    return p


def _split_debian_version(version):
    if not isinstance(version, str) or not version or _base.VERSION_RE.fullmatch(version) is None:
        fail(f'invalid version string: {version!r}')
    epoch = 0
    rest = version
    if ':' in rest:
        epoch_str, rest = rest.split(':', 1)
        if not epoch_str.isdigit():
            fail(f'invalid version epoch: {version!r}')
        epoch = int(epoch_str)
    if '-' in rest:
        upstream, debian = rest.rsplit('-', 1)
    else:
        upstream, debian = rest, '0'
    return epoch, upstream, debian


def _char_order(ch):
    if ch == '~':
        return -1
    if ch == '':
        return 0
    if ch.isalpha():
        return ord(ch)
    return ord(ch) + 256


def _compare_part(left, right):
    i = 0
    j = 0
    n_left = len(left)
    n_right = len(right)
    while i < n_left or j < n_right:
        while (i < n_left and not left[i].isdigit()) or (j < n_right and not right[j].isdigit()):
            c_left = left[i] if (i < n_left and not left[i].isdigit()) else ''
            c_right = right[j] if (j < n_right and not right[j].isdigit()) else ''
            o_left = _char_order(c_left)
            o_right = _char_order(c_right)
            if o_left != o_right:
                return -1 if o_left < o_right else 1
            if c_left:
                i += 1
            if c_right:
                j += 1
        num_left = []
        while i < n_left and left[i].isdigit():
            num_left.append(left[i])
            i += 1
        num_right = []
        while j < n_right and right[j].isdigit():
            num_right.append(right[j])
            j += 1
        v_left = int(''.join(num_left) or '0')
        v_right = int(''.join(num_right) or '0')
        if v_left != v_right:
            return -1 if v_left < v_right else 1
    return 0


def compare_versions(left, relation, right):
    """Pure-Python Debian/WrOS tilde version ordering (0.1.0~rc13 < 0.1.0~rc14 < 0.1.0)."""
    e_left, u_left, d_left = _split_debian_version(left)
    e_right, u_right, d_right = _split_debian_version(right)
    if e_left != e_right:
        cmp_val = -1 if e_left < e_right else 1
    else:
        cmp_val = _compare_part(u_left, u_right)
        if cmp_val == 0:
            cmp_val = _compare_part(d_left, d_right)
    if relation == 'lt':
        return cmp_val < 0
    if relation == 'le':
        return cmp_val <= 0
    if relation == 'eq':
        return cmp_val == 0
    if relation == 'ge':
        return cmp_val >= 0
    if relation == 'gt':
        return cmp_val > 0
    fail(f'unsupported version relation: {relation}')


def current_installed_version(version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM):
    for candidate in (Path(version_file), Path(current_system) / 'etc/wros/version'):
        try:
            if candidate.exists():
                text = candidate.read_text(encoding='utf-8').splitlines()[0].strip()
                if text:
                    return text
        except OSError:
            pass
    return None


def current_toplevel(current_system=DEFAULT_CURRENT_SYSTEM):
    try:
        path = Path(current_system)
        if path.exists():
            return str(path.resolve(strict=True))
    except OSError:
        pass
    return None


def store_has_path(store_path, store_dir=Path('/nix/store')):
    if not isinstance(store_path, str) or STORE_PATH_RE.fullmatch(store_path) is None:
        return False
    return (Path(store_dir) / Path(store_path).name).exists()


def parse_closure_manifest(raw_bytes):
    try:
        manifest = json.loads(raw_bytes.decode('utf-8'))
    except ValueError:
        fail('closure-manifest.json is not valid UTF-8 JSON')
    if (not isinstance(manifest, dict)
            or manifest.get('schema') != 2
            or manifest.get('format') != 'wros-nixos-cache-v2'):
        fail('unsupported NixOS closure manifest schema or format (expected wros-nixos-cache-v2)')
    version = manifest.get('version')
    if not isinstance(version, str) or _base.VERSION_RE.fullmatch(version) is None:
        fail('invalid version in closure-manifest.json')
    system = manifest.get('system')
    if system not in ACCEPTED_ARCHITECTURES:
        fail(f'closure manifest system {system!r} is not supported on this x86_64 appliance')
    toplevel = manifest.get('toplevel')
    if not isinstance(toplevel, str) or STORE_PATH_RE.fullmatch(toplevel) is None:
        fail('closure manifest toplevel is not a valid /nix/store path')
    baseline = manifest.get('baseline')
    if baseline is not None and (not isinstance(baseline, str) or STORE_PATH_RE.fullmatch(baseline) is None):
        fail('invalid baseline store path in closure-manifest.json')
    cache_files = manifest.get('cache_files')
    if not isinstance(cache_files, list) or not cache_files or len(cache_files) > 32768:
        fail('closure manifest cache_files list is empty or exceeds bounds')
    seen_paths = set()
    cleaned_files = []
    has_cache_info = False
    has_toplevel_narinfo = False
    top_hash = Path(toplevel).name.split('-', 1)[0]
    for entry in cache_files:
        if not isinstance(entry, dict):
            fail('invalid cache_files entry in closure manifest')
        rel = entry.get('path')
        sha256 = entry.get('sha256')
        size = entry.get('size')
        sp = entry.get('store_path')
        url = entry.get('url')
        if not isinstance(rel, str) or CACHE_REL_RE.fullmatch(rel) is None:
            fail(f'invalid cache file path in closure manifest: {rel!r}')
        if not isinstance(sha256, str) or HEX64_RE.fullmatch(sha256) is None:
            fail(f'invalid sha256 for {rel}')
        if not isinstance(size, int) or size <= 0 or size > ARCHIVE_LIMIT:
            fail(f'invalid size for {rel}')
        if sp is not None and (not isinstance(sp, str) or STORE_PATH_RE.fullmatch(sp) is None):
            fail(f'invalid store_path for {rel}: {sp!r}')
        if url is not None and (not isinstance(url, str) or RELEASE_ASSET_URL_RE.fullmatch(url) is None):
            fail(f'invalid release asset url for {rel}')
        if rel in seen_paths:
            fail(f'duplicate cache file in closure manifest: {rel}')
        seen_paths.add(rel)
        if rel == 'cache/nix-cache-info':
            has_cache_info = True
        if rel == f'cache/{top_hash}.narinfo':
            has_toplevel_narinfo = True
        cleaned_files.append({
            'path': rel,
            'sha256': sha256,
            'size': size,
            'store_path': sp,
            'url': url,
        })
    if not has_cache_info or not has_toplevel_narinfo:
        fail('closure manifest must include cache/nix-cache-info and the toplevel .narinfo')
    return {
        'schema': 2,
        'format': 'wros-nixos-cache-v2',
        'version': version,
        'system': system,
        'toplevel': toplevel,
        'baseline': baseline,
        'app_only': bool(manifest.get('app_only', False)),
        'cache_files': cleaned_files,
    }


def verify_closure_archives(bundle_root, closure, *, store_dir=Path('/nix/store'), allow_skipped_present=False):
    """Verify every binary-cache file (.narinfo, .nar.zst, nix-cache-info) against the signed manifest BEFORE import."""
    bundle_root = Path(bundle_root)
    if bundle_root.is_symlink() or not bundle_root.is_dir():
        fail('update bundle directory must be a regular directory')
    baseline = closure.get('baseline')
    if baseline is not None and not store_has_path(baseline, store_dir=store_dir):
        fail(f'delta bundle requires baseline closure {baseline}, which is not present in /nix/store')
    expected_rels = set()
    for item in closure['cache_files']:
        rel = item['path']
        file_path = bundle_root / rel
        sp = item.get('store_path')
        if not file_path.exists():
            if allow_skipped_present and sp and sp != closure['toplevel'] and store_has_path(sp, store_dir=store_dir):
                continue
            fail(f'missing binary-cache file {rel}')
        if file_path.is_symlink() or not file_path.is_file():
            fail(f'binary-cache entry must be a regular file: {rel}')
        actual_sha, actual_size = sha256_file(file_path)
        if actual_sha != item['sha256'] or actual_size != item['size']:
            fail(f'binary-cache file {rel} failed SHA-256 or size check against signed closure-manifest.json')
        expected_rels.add(rel)
    cache_dir = bundle_root / 'cache'
    if cache_dir.is_dir():
        for extra in sorted(cache_dir.rglob('*')):
            if extra.is_file() or extra.is_symlink():
                rel = extra.relative_to(bundle_root).as_posix()
                if rel not in expected_rels:
                    fail(f'unindexed file refused in binary cache: {rel}')


def verify_open_nixos_bundle(bundle_dir, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                             now=None, last_applied=None, suites=PINNED_SUITES,
                             version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                             store_dir=Path('/nix/store'), package_keyring=None,
                             allow_skipped_present=False):
    bundle_dir = Path(bundle_dir)
    if bundle_dir.is_symlink() or not bundle_dir.is_dir():
        fail('update bundle path must be a regular directory')
    keyring = resolve_trust_anchor(keyring)
    signers = resolve_trust_anchor(signers)
    meta = {}
    if (bundle_dir / 'InRelease').is_file():
        meta['InRelease'] = read_regular_bytes(bundle_dir / 'InRelease', 1024 * 1024)
    elif (bundle_dir / 'Release').is_file() and (bundle_dir / 'Release.gpg').is_file():
        meta['Release'] = read_regular_bytes(bundle_dir / 'Release', 1024 * 1024)
        meta['Release.gpg'] = read_regular_bytes(bundle_dir / 'Release.gpg', 65536)
    else:
        fail('update bundle requires signed InRelease or Release + Release.gpg')
    fingerprint, release_text = verify_signed_metadata(
        meta, keyring=keyring, signers=signers, package_keyring=package_keyring)
    release = parse_release(release_text, now=now, suites=suites)
    if last_applied is not None and release.get('date') <= last_applied:
        fail('update bundle is not newer than the last applied update (replay refused)')
    if CLOSURE_MANIFEST_NAME not in release['entries']:
        fail('signed Release metadata does not authenticate closure-manifest.json in this bundle')
    expected_sha, expected_size = release['entries'][CLOSURE_MANIFEST_NAME]
    manifest_path = bundle_dir / CLOSURE_MANIFEST_NAME
    if not manifest_path.is_file():
        fail('signed Release metadata does not authenticate closure-manifest.json in this bundle')
    manifest_raw = read_regular_bytes(manifest_path, LIMIT)
    if hashlib.sha256(manifest_raw).hexdigest() != expected_sha or len(manifest_raw) != expected_size:
        fail('closure-manifest.json SHA-256 or byte size does not match signed Release metadata')
    closure = parse_closure_manifest(manifest_raw)
    verify_closure_archives(
        bundle_dir, closure, store_dir=store_dir, allow_skipped_present=allow_skipped_present)
    installed_ver = current_installed_version(version_file=version_file, current_system=current_system)
    action = 'install'
    if installed_ver is not None:
        if compare_versions(closure['version'], 'gt', installed_ver):
            action = 'upgrade'
        elif compare_versions(closure['version'], 'eq', installed_ver):
            fail(f'WrOS {installed_ver} is already installed')
        else:
            fail(f'downgrade from {installed_ver} to {closure["version"]} refused')
    review_id = _base.review_id_of(meta, manifest_raw)
    nar_entries = [f for f in closure['cache_files'] if f['path'].startswith('cache/nar/')]
    synthetic_pkg = {
        'name': 'wros',
        'version': closure['version'],
        'architecture': 'amd64',
        'filename': CLOSURE_MANIFEST_NAME,
        'size': sum(f['size'] for f in nar_entries),
        'sha256': expected_sha,
        'action': action,
        'toplevel': closure['toplevel'],
    }
    return {
        'meta': meta,
        'fingerprint': fingerprint,
        'release': release,
        'closure_raw': manifest_raw,
        'closure': closure,
        'installed_version': installed_ver,
        'action': action,
        'packages': [synthetic_pkg],
        'review_id': review_id,
    }


def verify_bundle(bundle_dir, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, dpkg_dir=None,
                  now=None, host_arch=None, last_applied=None, strict_path=False,
                  suites=PINNED_SUITES, package_keyring=None, version_file=DEFAULT_VERSION_FILE,
                  current_system=DEFAULT_CURRENT_SYSTEM, store_dir=Path('/nix/store'),
                  allow_skipped_present=False):
    raw = verify_open_nixos_bundle(
        bundle_dir, keyring=keyring, signers=signers, now=now, last_applied=last_applied,
        suites=suites, version_file=version_file, current_system=current_system,
        store_dir=store_dir, package_keyring=package_keyring,
        allow_skipped_present=allow_skipped_present)
    narinfo_count = sum(1 for f in raw['closure']['cache_files'] if f['path'].endswith('.narinfo'))
    return {
        'verified': True,
        'signer_fingerprint': raw['fingerprint'],
        'suite': raw['release']['suite'],
        'date': raw['release']['date'].isoformat(),
        'valid_until': raw['release']['valid_until'].isoformat(),
        'review_id': raw['review_id'],
        'package_count': max(narinfo_count, 1),
        'packages': raw['packages'],
        'toplevel': raw['closure']['toplevel'],
        'version': raw['closure']['version'],
    }


def review_bundle(bundle_dir, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                  state_dir=DEFAULT_STATE_DIR, now=None, suites=PINNED_SUITES,
                  version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                  store_dir=Path('/nix/store'), **_unused):
    public = read_public_status(state_dir) or {}
    last_applied = None
    if public.get('last_applied_date'):
        last_applied = parse_iso(public['last_applied_date'], 'last applied')
    raw = verify_open_nixos_bundle(
        bundle_dir, keyring=keyring, signers=signers, now=now, last_applied=last_applied,
        suites=suites, version_file=version_file, current_system=current_system,
        store_dir=store_dir, allow_skipped_present=True)
    narinfo_count = sum(1 for f in raw['closure']['cache_files'] if f['path'].endswith('.narinfo'))
    return {
        'status': 'reviewed',
        'review_id': raw['review_id'],
        'signer': raw['fingerprint'],
        'suite': raw['release']['suite'],
        'date': raw['release']['date'].isoformat(),
        'valid_until': raw['release']['valid_until'].isoformat(),
        'packages': [{
            'name': 'wros',
            'from_version': raw['installed_version'],
            'to_version': raw['closure']['version'],
            'action': raw['action'],
        }],
        'toplevel': raw['closure']['toplevel'],
        'closure_paths': max(narinfo_count, 1),
        'removals': [],
        'backup_recommended': True,
    }


def closure_live_apply_check(closure, current_system=DEFAULT_CURRENT_SYSTEM):
    if not closure.get('app_only'):
        return False, 'system closure update requires offline reboot switch'
    curr = Path(current_system)
    target = Path(closure['toplevel'])
    if not curr.exists() or not target.exists():
        return False, 'running or target system closure is not mounted in /nix/store'
    for critical in ('kernel', 'initrd', 'systemd', 'etc/systemd/system'):
        c_path = curr / critical
        t_path = target / critical
        if c_path.exists() or t_path.exists():
            try:
                if c_path.resolve() != t_path.resolve():
                    return False, f'{critical} changed between closures'
            except OSError:
                return False, f'could not compare {critical}'
    return True, ''


def update_gc_roots(store, *, gcroots_dir=DEFAULT_GCROOTS_DIR, current_system=DEFAULT_CURRENT_SYSTEM):
    """Protect running, previous, and staged system closures from nix-collect-garbage."""
    gcroots = Path(gcroots_dir)
    gcroots.mkdir(parents=True, mode=0o755, exist_ok=True)
    roots = {
        'running': current_toplevel(current_system),
        'previous': _manifest_toplevel(store.previous / 'manifest.json'),
        'staged': _manifest_toplevel(store.staged / 'manifest.json'),
    }
    for name, target in roots.items():
        link = gcroots / name
        if link.is_symlink() or link.exists():
            link.unlink(missing_ok=True)
        if target and STORE_PATH_RE.fullmatch(target):
            os.symlink(target, link)
    fsync_dir(gcroots)
    return {k: v for k, v in roots.items() if v}


def _manifest_toplevel(manifest_path):
    try:
        if manifest_path.is_file() and not manifest_path.is_symlink():
            data = json.loads(read_regular_bytes(manifest_path, 262144).decode('utf-8'))
            top = data.get('toplevel')
            if isinstance(top, str) and STORE_PATH_RE.fullmatch(top):
                return top
    except (ValueError, OSError):
        pass
    return None


def require_free_space(target_dir, needed_bytes):
    """Check free space on the target filesystem before staging (U4)."""
    target_dir = Path(target_dir)
    target_dir.mkdir(parents=True, exist_ok=True)
    st = os.statvfs(target_dir)
    available = st.f_bavail * st.f_frsize
    required = needed_bytes + MIN_FREE_BYTES
    if available < required:
        avail_mib = available // (1024 * 1024)
        req_mib = required // (1024 * 1024)
        fail(f'not enough free disk space to stage update ({avail_mib} MiB available, {req_mib} MiB required)')


def write_nixos_tree(bundle_dir, raw, partial, tx_id, when, *,
                     current_system=DEFAULT_CURRENT_SYSTEM, store_dir=Path('/nix/store')):
    """Copy verified metadata and only the binary-cache files needed for missing store paths."""
    bundle_dir = Path(bundle_dir)
    closure = raw['closure']
    missing_files = []
    for item in closure['cache_files']:
        sp = item.get('store_path')
        if sp and sp != closure['toplevel'] and store_has_path(sp, store_dir=store_dir):
            continue
        missing_files.append(item)
    needed_bytes = sum(item['size'] for item in missing_files)
    require_free_space(partial.parent, needed_bytes)

    partial.mkdir(mode=0o700)
    (partial / 'cache/nar').mkdir(parents=True, mode=0o700)
    (partial / 'metadata').mkdir(mode=0o700)
    for item in missing_files:
        src = bundle_dir / item['path']
        dst = partial / item['path']
        dst.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
        shutil.copyfile(src, dst, follow_symlinks=False)
        os.chmod(dst, 0o600)
        sha, size = sha256_file(dst)
        if sha != item['sha256'] or size != item['size']:
            fail(f'staged binary-cache file hash mismatch after copy: {item["path"]}')
    metadata_hashes = {}
    for name, data in [*raw['meta'].items(), (CLOSURE_MANIFEST_NAME, raw['closure_raw'])]:
        atomic_write(partial / 'metadata' / name, data, mode=0o600)
        metadata_hashes[name] = hashlib.sha256(data).hexdigest()
    live, live_reason = closure_live_apply_check(closure, current_system=current_system)
    manifest = {
        'version': MANIFEST_VERSION,
        'format': 'wros-nixos-cache-v2',
        'transaction_id': tx_id,
        'review_id': raw['review_id'],
        'verified': True,
        'recorded_at_utc': when.isoformat(),
        'signer_fingerprint': raw['fingerprint'],
        'suite': raw['release']['suite'],
        'release_date': raw['release']['date'].isoformat(),
        'valid_until': raw['release']['valid_until'].isoformat(),
        'toplevel': closure['toplevel'],
        'previous_toplevel': current_toplevel(current_system),
        'wros_version': closure['version'],
        'closure_digest': metadata_hashes[CLOSURE_MANIFEST_NAME],
        'metadata': metadata_hashes,
        'package_count': 1,
        'packages': [{
            'name': 'wros',
            'version': closure['version'],
            'architecture': 'amd64',
            'filename': CLOSURE_MANIFEST_NAME,
            'size': needed_bytes,
            'sha256': metadata_hashes[CLOSURE_MANIFEST_NAME],
            'action': raw['action'],
        }],
        'live_apply': live,
        'live_apply_reason': live_reason,
    }
    json_write(partial / 'manifest.json', manifest)
    for directory in (partial / 'cache/nar', partial / 'cache', partial / 'metadata', partial):
        fsync_dir(directory)
    return manifest, live


def stage_update(bundle_dir, review_id, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                 state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER, now=None,
                 allowed_prefixes=ALLOWED_MEDIA_PREFIXES, suites=PINNED_SUITES,
                 version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                 store_dir=Path('/nix/store'), gcroots_dir=DEFAULT_GCROOTS_DIR, **_unused):
    if not isinstance(review_id, str) or HEX64_RE.fullmatch(review_id) is None:
        fail('invalid review id')
    _base.check_bundle_path(str(bundle_dir), allowed_prefixes)
    store = Store(state_dir)
    trigger_path = Path(trigger_path)
    when = clock(now)
    with store.exclusive():
        if os.path.lexists(trigger_path):
            fail('another offline update is already pending; original preserved')
        clear_partials(store)
        records = store.records()
        if records and records[-1]['state'] == 'applying':
            fail('an interrupted update transaction must be reconciled before staging a new update')
        last_applied = store.last_applied()
        tx_id = secrets.token_hex(16)
        partial = store.dir / f'staged.{tx_id}.partial'
        try:
            raw = verify_open_nixos_bundle(
                bundle_dir, keyring=keyring, signers=signers, now=when, last_applied=last_applied,
                suites=suites, version_file=version_file, current_system=current_system,
                store_dir=store_dir, allow_skipped_present=True)
            if raw['review_id'] != review_id:
                fail('update bundle changed since it was reviewed; review it again')
            manifest, live = write_nixos_tree(
                bundle_dir, raw, partial, tx_id, when, current_system=current_system, store_dir=store_dir)
            if os.path.lexists(store.staged):
                remove_staged(store)
            os.rename(partial, store.staged)
            fsync_dir(store.dir)
        except BaseException:
            shutil.rmtree(partial, ignore_errors=True)
            raise
        labels = [f'wros {manifest["wros_version"]}']
        try:
            store.append('staged', tx_id, when, packages=labels, review_id=raw['review_id'],
                         signer=raw['fingerprint'], suite=raw['release']['suite'],
                         date=raw['release']['date'].isoformat(), toplevel=manifest['toplevel'])
            os.symlink(str(store.staged), trigger_path)
            fsync_dir(trigger_path.parent)
        except BaseException:
            remove_staged(store)
            try:
                store.append('cancelled', tx_id, when, note='staging did not complete')
            except OSError:
                pass
            store.publish(trigger_path)
            raise
        update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
        store.publish(trigger_path)
    return {
        'status': 'staged',
        'transaction_id': tx_id,
        'review_id': raw['review_id'],
        'packages': 1,
        'toplevel': manifest['toplevel'],
        'restart_required': not live,
    }


def revalidate_staged_nixos(staged_dir, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                            now=None, last_applied=None, mode='apply', suites=PINNED_SUITES,
                            store_dir=Path('/nix/store')):
    staged = Path(staged_dir)
    require_private_entry(staged, 'dir')
    require_private_entry(staged / 'manifest.json', 'file')
    manifest = json.loads(read_regular_bytes(staged / 'manifest.json', 262144).decode('utf-8'))
    if (not isinstance(manifest, dict) or manifest.get('version') != MANIFEST_VERSION
            or manifest.get('format') != 'wros-nixos-cache-v2' or manifest.get('verified') is not True):
        fail('invalid staged NixOS update manifest')
    tx_id = manifest.get('transaction_id')
    if not isinstance(tx_id, str) or HEX32_RE.fullmatch(tx_id) is None:
        fail('invalid staged update transaction id')
    stage_time = parse_iso(manifest.get('recorded_at_utc'), 'staging')
    reference = clock(now)
    if mode == 'apply':
        if reference - stage_time > MAX_STAGED_AGE:
            fail('staged update is older than 30 days; stage a fresh bundle')
        if stage_time > reference + BOOT_CLOCK_SKEW:
            fail('system clock is earlier than the time the update was staged')
    metadata_hashes = manifest.get('metadata')
    if not isinstance(metadata_hashes, dict) or CLOSURE_MANIFEST_NAME not in metadata_hashes:
        fail('invalid staged update metadata record')
    signed_names = set(metadata_hashes) - {CLOSURE_MANIFEST_NAME}
    if signed_names not in ({'InRelease'}, {'Release', 'Release.gpg'}):
        fail('invalid staged update signature metadata record')
    metadata_dir = staged / 'metadata'
    require_private_entry(metadata_dir, 'dir')
    retained = {}
    for name, expected in metadata_hashes.items():
        require_private_entry(metadata_dir / name, 'file')
        data = read_regular_bytes(metadata_dir / name, LIMIT)
        if hashlib.sha256(data).hexdigest() != expected:
            fail(f'retained {name} no longer matches the staged digest')
        retained[name] = data
    meta = {name: retained[name] for name in signed_names}
    keyring = resolve_trust_anchor(keyring)
    signers = resolve_trust_anchor(signers)
    fingerprint, release_text = verify_signed_metadata(meta, keyring=keyring, signers=signers)
    if fingerprint != manifest.get('signer_fingerprint'):
        fail('retained signature signer differs from the staged signer')
    release = parse_release(release_text, now=stage_time, suites=suites)
    going_back = mode == 'rollback' or bool(manifest.get('rollback'))
    if mode == 'apply' and not going_back and last_applied is not None and release['date'] <= last_applied:
        fail('staged update is not newer than the last applied update (replay refused)')
    expected_sha, expected_size = release['entries'].get(CLOSURE_MANIFEST_NAME, (None, None))
    closure_raw = retained[CLOSURE_MANIFEST_NAME]
    if hashlib.sha256(closure_raw).hexdigest() != expected_sha or len(closure_raw) != expected_size:
        fail('retained closure-manifest.json does not match retained signed Release')
    closure = parse_closure_manifest(closure_raw)
    if not going_back and not store_has_path(closure['toplevel'], store_dir=store_dir):
        verify_closure_archives(staged, closure, store_dir=store_dir, allow_skipped_present=True)
    return {
        'manifest': manifest,
        'tx_id': tx_id,
        'closure': closure,
        'release': release,
        'fingerprint': fingerprint,
        'rollback': going_back,
    }


def import_verified_closure(staged_dir, closure, *, run=None, store_dir=Path('/nix/store')):
    """Import already-verified binary-cache files into /nix/store through Nix (U2)."""
    run = run or _base.default_runner
    staged = Path(staged_dir)
    if store_has_path(closure['toplevel'], store_dir=store_dir):
        return
    # 1. Re-verify every binary-cache file hash right before invoking nix copy.
    verify_closure_archives(staged, closure, store_dir=store_dir, allow_skipped_present=True)
    cache_uri = f'file://{staged.resolve() / "cache"}'
    # 2. Import through Nix (works against the read-only /nix/store mount).
    res = run([
        'nix', '--extra-experimental-features', 'nix-command',
        'copy', '--from', cache_uri, '--no-check-sigs', closure['toplevel'],
    ])
    if res.returncode != 0:
        fail(f'failed to import verified closure {closure["toplevel"]} from staged binary cache')
    res2 = run(['nix-store', '--verify-path', closure['toplevel']])
    if res2.returncode != 0:
        fail(f'nix-store --verify-path failed after importing {closure["toplevel"]}')


def switch_system_generation(toplevel, *, profile=DEFAULT_PROFILE, live=False, run=None):
    """Set the system profile generation and activate its boot entries (or switch live)."""
    run = run or _base.default_runner
    res = run(['nix-env', '-p', str(profile), '--set', str(toplevel)])
    if res.returncode != 0:
        fail('failed to switch system profile to new generation')
    verb = 'switch' if live else 'boot'
    res2 = run([str(Path(toplevel) / 'bin/switch-to-configuration'), verb])
    if res2.returncode != 0:
        fail(f'switch-to-configuration {verb} failed')


def apply_staged_transaction(*, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                             keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                             store_dir=Path('/nix/store'), profile=DEFAULT_PROFILE,
                             current_system=DEFAULT_CURRENT_SYSTEM, gcroots_dir=DEFAULT_GCROOTS_DIR,
                             runner=None, now=None, live=False, **_unused):
    store = Store(state_dir)
    trigger_path = Path(trigger_path)
    run = runner or _base.default_runner
    when = clock(now)
    with store.exclusive():
        if not store.staged.is_dir() or not owned_trigger(trigger_path, store.staged):
            fail('no verified staged update owns the update trigger')
        last_applied = store.last_applied()
        try:
            details = revalidate_staged_nixos(
                store.staged, keyring=keyring, signers=signers, now=when,
                last_applied=last_applied, mode='apply', store_dir=store_dir)
        except ValueError as exc:
            unlink_trigger(trigger_path)
            tx_id = secrets.token_hex(16)
            store.append('failed', tx_id, when, error=str(exc))
            store.publish(trigger_path)
            raise
        manifest = details['manifest']
        tx_id = manifest['transaction_id']
        if live and not manifest.get('live_apply'):
            fail('staged update requires a system restart')
        labels = [f'wros {manifest["wros_version"]}']
        store.append('applying', tx_id, when, packages=labels, toplevel=manifest['toplevel'],
                     rollback=bool(manifest.get('rollback')), automatic=bool(manifest.get('automatic')))
        unlink_trigger(trigger_path)
        try:
            if not details['rollback']:
                import_verified_closure(store.staged, details['closure'], run=run, store_dir=store_dir)
            switch_system_generation(manifest['toplevel'], profile=profile, live=live, run=run)
        except ValueError as exc:
            store.append('failed', tx_id, when, packages=labels, error=str(exc))
            store.publish(trigger_path)
            raise
        # Once the closure is in /nix/store, remove staged/cache/ so installed/ and previous/
        # never retain duplicate NAR copies on the 32 GB eMMC (U4).
        shutil.rmtree(store.staged / 'cache', ignore_errors=True)
        if details['rollback']:
            keep_gone_back(store)
            end_probation(store)
        else:
            keep_installed(store)
            store.record_applied(details['release']['date'], tx_id, manifest['review_id'])
            if not live:
                start_probation(store, tx_id)
        store.append('completed', tx_id, when, packages=labels, toplevel=manifest['toplevel'],
                     rollback=bool(manifest.get('rollback')), automatic=bool(manifest.get('automatic')))
        update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
        store.publish(trigger_path)
        return {
            'status': 'completed',
            'transaction_id': tx_id,
            'toplevel': manifest['toplevel'],
            'version': manifest['wros_version'],
        }


def execute_offline_boot_transaction(**kwargs):
    return apply_staged_transaction(live=False, **kwargs)


def apply_staged_now(**kwargs):
    return apply_staged_transaction(live=True, **kwargs)


def prune_old_generations_and_gc(store, *, profile=DEFAULT_PROFILE, gcroots_dir=DEFAULT_GCROOTS_DIR,
                                 current_system=DEFAULT_CURRENT_SYSTEM, run=None):
    """Delete older system generations and collect unreferenced store paths once healthy (U4)."""
    run = run or _base.default_runner
    update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
    # Keep the current generation and at most 1 previous generation in the system profile
    run(['nix-env', '-p', str(profile), '--delete-generations', '+2'])
    run(['nix-collect-garbage'])


def report_health(what, *, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                  profile=DEFAULT_PROFILE, gcroots_dir=DEFAULT_GCROOTS_DIR,
                  current_system=DEFAULT_CURRENT_SYSTEM, runner=None):
    res = _base.report_health(what, state_dir=state_dir, trigger_path=trigger_path)
    if what == 'ready' and res.get('status') == 'ready':
        try:
            prune_old_generations_and_gc(
                Store(state_dir), profile=profile, gcroots_dir=gcroots_dir,
                current_system=current_system, run=runner)
        except Exception:
            pass
    return res


def stage_rollback(*, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, state_dir=DEFAULT_STATE_DIR,
                   trigger_path=DEFAULT_TRIGGER, now=None, version_file=DEFAULT_VERSION_FILE,
                   current_system=DEFAULT_CURRENT_SYSTEM, store_dir=Path('/nix/store'),
                   gcroots_dir=DEFAULT_GCROOTS_DIR, **_unused):
    store = Store(state_dir)
    with store.exclusive():
        return stage_rollback_locked(
            store, keyring=keyring, signers=signers, trigger_path=Path(trigger_path),
            when=clock(now), version_file=version_file, current_system=current_system,
            store_dir=store_dir, gcroots_dir=gcroots_dir, automatic=False)


def stage_rollback_locked(store, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                          trigger_path=DEFAULT_TRIGGER, when=None,
                          version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                          store_dir=Path('/nix/store'), gcroots_dir=DEFAULT_GCROOTS_DIR,
                          automatic=False):
    when = clock(when)
    if os.path.lexists(trigger_path):
        fail('another offline update is already pending; original preserved')
    clear_partials(store)
    records = store.records()
    if records and records[-1]['state'] == 'applying':
        fail('an interrupted update transaction must be reconciled before going back')
    if not store.previous.is_dir() or store.previous.is_symlink():
        fail('there is no previous version to go back to')
    try:
        current = json.loads(read_regular_bytes(store.installed / 'manifest.json', 262144).decode('utf-8'))
        installed_target = current['wros_version']
    except (ValueError, OSError, KeyError, TypeError):
        fail('the installed update is not recorded, so going back is unavailable')
    have_ver = current_installed_version(version_file=version_file, current_system=current_system)
    if have_ver is not None and not compare_versions(have_ver, 'eq', installed_target):
        fail('the running system does not match the recorded installed update; going back is unavailable')
    details = revalidate_staged_nixos(
        store.previous, keyring=keyring, signers=signers, now=when, mode='rollback', store_dir=store_dir)
    prev_manifest = details['manifest']
    prev_ver = prev_manifest['wros_version']
    if not compare_versions(prev_ver, 'lt', installed_target):
        fail('the previous version is not older than the installed one')
    prev_top = prev_manifest['toplevel']
    if not store_has_path(prev_top, store_dir=store_dir):
        fail('previous generation closure is not present in /nix/store')
    tx_id = secrets.token_hex(16)
    partial = store.dir / f'staged.{tx_id}.partial'
    try:
        shutil.copytree(store.previous, partial, symlinks=True)
        manifest = prev_manifest | {
            'transaction_id': tx_id,
            'recorded_at_utc': when.isoformat(),
            'rollback': True,
            'automatic': automatic,
            'rollback_from': [f'wros {installed_target}'],
            'live_apply': False,
        }
        json_write(partial / 'manifest.json', manifest)
        if os.path.lexists(store.staged):
            remove_staged(store)
        os.rename(partial, store.staged)
        fsync_dir(store.dir)
    except BaseException:
        shutil.rmtree(partial, ignore_errors=True)
        raise
    labels = [f'wros {prev_ver}']
    store.append('staged', tx_id, when, packages=labels, review_id=manifest['review_id'],
                 rollback=True, automatic=automatic, signer=details['fingerprint'],
                 suite=details['release']['suite'], date=details['release']['date'].isoformat(),
                 toplevel=prev_top)
    os.symlink(str(store.staged), trigger_path)
    fsync_dir(Path(trigger_path).parent)
    update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
    store.publish(trigger_path)
    return {
        'status': 'staged',
        'transaction_id': tx_id,
        'review_id': manifest['review_id'],
        'packages': 1,
        'toplevel': prev_top,
        'restart_required': True,
        'rollback': True,
    }


def cancel_staged(*, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                  gcroots_dir=DEFAULT_GCROOTS_DIR, current_system=DEFAULT_CURRENT_SYSTEM, now=None):
    res = _base.cancel_staged(state_dir=state_dir, trigger_path=trigger_path, now=now)
    try:
        update_gc_roots(Store(state_dir), gcroots_dir=gcroots_dir, current_system=current_system)
    except OSError:
        pass
    return res


def repair_updates(*, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                   keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                   profile=DEFAULT_PROFILE, current_system=DEFAULT_CURRENT_SYSTEM,
                   gcroots_dir=DEFAULT_GCROOTS_DIR, runner=None, now=None, **_unused):
    store = Store(state_dir)
    run = runner or _base.default_runner
    when = clock(now)
    try:
        with store.exclusive():
            clear_partials(store)
            unlink_trigger(trigger_path)
            res = run(['nix-store', '--verify', '--check-contents'])
            if res.returncode != 0:
                fail('Nix store integrity check failed; re-stage a signed update bundle to restore missing paths')
            target_top = _manifest_toplevel(store.installed / 'manifest.json') or current_toplevel(current_system)
            if target_top:
                switch_system_generation(target_top, profile=profile, live=False, run=run)
            records = store.records()
            tx_id = records[-1]['transaction_id'] if records else secrets.token_hex(16)
            store.append('recovered', tx_id, when, note='verified nix store and boot profile')
            update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
            store.publish(trigger_path)
            return {'status': 'recovered', 'message': 'Nix store and system boot generation were verified and repaired.'}
    except StateBusy:
        return {'status': 'failed', 'message': 'An update operation is running; try again when it finishes.'}
    except ValueError as exc:
        return {'status': 'failed', 'message': str(exc)}


def reconcile(*, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
              keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
              version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
              store_dir=Path('/nix/store'), gcroots_dir=DEFAULT_GCROOTS_DIR,
              profile=DEFAULT_PROFILE, runner=None, now=None, **_unused):
    store = Store(state_dir)
    if not os.path.lexists(store.dir):
        return {'status': 'idle', 'message': 'No update state.'}
    when = clock(now)
    try:
        with store.exclusive():
            clear_partials(store)
            records = store.records()
            finish_keeping(store, records, Path(trigger_path))
            latest = records[-1] if records else None
            if latest is not None and latest['state'] == 'applying':
                store.append('interrupted', latest['transaction_id'], when,
                             error='update switch was interrupted before completion')
            probation = read_probation(store)
            if probation is not None:
                started = probation.get('started_boot')
                if started is not None and started != read_boot_id():
                    probation = probation | {'failed_logins': probation['failed_logins'] + 1, 'started_boot': None}
                if probation['failed_logins'] >= PROBATION_FAILED_LOGINS and store.previous.is_dir():
                    end_probation(store)
                    stage_rollback_locked(
                        store, keyring=keyring, signers=signers, trigger_path=Path(trigger_path),
                        when=when, version_file=version_file, current_system=current_system,
                        store_dir=store_dir, gcroots_dir=gcroots_dir, automatic=True)
                    return {'status': 'going-back',
                            'message': 'The new version did not start twice; going back to the previous one.'}
                json_write(store.probation_path, probation)
            update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
            store.publish(trigger_path)
            return {'status': 'idle', 'message': 'Update state is consistent.'}
    except StateBusy:
        return {'status': 'busy', 'message': 'An update operation owns the state; nothing to reconcile.'}


def inspect_recovery(mountinfo_text=None, state_dir=DEFAULT_STATE_DIR,
                     trigger_path=DEFAULT_TRIGGER, **_unused):
    if mountinfo_text is None:
        mountinfo_text = Path('/proc/self/mountinfo').read_text(encoding='utf-8', errors='replace')
    mounts = parse_mountinfo(mountinfo_text)
    root_fs, root_dev = mounts.get('/', ('unknown', 'unknown'))
    separate_home = '/home' in mounts
    state_dir = Path(state_dir)
    trigger_path = Path(trigger_path)
    public = read_public_status(state_dir) or {}
    update_state = public.get('update_state', 'idle')
    staged_update = update_state == 'staged' and owned_trigger(trigger_path, state_dir / 'staged')
    staged_packages = [p for p in public.get('packages', []) if isinstance(p, str)] if staged_update else []
    if not staged_update and update_state == 'staged':
        update_state = 'idle'
    prev = public.get('previous_version') if isinstance(public.get('previous_version'), str) else None
    strategy = (
        'System updates switch atomic NixOS generations in /nix/var/nix/profiles/system and keep '
        'the previous generation bootable in GRUB. Create an encrypted Restic project backup before '
        'major changes.'
    )
    return {
        'root_filesystem': root_fs,
        'root_device': root_dev,
        'separate_home': separate_home,
        'rollback_capable': prev is not None,
        'rollback_strategy': strategy,
        'interrupted_dpkg': False,
        'unconfigured_packages': [],
        'staged_update': staged_update,
        'staged_packages': staged_packages,
        'update_state': update_state,
        'previous_version': prev,
        'probation': public.get('probation') is True,
        'went_back': public.get('went_back') is True,
        'went_back_automatically': public.get('went_back_automatically') is True,
        'status': f'Root filesystem: {root_fs} (NixOS atomic generation rollback available).',
    }


def check_online(*, base_url=ONLINE_URL, now=None, keyring=DEFAULT_KEYRING,
                 signers=DEFAULT_SIGNERS, version_file=DEFAULT_VERSION_FILE,
                 current_system=DEFAULT_CURRENT_SYSTEM, package_keyring=None, opener=None, **_unused):
    moment = clock(now)
    reads = moment.astimezone().strftime('%Y-%m-%d %H:%M')
    installed = current_installed_version(version_file=version_file, current_system=current_system)
    result = {'status': 'invalid', 'version': None, 'installed': installed, 'clock': reads}
    seen = {}
    keyring = resolve_trust_anchor(keyring)
    signers = resolve_trust_anchor(signers)
    try:
        inrelease = http_get(base_url + 'InRelease', 1024 * 1024, opener, seen=seen)
        _, release_text = verify_signed_metadata({'InRelease': inrelease}, keyring=keyring,
                                                 signers=signers, package_keyring=package_keyring)
        release = parse_release(release_text, now=now, suites=PINNED_SUITES)
        expected_sha, expected_size = release['entries'][CLOSURE_MANIFEST_NAME]
        manifest_raw = http_get(base_url + CLOSURE_MANIFEST_NAME, LIMIT, opener)
        if hashlib.sha256(manifest_raw).hexdigest() != expected_sha or len(manifest_raw) != expected_size:
            fail('closure-manifest.json SHA-256 or byte size does not match signed Release metadata')
        closure = parse_closure_manifest(manifest_raw)
    except (OSError, TimeoutError) as exc:
        if getattr(exc, 'code', None) == 404 and str(getattr(exc, 'url', '')).endswith('/InRelease'):
            return result | {'status': 'unpublished'}
        wrong = tls_clock_failure(exc) or moment < CLOCK_FLOOR
        return result | {'status': 'clock' if wrong else 'unreachable'}
    except (ValueError, KeyError) as exc:
        text = str(exc)
        if moment < CLOCK_FLOOR:
            status = 'clock'
        elif 'has expired (Valid-Until passed)' in text and server_agrees(seen, moment):
            status = 'expired'
        elif any(failure in text for failure in CLOCK_FAILURES):
            status = 'clock'
        else:
            status = 'invalid'
        return result | {'status': status}
    newer = installed is None or compare_versions(closure['version'], 'gt', installed)
    return result | {'status': 'available' if newer else 'current', 'version': closure['version']}


def http_stream_to_file(url, dest_path, expected_size, expected_sha256, opener=None, timeout=120):
    """Stream download to disk in 64 KiB chunks while hashing (U8: never buffer large NARs in RAM)."""
    opener = opener or urllib.request.build_opener(_base._NoRedirect)
    dest_path = Path(dest_path)
    dest_path.parent.mkdir(parents=True, mode=0o755, exist_ok=True)
    hasher = hashlib.sha256()
    total = 0
    with opener.open(url, timeout=timeout) as response, open(dest_path, 'wb') as out_f:
        if getattr(response, 'status', 200) != 200:
            raise urllib.error.URLError(f'HTTP {response.status}')
        while True:
            chunk = response.read(65536)
            if not chunk:
                break
            total += len(chunk)
            if total > expected_size or total > ARCHIVE_LIMIT:
                fail(f'downloaded file {dest_path.name} exceeds expected size')
            hasher.update(chunk)
            out_f.write(chunk)
        out_f.flush()
        os.fsync(out_f.fileno())
    if total != expected_size or hasher.hexdigest() != expected_sha256:
        fail(f'downloaded file {dest_path.name} failed SHA-256 or size check')


def fetch_online(*, base_url=ONLINE_URL, cache=ONLINE_CACHE, now=None,
                 keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                 version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                 store_dir=Path('/nix/store'), package_keyring=None, opener=None, **_unused):
    previous = os.umask(0o022)
    keyring = resolve_trust_anchor(keyring)
    signers = resolve_trust_anchor(signers)
    try:
        inrelease = http_get(base_url + 'InRelease', 1024 * 1024, opener)
        _, release_text = verify_signed_metadata({'InRelease': inrelease}, keyring=keyring,
                                                 signers=signers, package_keyring=package_keyring)
        release = parse_release(release_text, now=now, suites=PINNED_SUITES)
        expected_sha, expected_size = release['entries'][CLOSURE_MANIFEST_NAME]
        manifest_raw = http_get(base_url + CLOSURE_MANIFEST_NAME, LIMIT, opener)
        if hashlib.sha256(manifest_raw).hexdigest() != expected_sha or len(manifest_raw) != expected_size:
            fail('closure-manifest.json SHA-256 or byte size does not match signed Release metadata')
        closure = parse_closure_manifest(manifest_raw)
        installed = current_installed_version(version_file=version_file, current_system=current_system)
        if installed is not None and not compare_versions(closure['version'], 'gt', installed):
            fail(f'WrOS is already up to date ({installed})')
        to_download = [
            item for item in closure['cache_files']
            if not (item.get('store_path') and item['store_path'] != closure['toplevel']
                    and store_has_path(item['store_path'], store_dir=store_dir))
        ]
        cache = Path(cache)
        require_free_space(cache, sum(item['size'] for item in to_download))
        target = cache / hashlib.sha256(inrelease).hexdigest()[:16]
        partial = cache / ('.partial-' + secrets.token_hex(8))
        (partial / 'cache/nar').mkdir(parents=True, mode=0o755)
        try:
            (partial / 'InRelease').write_bytes(inrelease)
            (partial / CLOSURE_MANIFEST_NAME).write_bytes(manifest_raw)
            for item in to_download:
                file_url = item.get('url') or (base_url + item['path'])
                http_stream_to_file(
                    file_url, partial / item['path'], item['size'], item['sha256'],
                    opener=opener, timeout=120)
            verify_bundle(partial, keyring=keyring, signers=signers, now=now,
                          version_file=version_file, current_system=current_system,
                          store_dir=store_dir, package_keyring=package_keyring,
                          allow_skipped_present=True)
            if target.exists():
                shutil.rmtree(target)
            os.replace(partial, target)
        except BaseException:
            shutil.rmtree(partial, ignore_errors=True)
            raise
        return {'status': 'fetched', 'path': str(target), 'version': closure['version']}
    finally:
        os.umask(previous)


def launcher_main(verb, argv, pkexec_uid, *, euid=None, runner=None, **paths):
    if verb not in LAUNCHER_VERBS:
        fail(UNAVAILABLE)
    if (os.geteuid() if euid is None else euid) != 0:
        fail('this update helper must run as root through polkit')
    if pkexec_uid is None or PKEXEC_UID_RE.fullmatch(pkexec_uid) is None:
        fail('this update helper must be started through polkit')
    os.umask(0o077)
    argv = list(argv)
    if verb == 'stage':
        if len(argv) != 2:
            fail('usage: wros-update-stage <bundle-directory> <review-id>')
        _base.check_bundle_path(argv[0], paths.pop('allowed_prefixes', ALLOWED_MEDIA_PREFIXES))
        if HEX64_RE.fullmatch(argv[1]) is None:
            fail('invalid review id')
        return stage_update(argv[0], argv[1], allowed_prefixes=None, **paths)
    if verb == 'cancel':
        if argv:
            fail('usage: wros-update-cancel')
        return cancel_staged(**paths)
    if verb == 'repair':
        if argv:
            fail('usage: wros-update-repair')
        return repair_updates(runner=runner, **paths)
    if verb == 'apply':
        if argv:
            fail('usage: wros-update-apply')
        return apply_staged_now(runner=runner, **paths)
    if verb == 'rollback':
        if argv:
            fail('usage: wros-update-rollback')
        return stage_rollback(**paths)
    if verb == 'healthy':
        if argv not in (['started'], ['ready']):
            fail('usage: wros-update-healthy started|ready')
        return report_health(argv[0], runner=runner, **{
            k: v for k, v in paths.items()
            if k in ('state_dir', 'trigger_path', 'profile', 'gcroots_dir', 'current_system')
        })
    if verb == 'fetch':
        if argv:
            fail('usage: wros-update-fetch')
        return fetch_online(**paths)
    if argv != ['mount']:
        fail('usage: wros-update-media mount')
    return find_update_media(runner=runner, **{k: v for k, v in paths.items() if k in ('media_root', 'mountinfo_text')})


def main(argv=None, environ=None):
    argv = sys.argv[1:] if argv is None else argv
    environ = os.environ if environ is None else environ
    if argv == ['recovery-status']:
        result = inspect_recovery()
    elif argv == ['check-online']:
        result = check_online()
    elif len(argv) == 2 and argv[0] == 'review':
        result = review_bundle(argv[1])
    elif len(argv) == 2 and argv[0] == 'verify-bundle':
        verified = verify_bundle(Path(argv[1]))
        result = {
            'status': 'verified',
            'message': (
                f"Signature, signer pin and closure contents verified for {verified['package_count']} store path(s). "
                'Use Review to see what would change before staging.'
            ),
        }
    elif argv == ['run-offline-boot']:
        reserved_role(environ, 'WROS_SYSTEMD_OFFLINE_UPDATE')
        ours = owned_trigger(DEFAULT_TRIGGER, DEFAULT_STATE_DIR / 'staged')
        try:
            result = execute_offline_boot_transaction()
        except ValueError as exc:
            if ours:
                # Say why first: continuing the boot isolates multi-user.target, which stops this
                # unit, so the caller's error report would never be written.
                print(f'wros-update: offline update failed: {exc}', file=sys.stderr, flush=True)
                continue_boot_after_failure()
            raise
        if ours and result.get('status') == 'completed':
            subprocess.run(['systemctl', '--no-block', 'reboot'],
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=15, check=True, env=safe_env())
    elif argv == ['reconcile']:
        reserved_role(environ, 'WROS_SYSTEMD_RECONCILE')
        result = reconcile()
        if result.get('status') == 'going-back':
            subprocess.run(['systemctl', '--no-block', 'reboot'],
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=15, check=False, env=safe_env())
    elif argv == ['rescue-repair']:
        if os.geteuid() != 0 or 'PKEXEC_UID' in environ or environ.get('WROS_RESCUE') != '1':
            fail(UNAVAILABLE)
        result = repair_updates()
    else:
        fail(UNAVAILABLE)
    print(json.dumps(result, sort_keys=True))
    return 1 if argv == ['rescue-repair'] and result.get('status') == 'failed' else 0
