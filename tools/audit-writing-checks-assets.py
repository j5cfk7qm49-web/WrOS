#!/usr/bin/env python3
"""Offline integrity/declared-notice audit, NOT training-data or legal clearance.

Requires the fetched locked Cargo graph. Never downloads a model or opens any
manuscript, profile, host device, credential or network connection.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version=1', '--locked', '--offline'], cwd=ROOT))
    packages = {(p['name'], p['version']): p for p in metadata['packages']}
    locked = {(p['name'], p['version']): p for p in tomllib.loads((ROOT/'Cargo.lock').read_text())['package']}
    retained = json.loads((ROOT/'docs/upstream-notice-sources.json').read_text())
    notices = set()
    for record in retained['packages']:
        package = packages[(record['name'], record['version'])]
        base = Path(package['manifest_path']).parent
        vcs = json.loads((base/'.cargo_vcs_info.json').read_text())['git']['sha1']
        assert vcs == record['package_vcs_commit'], f'notice VCS mismatch: {package["name"]}'
        for item in record['files']:
            file = ROOT/item['file']
            assert file.is_relative_to(ROOT/'os/licenses/upstream'), 'notice outside retained directory'
            assert file.is_file() and digest(file) == item['sha256'], f'notice drift: {file}'
            assert '/'+vcs+'/' in item['url'], 'notice URL is not revision-pinned'
            notices.add(item['file'])
    model = json.loads((ROOT/'docs/model-licenses.json').read_text())
    assert model['schema_version'] == 1 and not model['default_enabled']
    assert model['independent_legal_clearance'] is False and model['distribution_ready'] is False
    assert model['runtime'] == 'harper-core-2.13.0/brill-2.13.0/pos-2.13.0'
    expected = {
        ('harper-core', name) for name in ['dictionary.dict', 'annotations.json',
         'proper_noun_rules.json', 'irregular_nouns.json', 'irregular_verbs.json']
    } | {('harper-brill', name) for name in ['trained_tagger_model.json',
         'trained_chunker_model.json', 'finished_chunker/vocab.json', 'finished_chunker/model.mpk']}
    assert {(a['package'], a['file_in_crate']) for a in model['assets']} == expected
    assert len(model['assets']) == len(expected)
    for asset in model['assets']:
        key = asset['package'], asset['version']
        package = packages[key]
        base = Path(package['manifest_path']).parent
        file = base/asset['file_in_crate']
        assert file.is_relative_to(base) and file.is_file()
        assert asset['bytes'] == file.stat().st_size and digest(file) == asset['sha256'], f'asset drift: {file}'
        assert asset['declared_package_license'] == package['license'] == 'Apache-2.0'
        assert asset['crate_archive_sha256'] == locked[key]['checksum']
        assert asset['package_vcs_commit'] == json.loads((base/'.cargo_vcs_info.json').read_text())['git']['sha1']
        assert asset['retained_notice'] in notices
    print(f'PASS: {len(expected)} locked packaged asset identities; {len(notices)} verbatim revision-pinned notices.')
    print('LIMIT: integrity and upstream declarations, NOT independent model-training/dictionary provenance, corresponding-source completeness or legal/release clearance.')


if __name__ == '__main__':
    main()
