#!/usr/bin/env python3
"""Build the vendored, gamma-enabled Cage against Debian's system wlroots."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def build(output, repo=ROOT):
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='wros-cage-') as temporary:
        directory = Path(temporary) / 'build'
        source = (Path(repo) / 'os/vendor/cage').resolve()
        flags = [f'-ffile-prefix-map={source}=/usr/src/wros/os/vendor/cage',
                 '-D_FORTIFY_SOURCE=3', '-fstack-protector-strong']
        subprocess.run(['meson', 'setup', str(directory), str(source),
                        '--buildtype=release', '--wrap-mode=nodownload', '-Dman-pages=disabled',
                        '-Db_lto=true', '-Dc_args=' + json.dumps(flags),
                        '-Dc_link_args=' + json.dumps(['-Wl,-z,relro,-z,now'])], check=True)
        subprocess.run(['meson', 'compile', '-C', str(directory), '-j', '2'], check=True)
        shutil.copy2(directory / 'cage', output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    build(args.output)
