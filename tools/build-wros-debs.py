#!/usr/bin/env python3
"""Build the installed-system WrOS payload into the Debian package 'wros'.

    tools/build-wros-debs.py --binaries-dir DIR --version 0.1.0~rc1 --output DIR

--binaries-dir holds the built `grimoire` (and optionally `wros-display-hw`,
otherwise compiled from os/helpers). The version defaults to os/VERSION. The same
command with another --version (and, if wanted, modified inputs or --repo) builds
the newer package an update bundle carries. The result is deterministic for given
inputs and SOURCE_DATE_EPOCH. Prints one JSON object on stdout.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def load_library(repo):
    spec = importlib.util.spec_from_file_location('wros_packaging', Path(repo) / 'packaging/wros_packaging.py')
    module = importlib.util.module_from_spec(spec)
    sys.modules['wros_packaging'] = module
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--binaries-dir', type=Path, required=True)
    parser.add_argument('--version', help='Debian version (default: first line of os/VERSION)')
    parser.add_argument('--output', type=Path, required=True, help='directory that receives wros_<version>_amd64.deb')
    parser.add_argument('--repo', type=Path, default=ROOT, help='source tree to package (default: this checkout)')
    parser.add_argument('--keymaps-dir', type=Path, help='pre-generated *.map files (default: run generate-keymaps.sh)')
    parser.add_argument('--display-helper', type=Path, help='prebuilt wros-display-hw (default: compile it)')
    parser.add_argument('--commit', help='source commit for /etc/writingdeck/release (default: WROS_SOURCE_COMMIT or VCS head)')
    parser.add_argument('--source-date-epoch', type=int, help='default: $SOURCE_DATE_EPOCH or the head commit time')
    args = parser.parse_args(argv)
    lib = load_library(args.repo)
    version = args.version or lib.read_version(args.repo)
    path = lib.build_deb(binaries_dir=args.binaries_dir, version=version, output_dir=args.output, repo=args.repo,
                         keymaps_dir=args.keymaps_dir, display_helper=args.display_helper, commit=args.commit,
                         epoch=args.source_date_epoch)
    print(json.dumps({'package': lib.deb_field(path, 'Package'), 'version': lib.deb_field(path, 'Version'),
                      'architecture': lib.deb_field(path, 'Architecture'), 'path': str(path),
                      'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}, indent=2))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        detail = getattr(exc, 'stderr', None)
        sys.exit('build-wros-debs: ' + str(exc) + (' ' + detail.decode(errors='replace') if detail else ''))
