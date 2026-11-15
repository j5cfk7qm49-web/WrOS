#!/usr/bin/env python3
"""Collect full or delta NixOS binary-cache update bundles for WrOS (U2, U5).

Usage:
  collect-nixos-update-runtime.py TOPLEVEL VERSION OUT_DIR [--baseline-toplevel PATH] [--release-tag TAG] [--app-only]
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

PAGES_MAX_FILE_BYTES = 95 * 1024 * 1024


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('toplevel', help='/nix/store/...-nixos-system-wros-... path')
    parser.add_argument('version', help='WrOS version string (e.g. 0.1.0~rc14)')
    parser.add_argument('out_dir', type=Path, help='output directory to populate')
    parser.add_argument('--baseline-toplevel', help='optional baseline system toplevel store path for delta bundle')
    parser.add_argument('--release-tag', help='optional release tag (e.g. v0.1.0-rc14) for >95 MiB NAR asset URLs')
    parser.add_argument('--app-only', action='store_true')
    args = parser.parse_args(argv)

    out = args.out_dir.resolve()
    cache_dir = out / 'cache'
    cache_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run([
        'nix', '--extra-experimental-features', 'nix-command',
        'copy', '--to', f'file://{cache_dir}?compression=zstd',
        '--no-check-sigs', args.toplevel,
    ], check=True)

    baseline_set = set()
    if args.baseline_toplevel:
        baseline_set = {
            line.strip()
            for line in subprocess.check_output(['nix-store', '-qR', args.baseline_toplevel], text=True).splitlines()
            if line.strip()
        }

    narinfo_by_store = {}
    for narinfo_path in sorted(cache_dir.glob('*.narinfo')):
        fields = {}
        for line in narinfo_path.read_text(encoding='utf-8').splitlines():
            if ': ' in line:
                k, v = line.split(': ', 1)
                fields[k] = v.strip()
        sp = fields.get('StorePath', '')
        url = fields.get('URL', '')
        if sp in baseline_set and sp != args.toplevel:
            narinfo_path.unlink(missing_ok=True)
            if url:
                (cache_dir / url).unlink(missing_ok=True)
            continue
        narinfo_by_store[narinfo_path.name] = (sp, url)

    cache_files = []
    for file_path in sorted(cache_dir.rglob('*')):
        if not file_path.is_file():
            continue
        rel = file_path.relative_to(out).as_posix()
        data = file_path.read_bytes()
        sha256 = hashlib.sha256(data).hexdigest()
        size = len(data)
        sp = None
        if file_path.parent == cache_dir and file_path.name in narinfo_by_store:
            sp = narinfo_by_store[file_path.name][0]
        elif file_path.parent.name == 'nar':
            nar_rel = f'nar/{file_path.name}'
            for _, (cand_sp, cand_url) in narinfo_by_store.items():
                if cand_url == nar_rel:
                    sp = cand_sp
                    break
        entry = {
            'path': rel,
            'sha256': sha256,
            'size': size,
            'store_path': sp,
        }
        if size > PAGES_MAX_FILE_BYTES and args.release_tag:
            entry['url'] = (
                f'https://github.com/j5cfk7qm49-web/WrOS/releases/download/{args.release_tag}/{file_path.name}'
            )
        cache_files.append(entry)

    manifest = {
        'schema': 2,
        'format': 'wros-nixos-cache-v2',
        'version': args.version,
        'system': 'x86_64-linux',
        'toplevel': args.toplevel,
        'baseline': args.baseline_toplevel,
        'app_only': bool(args.app_only),
        'cache_files': cache_files,
    }
    (out / 'closure-manifest.json').write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    print(json.dumps({'cache_files': len(cache_files), 'toplevel': args.toplevel, 'version': args.version}, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
