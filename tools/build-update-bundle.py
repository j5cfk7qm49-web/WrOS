#!/usr/bin/env python3
"""Build a signed WrOS offline update bundle from already-built .deb files.

The signing key lives in a GnuPG home outside the repository; this tool never reads
secret key material itself, it only asks gpg to sign.  It refuses to sign unless the
key's PRIMARY fingerprint equals --expect-fingerprint, and it proves the result by
running the real update engine over the finished bundle with a keyring and pin built
from that same public key.

Output (a new, empty directory):
  pool/<name>_<version>_<arch>.deb   the packages, byte for byte
  Packages                           uncompressed index, sorted by package name
  Release                            Origin/Suite/Date/Valid-Until + SHA256 of Packages
  InRelease                          gpg --clearsign of Release
  Release.gpg                        gpg --detach-sign of Release
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
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parents[1]
GPG = '/usr/bin/gpg'
DPKG_DEB = '/usr/bin/dpkg-deb'
ENV = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C'}
FIELD_ORDER = ('Package', 'Version', 'Architecture', 'Pre-Depends', 'Depends', 'Breaks', 'Conflicts', 'Provides')
FINGERPRINT_RE = re.compile(r'^[0-9A-F]{40}$')


class BuildError(Exception):
    pass


def load_engine():
    path = REPO / 'os/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_update_engine_for_bundle_tool', str(path))
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
    """Primary fingerprint of the one secret key that key_id names (or refuse)."""
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


def deb_fields(deb):
    output = run([DPKG_DEB, '--field', str(deb), *FIELD_ORDER]).stdout.decode('utf-8')
    fields = {}
    key = None
    for line in output.splitlines():
        if line.startswith((' ', '\t')):
            if key:
                fields[key] += ' ' + line.strip()
            continue
        key, _, value = line.partition(':')
        fields[key] = value.strip()
    return {key: re.sub(r'\s+', ' ', value) for key, value in fields.items()}


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('debs', nargs='+', metavar='DEB', help='built .deb files to include')
    parser.add_argument('--out', required=True, help='new output directory (must not exist or be empty)')
    parser.add_argument('--gnupg-home', required=True, help='GNUPGHOME holding the secret signing key (outside the repo)')
    parser.add_argument('--key-id', required=True, help='key id, fingerprint or user id of the signing key')
    parser.add_argument('--expect-fingerprint', required=True,
                        help='40-hex PRIMARY fingerprint that must own --key-id; anything else is refused')
    parser.add_argument('--suite', default='wros-updates')
    parser.add_argument('--origin', default='WrOS')
    parser.add_argument('--date', help='Release Date as ISO 8601 UTC (default: SOURCE_DATE_EPOCH or now)')
    parser.add_argument('--valid-days', type=int, default=14, help='days of validity, 1 to 30 (default 14)')
    parser.add_argument('--use-primary', action='store_true',
                        help='sign with the primary key itself, not a signing subkey: for machines whose '
                             'keyring predates the subkey (rc1 learns it from rc2)')
    return parser.parse_args(argv)


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


def build(args):
    engine = load_engine()
    if not 1 <= args.valid_days <= 30:
        raise BuildError('--valid-days must be between 1 and 30')
    gnupg_home = Path(args.gnupg_home).resolve()
    if not gnupg_home.is_dir():
        raise BuildError('--gnupg-home is not a directory')
    if gnupg_home == REPO or REPO in gnupg_home.parents:
        raise BuildError('refusing a GNUPGHOME inside the repository; private keys never live in the repo')
    expected = normalize_fingerprint(args.expect_fingerprint)
    actual = primary_fingerprint(gnupg_home, args.key_id)
    if actual != expected:
        raise BuildError(f'refusing to sign: key {args.key_id} has primary fingerprint {actual}, '
                         f'not the expected {expected}')
    out = Path(args.out)
    if out.is_symlink() or (out.exists() and (not out.is_dir() or any(out.iterdir()))):
        raise BuildError('--out must not exist or must be an empty directory')
    out = out.absolute()

    packages = []
    seen = set()
    for raw in args.debs:
        deb = Path(raw)
        if deb.is_symlink() or not deb.is_file():
            raise BuildError(f'not a regular .deb file: {raw}')
        fields = deb_fields(deb)
        name, version, arch = fields.get('Package', ''), fields.get('Version', ''), fields.get('Architecture', '')
        if engine.PACKAGE_NAME_RE.fullmatch(name) is None or engine.VERSION_RE.fullmatch(version) is None:
            raise BuildError(f'{deb.name}: unsupported package name or version')
        if arch not in engine.ACCEPTED_ARCHITECTURES:
            raise BuildError(f'{deb.name}: architecture {arch} is not amd64 or all')
        if name in seen:
            raise BuildError(f'duplicate package {name}')
        seen.add(name)
        packages.append((name, version, arch, deb, fields))
    packages.sort(key=lambda item: item[0])

    moment = release_moment(args)
    until = moment + timedelta(days=args.valid_days)
    out.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='.wros-update-bundle-', dir=out.parent))
    try:
        (staging / 'pool').mkdir()
        stanzas = []
        for name, version, arch, deb, fields in packages:
            rel = f'pool/{name}_{version.split(":")[-1]}_{arch}.deb'
            if engine.SAFE_REL_RE.fullmatch(rel) is None:
                raise BuildError(f'{deb.name}: archive path {rel} is not representable')
            data = deb.read_bytes()
            (staging / rel).write_bytes(data)
            lines = [f'{key}: {fields[key]}' for key in FIELD_ORDER if fields.get(key)]
            lines += [f'Filename: {rel}', f'Size: {len(data)}', f'SHA256: {hashlib.sha256(data).hexdigest()}']
            stanzas.append('\n'.join(lines) + '\n')
        packages_bytes = '\n'.join(stanzas).encode('utf-8')
        (staging / 'Packages').write_bytes(packages_bytes)
        release = (f'Origin: {args.origin}\nLabel: WrOS updates\nSuite: {args.suite}\nCodename: trixie\n'
                   f'Date: {rfc2822(moment)}\nValid-Until: {rfc2822(until)}\nArchitectures: amd64\n'
                   f'SHA256:\n {hashlib.sha256(packages_bytes).hexdigest()} {len(packages_bytes)} Packages\n')
        (staging / 'Release').write_text(release, encoding='utf-8')
        # gpg picks a signing subkey when one exists; '!' forces the primary.
        signer = actual + '!' if args.use_primary else actual
        signing = [GPG, '--homedir', str(gnupg_home), '--batch', '--yes', '--local-user', signer]
        run([*signing, '--clearsign', '--output', str(staging / 'InRelease'), str(staging / 'Release')])
        run([*signing, '--detach-sign', '--output', str(staging / 'Release.gpg'), str(staging / 'Release')])

        # Prove it: the real engine, a keyring/pin made from this key only, both signature forms.
        with tempfile.TemporaryDirectory(prefix='wros-bundle-selfcheck-') as check:
            keyring, signers = Path(check) / 'keyring.gpg', Path(check) / 'update-signers'
            run([GPG, '--homedir', str(gnupg_home), '--batch', '--output', str(keyring), '--export', actual])
            signers.write_text(actual + '\n')
            engine.TRUSTED_UID = os.geteuid()
            verified = engine.verify_bundle(staging, keyring=keyring, signers=signers, host_arch='amd64',
                                            suites=(args.suite,), now=moment + timedelta(minutes=1))
            (staging / 'InRelease').rename(staging / 'InRelease.hold')
            engine.verify_bundle(staging, keyring=keyring, signers=signers, host_arch='amd64',
                                 suites=(args.suite,), now=moment + timedelta(minutes=1))
            (staging / 'InRelease.hold').rename(staging / 'InRelease')
        for path in [staging, staging / 'pool']:
            path.chmod(0o755)
        for path in staging.rglob('*'):
            if path.is_file():
                path.chmod(0o644)
        if out.exists():
            out.rmdir()
        staging.rename(out)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    return {'out': str(out), 'signer': actual, 'suite': args.suite, 'date': moment.isoformat(),
            'valid_until': until.isoformat(), 'packages': len(packages), 'review_id': verified['review_id']}


def main(argv=None):
    try:
        summary = build(parse_args(sys.argv[1:] if argv is None else argv))
    except (BuildError, ValueError, OSError) as exc:
        print(f'build-update-bundle: {exc}', file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
