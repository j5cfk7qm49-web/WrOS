#!/usr/bin/env python3
"""CI helper for online WrOS NixOS updates (.github/workflows/release-nixos-updates.yml).

  version TAG              print the WrOS version a v* tag publishes; refuse one that
                           differs from the first line of os/VERSION
  check-bundle TAR OUTDIR VERSION
                           unpack a NixOS update bundle signed offline into OUTDIR and
                           verify it with nixos/scripts/wros-update-engine.py against the
                           committed keyring and signer pin
"""
import argparse
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import re
import shutil
import sys
import tarfile
import tempfile

REPO = Path(__file__).resolve().parents[1]
NIXOS_UPDATE_URL = 'https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/'
KEYRING = REPO / 'os/config/update-trust/wros-archive-keyring.gpg'
PIN = REPO / 'os/config/update-trust/update-signers'
TAG_RE = re.compile(r'v([0-9][0-9A-Za-z.+-]*)')


def tag_version(tag, tree_version):
    match = TAG_RE.fullmatch(tag)
    if match is None:
        raise ValueError(f'not a release tag: {tag!r}')
    version = match.group(1).replace('-', '~')
    if version != tree_version:
        raise ValueError(f'tag {tag} publishes {version}, but os/VERSION says {tree_version}')
    return version


def load_engine():
    path = REPO / 'nixos/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_nixos_update_engine_for_publish', str(path))
    module = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(module)
    return module


def check_bundle(tar, outdir, version, *, keyring=KEYRING, pin=PIN):
    outdir = Path(outdir).absolute()
    outdir.mkdir(parents=True)
    try:
        with tarfile.open(tar) as archive:
            for member in archive.getmembers():
                relative = Path(member.name)
                if relative.anchor or '..' in relative.parts or '\\' in member.name:
                    raise ValueError(f'bundle path outside output directory: {member.name}')
                destination = outdir / relative
                if member.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                elif member.isfile():
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with archive.extractfile(member) as source, destination.open('xb') as output:
                        shutil.copyfileobj(source, output)
                else:
                    raise ValueError(f'bundle holds non-regular entry: {member.name}')
    except tarfile.TarError as exc:
        raise ValueError(f'bundle could not be unpacked safely: {exc}') from None
    engine = load_engine()
    engine.TRUSTED_UID = os.geteuid()
    engine._base.TRUSTED_UID = os.geteuid()
    with tempfile.TemporaryDirectory(prefix='wros-nixos-trust-') as trust:
        trusted = []
        for source in (keyring, pin):
            copy = Path(trust) / Path(source).name
            copy.write_bytes(Path(source).read_bytes())
            copy.chmod(0o644)
            trusted.append(copy)
        verified = engine.verify_bundle(
            outdir, keyring=trusted[0], signers=trusted[1],
            version_file=Path('/nonexistent'), current_system=Path('/nonexistent'),
            package_keyring=Path('/nonexistent'))
    if verified['version'] != version:
        raise ValueError(f'bundle publishes wros {verified["version"]}, not {version}')
    return verified


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    v_cmd = commands.add_parser('version')
    v_cmd.add_argument('tag')
    c_cmd = commands.add_parser('check-bundle')
    c_cmd.add_argument('tar', type=Path)
    c_cmd.add_argument('outdir', type=Path)
    c_cmd.add_argument('version')
    args = parser.parse_args(sys.argv[1:] if argv is None else argv)
    if args.command == 'version':
        print(tag_version(args.tag, (REPO / 'os/VERSION').read_text().splitlines()[0].strip()))
    else:
        verified = check_bundle(args.tar, args.outdir, args.version)
        print(f"verified wros {args.version} ({verified['toplevel']}), signed by {verified['signer_fingerprint']}")
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, OSError) as exc:
        print(f'publish-nixos-updates: {exc}', file=sys.stderr)
        raise SystemExit(1) from None
