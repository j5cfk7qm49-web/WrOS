#!/usr/bin/env python3
"""Verify retained upstream terminal source and replay the exact local patch."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def audit():
    source = ROOT / 'crates/crossterm-upstream'
    manifest = ROOT / 'os/config/terminal/crossterm-upstream.json'
    record = json.loads(manifest.read_bytes())
    assert record['version'] == '0.29.0' and record['license'] == 'MIT'
    assert record['crate_archive_sha256'] == 'd8b9f2e4c67f833b660cdb0a3523065869fb35570177239812ed4c905aeff87b'
    original = record['original_file_sha256']
    patched = record['patched_file_sha256']
    files = {p.relative_to(source).as_posix(): p for p in source.rglob('*') if p.is_file()}
    assert files.keys() == original.keys(), 'Retained upstream file set changed'
    assert patched.keys() <= original.keys() and len(patched) == 2
    for name, path in files.items():
        assert not path.is_symlink() and path.resolve().is_relative_to(source.resolve())
        assert '..' not in PurePosixPath(name).parts and not PurePosixPath(name).is_absolute()
        assert digest(path) == patched.get(name, original[name]), 'Retained upstream bytes changed: ' + name
    patch = ROOT / record['local_patch']
    assert digest(patch) == record['local_patch_sha256']
    with tempfile.TemporaryDirectory(prefix='wros-terminal-patch-') as directory:
        temporary = Path(directory)
        for name in patched:
            path = temporary / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(files[name].read_bytes())
        subprocess.run(['patch', '--binary', '--batch', '--reverse', '--fuzz=0', '-p1', '-i', str(patch)],
                       cwd=temporary, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        assert all(digest(temporary / name) == original[name] for name in patched)
    assert (source / 'LICENSE').is_file()
    return {'passed': True, 'upstream': 'crossterm', 'version': record['version'],
            'retained_files': len(original), 'changed_files': sorted(patched),
            'crate_archive_sha256': record['crate_archive_sha256'],
            'manifest_sha256': digest(manifest), 'patch_sha256': digest(patch),
            'original_license_sha256': digest(source / 'LICENSE'),
            'scope': 'Source integrity and patch replay only; actual PTY and guest qualification are separate.'}


if __name__ == '__main__':
    print(json.dumps(audit()))
