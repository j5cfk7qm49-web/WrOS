#!/usr/bin/env python3
"""Stage a verified offline runtime into a disposable Debian image directory.

Never installs into /, follows a destination symlink, executes the payload, or
replaces an existing unknown runtime. Does not authorize a physical install.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('wros_prose_provisioner', ROOT/'tools/prepare-prose-tools.py')
provisioner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provisioner)


def destination(staging_root):
    if staging_root.is_symlink():
        raise ValueError('staging root must not be a symlink')
    root = staging_root.resolve(strict=True)
    if root == Path('/') or not (root/'etc/debian_version').is_file():
        raise ValueError('requires a staged Debian root, never the live root')
    target = root/'opt/writingdeck/python'
    for part in (root/'opt', root/'opt/writingdeck', target):
        if part.is_symlink() or (part.exists() and not part.is_dir()):
            raise ValueError('destination symlink/non-directory refused; original retained')
    return target


def stage(staging_root, source):
    target = destination(staging_root)
    wheels = source.parent/'wheels'
    original = provisioner.verify_runtime(source, wheels)
    if target.exists():
        installed = provisioner.verify_runtime(target, wheels)
        if installed != original:
            raise ValueError('existing runtime differs from requested payload; retained')
        return target
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix='.python-stage-', dir=target.parent))
    try:
        shutil.copytree(source, temporary, dirs_exist_ok=True, symlinks=True)
        provisioner.verify_runtime(temporary, wheels)
        if os.geteuid() == 0:
            for path in [temporary, *temporary.rglob('*')]:
                os.chown(path, 0, 0, follow_symlinks=False)
        # All content is checked before it becomes the installed runtime.
        # Reject a destination that appeared while copying rather than replace it.
        if target.exists() or target.is_symlink():
            raise ValueError('runtime destination appeared during staging; retained')
        temporary.rename(target)
        temporary = None
        return target
    finally:
        if temporary is not None:
            shutil.rmtree(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('staging_root', type=Path)
    parser.add_argument('runtime', type=Path)
    args = parser.parse_args()
    print('staged verified runtime:', stage(args.staging_root, args.runtime))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError) as exc:
        raise SystemExit('runtime staging failed: '+str(exc))
