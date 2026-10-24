"""Stable source-only identity, without generated evidence or multi-GB artifacts."""
import hashlib
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def fingerprint():
    paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
    selected = sorted({p for p in paths if p and (
        p in ('Cargo.toml', 'Cargo.lock') or
        p.split('/')[0] in ('src', 'crates', 'assets') or
        any(p.startswith(d+'/') for d in ('os/scripts', 'os/config', 'os/systemd', 'os/tests'))
    )})
    result = hashlib.sha256()
    for name in selected:
        path = ROOT/name
        if path.is_file():
            result.update(name.encode()+b'\0'+path.read_bytes()+b'\0')
    return {'algorithm':'sha256(path + NUL + file bytes + NUL, sorted source paths)', 'sha256':result.hexdigest(),
            'file_count':len(selected), 'scope':'Cargo.toml/lock, src, crates, assets, os scripts/config/systemd/tests; no generated evidence/docs/builds'}

if __name__ == '__main__':
    import json
    print(json.dumps(fingerprint(), indent=2))
