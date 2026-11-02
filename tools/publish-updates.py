#!/usr/bin/env python3
"""CI side of online WrOS updates (.github/workflows/release-updates.yml).

  version TAG              print the Debian version a v* tag publishes; refuse one that
                           differs from the first line of os/VERSION
  published URL OUTDIR     download the currently published wros .deb for re-signing, only
                           if its InRelease verifies against the committed keyring with the
                           pinned primary and the .deb matches the signed index; print its
                           path, or nothing when nothing is published yet
  check-bundle TAR OUTDIR VERSION
                           unpack a bundle signed outside CI (a release asset) into OUTDIR
                           and verify it as a WrOS computer would: the committed keyring and
                           pin, the signed index, every package, and that it publishes VERSION

CI never signs a package it cannot trace to an earlier signature by the pinned key, and
serves a bundle signed elsewhere only unchanged and only after checking it.
"""
import argparse
import hashlib
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[1]
UPDATE_URL = 'https://j5cfk7qm49-web.github.io/WrOS/updates/'
KEYRING = REPO / 'os/config/update-trust/wros-archive-keyring.gpg'
PIN = REPO / 'os/config/update-trust/update-signers'
TAG_RE = re.compile(r'v([0-9][0-9A-Za-z.+-]*)')


def tag_version(tag, tree_version):
    match = TAG_RE.fullmatch(tag)
    if match is None:
        raise ValueError(f'not a release tag: {tag!r}')
    version = match.group(1).replace('-', '~')
    if version != tree_version:
        raise ValueError(f'tag {tag} publishes {version}, but os/VERSION says {tree_version}')
    return version


def fetch(url, limit):
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f'{url} is larger than expected')
    return data


def cleartext(inrelease):
    """The signed text of an InRelease, undoing OpenPGP dash-escaping."""
    lines = inrelease.decode('utf-8').split('\n')
    start = lines.index('') + 1
    end = lines.index('-----BEGIN PGP SIGNATURE-----')
    return '\n'.join(line[2:] if line.startswith('- ') else line for line in lines[start:end])


def stanza_fields(text):
    stanzas = []
    for block in text.strip('\n').split('\n\n'):
        fields = {}
        for line in block.splitlines():
            key, _, value = line.partition(':')
            fields[key] = value.strip()
        stanzas.append(fields)
    return stanzas


def published_deb(base_url, outdir, *, keyring=KEYRING, pin=None):
    pin = (pin or PIN.read_text().split()[0]).upper()
    try:
        inrelease = fetch(base_url + 'InRelease', 1024 * 1024)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return None
        raise
    with tempfile.TemporaryDirectory(prefix='wros-published-') as tmp:
        (Path(tmp) / 'InRelease').write_bytes(inrelease)
        result = subprocess.run(['/usr/bin/gpgv', '--status-fd', '1', '--keyring', str(keyring), 'InRelease'],
                                cwd=tmp, capture_output=True, text=True)
    primaries = [line.split()[-1].upper() for line in result.stdout.splitlines()
                 if line.startswith('[GNUPG:] VALIDSIG')]
    if result.returncode != 0 or pin not in primaries:
        raise ValueError('the published InRelease is not signed by the pinned WrOS key')
    sums = {}
    in_sha = False
    for line in cleartext(inrelease).splitlines():
        if line.startswith('SHA256:'):
            in_sha = True
        elif in_sha and line.startswith(' '):
            digest, size, name = line.split()
            sums[name] = (digest, int(size))
        else:
            in_sha = False
    digest, size = sums['Packages']
    packages = fetch(base_url + 'Packages', size)
    if hashlib.sha256(packages).hexdigest() != digest or len(packages) != size:
        raise ValueError('the published Packages index does not match its signed InRelease')
    wros = next(s for s in stanza_fields(packages.decode('utf-8')) if s.get('Package') == 'wros')
    data = fetch(base_url + wros['Filename'], int(wros['Size']))
    if hashlib.sha256(data).hexdigest() != wros['SHA256'] or len(data) != int(wros['Size']):
        raise ValueError('the published wros package does not match the signed index')
    path = Path(outdir) / Path(wros['Filename']).name
    path.write_bytes(data)
    return path


def load_engine():
    path = REPO / 'os/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_update_engine_for_publish', str(path))
    module = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(module)
    return module


def check_bundle(tar, outdir, version, *, keyring=KEYRING, pin=PIN):
    """Unpack a release's bundle into the new directory OUTDIR and verify it as a WrOS computer would."""
    outdir = Path(outdir)
    outdir.mkdir(parents=True)
    try:
        with tarfile.open(tar) as archive:
            for member in archive.getmembers():
                if not (member.isfile() or member.isdir()):
                    raise ValueError(f'the bundle holds something other than files: {member.name}')
            archive.extractall(outdir, filter='data')
    except tarfile.TarError as exc:
        raise ValueError(f'the bundle could not be unpacked safely: {exc}') from None
    engine = load_engine()
    engine.TRUSTED_UID = os.geteuid()
    verified = engine.verify_bundle(outdir, keyring=Path(keyring), signers=Path(pin), host_arch='amd64',
                                    package_keyring=Path('/nonexistent'))
    published = {p['name']: p['version'] for p in verified['packages']}.get('wros')
    if published != version:
        raise ValueError(f'the bundle publishes wros {published}, not {version}')
    return verified


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    version = commands.add_parser('version')
    version.add_argument('tag')
    published = commands.add_parser('published')
    published.add_argument('url')
    published.add_argument('outdir', type=Path)
    check = commands.add_parser('check-bundle')
    check.add_argument('tar', type=Path)
    check.add_argument('outdir', type=Path)
    check.add_argument('version')
    args = parser.parse_args(argv)
    if args.command == 'version':
        print(tag_version(args.tag, (REPO / 'os/VERSION').read_text().splitlines()[0].strip()))
    elif args.command == 'check-bundle':
        verified = check_bundle(args.tar, args.outdir, args.version)
        print(f"verified wros {args.version}, signed by {verified['signer_fingerprint']}")
    else:
        path = published_deb(args.url, args.outdir)
        if path is not None:
            print(path)
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (ValueError, OSError) as exc:
        print(f'publish-updates: {exc}', file=sys.stderr)
        raise SystemExit(1) from None
