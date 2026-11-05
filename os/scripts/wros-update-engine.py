"""WrOS update engine: authenticated offline bundles, crash-consistent staging, apply and repair.

Imported (never executed directly) by the thin entry points in /usr/libexec:
wros-update (unprivileged verbs), wros-update-stage / -cancel / -repair / -media
(pkexec launchers) and the two systemd roles.  Trust comes only from the WrOS
archive keyring plus a pinned primary-key fingerprint list; the Debian archive
keyring is never consulted.  APT stays the solver and transaction engine; this
module authenticates what APT is allowed to install and keeps an append-only
journal so a crash at any point leaves a state that reconcile/repair can name.

Test seams are explicit keyword arguments (paths, runner, clock); the launchers
and verbs never expose them.
"""
from contextlib import ExitStack, contextmanager
from datetime import datetime, timedelta, timezone
from email.utils import parsedate_to_datetime
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import ssl
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request

LIMIT = 16 * 1024 * 1024
ARCHIVE_LIMIT = 1024 * 1024 * 1024
DEFAULT_KEYRING = Path('/usr/share/keyrings/wros-archive-keyring.gpg')
DEFAULT_SIGNERS = Path('/usr/share/wros/update-signers')
PACKAGE_KEYRING = Path('/usr/share/wros/update-keyring.gpg')
DEFAULT_STATE_DIR = Path('/var/lib/wros-update')
DEFAULT_TRIGGER = Path('/system-update')
DEFAULT_DPKG_DIR = Path('/var/lib/dpkg')
DEFAULT_MEDIA_ROOT = Path('/run/wros-update/media')
MEDIA_LABEL = 'WROS-UPDATE'
MEDIA_FILESYSTEMS = ('ext4', 'vfat', 'iso9660')
# Online updates are downloaded by the root fetch helper into ONLINE_CACHE, which is reviewed like media.
ONLINE_URL = 'https://j5cfk7qm49-web.github.io/WrOS/updates/'
ONLINE_CACHE = Path('/var/cache/wros-update/online')
ONLINE_TIMEOUT = 10
ALLOWED_MEDIA_PREFIXES = ('/run/wros-update/media/', '/media/', '/run/media/', '/mnt/', '/var/cache/wros-update/online/')
PINNED_ORIGIN = 'WrOS'
PINNED_SUITES = ('wros-updates',)
ACCEPTED_ARCHITECTURES = ('amd64', 'all')
MAX_VALIDITY = timedelta(days=30)
MAX_STAGED_AGE = timedelta(days=30)
CLOCK_SKEW = timedelta(minutes=5)
BOOT_CLOCK_SKEW = timedelta(hours=1)
TRUSTED_UID = 0  # tests may point this at the invoking user; production never does
SAFE_PATH = '/usr/sbin:/usr/bin:/sbin:/bin'
MANIFEST_VERSION = 2
MAX_PACKAGES = 256
JOURNAL_LIMIT = 4 * 1024 * 1024
JOURNAL_KEEP = 64
JOURNAL_COMPACT_AT = 256
JOURNAL_STATES = ('staged', 'applying', 'completed', 'failed', 'interrupted', 'recovered', 'cancelled')
UPDATE_STATES = ('idle', 'staged', 'applying', 'failed', 'recovered', 'completed', 'interrupted')
UNAVAILABLE = 'Update installation and package repair are unavailable; no system changes were made.'

PACKAGE_NAME_RE = re.compile(r'^[a-z0-9][a-z0-9+.-]{0,127}$')
VERSION_RE = re.compile(r'^[0-9][A-Za-z0-9.+:~-]{0,127}$')
ARCH_RE = re.compile(r'^[a-z0-9-]{2,32}$')
HEX64_RE = re.compile(r'^[0-9a-f]{64}$')
HEX32_RE = re.compile(r'^[0-9a-f]{32}$')
FINGERPRINT_RE = re.compile(r'^[0-9A-Fa-f]{40}$')
SAFE_REL_RE = re.compile(r'^[A-Za-z0-9._+~-]+(?:/[A-Za-z0-9._+~-]+)*\.deb$')
PKEXEC_UID_RE = re.compile(r'^[0-9]{1,10}$')
BUNDLE_COMPONENT_RE = re.compile(r'^[A-Za-z0-9._+@=:,~-](?:[A-Za-z0-9._+@=:,~ -]{0,253}[A-Za-z0-9._+@=:,~-])?$')
DEVICE_RE = re.compile(r'^/dev/[A-Za-z0-9][A-Za-z0-9/_.-]{0,63}$')
DEP_TOKEN_RE = re.compile(
    r'^\s*([a-z0-9][a-z0-9+.-]{0,127})(?::[a-z0-9-]+)?'
    r'(?:\s*\(\s*(<<|<=|=|>=|>>)\s*([0-9][A-Za-z0-9.+:~-]{0,127})\s*\))?\s*$'
)
REL_TO_DPKG = {'<<': 'lt', '<=': 'le', '=': 'eq', '>=': 'ge', '>>': 'gt'}
BAD_SIGNATURE_STATUS = ('BADSIG', 'EXPSIG', 'EXPKEYSIG', 'REVKEYSIG', 'ERRSIG')


def fail(message):
    raise ValueError(message)


class StateBusy(ValueError):
    """Another update operation owns the state lock."""


def clock(now=None):
    return now if now is not None else datetime.now(timezone.utc)


def clean_text(value, limit=500):
    return re.sub(r'[\x00-\x1f\x7f]', ' ', str(value))[:limit]


def safe_env(extra=None):
    env = {'PATH': SAFE_PATH, 'LC_ALL': 'C'}
    if extra:
        env.update(extra)
    return env


# ---------------------------------------------------------------- file helpers

def fsync_dir(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def atomic_write(path, data, mode=0o600, dir_mode=0o700):
    path = Path(path)
    path.parent.mkdir(parents=True, mode=dir_mode, exist_ok=True)
    temporary = path.with_name(f'.{path.name}.{secrets.token_hex(8)}.tmp')
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
    try:
        with os.fdopen(descriptor, 'wb') as output:
            output.write(data)
            output.flush()
            os.fchmod(output.fileno(), mode)
            os.fsync(output.fileno())
        os.replace(temporary, path)
        fsync_dir(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode('utf-8')


def json_write(path, value, mode=0o600, dir_mode=0o700):
    atomic_write(path, json_bytes(value), mode, dir_mode)


def regular_stream(path):
    # No blocking FIFOs or final-component symlink races in caller-owned media.
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            fail(f'update input is not a regular file: {Path(path).name}')
        return os.fdopen(descriptor, 'rb')
    except BaseException:
        os.close(descriptor)
        raise


def read_regular_bytes(path, limit=LIMIT):
    path = Path(path)
    try:
        with regular_stream(path) as stream:
            data = stream.read(limit + 1)
    except OSError:
        fail(f'missing or symlinked update file: {path.name}')
    if len(data) > limit:
        fail(f'oversized update file: {path.name}')
    return data


def sha256_file(path):
    path = Path(path)
    digest = hashlib.sha256()
    size = 0
    try:
        with regular_stream(path) as stream:
            while chunk := stream.read(65536):
                size += len(chunk)
                if size > ARCHIVE_LIMIT:
                    fail(f'update archive exceeds 1 GiB limit: {path.name}')
                digest.update(chunk)
    except OSError:
        fail(f'missing or symlinked archive: {path.name}')
    return digest.hexdigest(), size


def require_private_entry(path, kind):
    """Staged state must be owned by the trusted uid and not group/other writable."""
    path = Path(path)
    try:
        info = os.lstat(path)
    except OSError:
        fail(f'missing staged update entry: {path.name}')
    wanted = stat.S_ISDIR if kind == 'dir' else stat.S_ISREG
    if not wanted(info.st_mode):
        fail(f'staged update entry has the wrong type: {path.name}')
    if info.st_uid != TRUSTED_UID or info.st_mode & 0o022:
        fail(f'staged update entry is not root-owned and private: {path.name}')


def require_trusted_file(path, what, limit=None):
    """Keyring and pin files: regular, no symlink, trusted owner, not group/other writable."""
    path = Path(path)
    try:
        info = os.lstat(path)
    except OSError:
        fail(f'missing {what}: {path}')
    if not stat.S_ISREG(info.st_mode):
        fail(f'{what} must be a regular non-symlink file: {path}')
    if info.st_uid != TRUSTED_UID or info.st_mode & 0o022:
        fail(f'{what} must be root-owned and not group/other writable: {path}')
    if limit is not None and info.st_size > limit:
        fail(f'oversized {what}')


def host_architecture():
    process = subprocess.Popen(['/usr/bin/dpkg', '--print-architecture'], stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=safe_env())
    output, _ = process.communicate(timeout=10)
    arch = output.decode('ascii', 'replace').strip()
    if process.returncode != 0 or ARCH_RE.fullmatch(arch) is None:
        fail('cannot determine the host dpkg architecture')
    return arch


# ------------------------------------------------------------ bundle directory

class BundleDir:
    """A bundle directory held open by descriptor; every read is O_NOFOLLOW below it."""

    def __init__(self, path, strict=False):
        path = Path(path)
        message = 'update bundle path must be an existing non-symlink directory'
        if not path.is_absolute() or '..' in path.parts:
            fail(message)
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
        try:
            if strict:
                descriptor = os.open('/', os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
                for part in path.parts[1:]:
                    following = os.open(part, flags, dir_fd=descriptor)
                    os.close(descriptor)
                    descriptor = following
            else:
                descriptor = os.open(path, flags)
        except OSError:
            fail(message)
        self.fd = descriptor
        self.path = path

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    def _directory_fd(self, parts):
        descriptor = os.dup(self.fd)
        try:
            for part in parts:
                following = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=descriptor)
                os.close(descriptor)
                descriptor = following
            return descriptor
        except OSError:
            os.close(descriptor)
            fail(f'symlinked directory in update bundle: {"/".join(parts)}')

    def open_regular(self, rel):
        parts = rel.split('/')
        if any(part in ('', '.', '..') for part in parts):
            fail(f'unsafe path in update bundle: {rel}')
        parent = self._directory_fd(parts[:-1])
        try:
            descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=parent)
        except OSError:
            fail(f'missing or symlinked update file: {parts[-1]}')
        finally:
            os.close(parent)
        try:
            if not stat.S_ISREG(os.fstat(descriptor).st_mode):
                fail(f'update input is not a regular file: {parts[-1]}')
            return os.fdopen(descriptor, 'rb')
        except BaseException:
            os.close(descriptor)
            raise

    def exists(self, name):
        try:
            os.stat(name, dir_fd=self.fd, follow_symlinks=False)
            return True
        except FileNotFoundError:
            return False

    def read(self, rel, limit):
        with self.open_regular(rel) as stream:
            data = stream.read(limit + 1)
        if len(data) > limit:
            fail(f'oversized update file: {rel}')
        return data

    def hash(self, rel):
        digest = hashlib.sha256()
        size = 0
        with self.open_regular(rel) as stream:
            while chunk := stream.read(65536):
                size += len(chunk)
                if size > ARCHIVE_LIMIT:
                    fail(f'update archive exceeds 1 GiB limit: {rel}')
                digest.update(chunk)
        return digest.hexdigest(), size

    def copy(self, rel, destination, mode=0o600):
        """Copy one regular file to a new private destination, hashing the bytes written."""
        digest = hashlib.sha256()
        size = 0
        with self.open_regular(rel) as stream:
            descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode)
            with os.fdopen(descriptor, 'wb') as target:
                while chunk := stream.read(1024 * 1024):
                    size += len(chunk)
                    if size > ARCHIVE_LIMIT:
                        fail(f'update archive exceeds 1 GiB limit: {rel}')
                    digest.update(chunk)
                    target.write(chunk)
                target.flush()
                os.fchmod(target.fileno(), mode)
                os.fsync(target.fileno())
        return digest.hexdigest(), size

    def list_archives(self):
        """Every directory entry named *.deb (any type), so symlinks and strays are seen."""
        found = []
        pending = [((), os.dup(self.fd))]
        entries = 0
        while pending:
            prefix, descriptor = pending.pop()
            try:
                with os.scandir(descriptor) as iterator:
                    for entry in iterator:
                        entries += 1
                        if entries > 4096 or len(prefix) > 8:
                            fail('update bundle has too many entries')
                        rel = '/'.join((*prefix, entry.name))
                        if entry.name.endswith('.deb'):
                            found.append(rel)
                        if entry.is_dir(follow_symlinks=False):
                            try:
                                child = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                                                dir_fd=descriptor)
                            except PermissionError:
                                # mkfs.ext4 puts a root-only lost+found on every ext4 stick. The
                                # writer's review cannot read it; staging re-verifies as root, which
                                # can, so a .deb hidden there is still refused before anything stages.
                                if prefix == () and entry.name == 'lost+found':
                                    continue
                                fail(f'unreadable directory in update bundle: {rel}')
                            except OSError:
                                fail(f'symlinked directory in update bundle: {rel}')
                            pending.append(((*prefix, entry.name), child))
            finally:
                os.close(descriptor)
        return found


# -------------------------------------------------------------- version policy

def compare_dpkg_versions(left, op, right):
    if VERSION_RE.fullmatch(left) is None or VERSION_RE.fullmatch(right) is None:
        fail('invalid Debian package version for comparison')
    if op not in ('lt', 'le', 'eq', 'ne', 'ge', 'gt'):
        fail(f'invalid dpkg version operator: {op}')
    res = subprocess.run(
        ['/usr/bin/dpkg', '--compare-versions', left, op, right],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=5,
        env=safe_env(),
    )
    return res.returncode == 0


def parse_dependency_field(field_text):
    """Parse a Debian Depends/Pre-Depends/Breaks/Conflicts field into OR-clauses."""
    if not field_text or not field_text.strip():
        return []
    clauses = []
    for raw_clause in field_text.split(','):
        raw_clause = raw_clause.strip()
        if not raw_clause:
            continue
        alternatives = []
        for raw_alt in raw_clause.split('|'):
            match = DEP_TOKEN_RE.fullmatch(raw_alt)
            if match is None:
                fail(f'invalid Debian dependency expression: {raw_alt!r}')
            pkg_name, rel, ver = match.groups()
            alternatives.append((pkg_name, rel, ver))
        clauses.append(alternatives)
    return clauses


# ------------------------------------------------------------ signed metadata

def extract_inrelease_payload(raw_bytes):
    text = raw_bytes.decode('utf-8', errors='strict')
    if '\r' in text:
        fail('InRelease must use LF line endings')
    lines = text.splitlines()
    if not lines or lines[0] != '-----BEGIN PGP SIGNED MESSAGE-----':
        fail('invalid InRelease cleartext header')
    idx = 1
    while idx < len(lines) and lines[idx] != '':
        idx += 1
    if idx >= len(lines):
        fail('InRelease missing header separator')
    idx += 1
    body = []
    found_sig = False
    while idx < len(lines):
        line = lines[idx]
        if line == '-----BEGIN PGP SIGNATURE-----':
            found_sig = True
            break
        if line.startswith('- '):
            line = line[2:]
        # Cleartext signatures ignore trailing blanks; parse exactly what was signed.
        body.append(line.rstrip(' \t'))
        idx += 1
    if not found_sig or not body:
        fail('InRelease missing PGP signature block')
    return '\n'.join(body) + '\n'


def load_pinned_signers(path=DEFAULT_SIGNERS):
    """One 40-hex PRIMARY key fingerprint per line; # comments; empty or missing refuses."""
    path = Path(path)
    require_trusted_file(path, 'pinned update signer list', 65536)
    pins = set()
    for raw in read_regular_bytes(path, 65536).decode('utf-8', errors='strict').splitlines():
        line = raw.split('#', 1)[0].strip()
        if not line:
            continue
        if FINGERPRINT_RE.fullmatch(line) is None:
            fail('pinned update signer list contains a line that is not a 40-hex fingerprint')
        pins.add(line.upper())
    if not pins:
        fail('pinned update signer list is empty; no update can be authenticated')
    return pins


def keyring_primary_fingerprints(data):
    """V4 primary-key fingerprints in a binary OpenPGP keyring, in order. gpg is not
    installed on WrOS (only gpgv), so the packets are walked here."""
    primaries, index = [], 0
    while index < len(data):
        header = data[index]
        if not header & 0x80:
            fail('update keyring is not a binary OpenPGP keyring')
        if header & 0x40:  # new format
            tag = header & 0x3F
            if index + 1 >= len(data):
                fail('truncated update keyring')
            first = data[index + 1]
            if first < 192:
                length, index = first, index + 2
            elif first < 224:
                if index + 2 >= len(data):
                    fail('truncated update keyring')
                length, index = ((first - 192) << 8) + data[index + 2] + 192, index + 3
            elif first == 255:
                if index + 5 >= len(data):
                    fail('truncated update keyring')
                length, index = int.from_bytes(data[index + 2:index + 6], 'big'), index + 6
            else:
                fail('partial-length OpenPGP packets are not supported in the update keyring')
        else:  # old format
            tag, kind = (header >> 2) & 0x0F, header & 0x03
            if kind == 3:
                fail('indeterminate-length OpenPGP packets are not supported in the update keyring')
            size = (1, 2, 4)[kind]
            if index + size >= len(data):
                fail('truncated update keyring')
            length, index = int.from_bytes(data[index + 1:index + 1 + size], 'big'), index + 1 + size
        if index + length > len(data):
            fail('truncated update keyring')
        body = data[index:index + length]
        index += length
        if tag in (5, 7):
            fail('update keyring contains secret key material')
        if tag == 6:
            if body[:1] != b'\x04':
                fail('update keyring holds a non-v4 primary key')
            primaries.append(hashlib.sha1(b'\x99' + len(body).to_bytes(2, 'big') + body,
                                          usedforsecurity=False).hexdigest().upper())
    return primaries


def verify_signed_metadata(meta, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, package_keyring=None):
    """gpgv against the WrOS keyrings only, then compare the PRIMARY fingerprint to the pins.

    The image keyring and pin are image inputs no package can replace. A `wros`
    package may add PACKAGE_KEYRING to bring a new signing subkey, but only if every
    primary key in it is pinned. With the system keyring, the package keyring is used
    by default; callers passing their own keyring opt in explicitly.

    meta is {'InRelease': bytes} or {'Release': bytes, 'Release.gpg': bytes}; the very
    bytes verified are the bytes parsed.  Returns (primary_fingerprint, release_text).
    """
    pins = load_pinned_signers(signers)
    keyring = Path(keyring)
    require_trusted_file(keyring, 'update verification keyring')
    if package_keyring is None and keyring == DEFAULT_KEYRING:
        package_keyring = PACKAGE_KEYRING
    extra = []
    if package_keyring is not None and os.path.lexists(package_keyring):
        # A bad package keyring is set aside, never given to gpgv: it lends no trust, and an
        # update signed by the pinned primary (which could repair it) still verifies.
        package_keyring = Path(package_keyring)
        try:
            require_trusted_file(package_keyring, 'packaged update keyring')
            primaries = keyring_primary_fingerprints(package_keyring.read_bytes())
            if not primaries or set(primaries) - set(pins):
                fail('update keyring holds a primary key the signer pin does not name')
            extra = ['--keyring', str(package_keyring)]
        except (OSError, ValueError) as exc:
            print(f'wros-update: ignoring {package_keyring}: {exc}', file=sys.stderr)
    if 'InRelease' in meta:
        names = ['InRelease']
        cmd_tail = ['InRelease']
        release_text = extract_inrelease_payload(meta['InRelease'])
    else:
        names = ['Release', 'Release.gpg']
        cmd_tail = ['Release.gpg', 'Release']
        if b'\r' in meta['Release']:
            fail('Release must use LF line endings')
        release_text = meta['Release'].decode('utf-8', errors='strict')
    with tempfile.TemporaryDirectory(prefix='wros-update-metadata-') as temporary:
        directory = Path(temporary)
        for name in names:
            (directory / name).write_bytes(meta[name])
        result = subprocess.run(
            ['/usr/bin/gpgv', '--status-fd', '1', '--keyring', str(keyring), *extra, *cmd_tail],
            cwd=directory,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=10,
            env=safe_env(),
        )
    if result.returncode != 0:
        fail('GPG signature verification failed for update bundle')
    goodsig = False
    primaries = []
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) < 2 or parts[0] != '[GNUPG:]':
            continue
        if parts[1] in BAD_SIGNATURE_STATUS:
            fail('gpgv reported a bad, expired or revoked signature on update metadata')
        if parts[1] == 'GOODSIG':
            goodsig = True
        elif parts[1] == 'VALIDSIG':
            # VALIDSIG <signing-fpr> <date> <ts> <expire> <ver> <rsvd> <pk> <hash> <class> <PRIMARY-fpr>
            if len(parts) >= 12 and FINGERPRINT_RE.fullmatch(parts[-1]):
                primaries.append(parts[-1].upper())
    if not goodsig or not primaries:
        fail('gpgv did not confirm a valid signature on update metadata')
    for primary in primaries:
        if primary in pins:
            return primary, release_text
    fail('update signature is not from a pinned WrOS signing key')


def parse_release_time(raw, what):
    try:
        value = parsedate_to_datetime(raw)
    except Exception:
        fail(f'invalid {what} header in signed Release metadata')
    if value is None:
        fail(f'invalid {what} header in signed Release metadata')
    if value.tzinfo is None:
        value = value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc)


def parse_release(release_text, now=None, suites=PINNED_SUITES):
    fields = {}
    entries = {}
    in_sha256 = False
    for line in release_text.splitlines():
        if line.startswith(' '):
            if not in_sha256:
                continue
            parts = line.split()
            if len(parts) != 3:
                fail('malformed SHA256 entry in signed Release metadata')
            digest, size_str, relpath = parts
            if HEX64_RE.fullmatch(digest) is None or not size_str.isdigit():
                fail('invalid SHA256 digest or byte count in signed Release metadata')
            if relpath.startswith('/') or '..' in Path(relpath).parts:
                fail('unsafe path in signed Release metadata')
            if relpath in entries:
                fail('duplicate SHA256 entry in signed Release metadata')
            entries[relpath] = (digest, int(size_str))
            continue
        in_sha256 = False
        if not line.strip():
            continue
        key, separator, value = line.partition(':')
        if not separator:
            fail('malformed line in signed Release metadata')
        key = key.strip()
        if key == 'SHA256':
            if entries or 'SHA256' in fields:
                fail('duplicate SHA256 block in signed Release metadata')
            fields['SHA256'] = ''
            in_sha256 = True
            continue
        if key in ('Origin', 'Suite', 'Date', 'Valid-Until'):
            if key in fields:
                fail(f'duplicate {key} header in signed Release metadata')
            fields[key] = value.strip()
    if not entries:
        fail('signed Release metadata contains no SHA256 file entries')
    if fields.get('Origin') != PINNED_ORIGIN:
        fail('signed Release metadata Origin is not WrOS')
    if fields.get('Suite') not in suites:
        fail('signed Release metadata Suite is not an accepted WrOS update suite')
    if 'Date' not in fields or 'Valid-Until' not in fields:
        fail('signed Release metadata requires both Date and Valid-Until')
    reference = clock(now)
    date = parse_release_time(fields['Date'], 'Date')
    until = parse_release_time(fields['Valid-Until'], 'Valid-Until')
    if date > reference + CLOCK_SKEW:
        fail('signed Release metadata Date is in the future')
    if until <= reference:
        fail('signed Release metadata has expired (Valid-Until passed)')
    if until <= date:
        fail('signed Release metadata Valid-Until is not after Date')
    if until - date > MAX_VALIDITY:
        fail('signed Release metadata Valid-Until is more than 30 days after Date')
    return {'suite': fields['Suite'], 'origin': fields['Origin'], 'date': date,
            'valid_until': until, 'entries': entries}


def parse_packages_index(packages_text):
    if '\0' in packages_text or '\r' in packages_text:
        fail('invalid Packages index encoding')
    packages = []
    seen_names = set()
    seen_files = set()
    for raw_stanza in packages_text.strip().split('\n\n'):
        if not raw_stanza.strip():
            continue
        fields = {}
        for line in raw_stanza.splitlines():
            if line.startswith((' ', '\t')) or ':' not in line:
                continue
            key, _, value = line.partition(':')
            fields[key.strip()] = value.strip()
        name = fields.get('Package', '')
        version = fields.get('Version', '')
        arch = fields.get('Architecture', '')
        filename = fields.get('Filename', '')
        size_str = fields.get('Size', '')
        sha256 = fields.get('SHA256', '')
        if PACKAGE_NAME_RE.fullmatch(name) is None:
            fail(f'invalid Debian package name in Packages: {name!r}')
        if VERSION_RE.fullmatch(version) is None:
            fail(f'invalid Debian package version in Packages: {version!r}')
        if ARCH_RE.fullmatch(arch) is None:
            fail(f'invalid Debian package architecture in Packages: {arch!r}')
        if SAFE_REL_RE.fullmatch(filename) is None or '..' in Path(filename).parts:
            fail(f'unsafe Debian archive path in Packages: {filename!r}')
        if not size_str.isdigit() or int(size_str) <= 0:
            fail(f'invalid Debian archive size in Packages: {size_str!r}')
        if HEX64_RE.fullmatch(sha256) is None:
            fail(f'invalid Debian archive SHA256 in Packages: {sha256!r}')
        if name in seen_names or filename in seen_files:
            fail(f'duplicate package or archive in Packages: {name}')
        seen_names.add(name)
        seen_files.add(filename)
        depends = (
            parse_dependency_field(fields.get('Pre-Depends', ''))
            + parse_dependency_field(fields.get('Depends', ''))
        )
        conflicts = (
            parse_dependency_field(fields.get('Breaks', ''))
            + parse_dependency_field(fields.get('Conflicts', ''))
        )
        provides = []
        for clause in parse_dependency_field(fields.get('Provides', '')):
            for prov_name, prov_rel, prov_ver in clause:
                if prov_rel and prov_rel != '=':
                    fail(f'invalid versioned Provides in Packages: {prov_name}')
                provides.append((prov_name, prov_ver))
        packages.append({
            'name': name,
            'version': version,
            'architecture': arch,
            'filename': filename,
            'size': int(size_str),
            'sha256': sha256,
            'depends': depends,
            'conflicts': conflicts,
            'provides': provides,
        })
    if not packages:
        fail('signed Packages index contains no installable packages')
    if len(packages) > MAX_PACKAGES:
        fail('update bundle exceeds 256 package limit')
    return packages


def check_architectures(packages, host_arch=None):
    accepted = {'all', host_arch or host_architecture()}
    for pkg in packages:
        if pkg['architecture'] not in accepted:
            fail(f'package architecture {pkg["architecture"]} is not accepted on this host: {pkg["name"]}')


# ---------------------------------------------------------------- dpkg policy

def load_installed_dpkg_database(dpkg_dir=DEFAULT_DPKG_DIR):
    status_file = Path(dpkg_dir) / 'status'
    installed = {}
    if not os.path.lexists(status_file):
        return installed
    text = read_regular_bytes(status_file, LIMIT).decode('utf-8', errors='replace')
    for stanza in text.split('\n\n'):
        if not stanza.strip():
            continue
        fields = {}
        for line in stanza.splitlines():
            if line.startswith((' ', '\t')) or ':' not in line:
                continue
            key, _, val = line.partition(':')
            fields[key.strip()] = val.strip()
        name = fields.get('Package', '')
        status = fields.get('Status', '')
        version = fields.get('Version', '')
        if not name or not version:
            continue
        if status in ('install ok installed', 'hold ok installed'):
            provides = []
            for clause in parse_dependency_field(fields.get('Provides', '')):
                for prov_name, _, prov_ver in clause:
                    provides.append((prov_name, prov_ver))
            installed[name] = {
                'version': version,
                'held': status == 'hold ok installed',
                'provides': provides,
            }
    return installed


def evaluate_bundle_against_installed(packages, installed):
    """Enforce anti-downgrade, held-package protection, and dependency/conflict checks."""
    merged_versions = {name: [info['version']] for name, info in installed.items()}
    for info in installed.values():
        for prov_name, prov_ver in info['provides']:
            merged_versions.setdefault(prov_name, []).append(prov_ver)

    actions = {}
    for pkg in packages:
        name = pkg['name']
        ver = pkg['version']
        if name in installed:
            inst_ver = installed[name]['version']
            if installed[name]['held'] and not compare_dpkg_versions(ver, 'eq', inst_ver):
                fail(f'refusing to modify held package in update bundle: {name}')
            if compare_dpkg_versions(ver, 'lt', inst_ver):
                fail(f'refusing package downgrade in update bundle: {name} {ver} < installed {inst_ver}')
            actions[name] = 'reinstall' if compare_dpkg_versions(ver, 'eq', inst_ver) else 'upgrade'
        else:
            actions[name] = 'install'
        merged_versions[name] = [ver]
        for prov_name, prov_ver in pkg.get('provides', ()):
            merged_versions.setdefault(prov_name, []).append(prov_ver)

    def matches_constraint(candidate_name, rel, target_ver):
        versions = merged_versions.get(candidate_name, [])
        if not versions:
            return False
        if rel is None:
            return True
        dpkg_op = REL_TO_DPKG[rel]
        return any(
            v is not None and compare_dpkg_versions(v, dpkg_op, target_ver)
            for v in versions
        )

    for pkg in packages:
        for clause in pkg.get('depends', ()):
            if not any(matches_constraint(dep_name, rel, ver) for dep_name, rel, ver in clause):
                formatted = ' | '.join(
                    f'{d} ({r} {v})' if r else d for d, r, v in clause
                )
                fail(f'unsatisfied dependency for {pkg["name"]} {pkg["version"]}: {formatted}')
        for clause in pkg.get('conflicts', ()):
            for conf_name, rel, ver in clause:
                if conf_name != pkg['name'] and matches_constraint(conf_name, rel, ver):
                    formatted = f'{conf_name} ({rel} {ver})' if rel else conf_name
                    fail(f'package conflict for {pkg["name"]} {pkg["version"]}: {formatted}')
    return actions


# --------------------------------------------------------------- bundle verify

def check_bundle_path(value, prefixes=ALLOWED_MEDIA_PREFIXES):
    """Strict token check for writer-typed bundle paths handed to a root helper."""
    if not isinstance(value, str) or not 2 <= len(value) <= 1024 or not value.startswith('/'):
        fail('update bundle path must be an absolute path')
    for part in value.split('/')[1:]:
        if part in ('', '.', '..') or BUNDLE_COMPONENT_RE.fullmatch(part) is None:
            fail('update bundle path contains an unsupported component')
    if prefixes is not None and not any(value.startswith(prefix) and len(value) > len(prefix)
                                        for prefix in prefixes):
        fail('update bundle path is outside the allowed removable-media locations')
    return value


def read_signed_metadata(bundle):
    if bundle.exists('InRelease'):
        return {'InRelease': bundle.read('InRelease', 1024 * 1024)}
    if bundle.exists('Release') and bundle.exists('Release.gpg'):
        return {'Release': bundle.read('Release', 1024 * 1024),
                'Release.gpg': bundle.read('Release.gpg', 65536)}
    fail('update bundle requires signed InRelease or Release + Release.gpg')


def review_id_of(meta, packages_bytes):
    if 'InRelease' in meta:
        signed = meta['InRelease']
    else:
        signed = meta['Release'] + b'\0' + meta['Release.gpg']
    return hashlib.sha256(signed + b'\0' + packages_bytes).hexdigest()


def select_packages_entry(release_entries):
    for candidate in ('Packages', 'main/binary-amd64/Packages'):
        if candidate in release_entries:
            return candidate
    fail('signed Release metadata does not authenticate a Packages index in this bundle')


def verify_open_bundle(bundle, *, keyring, signers, dpkg_dir, now, host_arch, last_applied, suites,
                       package_keyring=None):
    meta = read_signed_metadata(bundle)
    fingerprint, release_text = verify_signed_metadata(meta, keyring=keyring, signers=signers,
                                                       package_keyring=package_keyring)
    release = parse_release(release_text, now=now, suites=suites)
    if last_applied is not None and release.get('date') <= last_applied:
        fail('update bundle is not newer than the last applied update (replay refused)')
    packages_rel = select_packages_entry(release['entries'])
    if not bundle.exists('Packages'):
        fail('signed Release metadata does not authenticate a Packages index in this bundle')
    expected_pkg_sha, expected_pkg_size = release['entries'][packages_rel]
    packages_raw = bundle.read('Packages', LIMIT)
    if hashlib.sha256(packages_raw).hexdigest() != expected_pkg_sha or len(packages_raw) != expected_pkg_size:
        fail('Packages index SHA-256 or byte size does not match signed Release metadata')
    packages = parse_packages_index(packages_raw.decode('utf-8', errors='strict'))
    check_architectures(packages, host_arch)
    expected_files = set()
    for pkg in packages:
        actual_sha, actual_size = bundle.hash(pkg['filename'])
        if actual_sha != pkg['sha256'] or actual_size != pkg['size']:
            fail(f'archive {pkg["filename"]} failed SHA-256 or size check against signed Packages')
        expected_files.add(pkg['filename'])
    for extra in bundle.list_archives():
        if extra not in expected_files:
            fail(f'unindexed .deb archive refused in update bundle: {Path(extra).name}')
    actions = {}
    installed = {}
    if dpkg_dir is not None:
        installed = load_installed_dpkg_database(dpkg_dir=dpkg_dir)
        actions = evaluate_bundle_against_installed(packages, installed)
    return {
        'meta': meta,
        'fingerprint': fingerprint,
        'release': release,
        'packages_rel': packages_rel,
        'packages_raw': packages_raw,
        'packages': packages,
        'actions': actions,
        'installed': installed,
        'review_id': review_id_of(meta, packages_raw),
    }


def public_view(raw):
    cleaned_packages = [
        {
            'name': p['name'],
            'version': p['version'],
            'architecture': p['architecture'],
            'filename': p['filename'],
            'size': p['size'],
            'sha256': p['sha256'],
        }
        | ({'action': raw['actions'][p['name']]} if p['name'] in raw['actions'] else {})
        for p in raw['packages']
    ]
    return {
        'verified': True,
        'signer_fingerprint': raw['fingerprint'],
        'suite': raw['release']['suite'],
        'date': raw['release']['date'].isoformat(),
        'valid_until': raw['release']['valid_until'].isoformat(),
        'review_id': raw['review_id'],
        'package_count': len(cleaned_packages),
        'packages': cleaned_packages,
    }


def verify_bundle(bundle_dir, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, dpkg_dir=None, now=None,
                  host_arch=None, last_applied=None, strict_path=False, suites=PINNED_SUITES, package_keyring=None):
    with BundleDir(bundle_dir, strict=strict_path) as bundle:
        raw = verify_open_bundle(bundle, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir, now=now,
                                 host_arch=host_arch, last_applied=last_applied, suites=suites,
                                 package_keyring=package_keyring)
    return public_view(raw)


def default_runner(command):
    return subprocess.run(
        command,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=1800,
        env=safe_env({'DEBIAN_FRONTEND': 'noninteractive'}),
    )


# ----------------------------------------------------------------- dpkg locks

def posix_dpkg_lock_active(lock_path):
    """Query POSIX record lock state (F_GETLK) used by dpkg/apt without mutating it."""
    lock_path = Path(lock_path)
    if not lock_path.is_file() or lock_path.is_symlink():
        return False, None
    fd = os.open(lock_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        query_flock = struct.pack('hhqqI', fcntl.F_WRLCK, os.SEEK_SET, 0, 0, 0)
        reply = fcntl.fcntl(fd, fcntl.F_GETLK, query_flock)
        l_type, _, _, _, l_pid = struct.unpack('hhqqI', reply)
        if l_type != fcntl.F_UNLCK:
            return True, int(l_pid)
        return False, None
    finally:
        os.close(fd)


@contextmanager
def acquire_posix_dpkg_locks(dpkg_dir=DEFAULT_DPKG_DIR):
    """Acquire exclusive POSIX F_SETLK write locks on lock-frontend and lock in order.

    Callers must leave this context before launching apt or dpkg: those take the
    same record locks themselves and a lock held here would make them fail.
    """
    dpkg_dir = Path(dpkg_dir)
    dpkg_dir.mkdir(parents=True, exist_ok=True)
    fds = []
    try:
        for lock_name in ('lock-frontend', 'lock'):
            lock_path = dpkg_dir / lock_name
            if lock_path.is_symlink():
                fail(f'unsafe symlinked dpkg lock file: {lock_name}')
            fd = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o640)
            fds.append(fd)
            lock_struct = struct.pack('hhqqI', fcntl.F_WRLCK, os.SEEK_SET, 0, 0, 0)
            try:
                fcntl.fcntl(fd, fcntl.F_SETLK, lock_struct)
            except OSError:
                fail(f'dpkg POSIX lock {lock_name} is held by another package manager process')
        yield
    finally:
        unlock_struct = struct.pack('hhqqI', fcntl.F_UNLCK, os.SEEK_SET, 0, 0, 0)
        for fd in reversed(fds):
            try:
                fcntl.fcntl(fd, fcntl.F_SETLK, unlock_struct)
            except OSError:
                pass
            os.close(fd)


def ensure_dpkg_unlocked(dpkg_dir):
    for lock_name in ('lock-frontend', 'lock'):
        locked, holder = posix_dpkg_lock_active(Path(dpkg_dir) / lock_name)
        if locked:
            fail(f'dpkg POSIX lock {lock_name} is held by PID {holder}')


def owned_trigger(trigger_path, staged_dir):
    trigger_path = Path(trigger_path)
    return trigger_path.is_symlink() and os.readlink(trigger_path) == str(Path(staged_dir))


# ----------------------------------------------------------------- APT solver

def apt_command(deb_paths, dpkg_dir, work_dir, simulate=False):
    """APT's own solver and transaction engine, offline: no sources, no downloads, no removals.

    Offline comes from an empty source list: the only archives APT can fetch are the staged
    .deb files named by absolute path, which it copies from disk. --no-download is not used
    because APT 3.0 then refuses such paths outright ("Pathname to install is not absolute"),
    which failed every real apply while the simulation still passed.

    Scratch (list state, archive dir with its lock/partial) lives in work_dir, outside the
    staged tree, so applying never mutates authenticated staged bytes.
    """
    work_dir = Path(work_dir)
    empty_lists = work_dir / 'empty-lists'
    archives = work_dir / 'archives'
    work_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    empty_lists.mkdir(mode=0o700, exist_ok=True)
    archives.mkdir(mode=0o700, exist_ok=True)
    return ['/usr/bin/apt-get', *(['--simulate'] if simulate else []),
            '--no-remove', '--yes',
            '-o', 'Dir::Etc::sourcelist=/dev/null', '-o', 'Dir::Etc::sourceparts=-',
            '-o', 'Dir::State::lists=' + str(empty_lists),
            '-o', 'Dir::State::status=' + str(Path(dpkg_dir) / 'status'),
            '-o', 'Dir::Cache::archives=' + str(archives),
            '-o', 'Dpkg::Options::=--force-confdef', '-o', 'Dpkg::Options::=--force-confold',
            'install', *map(str, deb_paths)]


def plan_with_apt(deb_paths, packages, dpkg_dir, work_dir, require_complete=True):
    """Use maintained APT's real solver; no host sources, downloads or removals."""
    result = subprocess.run(apt_command(deb_paths, dpkg_dir, work_dir, simulate=True),
                            stdin=subprocess.DEVNULL, capture_output=True, text=True,
                            timeout=60, env=safe_env())
    if result.returncode != 0 or len(result.stdout) > 2 * 1024 * 1024:
        fail('APT could not resolve this exact offline bundle; no update applied')
    expected = {pkg['name']: pkg['version'] for pkg in packages}
    proposed = {}
    for line in result.stdout.splitlines():
        if line.startswith('Remv '):
            fail('APT proposes package removals; update refused')
        if line.startswith('Inst '):
            match = re.match(r'Inst ([a-z0-9+.:~-]+)(?: \[[^\]]+\])? \((\S+)', line)
            if match is None:
                fail('unrecognized APT plan; update refused')
            name, version = match.groups()
            name = name.split(':', 1)[0]
            if expected.get(name) != version:
                fail('APT proposes packages outside the authenticated bundle')
            proposed[name] = version
    if require_complete:
        installed = load_installed_dpkg_database(dpkg_dir)
        require_present = [name for name, version in expected.items()
                           if installed.get(name, {}).get('version') != version]
        if any(name not in proposed for name in require_present):
            fail('APT did not propose the complete authenticated bundle')
    return proposed


# ------------------------------------------------------- journal and status

def parse_iso(value, what):
    try:
        parsed = datetime.fromisoformat(value)
    except (TypeError, ValueError):
        fail(f'invalid {what} timestamp')
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def read_boot_id():
    try:
        return Path('/proc/sys/kernel/random/boot_id').read_text(encoding='ascii').strip()
    except OSError:
        return ''


def unresolved_failure(records):
    """Latest transaction whose last record is failed/interrupted (never hidden by newer records)."""
    last = {}
    for record in records:
        last[record.get('transaction_id')] = record
    bad = [r for r in last.values() if r['state'] in ('failed', 'interrupted')]
    return max(bad, key=lambda r: r['seq']) if bad else None


class Store:
    """Private update state (0711 dir, 0600 files) plus a 0755/0644 public status mirror."""

    def __init__(self, state_dir):
        self.dir = Path(state_dir)
        self.journal_path = self.dir / 'journal.jsonl'
        self.public_dir = self.dir / 'public'
        self.status_path = self.public_dir / 'status.json'
        self.replay_path = self.dir / 'replay.json'
        self.staged = self.dir / 'staged'
        self.installed = self.dir / 'installed'  # the tree of the update installed last
        self.previous = self.dir / 'previous'  # the one it replaced: what going back installs
        self.work = self.dir / 'work'
        self.lock_path = self.dir / '.lock'

    def prepare(self):
        if self.dir.is_symlink():
            fail('unsafe symlinked update state directory')
        self.dir.mkdir(parents=True, exist_ok=True)
        os.chmod(self.dir, 0o711)  # traversable so the public mirror is reachable; not listable
        self.public_dir.mkdir(exist_ok=True)
        os.chmod(self.public_dir, 0o755)

    @contextmanager
    def exclusive(self):
        """Non-blocking flock; the kernel drops it when the owner dies, which is how reconcile
        distinguishes a crashed 'applying' transaction from a running one."""
        self.prepare()
        descriptor = os.open(self.lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        try:
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                raise StateBusy('another update operation is in progress') from None
            yield
        finally:
            os.close(descriptor)

    def records(self):
        if not os.path.lexists(self.journal_path):
            return []
        data = read_regular_bytes(self.journal_path, JOURNAL_LIMIT)
        records = []
        for line in data.decode('utf-8', errors='replace').splitlines():
            try:
                value = json.loads(line)
            except ValueError:
                continue  # a torn final line from a crash is ignored, never trusted
            if (isinstance(value, dict) and value.get('state') in JOURNAL_STATES
                    and isinstance(value.get('seq'), int) and isinstance(value.get('transaction_id'), str)):
                records.append(value)
        return records

    def append(self, state, tx_id, when, **fields):
        """Append one durable record: fsync the file, then the directory."""
        self.prepare()
        records = self.records()
        if len(records) >= JOURNAL_COMPACT_AT:
            records = self.compact(records)
        seq = records[-1]['seq'] + 1 if records else 1
        record = {'version': 1, 'seq': seq, 'state': state, 'transaction_id': tx_id,
                  'recorded_at_utc': when.isoformat()} | fields
        line = (json.dumps(record, sort_keys=True) + '\n').encode('utf-8')
        torn = False
        if os.path.lexists(self.journal_path):
            existing = read_regular_bytes(self.journal_path, JOURNAL_LIMIT)
            torn = bool(existing) and not existing.endswith(b'\n')
        descriptor = os.open(self.journal_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        try:
            os.write(descriptor, (b'\n' if torn else b'') + line)
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        fsync_dir(self.dir)
        return record

    def compact(self, records):
        """Bounded history: the newest records plus everything for unresolved failures."""
        keep_tx = set()
        pending = unresolved_failure(records)
        if pending is not None:
            keep_tx.add(pending['transaction_id'])
        kept = [r for index, r in enumerate(records)
                if index >= len(records) - JOURNAL_KEEP or r['transaction_id'] in keep_tx]
        atomic_write(self.journal_path, b''.join((json.dumps(r, sort_keys=True) + '\n').encode() for r in kept))
        return kept

    def last_applied(self):
        if not os.path.lexists(self.replay_path):
            return None
        try:
            value = json.loads(read_regular_bytes(self.replay_path, 4096).decode('utf-8'))
            return parse_iso(value['date'], 'last applied')
        except (ValueError, KeyError, TypeError):
            fail('update replay state is unreadable; refusing to continue')

    def record_applied(self, date, tx_id, review_id):
        json_write(self.replay_path, {'version': 1, 'date': date.isoformat(), 'transaction_id': tx_id,
                                      'review_id': review_id})

    def publish(self, trigger_path=None, records=None):
        """Mirror the derived state to public/status.json (best effort never blocks a transaction)."""
        try:
            records = self.records() if records is None else records
            staged_valid = bool(trigger_path) and self.staged.is_dir() and owned_trigger(trigger_path, self.staged)
            status = summarize(records, staged_valid)
            applied = self.last_applied()
            status['last_applied_date'] = applied.isoformat() if applied else None
            status['previous_version'] = kept_version(self.previous)
            self.prepare()
            atomic_write(self.status_path, json_bytes(status), mode=0o644, dir_mode=0o755)
        except (OSError, ValueError):
            pass


def compact_record(record):
    out = {key: record[key] for key in ('seq', 'state', 'transaction_id', 'recorded_at_utc') if key in record}
    if record.get('error'):
        out['error'] = clean_text(record['error'])
    return out


def summarize(records, staged_valid):
    latest = records[-1] if records else None
    state = 'idle'
    tx_id = None
    packages = []
    error = None
    last_state = None
    recorded = None
    if latest is not None:
        last_state = latest['state']
        recorded = latest.get('recorded_at_utc')
        if last_state == 'staged':
            state = 'staged' if staged_valid else 'idle'
        elif last_state != 'cancelled':
            state = last_state
        if state != 'idle':
            tx_id = latest['transaction_id']
            packages = [clean_text(p, 200) for p in latest.get('packages', [])[:MAX_PACKAGES] if isinstance(p, str)]
        if last_state in ('failed', 'interrupted'):
            error = clean_text(latest.get('error', ''))
    pending = unresolved_failure(records)
    return {
        'version': 1,
        'update_state': state,
        'last_state': last_state,
        'transaction_id': tx_id,
        'recorded_at_utc': recorded,
        'packages': packages,
        'error': error,
        'unresolved_failure': compact_record(pending) if pending else None,
        'history': [compact_record(r) for r in records[-8:]],
    }


def read_public_status(state_dir):
    path = Path(state_dir) / 'public' / 'status.json'
    try:
        value = json.loads(read_regular_bytes(path, 65536).decode('utf-8'))
    except (ValueError, OSError):
        return None
    if not isinstance(value, dict) or value.get('update_state') not in UPDATE_STATES:
        return None
    return value


# --------------------------------------------------------- stage / cancel

def clear_partials(store):
    cleared = False
    if store.dir.is_dir():
        for entry in sorted([*store.dir.glob('staged.*.partial'), *store.dir.glob('*.discard')]):
            if entry.is_symlink() or not entry.is_dir():
                entry.unlink(missing_ok=True)
            else:
                shutil.rmtree(entry, ignore_errors=True)
            cleared = True
    return cleared


def remove_staged(store):
    if store.staged.is_symlink():
        store.staged.unlink()
    elif store.staged.exists():
        shutil.rmtree(store.staged, ignore_errors=True)
    if store.dir.is_dir():
        fsync_dir(store.dir)


def kept_version(tree):
    """'wros 0.1.0~rc5' for a kept tree, from its manifest; None when there is none."""
    try:
        require_private_entry(tree, 'dir')
        manifest = json.loads(read_regular_bytes(tree / 'manifest.json', 262144).decode('utf-8'))
        return ', '.join(clean_text(label, 200) for label in package_labels(manifest['packages'])) or None
    except (ValueError, OSError, KeyError, TypeError):
        return None


def keep_installed(store):
    """The applied tree becomes installed/ and the one it replaced previous/; only one is kept.

    Renames only, each made durable before the next. A crash leaves the old pair, or previous/
    plus a completed staged/ that reconcile finishes keeping, plus at most a *.discard
    directory that clear_partials removes.
    """
    discard = store.dir / 'previous.discard'
    if os.path.lexists(discard):
        shutil.rmtree(discard, ignore_errors=True)
    if store.installed.is_dir() and not store.installed.is_symlink():
        if os.path.lexists(store.previous):
            os.rename(store.previous, discard)
            fsync_dir(store.dir)
        os.rename(store.installed, store.previous)
        fsync_dir(store.dir)
    elif os.path.lexists(store.installed):
        store.installed.unlink()
    os.rename(store.staged, store.installed)
    fsync_dir(store.dir)
    shutil.rmtree(discard, ignore_errors=True)


def finish_keeping(store, records, trigger_path):
    """After a crash inside keep_installed: a completed transaction's tree still in staged/."""
    if (os.path.lexists(trigger_path) or os.path.lexists(store.installed) or not store.staged.is_dir()
            or not records or records[-1]['state'] != 'completed'):
        return
    try:
        manifest = json.loads(read_regular_bytes(store.staged / 'manifest.json', 262144).decode('utf-8'))
    except (ValueError, OSError):
        return
    if isinstance(manifest, dict) and manifest.get('transaction_id') == records[-1]['transaction_id']:
        os.rename(store.staged, store.installed)
        fsync_dir(store.dir)


def unlink_trigger(trigger_path):
    trigger_path = Path(trigger_path)
    if trigger_path.is_symlink() or trigger_path.is_file():
        trigger_path.unlink()
    fsync_dir(trigger_path.parent)


def package_labels(entries):
    return [f'{p["name"]} {p["version"]}' for p in entries]


# -------------------------------------------------------- app-only updates

DPKG_DEB = '/usr/bin/dpkg-deb'
# Files an update may add, change or remove and still install while WrOS runs: the
# writing app, the unprivileged helpers it runs itself, its docs and the version file.
LIVE_APPLY_FILES = ('usr/bin/grimoire', 'usr/libexec/wros-backup', 'usr/libexec/wros-sync',
                    'etc/writingdeck/release')
LIVE_APPLY_PREFIXES = ('usr/share/doc/',)
# Control fields that must not change: relationships and the package's shape.
LIVE_APPLY_FIELDS = ('Architecture', 'Pre-Depends', 'Depends', 'Recommends', 'Breaks', 'Conflicts',
                     'Provides', 'Replaces', 'Essential', 'Multi-Arch')
CONTROL_MEMBERS = {'control', 'md5sums', 'conffiles'}


def control_fields_from(text):
    fields, key = {}, None
    for line in text.splitlines():
        if line.startswith((' ', '\t')) and key:
            fields[key] += '\n' + line
        elif ':' in line:
            key, _, value = line.partition(':')
            key = key.strip()
            fields[key] = value.strip()
    return fields


def installed_fields(dpkg_dir, name):
    status_file = Path(dpkg_dir) / 'status'
    text = read_regular_bytes(status_file, LIMIT).decode('utf-8', errors='replace')
    for stanza in text.split('\n\n'):
        fields = control_fields_from(stanza)
        if fields.get('Package') == name and fields.get('Status') in ('install ok installed', 'hold ok installed'):
            return fields
    return None


def deb_control_members(deb_path):
    """The control archive's files of an already verified .deb, by name."""
    result = subprocess.run([DPKG_DEB, '--ctrl-tarfile', str(deb_path)], stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=60, env=safe_env())
    if result.returncode != 0 or len(result.stdout) > LIMIT:
        fail('cannot read the staged package control data')
    members = {}
    with tarfile.open(fileobj=io.BytesIO(result.stdout)) as archive:
        for member in archive.getmembers():
            name = member.name[2:] if member.name.startswith('./') else member.name
            if member.isdir() and name in ('', '.'):
                continue
            if not member.isfile() or '/' in name:
                members[name or member.name] = None
                continue
            members[name] = archive.extractfile(member).read()
    return members


def md5_entries(text):
    entries = {}
    for line in text.splitlines():
        digest, _, path = line.partition('  ')
        if not path or re.fullmatch(r'[0-9a-f]{32}', digest) is None:
            raise ValueError('malformed md5sums line')
        entries[path.lstrip('/')] = digest
    return entries


def live_path(path):
    return path in LIVE_APPLY_FILES or path.startswith(LIVE_APPLY_PREFIXES)


def live_apply_check(packages, deb_paths, dpkg_dir):
    """(True, '') when this update may install now and restart only the writing app.

    Conservative by construction: anything not proven app-only waits for the
    offline restart, which is always safe.
    """
    if len(packages) != 1 or len(deb_paths) != 1:
        return False, 'it changes more than the wros package'
    package = packages[0]
    if package.get('name') != 'wros' or package.get('action') != 'upgrade':
        return False, 'it is not an upgrade of the installed wros package'
    try:
        installed = installed_fields(dpkg_dir, 'wros')
        if installed is None:
            return False, 'the installed wros package is not recorded'
        info = Path(dpkg_dir) / 'info'
        old_files = md5_entries(read_regular_bytes(info / 'wros.md5sums', LIMIT).decode('utf-8'))
        conffiles_path = info / 'wros.conffiles'
        old_conffiles = (read_regular_bytes(conffiles_path, LIMIT).decode('utf-8')
                         if os.path.lexists(conffiles_path) else '')
        members = deb_control_members(deb_paths[0])
        if set(members) - CONTROL_MEMBERS or any(members[n] is None for n in members):
            return False, 'it carries maintainer scripts or other control data'
        new = control_fields_from(members['control'].decode('utf-8'))
        new_files = md5_entries(members.get('md5sums', b'').decode('utf-8'))
        new_conffiles = members.get('conffiles', b'').decode('utf-8')
    except (OSError, ValueError, UnicodeDecodeError, tarfile.TarError):
        return False, 'its files could not be compared with the installed package'
    for field in LIVE_APPLY_FIELDS:
        if new.get(field, '') != installed.get(field, ''):
            return False, f'its {field} field changes'
    if set(old_conffiles.split()) != set(new_conffiles.split()):
        return False, 'its configuration files change'
    if not old_files or not new_files:
        return False, 'its files could not be compared with the installed package'
    changed = sorted(path for path in old_files.keys() | new_files.keys()
                     if old_files.get(path) != new_files.get(path))
    outside = [path for path in changed if not live_path(path)]
    if outside:
        return False, f'it changes /{outside[0]}'
    return True, ''


def stage_update(bundle_dir, review_id, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS,
                 dpkg_dir=DEFAULT_DPKG_DIR, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                 now=None, host_arch=None, allowed_prefixes=ALLOWED_MEDIA_PREFIXES, suites=PINNED_SUITES):
    """Re-verify everything under the dpkg locks, then stage with a journal-before-trigger protocol.

    Crash points: before the rename nothing visible exists (partial dirs are junk);
    after the rename but before the journal record there is no trigger; the trigger is
    created last, so a reboot can only ever apply a fully journaled, fully staged tree.
    """
    if not isinstance(review_id, str) or HEX64_RE.fullmatch(review_id) is None:
        fail('invalid review id')
    check_bundle_path(str(bundle_dir), allowed_prefixes)
    store = Store(state_dir)
    trigger_path = Path(trigger_path)
    when = clock(now)
    with store.exclusive():
        if os.path.lexists(trigger_path):
            fail('another offline update is already pending; original preserved')
        clear_partials(store)
        interrupted, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
        if interrupted:
            detail = f' ({", ".join(unconfigured)})' if unconfigured else ''
            fail(f'interrupted dpkg state{detail} must be repaired before staging updates')
        records = store.records()
        if records and records[-1]['state'] == 'applying':
            fail('an interrupted update transaction must be reconciled before staging a new update')
        last_applied = store.last_applied()
        tx_id = secrets.token_hex(16)
        partial = store.dir / f'staged.{tx_id}.partial'
        try:
            with BundleDir(bundle_dir, strict=True) as bundle, acquire_posix_dpkg_locks(dpkg_dir):
                raw = verify_open_bundle(bundle, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir, now=when,
                                         host_arch=host_arch, last_applied=last_applied, suites=suites)
                if raw['review_id'] != review_id:
                    fail('update bundle changed since it was reviewed; review it again')
                partial.mkdir(mode=0o700)
                (partial / 'packages').mkdir(mode=0o700)
                (partial / 'metadata').mkdir(mode=0o700)
                staged_entries = []
                for pkg in raw['packages']:
                    staged_name = f'{pkg["name"]}_{pkg["sha256"]}.deb'
                    if SAFE_REL_RE.fullmatch(staged_name) is None:
                        fail(f'unsafe staged archive filename: {staged_name}')
                    digest, size = bundle.copy(pkg['filename'], partial / 'packages' / staged_name)
                    if digest != pkg['sha256'] or size != pkg['size']:
                        fail('staged archive hash mismatch after copy')
                    staged_entries.append({
                        'name': pkg['name'], 'version': pkg['version'], 'architecture': pkg['architecture'],
                        'filename': pkg['filename'], 'size': pkg['size'], 'sha256': pkg['sha256'],
                        'staged_filename': staged_name, 'action': raw['actions'][pkg['name']],
                    })
                live, live_reason = live_apply_check(
                    staged_entries, [partial / 'packages' / e['staged_filename'] for e in staged_entries], dpkg_dir)
                metadata_hashes = {}
                for name, data in [*raw['meta'].items(), ('Packages', raw['packages_raw'])]:
                    descriptor = os.open(partial / 'metadata' / name,
                                         os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
                    with os.fdopen(descriptor, 'wb') as target:
                        target.write(data)
                        target.flush()
                        os.fsync(target.fileno())
                    metadata_hashes[name] = hashlib.sha256(data).hexdigest()
                manifest = {
                    'version': MANIFEST_VERSION,
                    'transaction_id': tx_id,
                    'review_id': raw['review_id'],
                    'verified': True,
                    'recorded_at_utc': when.isoformat(),
                    'signer_fingerprint': raw['fingerprint'],
                    'suite': raw['release']['suite'],
                    'release_date': raw['release']['date'].isoformat(),
                    'valid_until': raw['release']['valid_until'].isoformat(),
                    'packages_rel': raw['packages_rel'],
                    'packages_digest': metadata_hashes['Packages'],
                    'metadata': metadata_hashes,
                    'package_count': len(staged_entries),
                    'packages': staged_entries,
                    'live_apply': live,
                    'live_apply_reason': live_reason,
                }
                json_write(partial / 'manifest.json', manifest)
                for directory in (partial / 'packages', partial / 'metadata', partial):
                    fsync_dir(directory)
            # APT's solver runs without our POSIX locks held (it takes its own).
            plan_with_apt([partial / 'packages' / e['staged_filename'] for e in staged_entries],
                          staged_entries, dpkg_dir, store.work)
            if os.path.lexists(store.staged):
                remove_staged(store)  # orphan or retained archives of a resolved failure
            os.rename(partial, store.staged)
            fsync_dir(store.dir)
        except BaseException:
            shutil.rmtree(partial, ignore_errors=True)
            raise
        labels = package_labels(staged_entries)
        try:
            store.append('staged', tx_id, when, packages=labels, review_id=raw['review_id'],
                         signer=raw['fingerprint'], suite=raw['release']['suite'],
                         date=raw['release']['date'].isoformat())
            try:
                os.symlink(str(store.staged), trigger_path)
            except FileExistsError:
                fail('another offline update is already pending; original preserved')
            fsync_dir(trigger_path.parent)
        except BaseException:
            remove_staged(store)
            try:
                store.append('cancelled', tx_id, when, note='staging did not complete')
            except OSError:
                pass
            store.publish(trigger_path)
            raise
        store.publish(trigger_path)
    return {'status': 'staged', 'transaction_id': tx_id, 'review_id': raw['review_id'],
            'packages': len(staged_entries), 'restart_required': not live}


def cancel_staged(*, state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER, now=None):
    """Cancel only while staged. Never touches a running apply or a foreign /system-update."""
    store = Store(state_dir)
    trigger_path = Path(trigger_path)
    when = clock(now)
    try:
        with store.exclusive():
            records = store.records()
            latest = records[-1] if records else None
            state = latest['state'] if latest else None
            ours = owned_trigger(trigger_path, store.staged)
            cleared = clear_partials(store)
            if state == 'applying':
                store.publish(trigger_path, records)
                return {'status': 'none', 'message': 'An update was already started and cannot be cancelled; '
                                                     'repair will resolve an interrupted one.'}
            if state == 'staged' and ours and store.staged.is_dir():
                unlink_trigger(trigger_path)
                store.append('cancelled', latest['transaction_id'], when, packages=latest.get('packages', []))
                remove_staged(store)
                store.publish(trigger_path)
                return {'status': 'cancelled', 'message': 'The staged update was cancelled; nothing was installed.'}
            retained = state in ('failed', 'interrupted')
            if store.staged.exists() and not retained and not ours:
                remove_staged(store)
                if state == 'staged':
                    store.append('cancelled', latest['transaction_id'], when, note='cleared incomplete staging')
                store.publish(trigger_path)
                return {'status': 'cancelled', 'message': 'Cleared an incomplete staged update.'}
            if cleared:
                store.publish(trigger_path)
                return {'status': 'cancelled', 'message': 'Cleared an incomplete staged update.'}
            return {'status': 'none', 'message': 'No staged update to cancel.'}
    except StateBusy:
        return {'status': 'none', 'message': 'Another update operation is running; nothing was cancelled.'}


# ------------------------------------------------------- boot-time revalidation

def revalidate_staged(staged_dir, *, keyring, signers, dpkg_dir, now, host_arch, last_applied,
                      mode='apply', suites=PINNED_SUITES):
    """Re-prove the retained bytes against the CURRENT keyring, pin and dpkg state.

    mode 'apply' enforces age, replay and the full dpkg policy; mode 'repair' re-proves
    authenticity and integrity only (the transaction already began; dependency
    policy would reject half-configured packages).
    """
    staged = Path(staged_dir)
    require_private_entry(staged, 'dir')
    require_private_entry(staged / 'manifest.json', 'file')
    try:
        manifest = json.loads(read_regular_bytes(staged / 'manifest.json', 262144).decode('utf-8'))
    except ValueError:
        fail('staged update manifest is not valid JSON')
    if (not isinstance(manifest, dict) or manifest.get('version') != MANIFEST_VERSION
            or manifest.get('verified') is not True or not isinstance(manifest.get('packages'), list)
            or not manifest['packages']):
        fail('invalid staged update manifest')
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
    if not isinstance(metadata_hashes, dict):
        fail('invalid staged update metadata record')
    signed_names = set(metadata_hashes) - {'Packages'}
    if signed_names not in ({'InRelease'}, {'Release', 'Release.gpg'}) or 'Packages' not in metadata_hashes:
        fail('invalid staged update metadata record')
    metadata_dir = staged / 'metadata'
    require_private_entry(metadata_dir, 'dir')
    if sorted(entry.name for entry in metadata_dir.iterdir()) != sorted(metadata_hashes):
        fail('staged update metadata directory has unexpected contents')
    retained = {}
    for name, expected in metadata_hashes.items():
        require_private_entry(metadata_dir / name, 'file')
        data = read_regular_bytes(metadata_dir / name, LIMIT)
        if hashlib.sha256(data).hexdigest() != expected:
            fail(f'retained {name} no longer matches the staged digest')
        retained[name] = data
    meta = {name: retained[name] for name in signed_names}
    fingerprint, release_text = verify_signed_metadata(meta, keyring=keyring, signers=signers)
    if fingerprint != manifest.get('signer_fingerprint'):
        fail('retained signature signer differs from the staged signer')
    # Valid-Until may have passed by boot; it had to be valid when the update was staged.
    release = parse_release(release_text, now=stage_time, suites=suites)
    if mode == 'apply' and last_applied is not None and release['date'] <= last_applied:
        fail('staged update is not newer than the last applied update (replay refused)')
    packages_rel = manifest.get('packages_rel')
    if packages_rel not in ('Packages', 'main/binary-amd64/Packages') or packages_rel not in release['entries']:
        fail('retained Release no longer authenticates the staged Packages index')
    expected_sha, expected_size = release['entries'][packages_rel]
    packages_raw = retained['Packages']
    packages_digest = hashlib.sha256(packages_raw).hexdigest()
    if packages_digest != expected_sha or len(packages_raw) != expected_size:
        fail('retained Packages index does not match the retained signed Release')
    if packages_digest != manifest.get('packages_digest'):
        fail('retained Packages digest differs from the staged digest')
    packages = parse_packages_index(packages_raw.decode('utf-8', errors='strict'))
    check_architectures(packages, host_arch)
    key = lambda p: (p['name'], p['version'], p['architecture'], p['filename'], p['size'], p['sha256'])
    try:
        manifest_keys = sorted(key(p) for p in manifest['packages'])
    except (KeyError, TypeError):
        fail('invalid staged update package record')
    if manifest_keys != sorted(key(p) for p in packages):
        fail('staged update manifest does not match the retained signed Packages index')
    packages_dir = staged / 'packages'
    require_private_entry(packages_dir, 'dir')
    expected_names = set()
    for pkg, record in zip(sorted(packages, key=key), sorted(manifest['packages'], key=key)):
        staged_name = f'{pkg["name"]}_{pkg["sha256"]}.deb'
        if record.get('staged_filename') != staged_name or SAFE_REL_RE.fullmatch(staged_name) is None:
            fail('unsafe or inconsistent staged archive filename')
        require_private_entry(packages_dir / staged_name, 'file')
        digest, size = sha256_file(packages_dir / staged_name)
        if digest != pkg['sha256'] or size != pkg['size']:
            fail(f'staged archive {staged_name} failed SHA-256 verification')
        pkg['staged_filename'] = staged_name
        expected_names.add(staged_name)
    if {entry.name for entry in packages_dir.iterdir()} != expected_names:
        fail('staged archive directory has unexpected contents')
    actions = {}
    if mode == 'apply':
        installed = load_installed_dpkg_database(dpkg_dir=dpkg_dir)
        actions = evaluate_bundle_against_installed(packages, installed)
    return {'manifest': manifest, 'tx_id': tx_id, 'packages': packages, 'actions': actions,
            'release': release, 'fingerprint': fingerprint}


def real_system_required(runner, **paths):
    defaults = {'state_dir': DEFAULT_STATE_DIR, 'dpkg_dir': DEFAULT_DPKG_DIR, 'trigger_path': DEFAULT_TRIGGER,
                'keyring': DEFAULT_KEYRING, 'signers': DEFAULT_SIGNERS}
    if runner is None and (os.geteuid() != 0 or any(Path(paths[k]) != v for k, v in defaults.items() if k in paths)):
        fail('real package application requires the fixed system paths and root service')


def execute_offline_boot_transaction(state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER,
                                     dpkg_dir=DEFAULT_DPKG_DIR, runner=None, now=None,
                                     keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, host_arch=None):
    """Offline apply: revalidate retained bytes, journal 'applying' BEFORE the trigger goes, run APT."""
    return apply_staged_transaction(state_dir=state_dir, trigger_path=trigger_path, dpkg_dir=dpkg_dir,
                                    runner=runner, now=now, keyring=keyring, signers=signers,
                                    host_arch=host_arch, live=False)


def apply_staged_now(state_dir=DEFAULT_STATE_DIR, trigger_path=DEFAULT_TRIGGER, dpkg_dir=DEFAULT_DPKG_DIR,
                     runner=None, now=None, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, host_arch=None):
    """Install an app-only staged update now, through the same journaled transaction as boot.

    Refused unless staging recorded it as app-only AND it still is against the dpkg
    database right now; then the update stays staged for the restart.
    """
    return apply_staged_transaction(state_dir=state_dir, trigger_path=trigger_path, dpkg_dir=dpkg_dir,
                                    runner=runner, now=now, keyring=keyring, signers=signers,
                                    host_arch=host_arch, live=True)


def apply_staged_transaction(*, state_dir, trigger_path, dpkg_dir, runner, now, keyring, signers, host_arch,
                             live):
    store = Store(state_dir)
    trigger_path = Path(trigger_path)
    dpkg_dir = Path(dpkg_dir)
    staged_dir = store.staged
    if not owned_trigger(trigger_path, staged_dir):
        return {'status': 'not-owned'}
    real_system_required(runner, state_dir=state_dir, dpkg_dir=dpkg_dir, trigger_path=trigger_path,
                         keyring=keyring, signers=signers)
    run = runner or default_runner
    when = clock(now)
    stack = ExitStack()
    try:
        stack.enter_context(store.exclusive())
    except StateBusy:
        return {'status': 'busy'}
    with stack:
        tx_id, labels = 'unknown', []

        def record_failure(reason):
            interrupted, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
            try:
                store.append('failed', tx_id, when, packages=labels, error=clean_text(reason),
                             interrupted_dpkg=interrupted, unconfigured_packages=unconfigured,
                             rescue_strategy='Run Update repair in Settings, or boot the WrOS live media and run '
                                             'wros-rescue; the authenticated archives are retained.')
            finally:
                try:
                    unlink_trigger(trigger_path)  # never loop on reboot, even if journaling failed
                finally:
                    store.publish(trigger_path)
            fail(reason)

        try:
            require_private_entry(staged_dir, 'dir')
            try:
                tx_id = str(json.loads(read_regular_bytes(staged_dir / 'manifest.json', 262144).decode('utf-8'))
                            .get('transaction_id', 'unknown'))
            except (ValueError, AttributeError):
                tx_id = 'unknown'
            interrupted_before, unconfigured_before = inspect_dpkg_state(dpkg_dir=dpkg_dir)
            if interrupted_before:
                fail(f'dpkg already interrupted before offline update ({", ".join(unconfigured_before)})')
            with acquire_posix_dpkg_locks(dpkg_dir):
                details = revalidate_staged(staged_dir, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir,
                                            now=when, host_arch=host_arch, last_applied=store.last_applied())
            tx_id, packages = details['tx_id'], details['packages']
            labels = package_labels(packages)
            deb_paths = [staged_dir / 'packages' / p['staged_filename'] for p in packages]
            if live:
                acting = [p | {'action': details['actions'].get(p['name'])} for p in packages]
                still, reason = live_apply_check(acting, deb_paths, dpkg_dir)
                if details['manifest'].get('live_apply') is not True or not still:
                    fail(f'this update needs a restart to install ({reason or "it was staged for one"})')
            plan_with_apt(deb_paths, packages, dpkg_dir, store.work)
        except ValueError as exc:
            if live:
                raise  # nothing was journaled or removed: it stays staged for the restart
            record_failure(f'staged update revalidation failed: {exc}')
        except Exception as exc:
            record_failure(f'staged update revalidation failed: {exc}')

        # 'applying' is durable BEFORE the trigger disappears (systemd.offline-updates(7) wants it
        # gone before the package manager runs so a failure cannot boot-loop).
        store.append('applying', tx_id, when, packages=labels, boot_id=read_boot_id(), owner_pid=os.getpid(),
                     review_id=details['manifest'].get('review_id'))
        unlink_trigger(trigger_path)
        store.publish(trigger_path)
        try:
            res = run(apt_command(deb_paths, dpkg_dir, store.work))
            interrupted_after, _ = inspect_dpkg_state(dpkg_dir=dpkg_dir)
            if res.returncode != 0 or interrupted_after:
                fail('offline apt-get transaction failed; authenticated archives retained for rescue')
            installed_after = load_installed_dpkg_database(dpkg_dir)
            if any(installed_after.get(p['name'], {}).get('version') != p['version'] for p in packages):
                fail('offline apt-get did not install the reviewed package versions')
        except Exception as exc:
            reason = str(exc) if str(exc).startswith('offline apt-get') else \
                'offline apt-get transaction failed or was interrupted; authenticated archives retained for rescue'
            record_failure(reason)
        store.record_applied(details['release']['date'], tx_id, details['manifest'].get('review_id'))
        store.append('completed', tx_id, when, packages=labels)  # durable before the archives move
        keep_installed(store)
        store.publish(trigger_path)
        return {'status': 'completed', 'transaction_id': tx_id, 'packages': labels}


# ----------------------------------------------------------- repair/reconcile

def run_checked(run, command):
    result = run(command)
    return result.returncode == 0


def repair_locked(store, *, dpkg_dir, trigger_path, keyring, signers, run, when, host_arch, mark_applying=True):
    records = store.records()
    latest = records[-1] if records else None
    if latest is not None and latest['state'] == 'applying' and mark_applying:
        # The state lock was free, so no process owns this transaction: it was interrupted.
        store.append('interrupted', latest['transaction_id'], when, packages=latest.get('packages', []),
                     error='the update was interrupted before it finished')
        records = store.records()
    pending = unresolved_failure(records)
    interrupted, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
    tx_id = (pending or latest or {}).get('transaction_id') or 'repair-' + secrets.token_hex(8)
    if not interrupted:
        if pending is not None:
            store.append('recovered', pending['transaction_id'], when,
                         note='dpkg state is consistent; the earlier failure was acknowledged')
            store.publish(trigger_path)
            return {'status': 'clean', 'message': 'Package state is consistent. The earlier failed update '
                                                  'left no half-installed packages.'}
        return {'status': 'clean', 'message': 'No interrupted package operation was found.'}
    ensure_dpkg_unlocked(dpkg_dir)
    labels = (pending or {}).get('packages', [])
    steps = []
    configure = ['/usr/bin/dpkg', f'--admindir={dpkg_dir}', '--force-confdef', '--force-confold', '--configure', '-a']
    try:
        run(configure)
    except (OSError, subprocess.SubprocessError):
        pass  # the state check below decides the outcome
    steps.append('dpkg --configure -a')
    interrupted, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
    details = None
    if interrupted and store.staged.is_dir():
        try:
            details = revalidate_staged(store.staged, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir,
                                        now=when, host_arch=host_arch, last_applied=None, mode='repair')
            debs = [store.staged / 'packages' / p['staged_filename'] for p in details['packages']]
            plan_with_apt(debs, details['packages'], dpkg_dir, store.work, require_complete=False)
            ensure_dpkg_unlocked(dpkg_dir)
            run(apt_command(debs, dpkg_dir, store.work))
            steps.append('apt-get install of retained archives')
        except (ValueError, OSError, subprocess.SubprocessError):
            details = None
        interrupted, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
    if interrupted:
        reason = 'package state is still inconsistent' + (f' ({", ".join(unconfigured)})' if unconfigured else '')
        store.append('failed', tx_id, when, packages=labels, error=clean_text(reason),
                     interrupted_dpkg=True, unconfigured_packages=unconfigured)
        store.publish(trigger_path)
        return {'status': 'failed', 'message': reason + '. Boot the WrOS live media and run wros-rescue.'}
    note = 'repaired with ' + ' then '.join(steps)
    if details is None and store.staged.is_dir():
        # dpkg --configure -a alone may have finished the interrupted transaction: if the retained,
        # re-authenticated archives are exactly what is now installed, the update is complete.
        try:
            details = revalidate_staged(store.staged, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir,
                                        now=when, host_arch=host_arch, last_applied=None, mode='repair')
        except (ValueError, OSError):
            details = None
    if details is not None:
        installed = load_installed_dpkg_database(dpkg_dir)
        if all(installed.get(p['name'], {}).get('version') == p['version'] for p in details['packages']):
            store.record_applied(details['release']['date'], tx_id, details['manifest'].get('review_id'))
            keep_installed(store)
            note += '; the staged update is now fully installed'
    store.append('recovered', tx_id, when, packages=labels, note=note)
    store.publish(trigger_path)
    return {'status': 'recovered', 'message': 'Package state was repaired (' + note + ').'}


def repair_updates(*, state_dir=DEFAULT_STATE_DIR, dpkg_dir=DEFAULT_DPKG_DIR, trigger_path=DEFAULT_TRIGGER,
                   keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, runner=None, now=None, host_arch=None):
    """Privileged bounded repair using only retained, re-authenticated archives."""
    real_system_required(runner, state_dir=state_dir, dpkg_dir=dpkg_dir, trigger_path=trigger_path,
                         keyring=keyring, signers=signers)
    store = Store(state_dir)
    try:
        with store.exclusive():
            return repair_locked(store, dpkg_dir=Path(dpkg_dir), trigger_path=Path(trigger_path), keyring=keyring,
                                 signers=signers, run=runner or default_runner, when=clock(now),
                                 host_arch=host_arch)
    except StateBusy:
        return {'status': 'failed', 'message': 'An update operation is running; try again when it finishes.'}


def reconcile(*, state_dir=DEFAULT_STATE_DIR, dpkg_dir=DEFAULT_DPKG_DIR, trigger_path=DEFAULT_TRIGGER,
              keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, runner=None, now=None, host_arch=None):
    """Normal-boot reconcile: name an orphaned 'applying' transaction, republish status, bounded repair.

    Never reboots and never touches a foreign /system-update.
    """
    real_system_required(runner, state_dir=state_dir, dpkg_dir=dpkg_dir, trigger_path=trigger_path,
                         keyring=keyring, signers=signers)
    store = Store(state_dir)
    if not os.path.lexists(store.dir):
        return {'status': 'idle', 'message': 'No update state.'}
    try:
        with store.exclusive():
            when = clock(now)
            clear_partials(store)
            records = store.records()
            finish_keeping(store, records, Path(trigger_path))
            latest = records[-1] if records else None
            interrupted, _ = inspect_dpkg_state(dpkg_dir=dpkg_dir)
            if (latest is not None and latest['state'] == 'applying') or interrupted:
                result = repair_locked(store, dpkg_dir=Path(dpkg_dir), trigger_path=Path(trigger_path),
                                       keyring=keyring, signers=signers, run=runner or default_runner,
                                       when=when, host_arch=host_arch)
                return result
            store.publish(trigger_path, records)
            return {'status': 'idle', 'message': 'Update state is consistent.'}
    except StateBusy:
        return {'status': 'busy', 'message': 'An update operation owns the state; nothing to reconcile.'}


# --------------------------------------------------------------------- media

def decode_mountinfo_path(token):
    return re.sub(r'\\([0-7]{3})', lambda m: chr(int(m.group(1), 8)), token)


def parse_mountinfo(mountinfo_text):
    mounts = {}
    for line in mountinfo_text.splitlines():
        if ' - ' not in line:
            continue
        left, _, right = line.partition(' - ')
        left_parts = left.split()
        right_parts = right.split()
        if len(left_parts) < 5 or len(right_parts) < 2:
            continue
        mountpoint = decode_mountinfo_path(left_parts[4])
        fstype = right_parts[0]
        source = decode_mountinfo_path(right_parts[1])
        mounts[mountpoint] = (fstype, source)
    return mounts


def find_update_media(*, runner=None, media_root=DEFAULT_MEDIA_ROOT, mountinfo_text=None):
    """Mount the block device labelled WROS-UPDATE read-only under media_root/<n>."""
    run = runner or default_runner
    media_root = Path(media_root)
    listing = run(['/usr/bin/lsblk', '-rn', '-o', 'PATH,FSTYPE,LABEL,TYPE'])
    if listing.returncode != 0:
        fail('cannot list block devices')
    candidates = []
    for line in listing.stdout.splitlines():
        fields = line.split(' ')
        if len(fields) != 4:
            continue
        path, fstype, label, kind = fields
        if (label == MEDIA_LABEL and fstype in MEDIA_FILESYSTEMS and kind in ('part', 'disk', 'rom', 'loop')
                and DEVICE_RE.fullmatch(path) is not None):
            candidates.append((path, fstype))
    if not candidates:
        return {'status': 'none'}
    if mountinfo_text is None:
        mountinfo_text = Path('/proc/self/mountinfo').read_text(encoding='utf-8', errors='replace')
    mounts = parse_mountinfo(mountinfo_text)
    for mountpoint, (_, source) in sorted(mounts.items()):
        if any(source == path for path, _ in candidates) and Path(mountpoint).parent == media_root:
            return {'status': 'mounted', 'path': mountpoint}
    path, fstype = sorted(candidates)[0]
    # The launchers run under umask 077, which turns a requested 0755 into 0700; the review
    # runs as the writer and must traverse to the bundle, so set the modes explicitly.
    created = [p for p in (*reversed(media_root.parents), media_root) if not p.exists()]
    media_root.mkdir(parents=True, exist_ok=True)
    for directory in dict.fromkeys((*created, media_root)):
        os.chmod(directory, 0o755)
    for index in range(16):
        target = media_root / str(index)
        if str(target) not in mounts and not target.exists():
            break
    else:
        fail('too many update media mounts')
    target.mkdir()
    os.chmod(target, 0o755)
    # vfat has no owners or modes of its own: without masks it takes the helper's umask.
    options = 'ro,nodev,nosuid,noexec' + {'ext4': ',noload', 'vfat': ',dmask=0022,fmask=0133'}.get(fstype, '')
    mounted = run(['/usr/bin/mount', '-t', fstype, '-o', options, '--', path, str(target)])
    if mounted.returncode != 0:
        target.rmdir()
        fail('could not mount the update media read-only')
    return {'status': 'mounted', 'path': str(target)}


# ----------------------------------------------------------- recovery status

def inspect_dpkg_state(dpkg_dir=DEFAULT_DPKG_DIR):
    dpkg_dir = Path(dpkg_dir)
    status_file = dpkg_dir / 'status'
    updates_dir = dpkg_dir / 'updates'
    unconfigured = []
    interrupted = False
    if updates_dir.is_dir() and not updates_dir.is_symlink():
        for entry in sorted(updates_dir.iterdir()):
            if entry.name.isdigit() and entry.is_file() and entry.stat().st_size > 0:
                interrupted = True
    # A trigger queued here is not in the status file until dpkg next runs;
    # `dpkg --configure -a` incorporates and processes it.
    unincorp = dpkg_dir / 'triggers' / 'Unincorp'
    if unincorp.is_file() and not unincorp.is_symlink() and unincorp.stat().st_size > 0:
        interrupted = True
    if status_file.is_file() and not status_file.is_symlink():
        text = read_regular_bytes(status_file, LIMIT).decode('utf-8', errors='replace')
        for stanza in text.split('\n\n'):
            pkg_name = None
            pkg_status = None
            for line in stanza.splitlines():
                if line.startswith('Package:'):
                    pkg_name = line.partition(':')[2].strip()
                elif line.startswith('Status:'):
                    pkg_status = line.partition(':')[2].strip()
            if pkg_name and pkg_status:
                if pkg_status not in (
                    'install ok installed',
                    'deinstall ok config-files',
                    'purge ok not-installed',
                    'hold ok installed',
                ):
                    interrupted = True
                    if pkg_name not in unconfigured:
                        unconfigured.append(pkg_name)
    return interrupted, sorted(unconfigured)


def inspect_recovery(
    mountinfo_text=None,
    dpkg_dir=DEFAULT_DPKG_DIR,
    state_dir=DEFAULT_STATE_DIR,
    trigger_path=DEFAULT_TRIGGER,
    root_dir=Path('/'),
):
    """Unprivileged: reads only the public status mirror and world-readable dpkg state."""
    if mountinfo_text is None:
        mountinfo_text = Path('/proc/self/mountinfo').read_text(encoding='utf-8', errors='replace')
    mounts = parse_mountinfo(mountinfo_text)
    root_fs, root_dev = mounts.get('/', ('unknown', 'unknown'))
    separate_home = '/home' in mounts
    rollback_capable = False  # A directory/layout alone is no tested bootable rollback protocol.
    if root_fs == 'ext4':
        strategy = (
            'Root filesystem is ext4: automatic filesystem snapshot rollback is not available '
            '(installing Snapper on ext4 does not provide rollback). Create an encrypted Restic '
            'project backup before applying updates and use Debian live/rescue media for system recovery.'
        )
    else:
        strategy = (
            f'Root filesystem is {root_fs}: automatic filesystem rollback is not qualified on this layout. '
            'Create an encrypted Restic project backup before applying updates.'
        )
    interrupted_dpkg, unconfigured = inspect_dpkg_state(dpkg_dir=dpkg_dir)
    state_dir = Path(state_dir)
    trigger_path = Path(trigger_path)
    public = read_public_status(state_dir) or {}
    update_state = public.get('update_state', 'idle')
    staged_update = update_state == 'staged' and owned_trigger(trigger_path, state_dir / 'staged')
    staged_packages = []
    if staged_update:
        staged_packages = [p for p in public.get('packages', []) if isinstance(p, str)][:MAX_PACKAGES]
    elif update_state == 'staged':
        update_state = 'idle'  # the trigger is gone or foreign: nothing will be applied
    parts = [
        f'Root filesystem: {root_fs} '
        + ('(btrfs snapshot rollback available)' if rollback_capable else '(no automatic filesystem rollback)'),
        f'dpkg: {"interrupted (" + (", ".join(unconfigured) or "queued triggers") + ")" if interrupted_dpkg else "clean"}',
        f'staged offline update: {len(staged_packages)} package(s)' if staged_update else 'no offline update staged',
    ]
    last_state = public.get('last_state')
    if last_state in ('completed', 'failed', 'applying', 'interrupted', 'recovered'):
        detail = ''
        if last_state in ('failed', 'interrupted') and public.get('error'):
            detail = f' ({clean_text(public["error"])})'
        parts.append(f'last offline update: {last_state}{detail}')
    if public.get('unresolved_failure') and last_state not in ('failed', 'interrupted'):
        parts.append('an earlier update failure is unresolved; run repair')
    return {
        'root_filesystem': root_fs,
        'root_device': root_dev,
        'separate_home': separate_home,
        'rollback_capable': rollback_capable,
        'rollback_strategy': strategy,
        'interrupted_dpkg': interrupted_dpkg,
        'unconfigured_packages': unconfigured,
        'staged_update': staged_update,
        'staged_packages': staged_packages,
        'update_state': update_state,
        'status': ' · '.join(parts) + '.',
    }


# --------------------------------------------------------------------- review

def review_bundle(bundle_dir, *, keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, dpkg_dir=DEFAULT_DPKG_DIR,
                  state_dir=DEFAULT_STATE_DIR, now=None, host_arch=None, suites=PINNED_SUITES, use_apt=True):
    """Unprivileged review: full authentication plus APT's own simulated plan."""
    public = read_public_status(state_dir) or {}
    last_applied = None
    if public.get('last_applied_date'):
        last_applied = parse_iso(public['last_applied_date'], 'last applied')
    with BundleDir(bundle_dir) as bundle:
        raw = verify_open_bundle(bundle, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir, now=now,
                                 host_arch=host_arch, last_applied=last_applied, suites=suites)
        if use_apt:
            with tempfile.TemporaryDirectory(prefix='wros-update-review-') as scratch:
                debs = [Path(bundle_dir) / p['filename'] for p in raw['packages']]
                plan_with_apt(debs, raw['packages'], dpkg_dir, scratch)
    packages = []
    for pkg in raw['packages']:
        before = raw['installed'].get(pkg['name'])
        packages.append({'name': pkg['name'], 'from_version': before['version'] if before else None,
                         'to_version': pkg['version'], 'action': raw['actions'][pkg['name']]})
    return {
        'status': 'reviewed',
        'review_id': raw['review_id'],
        'signer': raw['fingerprint'],
        'suite': raw['release']['suite'],
        'date': raw['release']['date'].isoformat(),
        'valid_until': raw['release']['valid_until'].isoformat(),
        'packages': packages,
        'removals': [],
        'backup_recommended': True,
    }


# ------------------------------------------------------------------ entry code

LAUNCHER_VERBS = ('stage', 'cancel', 'repair', 'media', 'fetch', 'apply')


# ----------------------------------------------------------- online updates

class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """The update URL is fixed; a redirect anywhere else is refused, not followed."""

    def redirect_request(self, *args, **kwargs):
        return None


def http_get(url, limit, opener=None, timeout=ONLINE_TIMEOUT, seen=None):
    """GET with a size cap. Network and HTTP failures raise OSError (URLError). `seen` receives
    the server's Date header, to tell a lapsed signature from a wrong local clock."""
    opener = opener or urllib.request.build_opener(_NoRedirect)
    with opener.open(url, timeout=timeout) as response:
        if getattr(response, 'status', 200) != 200:
            raise urllib.error.URLError(f'HTTP {response.status}')
        if seen is not None:
            seen['date'] = response.headers.get('Date')
        data = response.read(limit + 1)
    if len(data) > limit:
        fail('update server response exceeds the expected size')
    return data


def read_online_metadata(base_url, *, now, keyring, signers, package_keyring, opener, seen=None):
    """Signed InRelease and the Packages index it authenticates, from the update server."""
    inrelease = http_get(base_url + 'InRelease', 1024 * 1024, opener, seen=seen)
    _, release_text = verify_signed_metadata({'InRelease': inrelease}, keyring=keyring, signers=signers,
                                             package_keyring=package_keyring)
    release = parse_release(release_text, now=now)
    relative = select_packages_entry(release['entries'])
    expected_sha, expected_size = release['entries'][relative]
    packages_raw = http_get(base_url + relative, 4 * 1024 * 1024, opener)
    if hashlib.sha256(packages_raw).hexdigest() != expected_sha or len(packages_raw) != expected_size:
        fail('Packages index SHA-256 or byte size does not match signed Release metadata')
    packages = parse_packages_index(packages_raw.decode('utf-8', errors='strict'))
    wros = next((p for p in packages if p['name'] == 'wros'), None)
    if wros is None:
        fail('the signed update index lists no wros package')
    return inrelease, relative, packages_raw, packages, wros


CLOCK_FAILURES = ('Date is in the future', 'has expired (Valid-Until passed)')
# No correct clock reads earlier than this release. Before it, gpgv refuses the signing key
# as created in the future and TLS refuses certificates: the clock is at fault, not the files.
CLOCK_FLOOR = datetime(2026, 11, 1, tzinfo=timezone.utc)
SERVER_CLOCK_TOLERANCE = timedelta(hours=1)
CLOCK_TLS_CODES = (9, 10)  # X509_V_ERR_CERT_NOT_YET_VALID, X509_V_ERR_CERT_HAS_EXPIRED


def tls_clock_failure(exc):
    """A TLS certificate refused as not yet valid or expired: the local clock is wrong."""
    reason = getattr(exc, 'reason', exc)
    return isinstance(reason, ssl.SSLCertVerificationError) and getattr(reason, 'verify_code', None) in CLOCK_TLS_CODES


def server_agrees(seen, moment):
    """The update server's Date header is within an hour of this clock."""
    try:
        server = parsedate_to_datetime(seen.get('date') or '')
    except (TypeError, ValueError):
        return False
    return server.tzinfo is not None and abs(server - moment) <= SERVER_CLOCK_TOLERANCE


def check_online(*, base_url=ONLINE_URL, now=None, dpkg_dir=DEFAULT_DPKG_DIR, keyring=DEFAULT_KEYRING,
                 signers=DEFAULT_SIGNERS, package_keyring=None, opener=None):
    """Unprivileged: is a newer signed WrOS published? Fetches only InRelease and Packages."""
    moment = clock(now)
    reads = moment.astimezone().strftime('%Y-%m-%d %H:%M')
    installed = load_installed_dpkg_database(dpkg_dir=dpkg_dir).get('wros', {}).get('version')
    result = {'status': 'invalid', 'version': None, 'installed': installed, 'clock': reads}
    seen = {}
    try:
        _, _, _, _, wros = read_online_metadata(base_url, now=now, keyring=keyring, signers=signers,
                                                package_keyring=package_keyring, opener=opener, seen=seen)
    except (OSError, TimeoutError) as exc:
        if getattr(exc, 'code', None) == 404 and str(getattr(exc, 'url', '')).endswith('/InRelease'):
            return result | {'status': 'unpublished'}  # the server answered: nothing is published there yet
        wrong = tls_clock_failure(exc) or moment < CLOCK_FLOOR
        return result | {'status': 'clock' if wrong else 'unreachable'}
    except ValueError as exc:
        text = str(exc)
        if 'Packages index SHA-256' in text:
            status = 'unreachable'  # mid-publish: the signed index and its Packages disagree for a while
        elif moment < CLOCK_FLOOR:
            status = 'clock'
        elif 'has expired (Valid-Until passed)' in text and server_agrees(seen, moment):
            status = 'expired'  # this clock is right; the server stopped re-signing
        elif any(failure in text for failure in CLOCK_FAILURES):
            status = 'clock'
        else:
            status = 'invalid'
        return result | {'status': status}
    newer = installed is None or compare_dpkg_versions(wros['version'], 'gt', installed)
    return result | {'status': 'available' if newer else 'current', 'version': wros['version']}


def fetch_online(*, base_url=ONLINE_URL, cache=ONLINE_CACHE, now=None, dpkg_dir=DEFAULT_DPKG_DIR,
                 keyring=DEFAULT_KEYRING, signers=DEFAULT_SIGNERS, package_keyring=None, host_arch=None,
                 opener=None):
    """Root (via wros-update-fetch): download a newer signed update into the cache, checking every
    archive against the signed index, and publish the folder only once the whole bundle verifies.
    The launcher's umask is 077; review reads the result unprivileged, so it is published 0755/0644."""
    previous = os.umask(0o022)
    try:
        return _fetch_online(base_url=base_url, cache=cache, now=now, dpkg_dir=dpkg_dir, keyring=keyring,
                             signers=signers, package_keyring=package_keyring, host_arch=host_arch, opener=opener)
    finally:
        os.umask(previous)


def _fetch_online(*, base_url, cache, now, dpkg_dir, keyring, signers, package_keyring, host_arch, opener):
    try:
        inrelease, relative, packages_raw, packages, wros = read_online_metadata(
            base_url, now=now, keyring=keyring, signers=signers, package_keyring=package_keyring, opener=opener)
    except (OSError, TimeoutError):
        fail('could not download the update index from the WrOS update server')
    installed = load_installed_dpkg_database(dpkg_dir=dpkg_dir).get('wros', {}).get('version')
    if installed is not None and not compare_dpkg_versions(wros['version'], 'gt', installed):
        fail(f'WrOS is already up to date ({installed})')
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    for directory in (cache.parent, cache):  # may predate this umask
        os.chmod(directory, 0o755)
    target = cache / hashlib.sha256(inrelease).hexdigest()[:16]
    partial = cache / ('.partial-' + secrets.token_hex(8))
    partial.mkdir(mode=0o755)
    try:
        (partial / 'InRelease').write_bytes(inrelease)
        (partial / relative).parent.mkdir(parents=True, exist_ok=True)
        (partial / relative).write_bytes(packages_raw)
        for pkg in packages:
            try:
                data = http_get(base_url + pkg['filename'], pkg['size'], opener, timeout=60)
            except (OSError, TimeoutError):
                fail(f'could not download {Path(pkg["filename"]).name} from the WrOS update server')
            if len(data) != pkg['size'] or hashlib.sha256(data).hexdigest() != pkg['sha256']:
                fail(f'archive {pkg["filename"]} failed SHA-256 or size check against signed Packages')
            destination = partial / pkg['filename']
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        verify_bundle(partial, keyring=keyring, signers=signers, dpkg_dir=dpkg_dir, now=now,
                      host_arch=host_arch, package_keyring=package_keyring)
        if target.exists():
            shutil.rmtree(target)
        os.replace(partial, target)
    except BaseException:
        shutil.rmtree(partial, ignore_errors=True)
        raise
    for older in cache.iterdir():
        if older != target and older.is_dir() and re.fullmatch('[0-9a-f]{16}', older.name):
            shutil.rmtree(older, ignore_errors=True)
    return {'status': 'fetched', 'path': str(target), 'version': wros['version']}


def launcher_main(verb, argv, pkexec_uid, *, euid=None, runner=None, **paths):
    """Shared hardening for the pkexec launchers: exact argc, strict tokens, root + polkit caller."""
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
        check_bundle_path(argv[0], paths.pop('allowed_prefixes', ALLOWED_MEDIA_PREFIXES))
        if HEX64_RE.fullmatch(argv[1]) is None:
            fail('invalid review id')
        return stage_update(argv[0], argv[1], allowed_prefixes=None, **paths)
    if verb == 'cancel':
        if argv:
            fail('usage: wros-update-cancel')
        return cancel_staged(**{k: v for k, v in paths.items() if k in ('state_dir', 'trigger_path', 'now')})
    if verb == 'repair':
        if argv:
            fail('usage: wros-update-repair')
        return repair_updates(runner=runner, **paths)
    if verb == 'apply':
        if argv:
            fail('usage: wros-update-apply')
        return apply_staged_now(runner=runner, **{k: v for k, v in paths.items()
                                                  if k in ('state_dir', 'trigger_path', 'dpkg_dir', 'keyring',
                                                           'signers', 'host_arch', 'now')})
    if verb == 'fetch':
        if argv:
            fail('usage: wros-update-fetch')
        return fetch_online(**{k: v for k, v in paths.items()
                               if k in ('base_url', 'cache', 'now', 'dpkg_dir', 'keyring', 'signers', 'host_arch')})
    if argv != ['mount']:
        fail('usage: wros-update-media mount')
    return find_update_media(runner=runner, **{k: v for k, v in paths.items() if k in ('media_root', 'mountinfo_text')})


def continue_boot_after_failure(run=None):
    """A failed or unverified offline update is never followed by an automatic reboot, but the
    system must not stay in system-update.target with no login. Its trigger is already gone, so
    continue this boot into multi-user.target: wros-update-reconcile runs before greetd, and the
    writer sees the failure and the Repair action in Settings."""
    run = run or subprocess.run
    try:
        run(['/usr/bin/systemctl', '--no-block', 'isolate', 'multi-user.target'],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            timeout=15, check=False, env=safe_env())
    except (OSError, subprocess.SubprocessError):
        pass  # best effort: the journal already records the failure and the trigger is removed


def reserved_role(environ, marker):
    """run-offline-boot and reconcile exist only for systemd PID 1 children; never via pkexec."""
    if (os.geteuid() != 0 or os.getppid() != 1 or 'PKEXEC_UID' in environ
            or environ.get(marker) != '1'):
        fail(UNAVAILABLE)


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
                f"Signature, signer pin and contents verified for {verified['package_count']} package(s). "
                'Use Review to see what would change before staging.'
            ),
        }
    elif argv == ['run-offline-boot']:
        reserved_role(environ, 'WROS_SYSTEMD_OFFLINE_UPDATE')
        ours = owned_trigger(DEFAULT_TRIGGER, DEFAULT_STATE_DIR / 'staged')
        try:
            result = execute_offline_boot_transaction()
        except ValueError:
            if ours:
                continue_boot_after_failure()
            raise
        if ours and result.get('status') == 'completed':
            # Reboot only after a verified, fully completed transaction; never for failed,
            # foreign or unverified ones (the operator or reconcile handles those).
            subprocess.run(['/usr/bin/systemctl', '--no-block', 'reboot'],
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=15, check=True, env=safe_env())
    elif argv == ['reconcile']:
        reserved_role(environ, 'WROS_SYSTEMD_RECONCILE')
        result = reconcile()
    elif argv == ['rescue-repair']:
        # Reserved for wros-rescue running as root inside the chroot of an installed system.
        if os.geteuid() != 0 or 'PKEXEC_UID' in environ or environ.get('WROS_RESCUE') != '1':
            fail(UNAVAILABLE)
        result = repair_updates()
    else:
        fail(UNAVAILABLE)
    print(json.dumps(result, sort_keys=True))
    return 1 if argv == ['rescue-repair'] and result.get('status') == 'failed' else 0
