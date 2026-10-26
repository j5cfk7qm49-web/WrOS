#!/usr/bin/env python3
"""Read-only offline prose/WordNet artifact and notice consistency, not approval.

No imports of NLP libraries, model inference, networking or manuscript reads.
The complete release trust/security/legal/physical gates are independent.
"""
import argparse
import ast
import hashlib
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(ok, message):
    if not ok:
        raise ValueError(message)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime', type=Path, default=ROOT/'build/prose-runtime/runtime')
    p.add_argument('--wordnet', type=Path, default=Path('/usr/share/wordnet'))
    args = p.parse_args()
    spec = importlib.util.spec_from_file_location('writingdeck_prose_provisioner', ROOT/'tools/prepare-prose-tools.py')
    provisioner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(provisioner)
    lock = provisioner.locked_manifest()
    runtime = provisioner.verify_runtime(args.runtime)
    packages = {x['name'].lower().replace('_', '-'): x for x in lock['packages']}
    require(len(packages) == 49, 'complete reviewed package set changed; update/review audit')
    require(len(runtime['packages']) == len(packages), 'materialized runtime package set incomplete')
    helper = ROOT/'crates/grimoire-core/assets/writingdeck/prose_tools.py'
    constants = {}
    for node in ast.parse(helper.read_text()).body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            if node.targets[0].id in ('PACKAGE_DIGESTS', 'VERSIONS', 'ENGINE_VERSIONS'):
                constants[node.targets[0].id] = ast.literal_eval(node.value)
    for package, version in constants['VERSIONS'].items():
        require(packages[package.replace('_', '-')]['version'] == version, 'helper/lock version mismatch: '+package)
    for package, record in constants['PACKAGE_DIGESTS'].items():
        module = args.runtime/'lib/python3.13/site-packages'/record['module']
        items = []
        for path in module.rglob('*'):
            if path.is_file():
                require(not path.is_symlink(), 'symlink/unexpected data in critical package')
                items.append((path.relative_to(module.parent).as_posix(), sha(path)))
        h = hashlib.sha256()
        for name, value in sorted(items):
            h.update(name.encode()+b'\0'+value.encode()+b'\n')
        require(len(items) == record['files'] and h.hexdigest() == record['tree_sha256'], 'critical runtime source/data changed: '+package)
    notices = json.loads((ROOT/'docs/python-tool-notices.json').read_text())['files']
    all_notices = [n for package in lock['packages'] for n in package['notices']]
    require(len(all_notices) == len(notices) == 94, 'retained notice inventory incomplete/changed')
    for record in all_notices:
        path = ROOT/record['file']
        require(path.is_file() and sha(path) == record['sha256'] and path.stat().st_size == record['bytes'], 'retained notice changed: '+record['file'])
    for record in notices:
        require(sha(ROOT/record['file']) == record['sha256'], 'notice index digest mismatch')
    model = json.loads((ROOT/'docs/prose-model-licenses.json').read_text())
    card = model['upstream_model_card']
    require(sha(ROOT/card['file']) == card['sha256'], 'retained original model metadata changed')
    require(model['wheel']['sha256'] == packages['en-core-web-sm']['sha256'] == '1932429db727d4bff3deed6b34cfc05df17794f4a52eeb26cf8928f7c1a0fb85', 'pinned model mismatch')
    require(not model['default_enabled'] and not model['distribution_ready'] and not model['independent_legal_clearance'], 'model release/consent status misrepresented')
    require(len(model['upstream_sources']) == 3, 'retain actual training/source declarations and limitations')
    sources = [x['source_repack'] for x in lock['packages'] if x['source_repack']]
    require(len(sources) == 2, 'missing exact-commit source recipes')
    rust = (ROOT/'crates/grimoire-core/src/prose_tools.rs').read_text()
    for engine, version in constants['ENGINE_VERSIONS'].items():
        require(version in rust, 'Rust/Python engine version mismatch: '+engine)
    wordnet = json.loads((ROOT/'docs/lexicon-licenses.json').read_text())
    notice = ROOT/wordnet['retained_notice']
    require(sha(notice) == wordnet['retained_notice_sha256'], 'WordNet notice mismatch')
    for record in wordnet['data_files']:
        path = args.wordnet/record['file']
        require(path.is_file() and path.stat().st_size == record['bytes'] and sha(path) == record['sha256'], 'pinned WordNet data unavailable/changed: '+record['file'])
    print(json.dumps({
        'schema_version': 1, 'scope': 'offline integrity and retained upstream declarations only',
        'python_packages': len(packages), 'retained_notice_files': len(notices),
        'exact_commit_source_recipes': len(sources), 'critical_package_trees': len(constants['PACKAGE_DIGESTS']),
        'wordnet_data_files': len(wordnet['data_files']),
        'default_model_enabled': False, 'distribution_ready': False,
        'independent_legal_security_hardware_clearance': False,
    }, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError) as exc:
        raise SystemExit('prose asset audit failed: '+str(exc))
