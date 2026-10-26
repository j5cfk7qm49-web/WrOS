#!/usr/bin/env python3
"""Provision exact offline Linux amd64/CPython 3.13 prose runtime, unprivileged.

Only public locked artifacts are downloaded during this explicit BUILD step.
There is no index resolution, setup.py/backend execution, runtime downloader,
model inference, manuscript read, host installation, or credential use here.
--offline requires verified local artifacts. Unknown/corrupt outputs are kept.
The source repacks are deterministic ZIP_STORED exact Git source + retained
public dist-info. They do not depend on mutable build-backend dependencies.
"""
import argparse
import base64
import csv
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / 'docs/python-tools-lock.json'
REQUIREMENTS = ROOT / 'os/python-tools/requirements.lock'
MARKER = '.writingdeck-runtime.json'
MAX_ARTIFACT_BYTES = 128 * 1024 * 1024


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def checked_name(name):
    p = PurePosixPath(name)
    if p.is_absolute() or '..' in p.parts or not p.parts:
        raise ValueError('unsafe relative artifact path')
    return p


def locked_manifest():
    m = json.loads(LOCK.read_text())
    if m['schema_version'] != 1 or m['platform']['python_abi'] != 'cp313' or m['platform']['architecture'] != 'x86_64':
        raise ValueError('unsupported lock schema/platform; original preserved')
    names = [p['name'].lower().replace('_', '-') for p in m['packages']]
    if not names or len(names) != len(set(names)):
        raise ValueError('empty/duplicate locked package set')
    expected = [f"{p['name']}=={p['version']} --hash=sha256:{p['sha256']}" for p in m['packages']]
    actual = [line for line in REQUIREMENTS.read_text().splitlines() if line and not line.startswith('#')]
    if expected != actual:
        raise ValueError('requirements differ from reviewed artifact lock')
    for package in m['packages']:
        if str(checked_name(package['filename'])) != Path(package['filename']).name:
            raise ValueError('wheel filename is not a basename')
    return m


def artifact(record, target, caches, offline):
    """Never replace a mismatching pre-existing artifact, even on a retry."""
    if target.exists() or target.is_symlink():
        if target.is_symlink() or not target.is_file() or digest(target) != record['sha256']:
            raise ValueError(f'corrupt/unknown artifact retained: {target}')
        return target
    target.parent.mkdir(parents=True, exist_ok=True)
    for base in caches:
        source = base / target.name
        if source.is_file() and not source.is_symlink() and digest(source) == record['sha256']:
            shutil.copyfile(source, target)
            return target
    if offline:
        raise ValueError(f'offline artifact missing: {target.name}')
    url = record['url']
    if not url.startswith('https://'):
        raise ValueError('public artifact requires HTTPS')
    tmp = None
    try:
        with tempfile.NamedTemporaryFile(prefix='artifact-', dir=target.parent, delete=False) as f:
            tmp = Path(f.name)
            request = urllib.request.Request(url, headers={'User-Agent': 'WritingDeck-locked-public-build/1'})
            with urllib.request.urlopen(request, timeout=90) as response:
                count = 0
                while block := response.read(1024 * 1024):
                    count += len(block)
                    if count > MAX_ARTIFACT_BYTES:
                        raise ValueError('artifact exceeds bounded download size')
                    f.write(block)
        if digest(tmp) != record['sha256']:
            raise ValueError(f'locked artifact digest mismatch: {target.name}')
        # A racing/foreign file is never silently overwritten.
        os.link(tmp, target)
        print('verified public artifact', target.name, flush=True)
        return target
    finally:
        if tmp is not None:
            tmp.unlink(missing_ok=True)


def source_wheel(package, archive, target):
    """Rebuild from hashed archive without extracting or executing its contents."""
    source = package['source_repack']
    files = {}
    with tarfile.open(archive) as tar:
        members = {m.name: m for m in tar.getmembers() if m.isfile()}
        tops = {str(checked_name(n)).split('/')[0] for n in members}
        if len(tops) != 1:
            raise ValueError('ambiguous source archive root')
        top = next(iter(tops))
        for f in source['package_files']:
            name = str(checked_name(f['wheel_file']))
            member = members[top + '/' + str(checked_name(f['archive_file']))]
            if member.size > MAX_ARTIFACT_BYTES:
                raise ValueError('source member exceeds material bound')
            data = tar.extractfile(member).read()
            if hashlib.sha256(data).hexdigest() != f['sha256'] or name in files:
                raise ValueError('pinned source member changed/duplicated')
            files[name] = data
    metadata = ROOT / source['metadata_directory']
    if not metadata.resolve().is_relative_to((ROOT / 'os/python-tools/wheel-metadata').resolve()):
        raise ValueError('metadata outside retained public source recipe')
    for path in sorted(metadata.rglob('*')):
        if path.is_file():
            if path.is_symlink():
                raise ValueError('symlink in retained wheel metadata')
            name = path.relative_to(metadata).as_posix()
            checked_name(name)
            if '.dist-info/' not in name or name.endswith('/RECORD') or name in files:
                raise ValueError('unexpected retained metadata member')
            files[name] = path.read_bytes()
    record_name = next(n.split('/')[0] + '/RECORD' for n in files if '.dist-info/' in n)
    record = io.StringIO(newline='')
    writer = csv.writer(record, lineterminator='\n')
    for name, data in sorted(files.items()):
        value = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b'=').decode()
        writer.writerow([name, 'sha256=' + value, str(len(data))])
    writer.writerow([record_name, '', ''])
    files[record_name] = record.getvalue().encode()
    with zipfile.ZipFile(target, 'x', compression=zipfile.ZIP_STORED) as z:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            z.writestr(info, data)
    if digest(target) != package['sha256']:
        raise ValueError('source repack is not byte-reproducible; output kept for diagnosis')


def payload_files(root):
    files = []
    for p in sorted(root.rglob('*')):
        name = p.relative_to(root).as_posix()
        if name == MARKER:
            continue
        if p.is_symlink():
            files.append({'path': name, 'symlink': os.readlink(p)})
        elif p.is_file():
            files.append({'path': name, 'sha256': digest(p), 'bytes': p.stat().st_size})
    return files


def installed_payload(package, wheelhouse):
    """Derive expectations from locked wheel BYTES, not a self-asserted marker.

    Canonical RECORDs describe only the retained libraries/metadata. Removed CLI
    launcher hashes must not retain random temporary-build shebangs. This also
    makes two separately installed payloads byte-identical on the same CPython.
    """
    wheel = wheelhouse/package['filename']
    if wheel.is_symlink() or digest(wheel) != package['sha256']:
        raise ValueError('expected exact locked wheel: '+package['filename'])
    expected = {}
    with zipfile.ZipFile(wheel) as z:
        records = [n for n in z.namelist() if n.endswith('.dist-info/RECORD')]
        if len(records) != 1:
            raise ValueError('ambiguous wheel RECORD')
        record_name = records[0]
        for name in z.namelist():
            if name.endswith('/') or name == record_name:
                continue
            checked_name(name)
            if '.data/' in name or name in expected:
                raise ValueError('unsupported/duplicate wheel installation layout')
            data = z.read(name)
            expected[name] = {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
    distinfo = record_name.rsplit('/', 1)[0]
    generated = {distinfo+'/INSTALLER': b'pip; writingdeck-pinned-runtime-1\n', distinfo+'/REQUESTED': b''}
    for name, data in generated.items():
        expected[name] = {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
    record = io.StringIO(newline='')
    writer = csv.writer(record, lineterminator='\n')
    for name, item in sorted(expected.items()):
        value = base64.urlsafe_b64encode(bytes.fromhex(item['sha256'])).rstrip(b'=').decode()
        writer.writerow([name, 'sha256='+value, str(item['bytes'])])
    writer.writerow([record_name, '', ''])
    data = record.getvalue().encode()
    generated[record_name] = data
    expected[record_name] = {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
    return expected, generated


def verify_runtime(root, wheelhouse=None):
    manifest = locked_manifest()
    if root.is_symlink() or not root.is_dir():
        raise ValueError('runtime must be a real directory')
    wheelhouse = wheelhouse or (ROOT/'build/prose-runtime/wheels')
    record = json.loads((root / MARKER).read_text())
    if record['schema_version'] != 2 or record['lock_sha256'] != digest(LOCK):
        raise ValueError('runtime belongs to another/unknown lock or materialization schema; retained, not overwritten')
    if not re.fullmatch(r'3\.13\.\d+', record['python']):
        raise ValueError('unexpected runtime Python ABI')
    packages = [{'name': p['name'], 'version': p['version'], 'wheel_sha256': p['sha256']} for p in manifest['packages']]
    if record['packages'] != packages or record['platform'] != manifest['platform']:
        raise ValueError('runtime package/platform record differs from lock')
    expected = {}
    prefix = 'lib/python3.13/site-packages/'
    for package in manifest['packages']:
        members, _ = installed_payload(package, wheelhouse)
        for name, item in members.items():
            path = prefix+name
            if path in expected:
                raise ValueError('ambiguous ownership of installed package member')
            expected[path] = {'path': path, **item}
    for name, target in [('bin/python','python3'), ('bin/python3','/usr/bin/python3'), ('bin/python3.13','python3'), ('lib64','lib')]:
        expected[name] = {'path': name, 'symlink': target}
    cfg = f"home = /usr/bin\ninclude-system-site-packages = false\nversion = {record['python']}\nexecutable = /usr/bin/python3\n".encode()
    expected['pyvenv.cfg'] = {'path': 'pyvenv.cfg', 'sha256': hashlib.sha256(cfg).hexdigest(), 'bytes': len(cfg)}
    actual = payload_files(root)
    if actual != record['files'] or actual != sorted(expected.values(), key=lambda f: f['path']):
        raise ValueError('runtime differs from exact locked wheel payload (including unexpected files/bytecode); retained, not overwritten')
    return record


def install_runtime(output, manifest, python):
    root = output / 'runtime'
    if root.exists() or root.is_symlink():
        record = verify_runtime(root)
        print('reused verified runtime', len(record['files']), 'files', flush=True)
        return
    temporary = Path(tempfile.mkdtemp(prefix='.runtime-', dir=output))
    bootstrap = None
    try:
        subprocess.run([str(python), '-I', '-B', '-m', 'venv', '--without-pip', str(temporary)], check=True)
        bootstrap = Path(tempfile.mkdtemp(prefix='.pip-builder-', dir=output))
        subprocess.run([str(python), '-I', '-B', '-m', 'venv', str(bootstrap)], check=True)
        env = os.environ | {'PYTHONDONTWRITEBYTECODE': '1', 'PIP_NO_INDEX': '1', 'PIP_DISABLE_PIP_VERSION_CHECK': '1'}
        subprocess.run([
            str(bootstrap / 'bin/python3'), '-I', '-B', '-m', 'pip', '--isolated',
            '--python', str(temporary / 'bin/python3'), '--disable-pip-version-check',
            'install', '--no-index', '--no-cache-dir', '--no-deps', '--no-compile',
            '--find-links', str(output / 'wheels'), '--require-hashes', '-r', str(REQUIREMENTS),
        ], check=True, env=env)
        subprocess.run([
            str(bootstrap / 'bin/python3'), '-I', '-B', '-m', 'pip', '--isolated',
            '--python', str(temporary / 'bin/python3'), '--disable-pip-version-check', 'check',
        ], check=True, env=env)
        actual = json.loads(subprocess.check_output([
            str(temporary / 'bin/python3'), '-I', '-B', '-c',
            'import importlib.metadata as m,json;print(json.dumps({d.metadata["Name"]:d.version for d in m.distributions()}))',
        ], text=True))
        if actual != {p['name']: p['version'] for p in manifest['packages']}:
            raise ValueError('installed package set differs from complete lock')
        # The product invokes the interpreter directly, not upstream CLI
        # scripts carrying a temporary build-host shebang. Do not ship dead
        # launchers or a pip/downloader in the runtime.
        for p in (temporary / 'bin').iterdir():
            if p.name not in ('python', 'python3', 'python3.13'):
                p.unlink()
        (temporary / 'bin/python3').unlink()
        (temporary / 'bin/python3').symlink_to('/usr/bin/python3')
        for name in ('python', 'python3.13'):
            p = temporary / 'bin' / name
            p.unlink(missing_ok=True)
            p.symlink_to('python3')
        version = subprocess.check_output([str(python), '--version'], text=True).strip().split()[-1]
        (temporary / 'pyvenv.cfg').write_text(f'home = /usr/bin\ninclude-system-site-packages = false\nversion = {version}\nexecutable = /usr/bin/python3\n')
        for p in temporary.rglob('*'):
            if p.is_file() and not p.is_symlink():
                p.chmod(0o644)
            elif p.is_dir() and not p.is_symlink():
                p.chmod(0o755)
        temporary.chmod(0o755)
        # Never ship an unchecked bytecode path that could outrank pinned
        # source. Product invocations use -B, so subsequent .pyc files are
        # unexpected payload changes and hash-only verification rejects them.
        for cache in list(temporary.rglob('__pycache__')):
            if cache.is_dir() and not cache.is_symlink():
                shutil.rmtree(cache)
        for bytecode in temporary.rglob('*.pyc'):
            bytecode.unlink()
        files = payload_files(temporary)
        record = {
            'schema_version': 1, 'lock_sha256': digest(LOCK),
            'platform': manifest['platform'], 'python': version,
            'source': 'Exact offline wheels; no dependency resolution. CPython/stdlib supplied by separately inventoried Debian image.',
            'runtime_installation_path': '/opt/writingdeck/python',
            'packages': [{'name': p['name'], 'version': p['version'], 'wheel_sha256': p['sha256']} for p in manifest['packages']],
            'files': files,
        }
        (temporary / MARKER).write_text(json.dumps(record, indent=2) + '\n')
        temporary.rename(root)
        temporary = None
        verify_runtime(root)
        print('prepared verified relocatable runtime', root, len(files), 'files', flush=True)
    finally:
        if temporary is not None:
            shutil.rmtree(temporary)
        if bootstrap is not None:
            shutil.rmtree(bootstrap)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, default=ROOT / 'build/prose-runtime')
    p.add_argument('--cache-dir', type=Path, action='append', default=[])
    p.add_argument('--offline', action='store_true')
    p.add_argument('--include-sources', action='store_true', help='also retain each available upstream dependency sdist for source/release review')
    p.add_argument('--artifacts-only', action='store_true')
    p.add_argument('--verify-runtime', type=Path, help='hash-only validation, no interpreter/model execution')
    p.add_argument('--python', type=Path, default=Path('/usr/bin/python3'))
    args = p.parse_args()
    manifest = locked_manifest()
    if args.verify_runtime is not None:
        record = verify_runtime(args.verify_runtime)
        print('verified runtime', len(record['packages']), 'packages', len(record['files']), 'payload files')
        return
    if os.geteuid() == 0:
        raise ValueError('provision as a normal user; only staged-image copying needs root')
    if sys.platform != 'linux' or platform.machine() != 'x86_64':
        raise ValueError('locked artifacts require Linux x86_64')
    version = subprocess.check_output([str(args.python), '-I', '-B', '-c', 'import sys;print(f"{sys.version_info.major}.{sys.version_info.minor}")'], text=True).strip()
    if version != '3.13':
        raise ValueError('locked ABI requires Debian CPython 3.13')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for package in manifest['packages']:
        target = output / 'wheels' / package['filename']
        if source := package['source_repack']:
            archive = artifact(source, output / 'sources' / source['archive_filename'], args.cache_dir, args.offline)
            if target.exists() or target.is_symlink():
                artifact(package, target, args.cache_dir, True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                source_wheel(package, archive, target)
                print('reproduced pinned source wheel', package['name'], flush=True)
        else:
            artifact(package, target, args.cache_dir, args.offline)
        if args.include_sources and package['corresponding_source']:
            source = package['corresponding_source']
            artifact(source, output / 'sources' / source['filename'], args.cache_dir, args.offline)
    if not args.artifacts_only:
        install_runtime(output, manifest, args.python)
    print('all', len(manifest['packages']), 'locked artifacts verified; no runtime inference or downloads enabled')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        raise SystemExit(f'prose runtime provisioning failed: {exc}')
