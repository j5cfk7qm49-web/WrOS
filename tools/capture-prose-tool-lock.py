#!/usr/bin/env python3
"""Explicit maintainer capture of reviewed public Python artifacts, NOT an updater.

Reads an existing wheelhouse and exact GitHub source records. Fetches only public
PyPI release metadata, never a manuscript. The resulting Linux/CPython 3.13 lock,
not the moving index, is used by prepare-prose-tools.py. Review diffs before use.
Retaining notices and hashes is not independent legal/training-data clearance.
"""
import argparse
import base64
import csv
import email
import email.policy
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalized(name):
    return re.sub(r'[-_.]+', '-', name).lower()


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {'file': str(path.relative_to(ROOT)), 'sha256': sha(data), 'bytes': len(data)}


def repack(wheel, source, output):
    """No setup/build-backend execution; exact approved source + captured metadata.

    The original backends' package-file choices and METADATA are retained. Every
    runtime file must match the pinned Git archive. Fixed ZIP_STORED timestamps
    make the derived wheel independent of backend/clock/zlib compression.
    """
    package = source['name']
    prefix = 'src/' if package == 'nabokov' else ''
    archive = ROOT / source['archive']
    assert sha(archive.read_bytes()) == source['sha256']
    with tarfile.open(archive) as tar:
        members = {m.name: m for m in tar.getmembers() if m.isfile()}
        top = next(iter(members)).split('/')[0]
        with zipfile.ZipFile(wheel) as z:
            files = {}
            entries = []
            for name in z.namelist():
                if name.endswith('/') or name.endswith('/RECORD'):
                    continue
                p = PurePosixPath(name)
                assert not p.is_absolute() and '..' not in p.parts
                data = z.read(name)
                if name.startswith(package + '/'):
                    member = members[top + '/' + prefix + name]
                    source_data = tar.extractfile(member).read()
                    assert source_data == data, f'Wheel is not exact approved source: {name}'
                    entries.append({'wheel_file': name, 'archive_file': prefix + name, 'sha256': sha(data)})
                else:
                    assert '.dist-info/' in name, name
                    if name.endswith('/WHEEL'):
                        data = re.sub(rb'^Generator:.*$', b'Generator: writingdeck-source-repack-1', data, flags=re.M)
                    write(ROOT / 'os/python-tools/wheel-metadata' / package / name, data)
                files[name] = data
    record_name = next(n.split('/')[0] + '/RECORD' for n in files if '.dist-info/' in n)
    record = io.StringIO(newline='')
    writer = csv.writer(record, lineterminator='\n')
    for name, data in sorted(files.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b'=').decode()
        writer.writerow([name, 'sha256=' + digest, str(len(data))])
    writer.writerow([record_name, '', ''])
    files[record_name] = record.getvalue().encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_STORED) as z:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            z.writestr(info, data)
    return {k: v for k, v in source.items() if k != 'archive'} | {
        'archive_filename': archive.name,
        'package_files': entries,
        'metadata_directory': 'os/python-tools/wheel-metadata/' + package,
        'original_backend_wheel_sha256': sha(wheel.read_bytes()),
        'derived_format': 'writingdeck-source-repack-1: exact source + retained dist-info; sorted ZIP_STORED/1980/0644; regenerated RECORD',
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--wheelhouse', type=Path, required=True)
    p.add_argument('--sources', type=Path, required=True)
    p.add_argument('--derived-wheels', type=Path, required=True)
    args = p.parse_args()
    sources = {s['name']: s for s in json.loads(args.sources.read_text())}
    packages = []
    notices = []
    model = None
    for wheel in sorted(args.wheelhouse.glob('*.whl')):
        with zipfile.ZipFile(wheel) as z:
            m = email.message_from_bytes(z.read(next(n for n in z.namelist() if n.endswith('/METADATA'))), policy=email.policy.default)
        name, version = m['Name'], m['Version']
        key = normalized(name)
        source = None
        source_archive = None
        if key in sources:
            source = repack(wheel, sources[key], args.derived_wheels / wheel.name)
            selected = args.derived_wheels / wheel.name
            url = None
        elif key == 'en-core-web-sm':
            selected = wheel
            url = 'https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/' + wheel.name
        else:
            selected = wheel
            request = urllib.request.Request(f'https://pypi.org/pypi/{name}/{version}/json', headers={'User-Agent': 'WritingDeck-public-artifact-lock/1'})
            with urllib.request.urlopen(request, timeout=45) as response:
                metadata = json.load(response)
            artifact = next(a for a in metadata['urls'] if a['filename'] == wheel.name)
            assert artifact['digests']['sha256'] == sha(wheel.read_bytes()), f'PyPI byte mismatch: {wheel.name}'
            url = artifact['url']
            sdist = next((a for a in metadata['urls'] if a['packagetype'] == 'sdist'), None)
            if sdist:
                source_archive = {'filename': sdist['filename'], 'url': sdist['url'], 'sha256': sdist['digests']['sha256']}
        retained = []
        with zipfile.ZipFile(selected) as z:
            for f in z.namelist():
                if not f.endswith('/') and any(k in PurePosixPath(f).name.lower() for k in ('license', 'licence', 'copying', 'notice', 'copyright')):
                    item = write(ROOT / 'os/licenses/python' / key / f, z.read(f))
                    item['wheel_file'] = f
                    retained.append(item)
            if key == 'en-core-web-sm':
                f = next(n for n in z.namelist() if n.endswith('/en_core_web_sm-3.8.0/meta.json'))
                card = z.read(f)
                card_record = write(ROOT / 'os/python-tools/en_core_web_sm-3.8.0-meta.json', card)
                metadata = json.loads(card)
                model = {
                    'schema_version': 1, 'package': name, 'version': version,
                    'default_enabled': False, 'permission': 'Separate explicit one-run source/configuration-bound consent; no detector/authorship rules.',
                    'network': 'No runtime downloads or manuscript transmission by this service.',
                    'declared_license': m.get('License-Expression') or m.get('License'),
                    'upstream_model_card': card_record, 'upstream_sources': metadata.get('sources', []),
                    'wheel': {'filename': wheel.name, 'bytes': selected.stat().st_size, 'sha256': sha(selected.read_bytes()), 'url': url},
                    'notices': retained,
                    'distribution_ready': False, 'independent_legal_clearance': False,
                    'limits': 'Upstream model card and source declarations are retained, not independent training-data, representativeness, rights, authorship or literary-quality certification.',
                }
        assert retained, f'No retained notice: {name}'
        package = {
            'name': name, 'version': version, 'filename': selected.name,
            'bytes': selected.stat().st_size, 'sha256': sha(selected.read_bytes()),
            'url': url, 'source_repack': source, 'corresponding_source': source_archive,
            'declared_license': m.get('License-Expression') or m.get('License'),
            'requires_dist': m.get_all('Requires-Dist') or [],
            'project_urls': m.get_all('Project-URL') or [], 'notices': retained,
        }
        packages.append(package)
        notices.extend({'package': name, 'version': version, **n} for n in retained)
        print(name, version, package['sha256'], flush=True)
    assert len({normalized(p['name']) for p in packages}) == len(packages)
    lock = {
        'schema_version': 1, 'capture_date': '2026-10-26',
        'platform': {'os': 'linux', 'architecture': 'x86_64', 'python_abi': 'cp313', 'glibc_minimum': '2.28'},
        'scope': 'Complete explicit wheel set from the exercised Nabokov/proselint/spaCy/model runtime. No dependency resolution at provisioning or runtime. Debian supplies the pinned-image CPython interpreter/stdlib.',
        'packages': packages,
        'distribution_ready': False, 'independent_legal_clearance': False,
        'limits': ['Hashes identify bytes, not a signed trust chain or independent security review.', 'Upstream declarations/notices are not independent legal or training-data clearance.', 'OS/firmware/corresponding-source/release gates remain separate.'],
    }
    (ROOT / 'docs/python-tools-lock.json').write_text(json.dumps(lock, indent=2, ensure_ascii=False) + '\n')
    (ROOT / 'os/python-tools/requirements.lock').write_text('# Linux x86_64 / CPython 3.13. Offline exact wheels only; no resolution.\n' + '\n'.join(f"{p['name']}=={p['version']} --hash=sha256:{p['sha256']}" for p in packages) + '\n')
    assert model is not None
    (ROOT / 'docs/prose-model-licenses.json').write_text(json.dumps(model, indent=2, ensure_ascii=False) + '\n')
    (ROOT / 'docs/python-tool-notices.json').write_text(json.dumps({'schema_version': 1, 'distribution_ready': False, 'files': notices}, indent=2, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    main()
