#!/usr/bin/env python3
"""Generate reviewable license inventories + retained Rust notices.

This is an inventory, not legal approval. Run after cargo metadata --locked;
Debian packages are read from a staged appliance, never the host package list.
"""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version=1', '--locked', '--offline'], cwd=ROOT))
    tree = subprocess.check_output(['cargo', 'tree', '--locked', '--offline', '--no-default-features', '-p', 'grimoire-tui', '--target', 'x86_64-unknown-linux-gnu', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=ROOT, text=True)
    used = {tuple(line.split()[:2]) for line in tree.splitlines()}
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
                        'notice_files': texts})
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
        'schema_version': 1, 'review_date': '2026-10-25', 'distribution_ready': False,
        'scope': 'Locked Rust dependency graph (all resolved platforms/default features) + actual staged Debian package versions; not a blanket MIT license for the OS',
        'rust_packages': records, 'rust_notice_bundle': str(notice_path.relative_to(ROOT)),
        'missing_root_notice_files_requiring_review': missing,
        'selected_build': {'target': 'x86_64-unknown-linux-gnu', 'package': 'grimoire-tui', 'default_features': False, 'missing_root_notice_files': missing_selected},
        'existing_dictionaries': {'notice': 'assets/dict/DICTIONARY-LICENSE', 'notice_sha256': sha(ROOT/'assets/dict/DICTIONARY-LICENSE'),
                                  'files': [{'file': str(p.relative_to(ROOT)), 'sha256': sha(p)} for p in sorted((ROOT/'assets/dict').iterdir()) if p.is_file()]},
        'debian_packages': debian,
        'evaluated_not_integrated': [
            {'name':'harper-core','version':'2.13.0','license':'Apache-2.0','source':'https://github.com/Automattic/harper','bundled':False},
            {'name':'nabokov','version':'26.7.10 source metadata','license':'MIT','source':'https://github.com/viewflow/nabokov','bundled':False},
            {'name':'proselint','version':'0.16.0 upstream / 0.14.0 Debian evaluated','license':'BSD-3-Clause','source':'https://github.com/amperser/proselint','bundled':False},
            {'name':'spaCy + en_core_web_sm model','version':'runtime/model not pinned','license':'spaCy MIT; exact model/data attribution review pending','source':'https://github.com/explosion/spacy-models','bundled':False},
            {'name':'Princeton WordNet','version':'3.0 Debian evaluated','license':'LicenseRef-WordNet','source':'https://wordnet.princeton.edu/license-and-commercial-use','bundled':False},
            {'name':'Open English WordNet','version':'not selected','license':'CC-BY-4.0 with Princeton attribution','source':'https://github.com/globalwordnet/english-wordnet','bundled':False},
            {'name':'Colemak Mod-DH repository','version':'not vendored','license':'CC0-1.0','source':'https://github.com/ColemakMods/mod-dh','bundled':False,'note':'Actual keymaps are generated from Debian xkb-data and console-setup, whose notices remain in the guest'},
            {'name':'Piper engine/voices','version':'implementation and voice not selected','license':'Must review exact engine fork/version and each voice separately','bundled':False},
        ],
        'release_gates': ['Review missing notice files and all license expressions.', 'Distribute required notices and corresponding source for GPL/other OS components.',
                          'Pin/check model, corpus, firmware and voice licenses before adding them.', 'This inventory is not legal clearance.'],
    }
    (ROOT/'docs/software-licenses.json').write_text(json.dumps(inventory,indent=2)+'\n')
    content_path = ROOT/'docs/content-licenses.json'
    if content_path.exists():
        content = json.loads(content_path.read_text())
        for entry in content['bundled']:
            if entry['id'] in ('writingdeck-original-tutorial', 'writingdeck-original-demo') and entry['license'] == 'MIT':
                entry['sha256'] = sha(ROOT/entry['file'])
        content_path.write_text(json.dumps(content, indent=2, ensure_ascii=False)+'\n')
    print(f'{len(records)} Rust packages, {len(debian)} staged Debian packages, {len(missing)} missing root notice files to review; {len(missing_selected)} used by the Linux no-audio build')

if __name__ == '__main__':
    main()
