#!/usr/bin/env python3
"""Authenticate d-i kernel/initrd through Debian's signed Release hash chain.

APT verifies packages, but live-build fetches d-i boot images directly. Verify
those bytes separately before it unpacks them. No local signing key is created.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from urllib.request import urlopen

MIRROR = 'https://deb.debian.org/debian'
RELEASE = MIRROR + '/dists/trixie'
SUMS = 'main/installer-amd64/current/images/SHA256SUMS'
KEYRING = '/usr/share/keyrings/debian-archive-keyring.gpg'


def release_hashes(text):
    hashes = {}
    active = False
    for line in text.splitlines():
        if not line.startswith(' '):
            active = line == 'SHA256:'
        elif active:
            digest, size, path = line.split()
            if len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
                raise ValueError('invalid signed SHA256 entry')
            if path in hashes:
                raise ValueError('duplicate signed SHA256 entry')
            hashes[path] = (digest, int(size))
    return hashes


def check(data, expected):
    if len(data) != expected[1] or hashlib.sha256(data).hexdigest() != expected[0]:
        raise ValueError('Debian Installer hash/size mismatch; preserved input refused')


def fetch(url):
    with urlopen(url, timeout=90) as response:
        return response.read(64 * 1024 * 1024 + 1)


def verify(cache):
    cache = Path(cache)
    if cache.is_symlink():
        raise ValueError('refusing symlinked installer cache')
    cache.mkdir(parents=True, exist_ok=True)
    signed = fetch(RELEASE + '/InRelease')
    with tempfile.TemporaryDirectory(prefix='wros-debian-signature-') as tmp:
        signed_file = Path(tmp) / 'InRelease'
        plain = Path(tmp) / 'Release'
        signed_file.write_bytes(signed)
        subprocess.run(['gpgv', '--keyring', KEYRING, '--output', str(plain), str(signed_file)], check=True)
        release = plain.read_text()
    if 'Codename: trixie\n' not in release or 'Origin: Debian\n' not in release:
        raise ValueError('unexpected signed Debian release')
    expected = release_hashes(release).get(SUMS)
    if expected is None:
        raise ValueError('signed Release does not authenticate installer SHA256SUMS')
    sums = fetch(RELEASE + '/' + SUMS)
    check(sums, expected)
    image_hashes = {}
    for line in sums.decode().splitlines():
        digest, path = line.split(None, 1)
        image_hashes[path.strip().lstrip('*').removeprefix('./')] = digest
    images = {}
    for name in ('vmlinuz', 'initrd.gz'):
        relative = 'cdrom/' + name
        digest = image_hashes.get(relative)
        if digest is None:
            raise ValueError('installer image is absent from authenticated hashes: ' + relative)
        url = RELEASE + '/main/installer-amd64/current/images/' + relative
        path = cache / url.replace('/', '_')
        if path.is_symlink():
            raise ValueError('refusing symlinked installer image cache')
        data = path.read_bytes() if path.exists() else fetch(url)
        if hashlib.sha256(data).hexdigest() != digest:
            raise ValueError('authenticated installer image hash mismatch: ' + relative)
        if not path.exists():
            with path.open('xb') as output:
                output.write(data)
        images[relative] = {'sha256': digest, 'bytes': len(data)}
    (cache / 'verified-InRelease').write_bytes(signed)
    (cache / 'verified-SHA256SUMS').write_bytes(sums)
    return {'schema_version': 1, 'passed': True, 'origin': 'Debian', 'codename': 'trixie',
            'release_sha256': hashlib.sha256(signed).hexdigest(),
            'installer_sums_sha256': expected[0], 'images': images,
            'chain': 'Debian archive keyring -> gpgv InRelease -> SHA256SUMS -> kernel/initrd',
            'wros_release_signature': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('live_build_installer_cache', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.live_build_installer_cache), indent=2))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from None
