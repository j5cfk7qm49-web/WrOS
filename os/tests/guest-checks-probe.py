#!/usr/bin/python3
"""VM-only, run as writer: genuine model/UTF-8 apply/undo with zero NICs.

The parent root probe only creates this unprivileged child. All manuscripts
and HOME here are new disposable fixtures, not the dashboard's project.
"""
import json
import os
from pathlib import Path
import re
import tempfile
import time

os.environ['GRIMOIRE_BINARY'] = '/usr/bin/grimoire'
from test_checks_pty import check_fixture, open_checks, quit_normally, NOVEL, PERSONAL, TEXT
from test_productivity_pty import records
from test_secondary_pty import TerminalProcess


def main():
    assert os.geteuid() != 0, 'model app must never run as root'
    with tempfile.TemporaryDirectory(prefix='wd-vm-grammar-') as tmp:
        project, home, primary = check_fixture(Path(tmp))
        before = primary.read_bytes()
        terminal = TerminalProcess(project, home, 80, 24)
        try:
            terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
            open_checks(terminal); terminal.screen_shows('No current result')
            terminal.send(b'r'); terminal.screen_shows('explicit permission')
            start = time.monotonic(); terminal.send(b'y')
            terminal.until(lambda: '1 current suggestions' in terminal.screen.text
                           and 'AnA' in terminal.screen.text, 'real offline Harper in the 512-MiB guest', timeout=100)
            cold_ms = round((time.monotonic()-start)*1000, 2)
            frame = terminal.screen.text
            status = Path(f'/proc/{terminal.process.pid}/status').read_text()
            uid = int(re.search(r'^Uid:\s+(\d+)', status, re.M)[1])
            peak = int(re.search(r'^VmHWM:\s+(\d+)', status, re.M)[1])
            assert uid == os.geteuid() != 0
            assert primary.read_bytes() == before and records(project) == []
            assert not (project/NOVEL).exists() and not (home/PERSONAL).exists()
            terminal.send(b'a\x13')
            terminal.until(lambda: primary.read_bytes() == before.replace(b'a apple', b'an apple', 1)
                           and any(e['origin'] == 'replace' for e in records(project)), 'original-source explicit apply and origin checkpoint')
            terminal.send(b'\x1a\x13')
            terminal.until(lambda: primary.read_bytes() == before
                           and any(e['origin'] == 'undo' for e in records(project)), 'byte-identical Unicode undo/checkpoint')
            quit_normally(terminal)
            print(json.dumps({'uid':uid, 'cold_wall_ms':cold_ms, 'kernel_peak_rss_kib':peak,
                'source_body_bytes':len(TEXT.encode()), 'frame':frame,
                'checks':['explicit model permission','real AnA grammar/Dialogue','no analysis typing credit',
                          'once permission not persisted','exact source replacement','byte-identical native undo'],
                'limits':'Virtual CPU/debug binary/single small fixture only; not physical, production or long-load qualification.'}))
        finally:
            terminal.close()


if __name__ == '__main__':
    main()
