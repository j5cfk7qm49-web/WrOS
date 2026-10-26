#!/usr/bin/env python3
"""Execute offline software gates and retain a source-aligned evidence record.

Run unprivileged after fetching the locked Rust graph and installing build
prerequisites. Does not install packages, boot a VM, touch devices or push Git.
Failures retain a partial report under build/ and do not overwrite prior passing
evidence. VM/physical/provider/security/rights gates remain separate.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

from writingdeck_source import fingerprint

ROOT = Path(__file__).resolve().parents[1]


def utc():
    return datetime.now(timezone.utc).isoformat()


def verifier_inputs():
    paths = sorted({p for pattern in ('tools/*.py', 'tools/*.sh', '.github/workflows/*.yml')
                    for p in ROOT.glob(pattern)})
    return {p.relative_to(ROOT).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--milestone', required=True, help='Honest description of the tested scope, not a production claim')
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise SystemExit('Run as a normal user: root PTY skips are not accepted as passing evidence.')
    os.chdir(ROOT)
    logs = ROOT/'build/verification'
    logs.mkdir(parents=True, exist_ok=True)
    previous = json.loads((ROOT/'docs/evidence/test-results.json').read_text())
    start_source = fingerprint()
    start_inputs = verifier_inputs()
    record = {
        'recorded_at_utc': utc(), 'verification_started_at_utc': utc(),
        'milestone': args.milestone, 'source_fingerprint': start_source,
        'verifier_input_sha256': start_inputs,
        'git_head_at_start': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'previous_evidence': {'milestone': previous.get('milestone'), 'source_fingerprint': previous.get('source_fingerprint')},
        'toolchain': subprocess.check_output(['rustc', '--version'], text=True).strip(),
        'rust_suites': [], 'checks': [],
        'limitations': [
            'Overlapping configurations are not distinct-test counts; ignored external/performance/model tests are not passed.',
            'Temporary fixtures and compiled PTYs are not physical keyboard, laptop, power-cut, signed release or provider qualification.',
            'Recorded edit totals do not certify complete all-day/crash-tail coverage or exclusive human typing.',
            'This verifier does not install on a host disk, enroll credentials, open user manuscripts or push Git.',
        ],
    }

    def run(name, command, env=None):
        path = logs/(name+'.log')
        started = time.monotonic()
        with path.open('w') as log:
            result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, env=env)
        check = {'name': name, 'command': ' '.join(map(str, command)), 'exit_code': result.returncode,
                 'seconds': round(time.monotonic()-started, 3), 'log': str(path.relative_to(ROOT))}
        record['checks'].append(check)
        text = path.read_text(errors='replace')
        print(f'{name}: exit {result.returncode}, {check["seconds"]} s', flush=True)
        if result.returncode:
            (logs/'partial-results.json').write_text(json.dumps(record, indent=2)+'\n')
            print(text[-15000:], file=sys.stderr)
            raise SystemExit(result.returncode)
        return check, text

    run('format', ['cargo', 'fmt', '--all', '--', '--check'])
    run('spark-corpus', ['python3', 'tools/build-spark-corpus.py', '--check'])
    run('diagnostic-assets', ['python3', 'tools/audit-writing-checks-assets.py'])
    runtime = Path(os.environ.get('WRITINGDECK_TOOLS_PYTHON', ROOT/'build/prose-runtime/runtime/bin/python3')).parent.parent
    run('prose-assets', ['python3', 'tools/audit-prose-tools.py', '--runtime', str(runtime)])
    record['python_runtime_marker_sha256'] = hashlib.sha256((runtime/'.writingdeck-runtime.json').read_bytes()).hexdigest()
    for name, flags in [('default', []), ('no-audio', ['--no-default-features'])]:
        check, text = run('tests-'+name, ['cargo', 'test', '--workspace', *flags, '--locked', '--offline', '-j2'])
        totals = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
        if not totals:
            raise SystemExit('Missing Rust test summary; no evidence published.')
        passed, failed, ignored = map(sum, zip(*(map(int, row) for row in totals)))
        if failed or not passed:
            raise SystemExit('Invalid Rust totals; no passing evidence published.')
        record['rust_suites'].append(check | {'configuration': name, 'passed': passed, 'failed': failed, 'ignored': ignored,
            'ignored_tests': [line.strip() for line in text.splitlines() if ' ... ignored' in line]})
        run('clippy-'+name, ['cargo', 'clippy', '--workspace', '--all-targets', *flags, '--locked', '--offline', '-j2', '--', '-D', 'warnings'])
    run('build-no-audio', ['cargo', 'build', '--workspace', '--no-default-features', '--locked', '--offline', '-j2'])
    binary = Path(os.environ.get('GRIMOIRE_BINARY', ROOT/'target/debug/grimoire'))
    record['tested_no_audio_binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    check, text = run('python', ['python3', '-m', 'unittest', 'discover', '-s', 'os/tests', '-p', 'test_*.py', '-v'],
                      os.environ | {'WRITINGDECK_CAPTURE_PTY': '1'})
    summary = re.search(r'Ran (\d+) tests in ([\d.]+)s', text)
    skipped = len(re.findall(r'\.\.\. skipped ', text))
    if not summary or skipped or '\nOK\n' not in text:
        raise SystemExit('Missing/skipped Python coverage; no passing evidence published.')
    record['python'] = check | {'passed': int(summary.group(1)), 'failed': 0, 'skipped': skipped,
        'pty_scope': 'Actual non-root compiled app, new demo/HOME only; original edit/save/undo/restart, four terminal sizes, metadata failures/retry. Not a full emulator or physical certificate.'}
    run('terminal-capture', ['python3', 'tools/capture-writingdeck-screens.py'])
    run('shellcheck', ['shellcheck', *map(str, sorted(Path('os/scripts').glob('*.sh'))), 'os/scripts/writingdeck-session', 'tools/test-m7b.sh'])
    sources = [*sorted(Path('os/tests').glob('*.py')), *sorted(Path('tools').glob('*.py')),
               Path('os/scripts/install-plan.py'), Path('os/scripts/writingdeck-keyboard')]
    run('python-syntax', ['python3', '-m', 'py_compile', *map(str, sources)])
    run('inventories', ['python3', 'tools/generate-writingdeck-manifests.py'])
    if (ROOT/'build/appliance/rootfs/etc/debian_version').is_file():
        run('wifi-image-payload', ['python3', 'tools/audit-wifi-image.py', 'build/appliance/rootfs'])
    run('printable-reference', ['python3', 'tools/generate-keyboard-reference.py'])
    run('documentation', ['python3', 'tools/check-writingdeck.py'])
    run('diff-whitespace', ['git', 'diff', '--check'])
    if fingerprint() != start_source or verifier_inputs() != start_inputs:
        raise SystemExit('Source changed during verification; no passing evidence published.')
    record['verification_finished_at_utc'] = utc()
    (ROOT/'docs/evidence/test-results.json').write_text(json.dumps(record, indent=2, ensure_ascii=False)+'\n')
    print('Published executed software evidence. Refresh separate VM evidence before --require-current-evidence.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
