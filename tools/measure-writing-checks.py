#!/usr/bin/env python3
"""One-shot Linux/debug opt-in grammar measurement on disposable original fixtures.

No benchmark of physical battery, production optimizations, exclusive typing
or the future Python services. No networking, host device or user manuscript.
"""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'os/tests'))
from test_checks_pty import check_fixture, open_checks, quit_normally, TEXT
from test_secondary_pty import BINARY, TerminalProcess
from writingdeck_source import fingerprint


def stats(process):
    status = (Path('/proc')/str(process.pid)/'status').read_text()
    values = {k: int(v) for k, v in re.findall(r'^(VmRSS|VmHWM|Threads):\s+(\d+)', status, re.M)}
    ticks = (Path('/proc')/str(process.pid)/'stat').read_text().rsplit(')', 1)[1].split()
    values['cpu_seconds'] = (int(ticks[11])+int(ticks[12]))/os.sysconf('SC_CLK_TCK')
    return values


def measured_check(terminal, command):
    before = stats(terminal.process)
    start = time.monotonic()
    terminal.send(command)
    # Clear-result/pending frame is a real input-processing barrier. Merely
    # finding the prior report text would falsely measure a hot run as zero.
    terminal.until(lambda: 'Local check running' in terminal.screen.text,
                   'new request pending frame', timeout=90)
    samples = []
    def complete():
        samples.append(stats(terminal.process))
        return 'current suggestions' in terminal.screen.text and 'AnA' in terminal.screen.text
    terminal.until(complete, 'new actual Harper result', timeout=120)
    after = stats(terminal.process)
    engine_ms = re.search(r'current suggestions\s+·\s+(\d+) ms', terminal.screen.text)
    return {'wall_ms': round((time.monotonic()-start)*1000, 2),
            'engine_reported_ms': int(engine_ms[1]) if engine_ms else None,
            'cpu_seconds_delta': round(after['cpu_seconds']-before['cpu_seconds'], 3),
            'max_sampled_rss_kib': max(s['VmRSS'] for s in samples),
            'kernel_peak_rss_kib': after['VmHWM'], 'after': after}


def main():
    assert os.geteuid() != 0, 'never launch Grimoire as root'
    measured = []
    for name, text in [('small-unicode-dialogue', TEXT),
                       ('synthetic-2340-words', 'The courier carried a envelope past the harbor.\n\n'*260)]:
        with tempfile.TemporaryDirectory(prefix='wd-check-measure-') as tmp:
            project, home, primary = check_fixture(Path(tmp))
            front, delimiter, _ = primary.read_text().partition('\n---\n')
            assert delimiter
            primary.write_text(front+delimiter+'\n'+text)
            before = primary.read_bytes()
            terminal = TerminalProcess(project, home, 100, 30)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); open_checks(terminal)
                baseline = stats(terminal.process)
                terminal.send(b'r'); terminal.screen_shows('explicit permission')
                cold = measured_check(terminal, b'y')
                hot = measured_check(terminal, b'r')
                terminal.send(b'c'); terminal.screen_shows('No current result')
                stopped = stats(terminal.process)
                assert primary.read_bytes() == before, 'analysis changed the original'
                measured.append({'fixture':name, 'source_body_bytes':len(text.encode()),
                    'fixture_sha256':hashlib.sha256(text.encode()).hexdigest(), 'baseline':baseline,
                    'cold':cold, 'hot':hot, 'after_pause':stopped})
                quit_normally(terminal)
            finally:
                terminal.close()
    record = {'schema_version':1, 'recorded_at_utc':datetime.now(timezone.utc).isoformat(),
        'scope':'Sandbox Linux actual non-root no-audio DEBUG binary; isolated demo/HOME, 100x30 PTY, two specified synthetic fixtures. NOT physical or production/battery qualification.',
        'source_fingerprint':fingerprint(), 'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),
        'runtime':'harper-core-2.13.0/brill-2.13.0/pos-2.13.0', 'measurements':measured,
        'limits':['End-to-end includes polling/render timing; kernel VmHWM catches peaks between RSS samples.',
                  'Pause drops the receiver/worker reference; active native parsing is not preempted. Shared dictionary and allocator memory can remain.',
                  'No latency/RSS claim for Python/spaCy, physical low-power CPUs or arbitrary long prose. 65,536-byte service cap remains explicit.']}
    path = ROOT/'docs/evidence/writing-checks-resources.json'
    path.write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()
