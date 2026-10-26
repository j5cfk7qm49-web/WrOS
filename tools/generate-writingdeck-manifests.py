#!/usr/bin/env python3
"""Generate reviewable license inventories + retained Rust notices.

This is an inventory, not legal approval. Run after cargo metadata --locked;
Debian packages are read from a staged appliance, never the host package list.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version=1', '--locked', '--offline'], cwd=ROOT))
    tree = subprocess.check_output(['cargo', 'tree', '--locked', '--offline', '--no-default-features', '-p', 'grimoire-tui', '--target', 'x86_64-unknown-linux-gnu', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=ROOT, text=True)
    used = {tuple(line.split()[:2]) for line in tree.splitlines()}
    manual = json.loads((ROOT/'docs/upstream-notice-sources.json').read_text())
    retained = {(p['name'], p['version']): p for p in manual['packages']}
    records, notices, missing, missing_selected = [], [], [], []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        base = Path(package['manifest_path']).parent
        candidates = [p for p in base.iterdir() if p.is_file() and any(word in p.name.lower() for word in ('license', 'copying', 'copyright', 'notice')) and p.stat().st_size <= 262144]
        if not package.get('source') and base.is_relative_to(ROOT) and not candidates:
            candidates = [ROOT / 'LICENSE']
        if package.get('license_file'):
            p = base / package['license_file']
            if p.is_file() and p not in candidates:
                candidates.append(p)
        retained_record = retained.get((package['name'], package['version']))
        if not candidates and retained_record:
            vcs = json.loads((base/'.cargo_vcs_info.json').read_text())['git']['sha1']
            if vcs != retained_record['package_vcs_commit']:
                raise SystemExit('Retained license VCS mismatch for '+package['name'])
            for item in retained_record['files']:
                path = ROOT/item['file']
                if not path.is_file() or sha(path) != item['sha256']:
                    raise SystemExit('Retained upstream notice changed: '+str(path))
                candidates.append(path)
        texts = []
        for path in sorted(candidates):
            texts.append({'file': path.name, 'sha256': sha(path)})
            notices.append('\n' + '='*78 + '\n' + package['name'] + ' ' + package['version'] + ' / ' + path.name + '\n' + '='*78 + '\n' + path.read_text(errors='replace'))
        selected = (package['name'], 'v'+package['version']) in used
        if not texts:
            missing.append(package['name'] + '@' + package['version'])
            if selected:
                missing_selected.append(package['name'] + '@' + package['version'])
        records.append({'name': package['name'], 'version': package['version'], 'declared_license': package.get('license'),
                        'repository': package.get('repository'), 'used_in_linux_no_audio_build': selected, 'source': package.get('source') or 'WritingDeck workspace',
                        'notice_files': texts, 'retained_upstream_notice_source': retained_record})
    notice_path = ROOT / 'os/licenses/RUST-THIRD-PARTY-NOTICES.txt'
    notice_path.parent.mkdir(exist_ok=True, parents=True)
    notice_path.write_text('Rust dependency notices retained from the locked Cargo dependency graph.\nIncludes a superset of platform/default-feature dependencies, not just the no-audio Linux binary.\nOwn modifications remain under the repository MIT terms.\n\n' + '\n'.join(notices))
    debian = []
    stage = ROOT / 'build/appliance/rootfs'
    lock = ROOT / 'build/appliance/packages.lock.tsv'
    if lock.exists():
        for line in lock.read_text().splitlines():
            name, version, arch = line.split('\t')
            copyright = Path('usr/share/doc') / name / 'copyright'
            path = stage / copyright
            debian.append({'name': name, 'version': version, 'architecture': arch, 'copyright_path': '/'+str(copyright),
                           'copyright_sha256': sha(path) if path.is_file() else None,
                           'license_status': 'See retained Debian package copyright; corresponding-source release obligations remain to be satisfied'})
    inventory = {
        'schema_version': 1, 'review_date': '2026-10-26', 'distribution_ready': False,
        'scope': 'Locked Rust dependency graph (all resolved platforms/default features) + actual staged Debian package versions; not a blanket MIT license for the OS',
        'rust_packages': records, 'rust_notice_bundle': str(notice_path.relative_to(ROOT)),
        'missing_root_notice_files_requiring_review': missing,
        'selected_build': {'target': 'x86_64-unknown-linux-gnu', 'package': 'grimoire-tui', 'default_features': False, 'missing_root_notice_files': missing_selected},
        'existing_dictionaries': {'notice': 'assets/dict/DICTIONARY-LICENSE', 'notice_sha256': sha(ROOT/'assets/dict/DICTIONARY-LICENSE'),
                                  'files': [{'file': str(p.relative_to(ROOT)), 'sha256': sha(p)} for p in sorted((ROOT/'assets/dict').iterdir()) if p.is_file()]},
        'debian_packages': debian,
        'evaluated_not_integrated': [
            {'name':'Open English WordNet','version':'not selected','license':'CC-BY-4.0 with Princeton attribution','source':'https://github.com/globalwordnet/english-wordnet','bundled':False},
            {'name':'Colemak Mod-DH repository','version':'not vendored','license':'CC0-1.0','source':'https://github.com/ColemakMods/mod-dh','bundled':False,'note':'Actual keymaps are generated from Debian xkb-data and console-setup, whose notices remain in the guest'},
            {'name':'Piper engine/voices','version':'implementation and voice not selected','license':'Must review exact engine fork/version and each voice separately','bundled':False},
        ],
        'release_gates': ['Review missing notice files and all license expressions.', 'Distribute required notices and corresponding source for GPL/other OS components.',
                          'Pin/check model, corpus, firmware and voice licenses before adding them.', 'This inventory is not legal clearance.'],
    }
    by_name = {p['name']: p for p in metadata['packages'] if p['name'] in ('harper-core','harper-brill','harper-pos-utils')}
    locked = {(p['name'],p['version']):p for p in tomllib.loads((ROOT/'Cargo.lock').read_text())['package']}
    assets = []
    for name, files in [
        ('harper-core', ['dictionary.dict','annotations.json','proper_noun_rules.json','irregular_nouns.json','irregular_verbs.json']),
        ('harper-brill', ['trained_tagger_model.json','trained_chunker_model.json','finished_chunker/vocab.json','finished_chunker/model.mpk']),
    ]:
        package = by_name[name]
        if package['version'] != '2.13.0':
            raise SystemExit('Review model/runtime consent before changing '+name)
        base = Path(package['manifest_path']).parent
        vcs = json.loads((base/'.cargo_vcs_info.json').read_text())['git']['sha1']
        for relative in files:
            file = base/relative
            assets.append({'package':name,'version':package['version'],'file_in_crate':relative,
                'bytes':file.stat().st_size,'sha256':sha(file),'package_vcs_commit':vcs,
                'crate_archive_sha256':locked[(name,package['version'])]['checksum'],
                'source_archive':f'https://crates.io/api/v1/crates/{name}/{package["version"]}/download',
                'declared_package_license':package['license'], 'retained_notice':'os/licenses/upstream/harper-LICENSE.txt'})
    models = {'schema_version':1, 'distribution_ready':False, 'independent_legal_clearance':False,
        'runtime':'harper-core-2.13.0/brill-2.13.0/pos-2.13.0', 'default_enabled':False,
        'permission':'Explicit once-only process permission, or matching personal per-project canonical-root/runtime binding + novel persisted runtime permission. A copied novel at a new location cannot opt even a previously-consented profile into live work.',
        'network':'No runtime model download, remote analysis, telemetry or manuscript transmission by this service.',
        'assets':assets,
        'license_basis':'Upstream crate declares Apache-2.0 for packaged assets; exact repository-root text and archive/file hashes retained. This is upstream-declaration/integrity review, not an independent training-data or legal attestation.',
        'limits':['POS/tagger/chunker includes bundled model inference; NOT a non-AI grammar engine.',
                  'Training-data provenance, inherited dictionary provenance, corresponding source and distribution obligations remain release-review items.',
                  'Runtime/source hashes are integrity identifiers, not authentication or literary-quality certification.']}
    (ROOT/'docs/model-licenses.json').write_text(json.dumps(models,indent=2)+'\n')
    inventory['bundled_analyzers'] = [{'name':'harper-core','version':'2.13.0','default_enabled':False,
        'model_and_data_manifest':'docs/model-licenses.json','runtime':models['runtime'],
        'spelling':'Existing Spellbook is independent; Harper SpellCheck is disabled.'}]
    python_lock = json.loads((ROOT/'docs/python-tools-lock.json').read_text())
    inventory['python_runtime'] = {
        'platform': python_lock['platform'], 'default_enabled': False,
        'lock_manifest': 'docs/python-tools-lock.json',
        'notice_manifest': 'docs/python-tool-notices.json',
        'packages': [{'name': p['name'], 'version': p['version'], 'wheel_sha256': p['sha256'],
                      'declared_license': p['declared_license'], 'notices': p['notices']}
                     for p in python_lock['packages']],
        'model_manifest': 'docs/prose-model-licenses.json',
        'independent_legal_clearance': False,
    }
    inventory['bundled_analyzers'].extend([
        {'name': 'nabokov', 'version': '26.7.10', 'default_enabled': False,
         'lock_manifest': 'docs/python-tools-lock.json', 'model_manifest': 'docs/prose-model-licenses.json'},
        {'name': 'proselint', 'version': '0.16.0', 'default_enabled': False,
         'lock_manifest': 'docs/python-tools-lock.json', 'uses_model': False},
    ])
    inventory['bundled_reference_data'] = [
        {'name': 'Princeton WordNet', 'version': '3.0', 'manifest': 'docs/lexicon-licenses.json'}]
    inventory['wifi_firmware_policy'] = 'os/config/wifi-firmware.json'
    (ROOT/'docs/software-licenses.json').write_text(json.dumps(inventory,indent=2)+'\n')
    content_path = ROOT/'docs/content-licenses.json'
    if content_path.exists():
        content = json.loads(content_path.read_text())
        for entry in content['bundled']:
            if entry['id'] in ('writingdeck-original-tutorial', 'writingdeck-original-demo', 'writingdeck-original-productivity') and entry['license'] == 'MIT':
                entry['sha256'] = sha(ROOT/entry['file'])
            if entry['id'] == 'writingdeck-original-tutorial':
                entry['item_count'] = len(json.loads((ROOT/entry['file']).read_text()))
                entry['modifications'] = 'Original 39 lessons plus supplemental grammar lesson 40; M7b enables real dictionary/prose/voice review events. No third-party exercise text.'
        content_path.write_text(json.dumps(content, indent=2, ensure_ascii=False)+'\n')
    print(f'{len(records)} Rust packages, {len(debian)} staged Debian packages, {len(missing)} missing root notice files to review; {len(missing_selected)} used by the Linux no-audio build')

if __name__ == '__main__':
    main()
