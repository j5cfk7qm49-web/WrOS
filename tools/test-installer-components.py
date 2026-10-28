#!/usr/bin/env python3
"""Qualify the real source-built Debian backend without root or system mounts.

The optional build-recipe test runtime has its own compiled-in configuration
path. Synthetic credentials/databases stay in a new private tmpfs directory.
Only the resulting credential-free reports are retained in the workspace.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

from writingdeck_source import fingerprint

ROOT = Path(__file__).resolve().parents[1]


def config_text(fixture, runtime):
    def quote(path):
        value = str(path)
        if any(c in value for c in ('"', '\\', '\n', '\r', '\0')):
            raise ValueError('Unsafe test configuration path')
        return '"' + value + '"'
    library = runtime / 'usr/lib/cdebconf'
    return f'''global {{
 module_path {{ frontend {quote(library / 'frontend')}; database {quote(library / 'db')}; }};
 default {{ frontend "default_fe"; template "templatedb"; config "configdb"; }};
}};
frontend {{ instance "default_fe" {{ driver "passthrough"; }}; }};
template {{ instance "templatedb" {{ driver "rfc822db"; path {quote(fixture / 'templates.dat')}; }}; }};
config {{ instance "configdb" {{ driver "rfc822db"; path {quote(fixture / 'questions.dat')}; mode 600; }}; }};
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path, help='New build-installer-frontend.sh --test-runtime output')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise SystemExit('Run installer component qualification as a normal user.')
    build = args.build.resolve()
    manifest = json.loads((build / 'manifest.json').read_bytes())
    if not manifest.get('private_test_runtime') or not manifest.get('frontend_debug_formats_absent'):
        raise SystemExit('Requires the source-built private test runtime with frontend debug disabled.')
    runtime = build / 'test-runtime'
    configuration = runtime / 'etc/cdebconf.conf'
    module = runtime / 'usr/lib/cdebconf/frontend/passthrough.so'
    if hashlib.sha256(module.read_bytes()).hexdigest() != manifest['module_sha256']:
        raise SystemExit('Test module differs from the built manifest.')
    temporary = Path(os.environ.get('WROS_INSTALLER_TEST_TMP', '/dev/shm')).resolve()
    filesystem = subprocess.check_output(['findmnt', '-n', '-o', 'FSTYPE', '-T', str(temporary)], text=True).strip()
    if filesystem != 'tmpfs':
        raise SystemExit('Synthetic installer credentials require an available tmpfs test directory.')
    descriptor = os.open(configuration, os.O_RDWR | os.O_CLOEXEC | os.O_NOFOLLOW)
    with os.fdopen(descriptor, 'w') as config:
        metadata = os.fstat(config.fileno())
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.geteuid()
                or metadata.st_nlink != 1 or metadata.st_mode & 0o077):
            raise SystemExit('Test configuration must be a private owned regular file.')
        fcntl.flock(config, fcntl.LOCK_EX | fcntl.LOCK_NB)
        source = fingerprint()
        results = {}
        try:
            for name, script in (('frontend', ROOT / 'tools/test-installer-backend.py'),
                                 ('faults', ROOT / 'os/tests/cdebconf_protocol.py')):
                with tempfile.TemporaryDirectory(prefix='wros-installer-components-', dir=temporary) as directory:
                    fixture = Path(directory)
                    (fixture / 'WROS-SYNTHETIC-ONLY').touch()
                    contents = config_text(fixture, runtime)
                    (fixture / 'cdebconf.conf').write_text(contents)
                    config.seek(0)
                    config.truncate()
                    config.write(contents)
                    config.flush()
                    report = args.report.with_name(args.report.stem + '-' + name + '.json')
                    module_argument = '--passthrough-module' if name == 'frontend' else '--module'
                    command = [sys.executable, str(script), str(fixture), str(runtime), module_argument,
                               str(module), '--config-file', str(configuration), '--report', str(report)]
                    subprocess.run(command, cwd=ROOT, check=True, timeout=90)
                    results[name] = json.loads(report.read_bytes())
        finally:
            config.seek(0)
            config.truncate()
            config.flush()
    if any(r['source_fingerprint'] != source or not r['passed'] for r in results.values()) or fingerprint() != source:
        raise SystemExit('Component qualification failed or source changed.')
    record = {'passed': True, 'source_fingerprint': source,
              'scope': 'Actual maintained Debian backend and WrOS TUI, non-root tmpfs fixtures; no system mounts, target disk, account or privileged live-launch qualification',
              'module_sha256': manifest['module_sha256'],
              'build_manifest_sha256': hashlib.sha256((build / 'manifest.json').read_bytes()).hexdigest(),
              'frontend_checks': len(results['frontend']['checks']),
              'fault_cases': len(results['faults']['cases']),
              'reports': {name: args.report.stem + '-' + name + '.json' for name in results}}
    args.report.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
