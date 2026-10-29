#!/usr/bin/env python3
"""Stage a verified, regular-file backend capsule for the native live installer.

This unprivileged builder does not mount, install, select a disk or grant a
launcher. Debian's retriever will use its generated metadata only inside the
future private installer media view. Original Debian signatures stay separate.
"""
import argparse
from datetime import datetime, timezone
from email.utils import format_datetime
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess

from debian.deb822 import Release

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native_pool', ROOT/'tools/build-live-installer-pool.py')
pool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pool)
MODULE_SHA256 = 'be37dee7f39730b6369b54f72ac83a1ff27f9d59bcea10fed9fd11a0ea2adf18'
INDEX = 'main/debian-installer/binary-amd64/Packages'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular(path, limit):
    require(path.is_file() and not path.is_symlink() and 0 < path.stat().st_size <= limit,
            'Regular bounded backend input required: '+path.name)
    return path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def copy_verified(source, target, expected):
    regular(source, 256*1024*1024)
    require(digest(source) == expected, 'Backend input bytes differ from their receipt')
    target.parent.mkdir(parents=True, exist_ok=True)
    require(not target.exists() and not target.is_symlink(), 'Backend destination already exists')
    with source.open('rb') as incoming, target.open('xb') as outgoing:
        while block := incoming.read(1024*1024):
            outgoing.write(block)
    require(digest(target) == expected, 'Copied backend input differs')
    target.chmod(0o644)


def media_release(original, package_index, compressed_index, now):
    release = Release(original.decode('utf-8'))
    require(release.get('Codename') == 'trixie' and 'main' in release.get('Components', '').split(),
            'Expected the existing Debian 13 media repository')
    release['Origin'] = 'WrOS'
    release['Label'] = 'WrOS'
    release['Description'] = 'WrOS native installer private media view'
    release['Date'] = format_datetime(now)
    for field, algorithm, key in (('MD5Sum', 'md5', 'md5sum'), ('SHA1', 'sha1', 'sha1'),
                                  ('SHA256', 'sha256', 'sha256'), ('SHA512', 'sha512', 'sha512')):
        values = [row for row in release.get(field, [])
                  if row['name'] not in (INDEX, INDEX+'.gz', 'Release')]
        for name, contents in ((INDEX, package_index), (INDEX+'.gz', compressed_index)):
            values.append({key: hashlib.new(algorithm, contents).hexdigest(),
                           'size': str(len(contents)), 'name': name})
        release[field] = sorted(values, key=lambda row: row['name'])
    return release.dump().encode('utf-8')


def verify_backend(directory, medium):
    manifest = json.loads(regular(directory/'manifest.json', 4*1024*1024).read_bytes())
    require(manifest.get('passed') is True and manifest.get('architecture') == 'amd64'
            and manifest.get('kernel_version') == pool.ABI
            and manifest.get('kernel_bytes_match_signed_debian_package') is True
            and manifest.get('module_observed_in_compressed_initrd') is True
            and manifest.get('module_sha256') == MODULE_SHA256,
            'Unsupported or unqualified backend component receipt')
    kernel = regular(directory/'vmlinuz', 64*1024*1024)
    initrd = regular(directory/'initrd.gz', 128*1024*1024)
    live_kernel = regular(medium/('live/vmlinuz-'+pool.ABI), 64*1024*1024)
    require(digest(kernel) == manifest['kernel_sha256'] == digest(live_kernel)
            and digest(initrd) == manifest['initrd_sha256'], 'Backend and live kernel/image bytes differ')
    with gzip.open(initrd, 'rb') as stream:
        contents = stream.read(512*1024*1024+1)
    require(len(contents) <= 512*1024*1024, 'Oversized unpacked backend')
    module = subprocess.check_output(['cpio', '-i', '--to-stdout', '--quiet',
                                     'usr/lib/cdebconf/frontend/passthrough.so'],
                                    input=contents, stderr=subprocess.PIPE, timeout=30)
    require(hashlib.sha256(module).hexdigest() == MODULE_SHA256, 'Compressed native transport differs')
    listing = subprocess.check_output(['cpio', '-it', '--quiet'], input=contents,
                                     stderr=subprocess.PIPE, timeout=30).decode().splitlines()
    versions = set()
    for name in listing:
        path = PurePosixPath(name)
        require(not path.is_absolute() and '..' not in path.parts, 'Unsafe backend archive path')
        for prefix in (('usr', 'lib', 'modules'), ('lib', 'modules')):
            if path.parts[:len(prefix)] == prefix and len(path.parts) > len(prefix):
                versions.add(path.parts[len(prefix)])
    require(versions == {pool.ABI}, 'Backend contains modules for a different kernel')
    return manifest


def stage(backend, package_pool, medium, output):
    require(os.geteuid() != 0, 'Stage regular backend files as a normal user')
    for path in (backend, package_pool, medium):
        require(path.is_dir() and not path.is_symlink() and path.resolve() != Path('/'),
                'Existing regular staging directories required, never the live root')
    require(output.is_absolute() and not output.exists() and not output.is_symlink()
            and re.fullmatch('/[a-zA-Z0-9/_.-]+', str(output)) is not None,
            'Use a new absolute Linux capsule directory')
    native = verify_backend(backend, medium)
    packages = pool.audit_pool(package_pool)
    original = regular(medium/'dists/trixie/Release', 4*1024*1024).read_bytes()
    package_index = (package_pool/'Packages').read_bytes()
    compressed = (package_pool/'Packages.gz').read_bytes()
    now = datetime.now(timezone.utc)
    release = media_release(original, package_index, compressed, now)
    output.mkdir(mode=0o700)
    hashes = {}

    def copy(source, name, expected=None):
        expected = expected or digest(source)
        copy_verified(source, output/name, expected)
        hashes[name] = expected

    copy(backend/'vmlinuz', 'vmlinuz', native['kernel_sha256'])
    copy(backend/'initrd.gz', 'initrd.gz', native['initrd_sha256'])
    copy(backend/'manifest.json', 'backend-manifest.json')
    copy(package_pool/'manifest.json', 'pool-manifest.json')
    copy(package_pool/'Packages', 'Packages', packages['generated_packages_sha256'])
    copy(package_pool/'Packages.gz', 'Packages.gz', packages['generated_packages_gz_sha256'])
    for row in packages['archives']:
        name = str(pool.archive_path(row['file']))
        copy(package_pool/name, name, row['sha256'])
    for origin in pool.REPOSITORIES:
        for name, field in (('InRelease', 'inrelease_sha256'), ('Packages', 'packages_sha256')):
            path = 'sources/'+origin+'/'+name
            copy(package_pool/path, path, packages['authenticated_indexes'][origin][field])
    for name in ('debian-installer-copyright', 'COPYING.passthrough', 'cdebconf-copyright'):
        copy(regular(backend/'licenses'/name, 2*1024*1024), 'licenses/'+name)
    (output/'media-Release').write_bytes(release)
    hashes['media-Release'] = hashlib.sha256(release).hexdigest()
    manifest = {'passed': True, 'recorded_at_utc': now.isoformat(), 'architecture': 'amd64',
                'kernel_release': pool.ABI, 'media_subdirectory': pool.PREFIX,
                'module_sha256': MODULE_SHA256, 'module_observed_in_compressed_initrd': True,
                'backend_kernel_matches_live_medium': True, 'pool_independently_revalidated': True,
                'package_count': packages['package_count'], 'archive_bytes': packages['bytes'],
                'base_media_release_sha256': hashlib.sha256(original).hexdigest(),
                'backend_component_source_fingerprint': native['source_fingerprint'],
                'file_sha256': hashes, 'stager_sha256': digest(Path(__file__)),
                'generated_media_release_debian_signed': False,
                'scope': 'Verified regular-file backend capsule only. No root launcher, namespace mounts, disk selection, installation, reboot, firmware or hardware qualification.'}
    (output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('backend', 'pool', 'medium', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    record = stage(args.backend, args.pool, args.medium, args.output)
    print(json.dumps({name: record[name] for name in ('passed', 'kernel_release', 'package_count', 'archive_bytes')}))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from None
