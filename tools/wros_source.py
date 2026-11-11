"""Stable source-only identity, without generated evidence or multi-GB artifacts."""
import hashlib
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRS = ('.cargo/', 'src/', 'crates/', 'assets/', 'os/', 'packaging/', 'tools/')
SOURCE_FILES = {
    'Cargo.toml', 'Cargo.lock', 'LICENSE', 'docs/feature-status.json',
    'docs/keyboard-reference.html', 'docs/python-tools-lock.json',
    'docs/python-tool-notices.json', 'docs/upstream-notice-sources.json',
}


def _is_source(path):
    if path in SOURCE_FILES or path.startswith(SOURCE_DIRS):
        return True
    # The package manifest ships docs/*.md and docs/*licenses.json.
    return (path.startswith('docs/') and path.count('/') == 1
            and path.endswith(('.md', 'licenses.json')))


def _source_files(*flags):
    if not (ROOT / '.git').exists():
        raise SystemExit('Source evidence needs a WrOS Git checkout to identify the source files.')
    paths = subprocess.check_output(['git', 'ls-files', *flags, '--exclude-standard', '-z'], cwd=ROOT)
    return sorted({path for path in paths.decode().split('\0') if _is_source(path)})


def is_dirty():
    changes = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT)
    return bool(changes or _source_files('--others'))


def fingerprint():
    selected = _source_files('--cached', '--others')
    result = hashlib.sha256()
    for name in selected:
        path = ROOT/name
        if path.is_file():
            result.update(name.encode()+b'\0'+path.read_bytes()+b'\0')
    return {'algorithm':'sha256(path + NUL + file bytes + NUL, sorted source paths)', 'sha256':result.hexdigest(),
            'file_count':len(selected), 'scope':'Cargo files and config, src, crates, assets, os, packaging, tools and shipped documentation'}

if __name__ == '__main__':
    import json
    print(json.dumps(fingerprint(), indent=2))
