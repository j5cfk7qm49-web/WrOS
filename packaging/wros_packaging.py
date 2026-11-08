#!/usr/bin/env python3
"""Shared packaging data and helpers for the WrOS installed-system package.

One manifest (packaging/manifest.json) is the file list for both "stage into an
image root" (os/scripts/install-files.sh) and "build the Debian package"
(tools/build-wros-debs.py), and the image audit reads the same data back. The
module also validates the update trust inputs (keyring + signer pin) and enables
the update units. It never touches anything outside the paths it is given.

CLI (used by install-files.sh):
    wros_packaging.py version
    wros_packaging.py check-trust
    wros_packaging.py install-trust ROOT
    wros_packaging.py enable-units ROOT
"""
from dataclasses import dataclass
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

PACKAGE_DIR = Path(__file__).resolve().parent
REPO = PACKAGE_DIR.parent
VERSION_RE = re.compile(r'^[0-9][A-Za-z0-9.+~-]{0,63}\Z')
FINGERPRINT_RE = re.compile(r'^[0-9A-Fa-f]{40}\Z')
COMMIT_RE = re.compile(r'^[0-9a-f]{7,64}\Z')
KEYRING_DEST = 'usr/share/keyrings/wros-archive-keyring.gpg'
SIGNERS_DEST = 'usr/share/wros/update-signers'
# The package's own keyring: the pinned primary plus the CI signing subkey. The engine
# refuses it unless every primary in it is pinned; the image keyring and pin stay image inputs.
PACKAGE_KEYRING_DEST = 'usr/share/wros/update-keyring.gpg'
MAX_KEYRING = 1024 * 1024
# A UID containing one of these words marks a throwaway/test identity. Tests in
# this repository generate such keys at run time, so no static fingerprint list
# can enumerate them; production media must never trust them.
FIXTURE_UID_RE = re.compile(
    r'(?<![A-Za-z])(test|tests|testing|fixture|fixtures|throwaway|ephemeral|disposable|dummy|fake|insecure|sample)(?![A-Za-z])',
    re.IGNORECASE)
FIXTURE_FINGERPRINT_FILE = PACKAGE_DIR / 'fixture-fingerprints.txt'


class PackagingError(ValueError):
    """Any refusal; the message is safe to show (never contains key material)."""


# ---------------------------------------------------------------- versions

def check_version(version):
    if not isinstance(version, str) or not VERSION_RE.match(version):
        raise PackagingError('not a Debian upstream-style version (no epoch): ' + repr(version))
    return version


def read_version(repo=REPO):
    path = Path(repo) / 'os/VERSION'
    if path.is_symlink() or not path.is_file():
        raise PackagingError('os/VERSION is missing')
    lines = path.read_text(encoding='utf-8').splitlines()
    if not lines:
        raise PackagingError('os/VERSION is empty')
    return check_version(lines[0].strip())


def source_date_epoch(repo=REPO, environ=None):
    """SOURCE_DATE_EPOCH, else the committer time of HEAD; never the wall clock."""
    environ = os.environ if environ is None else environ
    value = environ.get('SOURCE_DATE_EPOCH', '')
    if value:
        if not value.isdigit():
            raise PackagingError('SOURCE_DATE_EPOCH must be an integer')
        return int(value)
    try:
        out = subprocess.run(['git', '-c', 'safe.directory=' + str(repo), '-C', str(repo), 'log', '-1',
                              '--format=%ct'], check=True, capture_output=True, text=True).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        out = ''
    if not out.isdigit():
        raise PackagingError('set SOURCE_DATE_EPOCH (no git history available to derive a build date)')
    return int(out)


def source_commit(repo=REPO, environ=None):
    """WROS_SOURCE_COMMIT, else the VCS head of *repo*. Never silently 'unknown'."""
    environ = os.environ if environ is None else environ
    value = environ.get('WROS_SOURCE_COMMIT', '')
    if not value:
        try:
            value = subprocess.run(['git', '-c', 'safe.directory=' + str(repo), '-C', str(repo), 'rev-parse',
                                    'HEAD'], check=True, capture_output=True, text=True).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            value = ''
    if not value:
        if environ.get('WROS_ALLOW_UNKNOWN_COMMIT') == '1':
            return 'unknown'
        raise PackagingError('cannot determine the source commit; set WROS_SOURCE_COMMIT')
    if not COMMIT_RE.match(value):
        raise PackagingError('source commit is not a git object name')
    return value


def release_text(version, commit, epoch, manifest):
    date = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime('%Y-%m-%d')
    return ('WROS_VERSION=%s\nWROS_COMMIT=%s\nBUILD_DATE=%s\nGRIMOIRE_BASE=%s\nBASE_COMMIT=%s\n'
            % (check_version(version), commit, date, manifest['grimoire_base'],
               manifest['upstream_base_commit'])).encode()


# ---------------------------------------------------------------- manifest

@dataclass(frozen=True)
class Spec:
    dest: str                # image-root-relative, no leading slash
    mode: int
    klass: str               # code | config | doc | binary | generated
    source: Path = None      # repository file, when the content comes from the repo
    generated: str = None    # binary:NAME | release | keymaps:NAME
    conffile: bool = False
    pending: bool = False
    strip: bool = False
    missing: bool = False    # pending entry whose source is not in this tree yet


def load_manifest(repo=REPO):
    path = Path(repo) / 'packaging/manifest.json'
    manifest = json.loads(path.read_text(encoding='utf-8'))
    if manifest.get('schema') != 1:
        raise PackagingError('unsupported packaging manifest schema')
    return manifest


def _safe_dest(dest):
    parts = dest.split('/')
    if (dest.startswith('/') or not dest or any(p in ('', '.', '..') for p in parts)):
        raise PackagingError('unsafe package path: ' + dest)
    return dest


def _mode(text):
    mode = int(text, 8)
    if mode & ~0o755 or not mode & 0o400:
        raise PackagingError('manifest mode must be 0644 or 0755: ' + text)
    return mode


def _regular(repo, relative):
    path = Path(repo) / relative
    return path if path.is_file() and not path.is_symlink() else None


def expand(manifest, repo=REPO):
    """Every package file as a Spec, in sorted order. Globs and trees are
    expanded against *repo*; a pending entry whose source is absent is kept with
    missing=True; any other missing source is an error."""
    repo = Path(repo)
    specs = {}

    def add(spec):
        if spec.dest in specs:
            raise PackagingError('duplicate package path: ' + spec.dest)
        specs[spec.dest] = spec

    for item in manifest['files']:
        klass = item['class']
        pending = bool(item.get('pending'))
        if 'glob' in item:
            base = _safe_dest(item['dest_dir'])
            pattern = item['glob']
            directory, _, name_pattern = pattern.rpartition('/')
            for path in sorted((repo / directory).glob(name_pattern)):
                if path.is_file() and not path.is_symlink():
                    add(Spec(base + '/' + path.name, _mode(item['mode']), klass, source=path, pending=pending))
        elif 'tree' in item:
            base = _safe_dest(item['dest_dir'])
            root = repo / item['tree']
            if root.is_symlink() or not root.is_dir():
                raise PackagingError('missing source tree: ' + item['tree'])
            for path in sorted(root.rglob('*')):
                if path.is_symlink():
                    raise PackagingError('symlink in source tree: ' + str(path.relative_to(repo)))
                if path.is_file():
                    mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
                    add(Spec(base + '/' + path.relative_to(root).as_posix(), mode, klass, source=path))
        elif item.get('generated') == 'keymaps':
            base = _safe_dest(item['dest_dir'])
            for name in item['names']:
                add(Spec(base + '/' + name, _mode(item['mode']), klass, generated='keymaps:' + name))
        elif 'generated' in item:
            add(Spec(_safe_dest(item['dest']), _mode(item['mode']), klass, generated=item['generated'],
                     conffile=bool(item.get('conffile')), strip=bool(item.get('strip'))))
        else:
            dest = _safe_dest(item['dest'])
            candidates = item.get('source_candidates') or [item['source']]
            source = next((p for p in (_regular(repo, c) for c in candidates) if p), None)
            if source is None and not pending:
                raise PackagingError('missing source file for %s: %s' % (dest, candidates[0]))
            add(Spec(dest, _mode(item['mode']), klass, source=source, conffile=bool(item.get('conffile')),
                     pending=pending, missing=source is None))
    return [specs[k] for k in sorted(specs)]


def conffile_paths(specs):
    return ['/' + s.dest for s in specs if s.conffile]


def postinst(manifest, stage):
    """The package's only maintainer script. Online updates install the .deb with dpkg, which had no
    way to enable a new unit or add a boot-menu entry. Links are made only where none exists (a
    masked or removed link is the administrator's). Under DPKG_ROOT (image staging, chrootless) the
    boot menu is left to the installer. The naming migration must succeed before configuration
    completes. Every enabled unit is checked at build time against the packaged [Install] section."""
    lines = ['#!/bin/sh',
             '# Generated by packaging/wros_packaging.py from packaging/manifest.json.',
             'set -e',
             '[ "$1" = configure ] || exit 0',
             'root=${DPKG_ROOT:-}',
             'enable_unit() {',
             '    wants="$root/etc/systemd/system/$2.wants"',
             '    if [ ! -e "$wants/$1" ] && [ ! -L "$wants/$1" ]; then',
             '        mkdir -p "$wants" && ln -s "../$1" "$wants/$1" || echo "wros: could not enable $1" >&2',
             '    fi',
             '}']
    for item in manifest['enable']:
        unit, target = item['unit'], item['target']
        if not re.fullmatch(r'[A-Za-z0-9@._-]+', unit) or not re.fullmatch(r'[A-Za-z0-9@._-]+', target):
            raise PackagingError('unsafe unit name in manifest: %s -> %s' % (unit, target))
        unit_path = Path(stage) / 'etc/systemd/system' / unit
        if unit_path.is_symlink() or not unit_path.is_file():
            raise PackagingError('unit to enable is not packaged: ' + unit)
        if item.get('verify_install_section'):
            declared = install_section(unit_path.read_text(encoding='utf-8'))
            if target not in declared['WantedBy'] + declared['RequiredBy']:
                raise PackagingError('%s has no [Install] WantedBy/RequiredBy=%s; refusing to enable it'
                                     % (unit, target))
        lines.append('enable_unit %s %s' % (unit, target))
    lines += ['if [ -f "$root/usr/libexec/wros-name-migrate" ]; then',
              '    /usr/bin/python3 -I "$root/usr/libexec/wros-name-migrate" "$root"',
              'fi',
              'system="$root/etc/systemd/system"',
              'if [ -n "${2:-}" ] && dpkg --compare-versions "$2" le "0.1.0~rc6"; then',
              '    for unit in sleep.target suspend.target; do',
              '        path="$system/$unit"',
              '        if [ "$(readlink "$path")" = /dev/null ]; then',
              '            rm -f "$path" || echo "wros: could not retire the old $unit mask" >&2',
              '        fi',
              '    done',
              'fi',
              'if [ ! -e "$system/ctrl-alt-del.target" ] && [ ! -L "$system/ctrl-alt-del.target" ]; then',
              '    ln -s /dev/null "$system/ctrl-alt-del.target" || echo "wros: could not mask Ctrl-Alt-Del" >&2',
              'fi',
              # The login screen. The package owns only its copy in /usr/lib/wros; greetd runs the
              # unowned /usr/bin copy, refreshed here. Going back to a version without the file then
              # removes only the package copy and never the login itself.
              'greeter="$root/usr/bin/wros-greeter"',
              'packaged="$root/usr/lib/wros/wros-greeter"',
              'if [ -f "$packaged" ] && [ -f "$greeter" ] && [ ! -L "$greeter" ] && ! cmp -s "$packaged" "$greeter"; then',
              '    if cp "$packaged" "$greeter.wros-new" && chmod 0755 "$greeter.wros-new"; then',
              '        mv -f "$greeter.wros-new" "$greeter" || echo "wros: could not update the login screen" >&2',
              '    else',
              '        rm -f "$greeter.wros-new"; echo "wros: could not update the login screen" >&2',
              '    fi',
              'fi',
              # The Ratatui start screen (docs/RATATUI_BOOT.md), only while its switch is on: the boot
              # image takes the new wros-boot. With the switch off the image stays Debian's.
              'if command -v systemd-sysusers >/dev/null 2>&1; then',
              '    systemd-sysusers --root="${root:-/}" wros-graphics.conf || echo "wros: native login permissions will be retried at boot" >&2',
              'fi',
              'if [ -z "$root" ] && { [ ! -e /etc/wros/boot-screen ] || [ "$(cat /etc/wros/boot-screen 2>/dev/null)" = ratatui ]; } && [ -x /usr/sbin/update-initramfs ]; then',
              '    /usr/sbin/update-initramfs -u >/dev/null 2>&1 || echo "wros: the start screen updates with the next boot image" >&2',
              'fi',
              'if [ -z "$root" ] && [ -x /usr/sbin/update-grub ] && [ -e /boot/grub/grub.cfg ]; then',
              '    /usr/sbin/update-grub >/dev/null 2>&1 || echo "wros: update-grub failed; the boot menu is unchanged" >&2',
              'fi',
              'exit 0']
    return ('\n'.join(lines) + '\n').encode()


# ---------------------------------------------------------------- inputs

def compile_display_helper(repo, output):
    """Build wros-display-hw exactly as the image always has (hardened flags)."""
    repo = Path(repo)
    cflags = subprocess.run(['pkg-config', '--cflags', 'libdrm'], check=True, capture_output=True,
                            text=True).stdout.split()
    libs = subprocess.run(['pkg-config', '--libs', 'libdrm'], check=True, capture_output=True,
                          text=True).stdout.split()
    subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-U_FORTIFY_SOURCE',
                    '-D_FORTIFY_SOURCE=3', '-fstack-protector-strong', *cflags,
                    str(repo / 'os/helpers/wros-display-hw.c'), '-Wl,-z,relro,-z,now', '-o',
                    str(output), *libs], check=True)


def generate_keymaps(repo, output):
    subprocess.run([str(Path(repo) / 'os/scripts/generate-keymaps.sh'), str(output)], check=True)


def _stripped(path):
    data = Path(path).read_bytes()
    if not data.startswith(b'\x7fELF'):
        return data
    with tempfile.TemporaryDirectory(prefix='wros-strip-') as tmp:
        copy = Path(tmp) / 'binary'
        shutil.copyfile(path, copy)
        subprocess.run(['strip', str(copy)], check=True)
        return copy.read_bytes()


def content(spec, *, binaries_dir, keymaps_dir, release):
    """Bytes for one Spec."""
    if spec.generated is None:
        if spec.source is None:
            raise PackagingError('no source for ' + spec.dest)
        return spec.source.read_bytes()
    if spec.generated == 'release':
        return release
    kind, _, name = spec.generated.partition(':')
    directory = binaries_dir if kind == 'binary' else keymaps_dir
    if directory is None:
        raise PackagingError('build input directory not supplied for ' + spec.dest)
    path = Path(directory) / name
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise PackagingError('missing build input: %s (in %s)' % (name, directory))
    return _stripped(path) if spec.strip else path.read_bytes()


# ---------------------------------------------------------------- package build

def _control(manifest, version, installed_kib):
    package = manifest['package']
    lines = ['Package: ' + package['name'], 'Version: ' + version, 'Architecture: ' + package['architecture'],
             'Maintainer: ' + package['maintainer'], 'Installed-Size: %d' % installed_kib,
             'Depends: ' + ', '.join(package['depends']), 'Section: ' + package['section'],
             'Priority: ' + package['priority'], 'Description: ' + package['summary']]
    lines += [' ' + (row if row else '.') for row in package['description']]
    return ('\n'.join(lines) + '\n').encode()


GOOGLE_CLIENTS_DEST = 'usr/share/wros/google-client.json'


def google_clients_bytes(path):
    """WrOS's two Google clients (phone linking), from a file kept out of the repository."""
    try:
        clients = json.loads(Path(path).read_text(encoding='utf-8'))
    except (OSError, ValueError) as error:
        raise PackagingError('unreadable Google client file: %s' % error) from None
    ok = isinstance(clients, dict) and all(
        isinstance(clients.get(name), dict)
        and all(isinstance(clients[name].get(key), str) and clients[name][key] for key in ('client_id', 'client_secret'))
        for name in ('linking', 'mailbox'))
    if not ok:
        raise PackagingError('the Google client file needs linking and mailbox, each with client_id and client_secret')
    return (json.dumps({name: {key: clients[name][key] for key in ('client_id', 'client_secret')}
                        for name in ('linking', 'mailbox')}, sort_keys=True) + '\n').encode()


def build_deb(*, binaries_dir, version, output_dir, repo=REPO, keymaps_dir=None, display_helper=None,
              commit=None, epoch=None, environ=None, google_clients=None):
    """Build wros_<version>_amd64.deb in *output_dir* and return its path.

    Same inputs always give the same bytes: entries are written in sorted order
    with owner root:root, modes from the manifest and every mtime fixed to
    SOURCE_DATE_EPOCH. Unresolved pending sources are skipped with a warning on
    stderr; the image audit requires them regardless.
    """
    repo = Path(repo)
    version = check_version(version)
    manifest = load_manifest(repo)
    epoch = source_date_epoch(repo, environ) if epoch is None else int(epoch)
    commit = source_commit(repo, environ) if commit is None else commit
    release = release_text(version, commit, epoch, manifest)
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    specs = expand(manifest, repo)
    check_package_keyring(repo)
    with tempfile.TemporaryDirectory(prefix='wros-deb-') as tmp:
        tmp = Path(tmp)
        binaries = tmp / 'binaries'
        binaries.mkdir()
        if binaries_dir is None or not Path(binaries_dir).is_dir():
            raise PackagingError('--binaries-dir must name a directory holding the built grimoire binary')
        for path in Path(binaries_dir).iterdir():
            if path.is_file() and not path.is_symlink():
                shutil.copyfile(path, binaries / path.name)
        if display_helper is not None:
            shutil.copyfile(display_helper, binaries / 'wros-display-hw')
        if not (binaries / 'wros-display-hw').exists():
            compile_display_helper(repo, binaries / 'wros-display-hw')
        if not (binaries / 'wros-console-reset').exists():
            subprocess.run(['cc', '-static', '-Os', '-Wall', '-Wextra', '-Werror',
                            '-fstack-protector-strong', '-o', str(binaries / 'wros-console-reset'),
                            str(repo / 'os/helpers/wros-console-reset.c')], check=True)
        if keymaps_dir is None:
            keymaps_dir = tmp / 'keymaps'
            generate_keymaps(repo, keymaps_dir)
        stage = tmp / 'pkg'
        stage.mkdir(mode=0o755)
        os.chmod(stage, 0o755)
        md5 = []
        dirs = set()
        size = 0
        for spec in specs:
            if spec.missing:
                print('wros-packaging: pending file not in this tree, not packaged: ' + spec.dest,
                      file=sys.stderr)
                continue
            data = content(spec, binaries_dir=binaries, keymaps_dir=keymaps_dir, release=release)
            target = stage / spec.dest
            parent = target.parent
            parent.mkdir(parents=True, exist_ok=True)
            for ancestor in [parent, *parent.parents]:
                if ancestor == stage:
                    break
                dirs.add(ancestor)
            target.write_bytes(data)
            os.chmod(target, spec.mode)
            md5.append('%s  %s\n' % (hashlib.md5(data, usedforsecurity=False).hexdigest(), spec.dest))
            size += -(-len(data) // 1024)
        if google_clients is not None:
            # Phone linking needs WrOS's own Google clients; a build without them hides it.
            data = google_clients_bytes(google_clients)
            target = stage / GOOGLE_CLIENTS_DEST
            target.parent.mkdir(parents=True, exist_ok=True)
            for ancestor in [target.parent, *target.parent.parents]:
                if ancestor == stage:
                    break
                dirs.add(ancestor)
            target.write_bytes(data)
            os.chmod(target, 0o644)
            md5.append('%s  %s\n' % (hashlib.md5(data, usedforsecurity=False).hexdigest(), GOOGLE_CLIENTS_DEST))
            size += -(-len(data) // 1024)
        control_dir = stage / 'DEBIAN'
        control_dir.mkdir(mode=0o755)
        (control_dir / 'control').write_bytes(_control(manifest, version, size))
        (control_dir / 'postinst').write_bytes(postinst(manifest, stage))
        (control_dir / 'md5sums').write_text(''.join(sorted(md5, key=lambda row: row.split('  ', 1)[1])))
        conffiles = [p for p in conffile_paths(specs) if (stage / p[1:]).exists()]
        if conffiles:
            (control_dir / 'conffiles').write_text(''.join(p + '\n' for p in sorted(conffiles)))
        for directory in dirs:
            os.chmod(directory, 0o755)
        os.chmod(control_dir, 0o755)
        for name in ('control', 'md5sums', 'conffiles'):
            if (control_dir / name).exists():
                os.chmod(control_dir / name, 0o644)
        os.chmod(control_dir / 'postinst', 0o755)
        for path in [stage, *stage.rglob('*')]:
            os.utime(path, (epoch, epoch), follow_symlinks=False)
        out = output_dir / ('%s_%s_%s.deb' % (manifest['package']['name'], version,
                                              manifest['package']['architecture']))
        partial = output_dir / (out.name + '.partial')
        partial.unlink(missing_ok=True)
        env = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'LC_ALL': 'C', 'TZ': 'UTC',
               'SOURCE_DATE_EPOCH': str(epoch)}
        build_cmd = ['dpkg-deb', '--root-owner-group', '-Zxz']
        try:
            subprocess.run(build_cmd + ['--threads-max=1', '--build', str(stage), str(partial)],
                           check=True, env=env, capture_output=True)
        except subprocess.CalledProcessError as e:
            if b'unknown option --threads-max' in (e.stderr or b''):
                subprocess.run(build_cmd + ['--build', str(stage), str(partial)],
                               check=True, env=env, capture_output=True)
            else:
                raise
        os.replace(partial, out)
    return out


def deb_field(deb, field):
    return subprocess.run(['dpkg-deb', '--field', str(deb), field], check=True, capture_output=True,
                          text=True).stdout.strip()


# ---------------------------------------------------------------- OpenPGP keyring inspection

def _packets(data):
    """Yield (tag, body) for a binary OpenPGP packet stream."""
    pos, end = 0, len(data)
    while pos < end:
        first = data[pos]
        pos += 1
        if not first & 0x80:
            raise PackagingError('keyring is not a binary OpenPGP packet stream')
        if first & 0x40:
            tag = first & 0x3f
            if pos >= end:
                raise PackagingError('truncated keyring')
            octet = data[pos]
            pos += 1
            if octet < 192:
                length = octet
            elif octet < 224:
                if pos >= end:
                    raise PackagingError('truncated keyring')
                length = ((octet - 192) << 8) + data[pos] + 192
                pos += 1
            elif octet == 255:
                length = int.from_bytes(data[pos:pos + 4], 'big')
                pos += 4
            else:
                raise PackagingError('partial-length packets are not valid in a keyring')
        else:
            tag = (first >> 2) & 0x0f
            kind = first & 3
            if kind == 3:
                length = end - pos
            else:
                width = (1, 2, 4)[kind]
                length = int.from_bytes(data[pos:pos + width], 'big')
                pos += width
        if pos + length > end:
            raise PackagingError('truncated keyring packet')
        yield tag, data[pos:pos + length]
        pos += length


def inspect_keyring(data):
    """-> {'primaries': [FPR...], 'uids': [...], 'secret': bool}. Only v4 keys."""
    if data.lstrip()[:5] == b'-----':
        raise PackagingError('keyring must be a binary (dearmored) OpenPGP keyring')
    primaries, uids, secret = [], [], False
    for tag, body in _packets(data):
        if tag in (5, 7):
            secret = True
        elif tag == 6:
            if not body or body[0] != 4:
                raise PackagingError('only version 4 OpenPGP primary keys are supported')
            primaries.append(hashlib.sha1(b'\x99' + len(body).to_bytes(2, 'big') + body,
                                          usedforsecurity=False).hexdigest().upper())
        elif tag == 13:
            uids.append(body.decode('utf-8', 'replace'))
    return {'primaries': primaries, 'uids': uids, 'secret': secret}


def parse_pin(text):
    fingerprints = []
    for number, row in enumerate(text.splitlines(), 1):
        row = row.split('#', 1)[0].strip()
        if not row:
            continue
        if not FINGERPRINT_RE.match(row):
            raise PackagingError('signer pin line %d is not a 40-hex fingerprint' % number)
        if row.upper() not in fingerprints:
            fingerprints.append(row.upper())
    return fingerprints


def load_fixture_fingerprints(path=FIXTURE_FINGERPRINT_FILE):
    try:
        return set(parse_pin(Path(path).read_text(encoding='utf-8')))
    except OSError:
        raise PackagingError('fixture fingerprint list is missing') from None


def check_trust_material(keyring_bytes, pin_text, fixtures=None):
    """Validate a keyring + signer pin pair. Returns (pinned fingerprints, uids)."""
    if not keyring_bytes:
        raise PackagingError('update keyring is empty')
    if len(keyring_bytes) > MAX_KEYRING:
        raise PackagingError('update keyring is implausibly large')
    info = inspect_keyring(keyring_bytes)
    if info['secret']:
        raise PackagingError('update keyring contains secret key material; refusing to stage it')
    if not info['primaries']:
        raise PackagingError('update keyring holds no public primary key')
    pinned = parse_pin(pin_text)
    if not pinned:
        raise PackagingError('update signer pin is empty')
    absent = [f for f in pinned if f not in info['primaries']]
    if absent:
        raise PackagingError('pinned signer %s is not a primary key in the update keyring' % absent[0])
    extra = [f for f in info['primaries'] if f not in pinned]
    if extra:
        raise PackagingError('update keyring holds primary key %s that the signer pin does not name' % extra[0])
    fixtures = load_fixture_fingerprints() if fixtures is None else {f.upper() for f in fixtures}
    hit = [f for f in info['primaries'] if f in fixtures]
    if hit:
        raise PackagingError('update signer %s is a known test/fixture key; refusing production media' % hit[0])
    for uid in info['uids']:
        if FIXTURE_UID_RE.search(uid):
            raise PackagingError('update key identity %r looks like a test/fixture key; refusing production media'
                                 % uid)
    return pinned, info['uids']


def check_package_keyring(repo=REPO):
    """The committed package keyring must hold exactly the committed pinned primary."""
    trust = Path(repo) / 'os/config/update-trust'
    keyring = _read_input(trust / 'wros-archive-keyring.gpg', 'package update keyring')
    pin = _read_input(trust / 'update-signers', 'package update signer pin')
    pinned, _ = check_trust_material(keyring, pin.decode('utf-8'))
    return pinned


def pin_file_text(fingerprints):
    return ('# WrOS update signers: primary-key fingerprints trusted for update bundles.\n'
            + ''.join(f + '\n' for f in fingerprints))


def _read_input(path, label):
    if not path:
        raise PackagingError(label + ' is not set')
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise PackagingError(label + ' is not a regular file')
    data = path.read_bytes()
    if not data:
        raise PackagingError(label + ' is empty')
    return data


def read_trust_inputs(environ=None, fixtures=None):
    """Read and validate WROS_UPDATE_KEYRING / WROS_UPDATE_SIGNERS -> (keyring bytes, pinned list)."""
    environ = os.environ if environ is None else environ
    keyring = _read_input(environ.get('WROS_UPDATE_KEYRING'), 'WROS_UPDATE_KEYRING')
    pin = _read_input(environ.get('WROS_UPDATE_SIGNERS'), 'WROS_UPDATE_SIGNERS')
    try:
        pin_text = pin.decode('utf-8')
    except UnicodeDecodeError:
        raise PackagingError('WROS_UPDATE_SIGNERS is not UTF-8 text') from None
    pinned, _ = check_trust_material(keyring, pin_text, fixtures)
    return keyring, pinned


def install_trust(root, environ=None, fixtures=None):
    """Validate the build's update trust inputs and install them into *root*."""
    keyring, pinned = read_trust_inputs(environ, fixtures)
    root = Path(root)
    items = ((KEYRING_DEST, keyring), (SIGNERS_DEST, pin_file_text(pinned).encode()))
    # Check every destination before writing anything: no symlinked component, no odd target.
    for relative, _ in items:
        here = root
        for part in relative.split('/')[:-1]:
            here = here / part
            if here.is_symlink() or (here.exists() and not here.is_dir()):
                raise PackagingError('refusing symlinked or non-directory component: ' + relative)
        target = root / relative
        if target.is_symlink() or (target.exists() and not target.is_file()):
            raise PackagingError('refusing to replace non-regular ' + relative)
    for relative, data in items:
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        os.chmod(target.parent, 0o755)
        partial = target.with_name(target.name + '.partial')
        partial.write_bytes(data)
        os.chmod(partial, 0o644)
        os.replace(partial, target)
    return pinned


# ---------------------------------------------------------------- units

def install_section(text):
    """-> {'WantedBy': [...], 'RequiredBy': [...]} from the unit's [Install] section(s)."""
    result, active = {'WantedBy': [], 'RequiredBy': []}, False
    for row in text.splitlines():
        row = row.strip()
        if not row or row.startswith(('#', ';')):
            continue
        if row.startswith('['):
            active = row == '[Install]'
            continue
        if active and '=' in row:
            key, _, value = row.partition('=')
            key = key.strip()
            if key in result:
                result[key] += value.split()
    return result


def enable_units(root, manifest=None, log=None):
    """Create the wants symlinks, but only after the installed unit file itself
    declares the same target in [Install]. Returns the list of enabled units."""
    manifest = load_manifest() if manifest is None else manifest
    log = log or (lambda message: print('wros-packaging: ' + message, file=sys.stderr))
    system = Path(root) / 'etc/systemd/system'
    enabled = []
    for item in manifest['enable']:
        unit, target = item['unit'], item['target']
        unit_path = system / unit
        if unit_path.is_symlink() or not unit_path.is_file():
            if item.get('pending'):
                log('pending unit not present, not enabled: ' + unit)
                continue
            raise PackagingError('unit to enable is missing: ' + unit)
        if item.get('verify_install_section'):
            declared = install_section(unit_path.read_text(encoding='utf-8'))
            if target not in declared['WantedBy'] + declared['RequiredBy']:
                raise PackagingError('%s has no [Install] WantedBy/RequiredBy=%s; refusing to enable it'
                                     % (unit, target))
        wants = system / (target + '.wants')
        if wants.is_symlink():
            raise PackagingError('refusing symlinked ' + target + '.wants')
        wants.mkdir(parents=True, exist_ok=True)
        link = wants / unit
        if link.is_symlink() or link.exists():
            link.unlink()
        link.symlink_to('../' + unit)
        enabled.append(unit)
    return enabled


# ---------------------------------------------------------------- CLI

def main(argv):
    if len(argv) == 2 and argv[1] == 'version':
        print(read_version())
        return 0
    if len(argv) == 2 and argv[1] == 'check-trust':
        read_trust_inputs()
        return 0
    if len(argv) == 3 and argv[1] == 'install-trust':
        for fpr in install_trust(argv[2]):
            print('wros-packaging: update signer pinned: ' + fpr, file=sys.stderr)
        return 0
    if len(argv) == 3 and argv[1] == 'enable-units':
        for unit in enable_units(argv[2]):
            print('wros-packaging: enabled ' + unit, file=sys.stderr)
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (PackagingError, OSError, subprocess.SubprocessError) as exc:
        sys.exit('wros-packaging: ' + str(exc))
