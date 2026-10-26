#!/usr/bin/env python3
"""Measure explicit offline analyzers on disposable synthetic Markdown only.

Each sample starts a new worker. A repeated run is filesystem-cache-warm, not
a resident/warm model. These measurements do not certify typing latency,
physical battery use, optimized builds, or whole-book throughput.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import tempfile
import time

from writingdeck_source import fingerprint

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='new result file; never overwrite evidence')
    args = parser.parse_args()
    if os.geteuid() == 0 or args.output.exists():
        raise SystemExit('Use a non-root user and a new output file')
    binary = Path(os.environ.get('GRIMOIRE_BINARY', ROOT/'target/debug/grimoire'))
    runtime = Path(os.environ.get('WRITINGDECK_TOOLS_PYTHON', ROOT/'build/prose-runtime/runtime/bin/python3'))
    before_source = fingerprint()
    measurements = []
    fixtures = [
        ('unicode-dialogue', 'Café 🦊 e\u0301. “He could of gone,” Mara said. The gate was opened by Mara. We waited in order to see.\n'),
        ('synthetic-repeated-prose', ('The courier waited in order to see the gate. It was opened by Mara. She carried a lantern through the rain.\n\n')*200),
    ]
    with tempfile.TemporaryDirectory(prefix='wros-prose-measure-') as tmp:
        base = Path(tmp)
        for name, text in fixtures:
            source = base/(name+'.md')
            payload = text.encode()
            source.write_bytes(payload)
            for engine in ('native', 'proselint', 'nabokov'):
                for iteration in (1, 2):
                    command = [str(binary), 'writingdeck', 'prose', str(source), '--engine', engine]
                    if engine == 'nabokov':
                        command.append('--allow-model')
                    before = resource.getrusage(resource.RUSAGE_CHILDREN)
                    started = time.monotonic()
                    result = subprocess.run(command, capture_output=True, text=True, timeout=70,
                                            env=os.environ | {'HOME': tmp, 'WRITINGDECK_TOOLS_PYTHON': str(runtime)})
                    elapsed = time.monotonic()-started
                    after = resource.getrusage(resource.RUSAGE_CHILDREN)
                    if result.returncode:
                        raise RuntimeError(f'{engine} failed: {result.stderr}\n{result.stdout}')
                    report = json.loads(result.stdout)
                    if report['status'] != 'complete' or source.read_bytes() != payload:
                        raise RuntimeError('non-complete result or source modification')
                    measurements.append({
                        'fixture': name, 'bytes': len(payload), 'whitespace_words': len(text.split()),
                        'fixture_sha256': hashlib.sha256(payload).hexdigest(),
                        'engine': engine, 'fresh_process_iteration': iteration,
                        'wall_seconds': round(elapsed, 4),
                        'cpu_seconds': round(after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime, 4),
                        'worker_peak_rss_kib': report['child_peak_rss_kib'] if engine != 'native' else None,
                        'engine_reported_ms': report['elapsed_ms'],
                        'findings': len(report['findings']), 'omitted': report['omitted'],
                    })
    if fingerprint() != before_source:
        raise RuntimeError('source changed during measurement')
    record = {
        'schema_version': 1, 'recorded_at_utc': datetime.now(timezone.utc).isoformat(),
        'source_fingerprint': before_source,
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'platform': platform.platform(), 'python': platform.python_version(),
        'runtime_marker_sha256': hashlib.sha256((runtime.parent.parent/'.writingdeck-runtime.json').read_bytes()).hexdigest(),
        'scope': 'Non-root synthetic CLI runs; no manuscript edits or external network required.',
        'limitations': ['Fresh subprocess per sample; no resident warm-model claim.',
                       'Worker RSS excludes the Rust UI and rest of the OS; native RSS is not measured.',
                       'Repetitive fixture is not a representative literary corpus.',
                       'No physical laptop, battery, typing-latency or production-readiness claim.'],
        'measurements': measurements,
    }
    with args.output.open('x') as output:
        json.dump(record, output, indent=2)
        output.write('\n')
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()
