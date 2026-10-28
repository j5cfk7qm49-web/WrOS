#!/usr/bin/env python3
"""Assemble regular installer media files from authenticated Debian indexes.

Run as a normal Debian builder with python3-debian/python3-apt. This never
installs a package, opens a target device, mounts filesystems or launches d-i.
The generated subset index is WrOS metadata, not a Debian-signed Release.
"""
import argparse
from datetime import datetime, timedelta, timezone
from email.utils import parsedate_to_datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile

import apt_pkg
from debian.deb822 import Packages, Release
from debian.debfile import DebFile

ABI = '6.12.113+deb13-amd64'
RESCUE_ABI = '6.12.107+deb13-amd64'
CDEBCONF_VERSION = '0.280'
INDEX = 'main/debian-installer/binary-amd64/Packages'
PREFIX = 'native-installer'
REQUIRED = {
    'anna', 'main-menu', 'cdebconf-udeb', 'cdebconf-newt-udeb', 'cdebconf-text-udeb',
    'live-installer', 'partman-auto-crypto', 'partman-crypto', 'partman-efi',
    'partman-ext3', 'grub-installer', 'user-setup-udeb', 'finish-install',
    'apt-cdrom-setup', 'di-utils-exit-installer', 'rescue-mode',
}
REPOSITORIES = {
    'main': ('https://deb.debian.org/debian', 'trixie',
             'deb.debian.org_debian_dists_trixie'),
    'security': ('https://security.debian.org/debian-security', 'trixie-security',
                 'security.debian.org_debian-security_dists_trixie-security'),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def archive_path(value):
    path = PurePosixPath(value)
    require(not path.is_absolute() and len(path.parts) >= 2 and path.parts[0] == 'pool'
            and all(p not in ('.', '..') for p in path.parts)
            and str(path) == value and path.suffix == '.udeb', 'Unsafe archive filename')
    return path


def load_index(inrelease, packages, codename, keyring, now):
    require(inrelease.is_file() and not inrelease.is_symlink()
            and packages.is_file() and not packages.is_symlink(), 'Regular signed inputs required')
    require(inrelease.stat().st_size <= 4 * 1024 * 1024
            and packages.stat().st_size <= 32 * 1024 * 1024, 'Oversized archive metadata')
    with inrelease.open('rb') as stream:
        signed_bytes = stream.read(4 * 1024 * 1024+1)
    with packages.open('rb') as stream:
        package_bytes = stream.read(32 * 1024 * 1024+1)
    require(len(signed_bytes) <= 4 * 1024 * 1024 and len(package_bytes) <= 32 * 1024 * 1024,
            'Oversized archive metadata')
    with tempfile.TemporaryDirectory(prefix='wros-pool-signature-') as directory:
        sealed = Path(directory)/'InRelease'; sealed.write_bytes(signed_bytes)
        plain = Path(directory)/'Release'
        subprocess.run(['gpgv', '--keyring', str(keyring), '--output', str(plain), str(sealed)],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        release = Release(plain.read_text(encoding='utf-8'))
        release_bytes = plain.read_bytes()
    require(release.get('Origin') == 'Debian' and release.get('Codename') == codename,
            'Unexpected signed archive identity')
    require('Date' in release and parsedate_to_datetime(release['Date']) <= now+timedelta(minutes=5),
            'Signed archive date is in the future')
    if 'Valid-Until' in release:
        require(parsedate_to_datetime(release['Valid-Until']) > now, 'Signed archive metadata expired')
    hashes = [row for row in release.get('SHA256', []) if row['name'] == INDEX]
    require(len(hashes) == 1, 'Signed archive does not uniquely authenticate the package index')
    expected = hashes[0]
    require(re.fullmatch('[0-9a-f]{64}', expected['sha256']) is not None
            and int(expected['size']) == len(package_bytes)
            and hashlib.sha256(package_bytes).hexdigest() == expected['sha256'],
            'Authenticated package index hash mismatch')
    records = []
    seen = set()
    with io.BytesIO(package_bytes) as stream:
        for paragraph in Packages.iter_paragraphs(stream, use_apt_pkg=False):
            for field in ('Package', 'Version', 'Architecture', 'Filename', 'SHA256', 'Size'):
                require(field in paragraph, 'Incomplete authenticated package record')
            require(paragraph['Architecture'] in ('amd64', 'all'), 'Unexpected installer architecture')
            require(re.fullmatch('[a-z0-9][a-z0-9+.-]*', paragraph['Package']) is not None,
                    'Invalid installer package name')
            require(re.fullmatch('[0-9a-f]{64}', paragraph['SHA256']) is not None
                    and 0 < int(paragraph['Size']) <= 128 * 1024 * 1024,
                    'Invalid installer archive size or hash')
            archive_path(paragraph['Filename'])
            identity = paragraph['Package'], paragraph['Version'], paragraph['Architecture']
            require(identity not in seen, 'Duplicate authenticated package identity')
            seen.add(identity)
            records.append(paragraph)
    return records, release_bytes, {
        'codename': codename, 'inrelease_sha256': hashlib.sha256(signed_bytes).hexdigest(),
        'packages_sha256': hashlib.sha256(package_bytes).hexdigest(), 'date': release['Date'],
        'valid_until': release.get('Valid-Until'), 'package_records': len(records),
    }


def copy_index(source, target, expected):
    require(source.is_file() and not source.is_symlink(), 'Authenticated input file changed')
    with source.open('rb') as stream:
        contents = stream.read(32 * 1024 * 1024+1)
    require(len(contents) <= 32 * 1024 * 1024
            and hashlib.sha256(contents).hexdigest() == expected, 'Authenticated input bytes changed')
    target.write_bytes(contents)


def select_packages(indexes):
    selected = {}
    for origin, records in indexes.items():
        for record in records:
            name = record['Package']
            if name.endswith('-amd64-di') and not any(name.endswith('-'+abi+'-di')
                                                      for abi in (ABI, RESCUE_ABI)):
                continue
            previous = selected.get(name)
            comparison = apt_pkg.version_compare(record['Version'], previous[1]['Version']) if previous else 1
            if comparison == 0:
                require(record['SHA256'] == previous[1]['SHA256'],
                        'Different authenticated archives have the same package version')
            if comparison > 0:
                selected[name] = origin, record
    for abi in (ABI, RESCUE_ABI):
        for family in ('kernel-image', 'crypto-modules', 'crypto-dm-modules', 'ext4-modules'):
            require(family+'-'+abi+'-di' in selected, 'Required matching or rescue kernel component absent')
    return selected


def dependency_gaps(selected):
    available = {}
    for name, (_, record) in selected.items():
        available.setdefault(name, []).append(record['Version'])
        for alternatives in apt_pkg.parse_depends(record.get('Provides', ''), True, 'amd64'):
            for provided, version, relation in alternatives:
                require(not relation or relation == '=', 'Invalid versioned virtual package')
                available.setdefault(provided, []).append(version or None)
    gaps = []
    for name, (_, record) in selected.items():
        for field in ('Pre-Depends', 'Depends'):
            for alternatives in apt_pkg.parse_depends(record.get(field, ''), True, 'amd64'):
                if not any(any(not relation or (version is not None
                              and apt_pkg.check_dep(version, relation, requested))
                              for version in available.get(dependency, []))
                           for dependency, requested, relation in alternatives):
                    gaps.append({'package': name, 'field': field, 'alternatives': alternatives})
    return gaps


def native_catalogue(selected, required=REQUIRED):
    selected = dict(selected)
    required = set(required) | {name for name in selected
                               if any(name.endswith('-'+abi+'-di') for abi in (ABI, RESCUE_ABI))}
    omitted = []
    # An archive catalogue is not an installable system. Keep the native/text
    # engines mandatory; optional unavailable components never gain invented
    # providers or dependencies on packages from the builder's own installation.
    while True:
        gaps = dependency_gaps(selected)
        if not gaps:
            break
        names = {row['package'] for row in gaps}
        require(not names.intersection(required),
                'Required native installer dependency unavailable: '+json.dumps(gaps[:12]))
        for name in sorted(names):
            omitted.append({'package': name, 'version': selected[name][1]['Version'],
                            'reason': 'Optional component has no complete installer-package dependency availability',
                            'dependencies': [row for row in gaps if row['package'] == name]})
            del selected[name]
    require(required <= selected.keys(), 'Required native/text installer component absent')
    return selected, omitted


def cached_archives(roots, selected):
    expected = {record['SHA256']: int(record['Size']) for _, record in selected.values()}
    sizes = set(expected.values())
    found = {}
    for root in roots:
        require(root.is_dir() and not root.is_symlink() and root.resolve() != Path('/'),
                'Use existing archive directories, never the live root')
        for path in sorted(root.rglob('*.udeb')):
            if path.is_symlink() or not path.is_file() or path.stat().st_size not in sizes:
                continue
            sha = digest(path)
            if sha in expected:
                found.setdefault(sha, path)
    return found


def download_missing(output, indexes, receipts, selected, missing, keyring):
    apt = output/'apt'
    (apt/'lists/partial').mkdir(parents=True)
    (apt/'cache/archives/partial').mkdir(parents=True)
    (apt/'status').write_bytes(b'')
    sources = []
    for origin, (url, suite, basename) in REPOSITORIES.items():
        inrelease, packages = indexes[origin]
        copy_index(inrelease, apt/('lists/'+basename+'_InRelease'), receipts[origin]['inrelease_sha256'])
        copy_index(packages, apt/('lists/'+basename+'_main_debian-installer_binary-amd64_Packages'),
                   receipts[origin]['packages_sha256'])
        sources.append(f'deb [signed-by={keyring}] {url} {suite} main/debian-installer')
    (apt/'sources.list').write_text('\n'.join(sources)+'\n')
    # APT reads only this private state; the original indexes/caches stay intact.
    config = (f'Dir "{apt}";\nDir::Etc::sourcelist "{apt}/sources.list";\n'
              'Dir::Etc::sourceparts "/dev/null";\nDir::Etc::parts "/dev/null";\n'
              'Dir::Etc::preferences "/dev/null";\nDir::Etc::preferencesparts "/dev/null";\n'
              f'Dir::State::lists "{apt}/lists";\nDir::State::status "{apt}/status";\n'
              f'Dir::Cache "{apt}/cache";\nDir::Cache::pkgcache "";\nDir::Cache::srcpkgcache "";\n'
              'APT::Architecture "amd64";\nDebug::NoLocking "true";\nAcquire::Retries "3";\n')
    (apt/'apt.conf').write_text(config)
    downloads = output/'downloads'; downloads.mkdir()
    command = ['apt-get', '-c', str(apt/'apt.conf'), 'download']
    command += [name+'='+selected[name][1]['Version'] for name in sorted(missing)]
    subprocess.run(command, cwd=downloads, check=True)
    return downloads


def write_pool(output, selected, found):
    paragraphs = []
    archives = []
    for name, (origin, record) in sorted(selected.items()):
        source = found[record['SHA256']]
        require(digest(source) == record['SHA256'] and source.stat().st_size == int(record['Size']),
                'Archive changed after input verification')
        with DebFile(str(source)) as package:
            control = package.debcontrol()
            require(all(control.get(field) == record[field] for field in ('Package', 'Version', 'Architecture')),
                    'Archive control identity differs from authenticated index')
            for field in ('Depends', 'Pre-Depends', 'Provides'):
                require(apt_pkg.parse_depends(control.get(field, ''), True, 'amd64')
                        == apt_pkg.parse_depends(record.get(field, ''), True, 'amd64'),
                        'Archive dependency metadata differs from authenticated index')
            for path in ('usr/lib/cdebconf/frontend/passthrough.so', 'lib/cdebconf/frontend/passthrough.so'):
                require(not package.data.has_file(path) and not package.data.has_file('./'+path),
                        'Archive would replace the qualified native passthrough module')
        relative = archive_path(record['Filename'])
        target = output/str(relative); target.parent.mkdir(parents=True, exist_ok=True)
        require(not target.exists() and not target.is_symlink(), 'Installer archive destination collision')
        shutil.copyfile(source, target)
        require(digest(target) == record['SHA256'], 'Copied archive differs')
        paragraph = Packages(record)
        paragraph['Filename'] = PREFIX+'/'+str(relative)
        paragraphs.append(paragraph.dump())
        archives.append({'package': name, 'version': record['Version'], 'origin': origin,
                         'file': str(relative), 'sha256': record['SHA256'], 'bytes': int(record['Size'])})
    payload = ('\n'.join(paragraphs)+'\n').encode('utf-8')
    (output/'Packages').write_bytes(payload)
    (output/'Packages.gz').write_bytes(gzip.compress(payload, mtime=0))
    return archives


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--indexes', type=Path, required=True, help='Authenticated APT lists directory')
    parser.add_argument('--archive-root', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, required=True, help='New absolute output directory')
    parser.add_argument('--download', action='store_true', help='Allow APT to fetch missing pinned archives')
    args = parser.parse_args()
    require(os.geteuid() != 0, 'Run the regular-file media builder as a normal user')
    output = args.output
    require(output.is_absolute() and not output.exists() and not output.is_symlink()
            and re.fullmatch('/[a-zA-Z0-9/_.-]+', str(output)) is not None, 'Use a new absolute Linux output path')
    keyring = Path('/usr/share/keyrings/debian-archive-keyring.pgp')
    if not keyring.is_file():
        keyring = keyring.with_suffix('.gpg')
    require(keyring.is_file(), 'Debian archive keyring required')
    now = datetime.now(timezone.utc)
    records, inputs, receipts, releases = {}, {}, {}, {}
    for origin, (_, suite, basename) in REPOSITORIES.items():
        inputs[origin] = (args.indexes/(basename+'_InRelease'),
                          args.indexes/(basename+'_main_debian-installer_binary-amd64_Packages'))
        records[origin], releases[origin], receipts[origin] = load_index(*inputs[origin], suite, keyring, now)
    selected, omitted = native_catalogue(select_packages(records))
    require(selected['cdebconf-udeb'][1]['Version'] == CDEBCONF_VERSION,
            'Review frontend ABI and rebuild the native transport before changing cdebconf')
    gaps = dependency_gaps(selected)
    require(not gaps, 'Installer package dependency availability is incomplete: '+json.dumps(gaps[:12]))
    found = cached_archives(args.archive_root, selected)
    missing = {name for name, (_, record) in selected.items() if record['SHA256'] not in found}
    require(not missing or args.download, 'Missing verified archives: '+', '.join(sorted(missing)))
    output.mkdir(mode=0o700)
    if missing:
        downloads = download_missing(output, inputs, receipts, selected, missing, keyring)
        found.update(cached_archives([downloads], selected))
    require(all(record['SHA256'] in found for _, record in selected.values()), 'Incomplete downloaded pool')
    archives = write_pool(output, selected, found)
    for origin, paths in inputs.items():
        directory = output/'sources'/origin; directory.mkdir(parents=True)
        for name, path in zip(('InRelease', 'Packages'), paths):
            copy_index(path, directory/name, receipts[origin][name.lower()+'_sha256'])
        (directory/'Release').write_bytes(releases[origin])
    manifest = {'passed': True, 'recorded_at_utc': now.isoformat(), 'kernel_abi': ABI,
                'rescue_kernel_abi': RESCUE_ABI, 'architecture': 'amd64', 'media_subdirectory': PREFIX,
                'package_count': len(archives), 'bytes': sum(row['bytes'] for row in archives),
                'dependency_availability_closed': True, 'archives': archives, 'authenticated_indexes': receipts,
                'required_native_text_components': sorted(REQUIRED), 'omitted_optional_components': omitted,
                'cdebconf_version': CDEBCONF_VERSION, 'qualified_passthrough_path_preserved': True,
                'required_kernel_components': sorted(name for name in selected
                    if any(name.endswith('-'+abi+'-di') for abi in (ABI, RESCUE_ABI))),
                'generated_packages_sha256': digest(output/'Packages'),
                'generated_packages_gz_sha256': digest(output/'Packages.gz'),
                'keyring_sha256': digest(keyring), 'recipe_sha256': digest(Path(__file__)),
                'generated_index_debian_signed': False,
                'scope': 'Authenticated regular-file media pool and dependency availability only; no root launcher, media integration, installation, reboot or hardware qualification.'}
    (output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps({k: manifest[k] for k in ('passed', 'package_count', 'bytes', 'kernel_abi', 'rescue_kernel_abi')}))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from None
