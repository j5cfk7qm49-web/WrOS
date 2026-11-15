#!/usr/bin/env python3
"""Build a signed WrOS NixOS offline/online binary-cache update bundle.

Takes an unsigned closure directory (produced by nix build .#wros-update-bundle or
tools/collect-nixos-update-runtime.py) containing closure-manifest.json and cache/
(nix-cache-info, *.narinfo, nar/*.nar.zst), optionally prunes paths present in
--baseline-store-paths for a delta bundle (U5), signs Release into InRelease and
Release.gpg using the GnuPG key whose primary fingerprint equals --expect-fingerprint,
and proves the finished bundle with nixos/scripts/wros-update-engine.py.
"""
import argparse
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parents[1]
GPG = 'gpg'
ENV = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C'}
FINGERPRINT_RE = re.compile(r'^[0-9A-F]{40}$')


class BuildError(Exception):
    pass


def load_engine():
    path = REPO / 'nixos/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_nixos_update_engine_for_tool', str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def run(cmd, **kwargs):
    kwargs.setdefault('env', ENV)
    kwargs.setdefault('stdin', subprocess.DEVNULL)
    kwargs.setdefault('stderr', subprocess.PIPE)
    kwargs.setdefault('stdout', subprocess.PIPE)
    result = subprocess.run(cmd, **kwargs)
    if result.returncode != 0:
        detail = (result.stderr or b'').decode('utf-8', 'replace').strip().splitlines()[-1:] or ['no detail']
        raise BuildError(f'{Path(cmd[0]).name} failed: {detail[0]}')
    return result


def normalize_fingerprint(value):
    cleaned = re.sub(r'\s+', '', value).upper()
    if FINGERPRINT_RE.fullmatch(cleaned) is None:
        raise BuildError('--expect-fingerprint must be a 40-hex primary key fingerprint')
    return cleaned


def primary_fingerprint(gnupg_home, key_id):
    listing = run([GPG, '--homedir', str(gnupg_home), '--batch', '--with-colons', '--fingerprint',
                   '--list-secret-keys', '--', key_id]).stdout.decode('utf-8', 'replace')
    primaries = []
    take_next = False
    for line in listing.splitlines():
        fields = line.split(':')
        if fields[0] == 'sec':
            take_next = True
            if len(fields) > 11 and fields[1] in ('e', 'r', 'd', 'i'):
                raise BuildError('the signing key is expired, revoked or disabled')
        elif fields[0] == 'ssb':
            take_next = False
        elif fields[0] == 'fpr' and take_next:
            primaries.append(fields[9].upper())
            take_next = False
    if len(primaries) != 1:
        raise BuildError(f'--key-id must match exactly one secret key (found {len(primaries)})')
    return primaries[0]


def release_moment(args):
    if args.date:
        moment = datetime.fromisoformat(args.date)
        if moment.tzinfo is None:
            moment = moment.replace(tzinfo=timezone.utc)
    elif os.environ.get('SOURCE_DATE_EPOCH', '').isdigit():
        moment = datetime.fromtimestamp(int(os.environ['SOURCE_DATE_EPOCH']), timezone.utc)
    else:
        moment = datetime.now(timezone.utc)
    return moment.astimezone(timezone.utc).replace(microsecond=0)


def rfc2822(moment):
    return format_datetime(moment, usegmt=True).replace('GMT', 'UTC')


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('closure_dir', help='unsigned closure directory containing closure-manifest.json and cache/')
    parser.add_argument('--out', required=True, help='new output directory (must not exist or be empty)')
    parser.add_argument('--gnupg-home', required=True, help='GNUPGHOME holding the secret signing key (outside the repo)')
    parser.add_argument('--key-id', required=True, help='key id, fingerprint or user id of the signing key')
    parser.add_argument('--expect-fingerprint', required=True, help='40-hex PRIMARY fingerprint that must own --key-id')
    parser.add_argument('--baseline-toplevel', help='optional baseline system toplevel store path for delta bundle')
    parser.add_argument('--baseline-store-paths', type=Path, help='optional file listing store paths in baseline')
    parser.add_argument('--suite', default='wros-nixos-updates')
    parser.add_argument('--origin', default='WrOS')
    parser.add_argument('--date', help='Release Date as ISO 8601 UTC')
    parser.add_argument('--valid-days', type=int, default=14)
    parser.add_argument('--use-primary', action='store_true')
    return parser.parse_args(argv)


def build(args):
    engine = load_engine()
    if not 1 <= args.valid_days <= 30:
        raise BuildError('--valid-days must be between 1 and 30')
    gnupg_home = Path(args.gnupg_home).resolve()
    if not gnupg_home.is_dir() or gnupg_home == REPO or REPO in gnupg_home.parents:
        raise BuildError('invalid or in-repo --gnupg-home')
    expected = normalize_fingerprint(args.expect_fingerprint)
    actual = primary_fingerprint(gnupg_home, args.key_id)
    if actual != expected:
        raise BuildError(f'refusing to sign: key {args.key_id} has primary fingerprint {actual}, not {expected}')
    src = Path(args.closure_dir).resolve()
    out = Path(args.out).absolute()
    if out.is_symlink() or (out.exists() and (not out.is_dir() or any(out.iterdir()))):
        raise BuildError('--out must not exist or must be an empty directory')
    moment = release_moment(args)
    until = moment + timedelta(days=args.valid_days)
    out.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='.wros-nixos-bundle-', dir=out.parent))
    try:
        shutil.copytree(src, staging, dirs_exist_ok=True)
        manifest_data = json.loads((staging / 'closure-manifest.json').read_text(encoding='utf-8'))
        if args.baseline_store_paths is not None and args.baseline_toplevel is not None:
            baseline_set = {
                line.strip()
                for line in args.baseline_store_paths.read_text(encoding='utf-8').splitlines()
                if line.strip()
            }
            kept_files = []
            for item in manifest_data['cache_files']:
                sp = item.get('store_path')
                if sp and sp in baseline_set and sp != manifest_data['toplevel']:
                    (staging / item['path']).unlink(missing_ok=True)
                    continue
                kept_files.append(item)
            manifest_data['baseline'] = args.baseline_toplevel
            manifest_data['cache_files'] = kept_files
            (staging / 'closure-manifest.json').write_text(
                json.dumps(manifest_data, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        manifest_bytes = (staging / 'closure-manifest.json').read_bytes()
        closure = engine.parse_closure_manifest(manifest_bytes)
        release = (
            f'Origin: {args.origin}\nLabel: WrOS NixOS updates\nSuite: {args.suite}\nCodename: nixos-26.05\n'
            f'Date: {rfc2822(moment)}\nValid-Until: {rfc2822(until)}\nArchitectures: amd64\n'
            f'SHA256:\n {hashlib.sha256(manifest_bytes).hexdigest()} {len(manifest_bytes)} closure-manifest.json\n'
        )
        (staging / 'Release').write_text(release, encoding='utf-8')
        signer = actual + '!' if args.use_primary else actual
        signing = [GPG, '--homedir', str(gnupg_home), '--batch', '--yes', '--local-user', signer]
        run([*signing, '--clearsign', '--output', str(staging / 'InRelease'), str(staging / 'Release')])
        run([*signing, '--detach-sign', '--output', str(staging / 'Release.gpg'), str(staging / 'Release')])

        with tempfile.TemporaryDirectory(prefix='wros-nixos-selfcheck-') as check:
            keyring, signers = Path(check) / 'keyring.gpg', Path(check) / 'update-signers'
            run([GPG, '--homedir', str(gnupg_home), '--batch', '--output', str(keyring), '--export', actual])
            signers.write_text(actual + '\n', encoding='utf-8')
            fake_store = Path(check) / 'store'
            fake_store.mkdir()
            if closure.get('baseline'):
                (fake_store / Path(closure['baseline']).name).mkdir()
            engine.TRUSTED_UID = os.geteuid()
            engine._base.TRUSTED_UID = os.geteuid()
            verified = engine.verify_bundle(
                staging, keyring=keyring, signers=signers, suites=(args.suite,),
                now=moment + timedelta(minutes=1), version_file=Path('/nonexistent'),
                current_system=Path('/nonexistent'), store_dir=fake_store)
        if out.exists():
            out.rmdir()
        staging.rename(out)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    return {
        'out': str(out),
        'signer': actual,
        'suite': args.suite,
        'date': moment.isoformat(),
        'valid_until': until.isoformat(),
        'version': closure['version'],
        'toplevel': closure['toplevel'],
        'baseline': closure.get('baseline'),
        'cache_files': len(closure['cache_files']),
        'review_id': verified['review_id'],
    }


def main(argv=None):
    try:
        summary = build(parse_args(sys.argv[1:] if argv is None else argv))
    except (BuildError, ValueError, OSError) as exc:
        print(f'build-nixos-update-bundle: {exc}', file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
