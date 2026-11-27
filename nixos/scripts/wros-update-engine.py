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
import http.client
import fcntl
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import ssl
import stat
import subprocess
import sys
import tempfile
import time
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
# Thin bundles (rc23, #169): upstream packages are listed in the signed manifest under their
# cache.nixos.org address and fetched from there, checked like every other file. The system
# configuration may name another base to fetch them from (a VM test's local copy); the manifest
# itself only ever names cache.nixos.org.
UPSTREAM_URL = 'https://cache.nixos.org/'
DEFAULT_UPSTREAM_FILE = Path('/etc/wros/update-upstream-url')
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
UPSTREAM_URL_RE = re.compile(
    r'^https://cache\.nixos\.org/(?:[0-9a-df-np-sv-z]{32}\.narinfo|nar/[0-9a-df-np-sv-z]{52}\.nar(?:\.xz|\.zst)?)$'
)


def upstream_base(path=DEFAULT_UPSTREAM_FILE):
    """Where to fetch cache.nixos.org's files from: cache.nixos.org itself, unless the (root-owned)
    system configuration names another http(s) base ending in '/'."""
    try:
        value = Path(path).read_text(encoding='utf-8').strip()
    except OSError:
        return UPSTREAM_URL
    if re.fullmatch(r'https?://[A-Za-z0-9.:\[\]-]+(?:/[A-Za-z0-9._~-]+)*/', value):
        return value
    fail(f'{path} must name an http(s) address ending in /')
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
class Store(_base.Store):
    """The Debian engine's state store; on NixOS "the version going back installs", as Settings
    and the recovery menu show it, is the previous system generation's when there is one."""

    def publish(self, trigger_path=None, records=None):
        super().publish(trigger_path, records)
        try:
            target = previous_generation(DEFAULT_PROFILE)
        except (OSError, ValueError):
            target = None
        if target is None:
            return
        try:
            status = json.loads(read_regular_bytes(self.status_path, 262144).decode('utf-8'))
            if isinstance(status, dict) and status.get('previous_version') != target[2]:
                status['previous_version'] = target[2]
                atomic_write(self.status_path, json_bytes(status), mode=0o644, dir_mode=0o755)
        except (OSError, ValueError):
            pass  # best effort, as the base publish is
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


REAL_STORE_DIR = Path('/nix/store')
_valid_store_paths_cache = None


def _valid_store_paths():
    """Every path registered in the Nix database, read once per process (one `nix path-info --all`
    call instead of one call per closure entry). None when Nix cannot be asked."""
    global _valid_store_paths_cache
    if _valid_store_paths_cache is None:
        try:
            res = subprocess.run(
                ['nix', '--extra-experimental-features', 'nix-command', 'path-info', '--all'],
                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=300, env=safe_env())
        except (OSError, subprocess.SubprocessError):
            return None
        if res.returncode != 0:
            return None
        _valid_store_paths_cache = frozenset(res.stdout.split())
    return _valid_store_paths_cache


def forget_valid_store_paths():
    global _valid_store_paths_cache
    _valid_store_paths_cache = None


def store_has_path(store_path, store_dir=REAL_STORE_DIR):
    """A path counts as present only when Nix has it registered, not merely when its directory
    exists: an unregistered directory (an interrupted import, or a store overlaid from elsewhere)
    is neither usable by nix-env nor a reason to skip importing it. Scratch store directories
    (the engine's unit tests) have no database, so there existence is all there is."""
    if not isinstance(store_path, str) or STORE_PATH_RE.fullmatch(store_path) is None:
        return False
    if not (Path(store_dir) / Path(store_path).name).exists():
        return False
    if Path(store_dir) != REAL_STORE_DIR:
        return True
    valid = _valid_store_paths()
    return valid is None or store_path in valid


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
        if url is not None and (
                not isinstance(url, str)
                or (RELEASE_ASSET_URL_RE.fullmatch(url) is None
                    and UPSTREAM_URL_RE.fullmatch(url) is None)):
            fail(f'invalid download address for {rel}')
        if (url is not None and UPSTREAM_URL_RE.fullmatch(url) is not None
                and rel != 'cache/' + url.removeprefix(UPSTREAM_URL)):
            fail(f'{rel} must be staged where cache.nixos.org names it')
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
        # Some files come from cache.nixos.org instead of the bundle (rc23).
        'thin': bool(manifest.get('thin', False)),
        'cache_files': cleaned_files,
    }


def is_upstream(item):
    return bool(item.get('url')) and UPSTREAM_URL_RE.fullmatch(item['url']) is not None


NEEDS_INTERNET = ('This update needs the internet for packages it gets from cache.nixos.org: '
                  'install it online from Settings › Updates, or put the full bundle from the '
                  'release (wros-nixos-updates.tar) on the stick.')


def missing_baseline(closure, store_dir):
    baseline = closure.get('baseline')
    if baseline is None or store_has_path(baseline, store_dir=store_dir):
        return None
    name = Path(baseline).name
    match = re.fullmatch(
        r'nixos-system-.+-([0-9]+(?:\.[0-9]+)+[A-Za-z0-9.+-]*)', name.partition('-')[2])
    return match[1].replace('-rc', '~rc', 1) if match else name


def require_baseline(closure, store_dir):
    needs = missing_baseline(closure, store_dir)
    if needs is not None:
        fail(f'This update needs WrOS {needs} first')


def verify_closure_archives(bundle_root, closure, *, store_dir=Path('/nix/store'), allow_skipped_present=False,
                            allow_upstream_missing=False):
    """Verify every binary-cache file (.narinfo, .nar.zst, nix-cache-info) against the signed manifest BEFORE import."""
    bundle_root = Path(bundle_root)
    if bundle_root.is_symlink() or not bundle_root.is_dir():
        fail('update bundle directory must be a regular directory')
    require_baseline(closure, store_dir)
    expected_rels = set()
    for item in closure['cache_files']:
        rel = item['path']
        file_path = bundle_root / rel
        sp = item.get('store_path')
        if not file_path.exists():
            if allow_skipped_present and sp and sp != closure['toplevel'] and store_has_path(sp, store_dir=store_dir):
                continue
            if is_upstream(item):
                if allow_upstream_missing:
                    # The Pages channel of a thin bundle: the laptop fetches these itself.
                    continue
                fail(NEEDS_INTERNET)
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
                             allow_skipped_present=False, allow_upstream_missing=False):
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
        bundle_dir, closure, store_dir=store_dir, allow_skipped_present=allow_skipped_present,
        allow_upstream_missing=allow_upstream_missing)
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
                  allow_skipped_present=False, allow_upstream_missing=False):
    raw = verify_open_nixos_bundle(
        bundle_dir, keyring=keyring, signers=signers, now=now, last_applied=last_applied,
        suites=suites, version_file=version_file, current_system=current_system,
        store_dir=store_dir, package_keyring=package_keyring,
        allow_skipped_present=allow_skipped_present,
        allow_upstream_missing=allow_upstream_missing)
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
    # 2. Import through Nix (works against the read-only /nix/store mount). Only the paths whose
    #    .narinfo was staged, and --no-recursive: staging (write_nixos_tree) and delta bundles both
    #    leave out the files of paths this machine already has, and a recursive copy walks the
    #    closure in the *source* cache, so it stopped at the first such path ("path … is not
    #    valid"). The paths left out are valid here, which is all an import of the rest needs.
    wanted = sorted({
        item['store_path'] for item in closure['cache_files']
        if item.get('store_path') and item['path'].endswith('.narinfo')
        and (staged / item['path']).is_file()
    } | {closure['toplevel']})
    res = run([
        'nix', '--extra-experimental-features', 'nix-command',
        'copy', '--from', cache_uri, '--no-check-sigs', '--no-recursive', *wanted,
    ])
    forget_valid_store_paths()
    if res.returncode != 0:
        fail(f'failed to import verified closure {closure["toplevel"]} from staged binary cache: '
             f'{_tail(res)}')
    res2 = run(['nix-store', '--verify-path', closure['toplevel']])
    if res2.returncode != 0:
        fail(f'nix-store --verify-path failed after importing {closure["toplevel"]}')


def switch_system_generation(toplevel, *, profile=DEFAULT_PROFILE, live=False, run=None,
                             generation=None):
    """Set the system profile generation and activate its boot entries (or switch live).
    With `generation`, go back to that existing generation the way NixOS itself rolls back
    (nix-env --switch-generation), rather than adding a new generation that repeats it."""
    run = run or _base.default_runner
    if generation is None:
        res = run(['nix-env', '-p', str(profile), '--set', str(toplevel)])
    else:
        res = run(['nix-env', '-p', str(profile), '--switch-generation', str(int(generation))])
    if res.returncode != 0:
        fail(f'failed to switch system profile to new generation: {_tail(res)}')
    verb = 'switch' if live else 'boot'
    res2 = run([str(Path(toplevel) / 'bin/switch-to-configuration'), verb])
    if res2.returncode != 0:
        fail(f'switch-to-configuration {verb} failed: {_tail(res2)}')


GENERATION_FORMAT = 'wros-nixos-generation-v1'
GENERATION_VERSION_RE = re.compile(r'[0-9][0-9A-Za-z.+~-]{0,63}')


def system_generations(profile=DEFAULT_PROFILE):
    """{number: toplevel} for the profile's generation links (<profile>-<n>-link), and the
    number of the current one (None when the profile is not a generation link)."""
    profile = Path(profile)
    pattern = re.compile(rf'{re.escape(profile.name)}-([0-9]+)-link')
    generations = {}
    try:
        entries = list(profile.parent.iterdir())
    except OSError:
        return {}, None
    for link in entries:
        found = pattern.fullmatch(link.name)
        if found and link.is_symlink():
            target = os.readlink(link)
            if STORE_PATH_RE.fullmatch(target):
                generations[int(found.group(1))] = target
    try:
        current = pattern.fullmatch(os.readlink(profile))
    except OSError:
        current = None
    return generations, (int(current.group(1)) if current else None)


def generation_version(toplevel, store_dir=REAL_STORE_DIR):
    """The WrOS version a system generation carries in its own /etc/wros/version."""
    # etc/wros/version is a symlink into the (immutable, root-owned) store, so it is followed.
    try:
        with open(Path(store_dir) / Path(toplevel).name / 'etc/wros/version', 'rb') as handle:
            version = handle.read(4096).decode('utf-8').strip()
    except (OSError, ValueError):
        return None
    return version if version and GENERATION_VERSION_RE.fullmatch(version) else None


def previous_generation(profile=DEFAULT_PROFILE, store_dir=REAL_STORE_DIR):
    """What going back installs on NixOS: the newest generation older than the current one that
    is a different system and still in the store, as (number, toplevel, version), else None.
    NixOS keeps it anyway (and GRUB lists it), so the first update can go back to the version
    the machine was installed with; no kept update tree is needed."""
    generations, current = system_generations(profile)
    if current is None or current not in generations:
        return None
    current_version = generation_version(generations[current], store_dir=store_dir)
    for number in sorted(generations, reverse=True):
        top = generations[number]
        if number >= current or top == generations[current]:
            continue
        if not store_has_path(top, store_dir=store_dir):
            continue
        version = generation_version(top, store_dir=store_dir)
        # Going back means an older version; a generation of the same version is not offered.
        if version is not None and (current_version is None
                                    or compare_versions(version, 'lt', current_version)):
            return number, top, version
    return None


def _tail(res, limit=400):
    """The end of a failed command's error output, for the failure record (it was discarded)."""
    text = (getattr(res, 'stderr', '') or getattr(res, 'stdout', '') or '').strip()
    return text[-limit:] if text else f'exit status {res.returncode}'


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
            if staged_format(store.staged) == GENERATION_FORMAT:
                details = revalidate_generation_rollback(store.staged, profile=profile, store_dir=store_dir)
            else:
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
            switch_system_generation(manifest['toplevel'], profile=profile, live=live, run=run,
                                     generation=manifest.get('generation'))
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


def staged_format(staged_dir):
    try:
        manifest = json.loads(read_regular_bytes(Path(staged_dir) / 'manifest.json', 262144).decode('utf-8'))
    except (ValueError, OSError):
        return None
    return manifest.get('format') if isinstance(manifest, dict) else None


def revalidate_generation_rollback(staged_dir, *, profile=DEFAULT_PROFILE, store_dir=REAL_STORE_DIR):
    """A staged go-back to an existing system generation. Nothing was downloaded, so there is no
    signature to re-check: the target must still be that generation of the root-owned system
    profile, and still in the store."""
    staged = Path(staged_dir)
    require_private_entry(staged, 'dir')
    require_private_entry(staged / 'manifest.json', 'file')
    manifest = json.loads(read_regular_bytes(staged / 'manifest.json', 262144).decode('utf-8'))
    if (not isinstance(manifest, dict) or manifest.get('version') != MANIFEST_VERSION
            or manifest.get('format') != GENERATION_FORMAT or manifest.get('rollback') is not True):
        fail('invalid staged go-back manifest')
    tx_id = manifest.get('transaction_id')
    if not isinstance(tx_id, str) or HEX32_RE.fullmatch(tx_id) is None:
        fail('invalid staged update transaction id')
    number = manifest.get('generation')
    generations, _ = system_generations(profile)
    if not isinstance(number, int) or generations.get(number) != manifest.get('toplevel'):
        fail('the generation to go back to is no longer in the system profile')
    if not store_has_path(manifest['toplevel'], store_dir=store_dir):
        fail('previous generation closure is not present in /nix/store')
    return {'manifest': manifest, 'tx_id': tx_id, 'closure': {'toplevel': manifest['toplevel']},
            'release': None, 'fingerprint': None, 'rollback': True}


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
                   gcroots_dir=DEFAULT_GCROOTS_DIR, profile=DEFAULT_PROFILE, **_unused):
    store = Store(state_dir)
    with store.exclusive():
        return stage_rollback_locked(
            store, keyring=keyring, signers=signers, trigger_path=Path(trigger_path),
            when=clock(now), version_file=version_file, current_system=current_system,
            store_dir=store_dir, gcroots_dir=gcroots_dir, automatic=False, profile=profile)


def stage_rollback_locked(store, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                          trigger_path=DEFAULT_TRIGGER, when=None,
                          version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                          store_dir=Path('/nix/store'), gcroots_dir=DEFAULT_GCROOTS_DIR,
                          automatic=False, profile=DEFAULT_PROFILE):
    when = clock(when)
    if os.path.lexists(trigger_path):
        fail('another offline update is already pending; original preserved')
    clear_partials(store)
    records = store.records()
    if records and records[-1]['state'] == 'applying':
        fail('an interrupted update transaction must be reconciled before going back')
    # NixOS keeps the previous system generation (and GRUB lists it): going back is going to it.
    # The signed tree kept from an earlier update (rc13's mechanism) is only the fallback for a
    # profile without generations.
    target = previous_generation(profile, store_dir=store_dir)
    if target is not None:
        return stage_generation_rollback(
            store, target, trigger_path=Path(trigger_path), when=when, version_file=version_file,
            current_system=current_system, gcroots_dir=gcroots_dir, automatic=automatic)
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


def stage_generation_rollback(store, target, *, trigger_path, when, version_file, current_system,
                              gcroots_dir, automatic):
    """Stage going back to an existing system generation; applied in the same offline boot as an
    update (so cancel, probation and the recovery menu work unchanged), by
    nix-env --switch-generation."""
    number, prev_top, prev_ver = target
    have_ver = current_installed_version(version_file=version_file, current_system=current_system)
    if have_ver is not None and not compare_versions(prev_ver, 'lt', have_ver):
        fail('the previous version is not older than the installed one')
    tx_id = secrets.token_hex(16)
    review_id = hashlib.sha256(f'generation {number} {prev_top}'.encode('utf-8')).hexdigest()
    partial = store.dir / f'staged.{tx_id}.partial'
    try:
        partial.mkdir(mode=0o700)
        json_write(partial / 'manifest.json', {
            'version': MANIFEST_VERSION,
            'format': GENERATION_FORMAT,
            'verified': True,
            'transaction_id': tx_id,
            'review_id': review_id,
            'recorded_at_utc': when.isoformat(),
            'toplevel': prev_top,
            'generation': number,
            'wros_version': prev_ver,
            'packages': [{'name': 'wros', 'version': prev_ver}],
            'rollback': True,
            'automatic': automatic,
            'rollback_from': [f'wros {have_ver}'] if have_ver else [],
            'live_apply': False,
        })
        if os.path.lexists(store.staged):
            remove_staged(store)
        os.rename(partial, store.staged)
        fsync_dir(store.dir)
    except BaseException:
        shutil.rmtree(partial, ignore_errors=True)
        raise
    store.append('staged', tx_id, when, packages=[f'wros {prev_ver}'], review_id=review_id,
                 rollback=True, automatic=automatic, toplevel=prev_top, generation=number)
    os.symlink(str(store.staged), trigger_path)
    fsync_dir(Path(trigger_path).parent)
    update_gc_roots(store, gcroots_dir=gcroots_dir, current_system=current_system)
    store.publish(trigger_path)
    return {
        'status': 'staged',
        'transaction_id': tx_id,
        'review_id': review_id,
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
            # Rewrite the boot entries for the profile's current generation. Not /run/current-system:
            # in the recovery menu that is the recovery specialisation, and setting the profile to
            # it made every later normal boot start the recovery configuration. And no new
            # generation: re-activating the current one is NixOS's own repair, and a duplicate
            # generation looked like something to go back to.
            generations, current = system_generations(profile)
            profile_top = generations.get(current) if current is not None else None
            target_top = _manifest_toplevel(store.installed / 'manifest.json') or profile_top
            # The store's database and paths, then the contents of what will boot. Only that closure
            # is hashed: the whole store is slow on the laptop's eMMC, and a path an older
            # generation kept (one an installer's chroot once wrote bytecode into) must not stop
            # repairing the system that boots.
            res = run(['nix-store', '--verify'])
            if res.returncode == 0:
                closure = []
                if target_top:
                    query = run(['nix-store', '--query', '--requisites', target_top])
                    closure = (query.stdout or '').split() if query.returncode == 0 else []
                    closure = closure or [target_top]
                res = (run(['nix-store', '--verify-path', *closure]) if closure
                       else run(['nix-store', '--verify', '--check-contents']))
            if res.returncode != 0:
                fail('Nix store integrity check failed; re-stage a signed update bundle to restore missing paths'
                     f' ({_tail(res)})')
            if target_top and target_top == profile_top:
                res = run([str(Path(target_top) / 'bin/switch-to-configuration'), 'boot'])
                if res.returncode != 0:
                    fail(f'switch-to-configuration boot failed: {_tail(res)}')
            elif target_top:
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
                can_go_back = (previous_generation(profile, store_dir=store_dir) is not None
                               or store.previous.is_dir())
                if probation['failed_logins'] >= PROBATION_FAILED_LOGINS and can_go_back:
                    end_probation(store)
                    stage_rollback_locked(
                        store, keyring=keyring, signers=signers, trigger_path=Path(trigger_path),
                        when=when, version_file=version_file, current_system=current_system,
                        store_dir=store_dir, gcroots_dir=gcroots_dir, automatic=True, profile=profile)
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
        'Each update installs a complete new WrOS beside the one you have and keeps that one in the '
        'start menu, so you can go back. Make an encrypted project backup before major changes.'
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
        'status': f'Root filesystem: {root_fs} (each update keeps the version before it).',
    }


def check_online(*, base_url=ONLINE_URL, now=None, keyring=DEFAULT_KEYRING,
                 signers=DEFAULT_SIGNERS, version_file=DEFAULT_VERSION_FILE,
                 current_system=DEFAULT_CURRENT_SYSTEM, store_dir=REAL_STORE_DIR,
                 package_keyring=None, opener=None, **_unused):
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
    if newer and (needs := missing_baseline(closure, store_dir)) is not None:
        return result | {'status': 'needs-earlier-update', 'version': closure['version'], 'needs': needs}
    return result | {'status': 'available' if newer else 'current', 'version': closure['version']}


FETCH_PROGRESS_NAME = 'progress.json'
FETCH_TRIES = 4
RETRY_PAUSE = 5  # seconds, doubled after each failed try
FETCH_KEPT = 'Downloaded files are kept so the next try can continue.'
FETCH_LOCK_NAME = '.fetch.lock'


class FetchProgress:
    """How far a download got, for Settings to show (#167): cache/progress.json, replaced
    atomically at most twice a second. The cache directory is root's (0755), so the writer
    can read it but not plant one. Failing to write it never fails the download."""

    def __init__(self, cache, version, total):
        self.path = Path(cache) / FETCH_PROGRESS_NAME
        self.version, self.total, self.done = version, total, 0
        self.written = 0.0

    def write(self, step, force=False):
        now = time.monotonic()
        if not force and now - self.written < 0.5:
            return
        self.written = now
        record = {'step': step, 'version': self.version, 'done': self.done, 'total': self.total}
        tmp = self.path.with_name('.' + FETCH_PROGRESS_NAME + '.tmp')
        try:
            tmp.write_text(json.dumps(record), encoding='utf-8')
            os.chmod(tmp, 0o644)
            os.replace(tmp, self.path)
        except OSError:
            pass

    def add(self, size):
        self.done += size
        self.write('download')

    @staticmethod
    def clear(cache):
        try:
            (Path(cache) / FETCH_PROGRESS_NAME).unlink()
        except OSError:
            pass


def _download_matches(path, expected_sha256):
    with regular_stream(path) as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest() == expected_sha256


def _saved_download_size(path, size, sha256, *, allow_short=False):
    try:
        info = path.lstat()
    except FileNotFoundError:
        return 0
    if (stat.S_ISREG(info.st_mode) and info.st_uid == TRUSTED_UID
            and not info.st_mode & 0o022 and info.st_nlink == 1):
        if ((allow_short and info.st_size < size)
                or (info.st_size == size and _download_matches(path, sha256))):
            return info.st_size
    path.unlink()
    return 0


def _stream_download(url, part, offset, size, opener, timeout, report):
    request = urllib.request.Request(url, headers={'Range': f'bytes={offset}-'}) if offset else url
    with opener.open(request, timeout=timeout) as response:
        status = getattr(response, 'status', 200)
        if status == 200:
            offset = 0  # The response already carries the whole file.
        elif not (offset and status == 206 and re.fullmatch(
                rf'bytes {offset}-[0-9]+/(?:[0-9]+|\*)', response.headers.get('Content-Range', ''))):
            if offset:
                part.unlink(missing_ok=True)
            raise urllib.error.URLError(f'HTTP {status}')
        flags = os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | (os.O_APPEND if offset else os.O_TRUNC)
        with os.fdopen(os.open(part, flags, 0o666), 'wb') as out_f:
            while (chunk := response.read(65536)):
                offset += len(chunk)
                if offset > size or offset > ARCHIVE_LIMIT:
                    return offset
                out_f.write(chunk)
                report(offset)
            out_f.flush()
            os.fsync(out_f.fileno())
    return offset


def http_stream_to_file(url, dest_path, expected_size, expected_sha256, opener=None, timeout=120,
                        on_chunk=None):
    """Keep short downloads and publish only a complete, verified file."""
    opener = opener or urllib.request.build_opener(_base._NoRedirect)
    dest_path = Path(dest_path)
    dest_path.parent.mkdir(parents=True, mode=0o755, exist_ok=True)
    part = dest_path.with_name(dest_path.name + '.part')
    offset = _saved_download_size(part, expected_size, expected_sha256, allow_short=True)
    reported = offset

    def report(size):
        nonlocal reported
        if size > reported:
            if on_chunk is not None:
                on_chunk(size - reported)
            reported = size

    pause = RETRY_PAUSE
    for attempt in range(1, FETCH_TRIES + 1):
        if offset == expected_size:
            os.replace(part, dest_path)
            return
        try:
            total = _stream_download(url, part, offset, expected_size, opener, timeout, report)
        except urllib.error.HTTPError as error:
            error.close()
            if error.code == 404:
                raise
            if offset:
                part.unlink(missing_ok=True)
            elif not 500 <= error.code < 600:
                raise
            if attempt == FETCH_TRIES:
                error.msg = f'{error.msg}. {FETCH_KEPT}'
                raise
        except (urllib.error.URLError, ConnectionError, TimeoutError, ssl.SSLError,
                http.client.HTTPException) as error:
            if attempt == FETCH_TRIES:
                reason = str(getattr(error, 'reason', error)) or type(error).__name__
                raise urllib.error.URLError(f'{reason}. {FETCH_KEPT}') from error
        else:
            if total == expected_size and _download_matches(part, expected_sha256):
                os.replace(part, dest_path)
                return
            if total >= expected_size:
                part.unlink()
            if attempt == FETCH_TRIES:
                problem = ('exceeds expected size' if total > expected_size or total > ARCHIVE_LIMIT
                           else 'failed SHA-256 or size check')
                fail(f'downloaded file {dest_path.name} {problem}. {FETCH_KEPT}')
        time.sleep(pause)
        pause *= 2
        offset = _saved_download_size(part, expected_size, expected_sha256, allow_short=True)


def _make_fetch_dir(path, *, parents=False):
    try:
        path.mkdir(parents=parents, mode=0o755)
    except FileExistsError:
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != TRUSTED_UID or info.st_mode & 0o022:
            fail(f'update download directory must be a root-owned directory: {path.name}')


def fetch_online(*, base_url=ONLINE_URL, cache=ONLINE_CACHE, now=None,
                 keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                 version_file=DEFAULT_VERSION_FILE, current_system=DEFAULT_CURRENT_SYSTEM,
                 store_dir=Path("/nix/store"), package_keyring=None, opener=None,
                 upstream_file=DEFAULT_UPSTREAM_FILE, **_unused):
    previous = os.umask(0o022)
    keyring = resolve_trust_anchor(keyring)
    signers = resolve_trust_anchor(signers)
    cache = Path(cache)
    lock = None
    try:
        # One fetch at a time: every attempt for a release shares one partial folder (#179).
        _make_fetch_dir(cache, parents=True)
        lock = os.open(cache / FETCH_LOCK_NAME, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o644)
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(lock)
            lock = None
            fail('Another WrOS update download is already running.')
        FetchProgress.clear(cache)  # a killed earlier fetch may have left one
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
        require_baseline(closure, store_dir)
        target = cache / hashlib.sha256(inrelease).hexdigest()[:16]
        partial = cache / ('.partial-' + target.name)
        for old in cache.glob('.partial-*'):
            if old != partial and stat.S_ISDIR(old.lstat().st_mode):
                shutil.rmtree(old)
        for directory in (partial, partial / 'cache', partial / 'cache/nar'):
            _make_fetch_dir(directory)

        total = done = 0
        to_download = []
        for item in closure['cache_files']:
            path = partial / item['path']
            part = path.with_name(path.name + '.part')
            sp = item['store_path']
            if sp and sp != closure['toplevel'] and store_has_path(sp, store_dir=store_dir):
                path.unlink(missing_ok=True)
                part.unlink(missing_ok=True)
                continue
            size, sha256 = item['size'], item['sha256']
            total += size
            if _saved_download_size(path, size, sha256) == size:
                done += size
                part.unlink(missing_ok=True)
            else:
                done += _saved_download_size(part, size, sha256, allow_short=True)
                to_download.append(item)

        require_free_space(cache, total - done)
        progress = FetchProgress(cache, closure['version'], total)
        progress.done = done
        progress.write('download', force=True)
        atomic_write(partial / 'InRelease', inrelease, mode=0o644)
        atomic_write(partial / CLOSURE_MANIFEST_NAME, manifest_raw, mode=0o644)
        mirror = upstream_base(upstream_file)
        for item in to_download:
            upstream = is_upstream(item)
            if upstream:
                file_url = mirror + item['url'].removeprefix(UPSTREAM_URL)
            else:
                file_url = item.get('url') or (base_url + item['path'])
            try:
                http_stream_to_file(
                    file_url, partial / item['path'], item['size'], item['sha256'],
                    opener=opener, timeout=120, on_chunk=progress.add)
            except urllib.error.HTTPError as error:
                if not upstream:
                    raise
                package = Path(item['store_path'] or item['path']).name
                if error.code == 404:
                    fail(f'cache.nixos.org no longer has {package}, which this update '
                         'needs. Nothing was staged. Install the full bundle from the '
                         'release on a stick.')
                kept = f' {FETCH_KEPT}' if error.msg.endswith(FETCH_KEPT) else ''
                fail(f"Couldn't download {package} from cache.nixos.org (HTTP "
                     f'{error.code}). Nothing was staged; try again later.{kept}')
            except OSError as error:
                if not upstream:
                    raise
                package = Path(item['store_path'] or item['path']).name
                fail(f"Couldn't reach cache.nixos.org for {package}. Nothing was staged. "
                     f"{getattr(error, 'reason', error)}")
        progress.write('verify', force=True)
        verify_bundle(partial, keyring=keyring, signers=signers, now=now,
                      version_file=version_file, current_system=current_system,
                      store_dir=store_dir, package_keyring=package_keyring,
                      allow_skipped_present=True)
        if target.exists():
            shutil.rmtree(target)
        os.replace(partial, target)
        return {'status': 'fetched', 'path': str(target), 'version': closure['version']}
    finally:
        if lock is not None:
            FetchProgress.clear(cache)
            os.close(lock)  # closing drops the flock
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
