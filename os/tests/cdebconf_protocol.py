#!/usr/bin/env python3
"""Fault-test real cdebconf using synthetic RAM markers, never target devices.

Run non-root in a prepared private mount/network namespace, with its dedicated
cdebconf.conf bound over /etc/cdebconf.conf. Normal acceptance is the positive
control; broken transport/status must not create the synthetic write marker.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools'))
from writingdeck_source import fingerprint

CASES = ('yes', 'no', 'back', 'half-close', 'truncated', 'malformed',
         'status-suffix', 'negative', 'error-status', 'nul', 'crlf',
         'oversized', 'write-failure', 'back-during-data')


def adopted_children():
    path = Path(f'/proc/{os.getpid()}/task/{os.getpid()}/children')
    return [int(value) for value in path.read_text().split()]


def reap_config_scripts():
    """Only this harness's adopted children; use pidfds, never stale group IDs."""
    deadline = time.monotonic() + .5
    forced = False
    while adopted_children():
        for pid in adopted_children():
            try:
                if os.waitpid(pid, os.WNOHANG)[0]:
                    continue
                if time.monotonic() >= deadline:
                    fd = os.pidfd_open(pid)
                    try:
                        signal.pidfd_send_signal(fd, signal.SIGKILL)
                    finally:
                        os.close(fd)
                    os.waitpid(pid, 0)
                    forced = True
            except (ProcessLookupError, ChildProcessError):
                pass
        time.sleep(.01)
    return forced


def run_case(fixture, package, module, mode):
    case = fixture / mode
    case.mkdir()
    tag = 'fixture/' + mode
    (case / 'case.templates').write_text(
        f'Template: {tag}\nType: boolean\nDefault: true\n'
        'Description: Synthetic write confirmation\n'
        ' No device is opened; only a private RAM marker can be created.\n')
    script = case / 'case.postinst'
    script.write_text(f'''#!/bin/sh
set -eu
. /usr/share/debconf/confmodule
db_x_loadtemplatefile case.templates fixture
db_capb backup
db_input critical {tag}
if db_go; then
    db_get {tag}
    if test "$RET" = true; then printf synthetic-only > fake-write-marker; fi
else
    result=$?
    test "$result" = 30 || exit 91
    printf back > back-marker
fi
db_stop
''')
    script.chmod(0o700)
    frontend, backend = socket.socketpair()
    environment = {
        'PATH': '/usr/bin:/bin', 'HOME': str(case), 'LANG': 'C.UTF-8',
        'LD_LIBRARY_PATH': str(package / 'usr/lib/cdebconf') + ':' + str(package / 'usr/lib/x86_64-linux-gnu'),
        'DEBCONF_READFD': str(backend.fileno()), 'DEBCONF_WRITEFD': str(backend.fileno()),
        'DEBCONF_DEBUG': '0',
    }
    with (case / 'backend.log').open('wb') as log:
        process = subprocess.Popen(
            [str(package / 'usr/lib/cdebconf/debconf'), '-fpassthrough', '-ofixture', '/bin/sh', str(script)],
            cwd=case, env=environment, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
            pass_fds=(backend.fileno(),), start_new_session=True,
            # Python ignores SIGPIPE. Keep that disposition to exercise the
            # module's explicit write-error handling rather than signal luck.
            restore_signals=False)
    backend.close()
    wire = frontend.makefile('rwb', buffering=0)
    observed, loaded, timed_out = [], False, False
    started = time.monotonic()
    try:
        while process.poll() is None and time.monotonic() - started < 5:
            if not select.select([frontend], [], [], .1)[0]:
                continue
            try:
                line = wire.readline(65_538)
            except ConnectionResetError:
                break
            if not line:
                break
            command = line.split(b' ', 1)[0].strip().decode('ascii')
            observed.append(command)
            if not loaded:
                loaded = str(module.resolve()) in Path(f'/proc/{process.pid}/maps').read_text()
            if mode == 'write-failure':
                frontend.shutdown(socket.SHUT_RDWR)
                break
            if command == 'GO':
                if mode in ('half-close', 'truncated'):
                    if mode == 'truncated':
                        wire.write(b'0')
                    frontend.shutdown(socket.SHUT_WR)
                    continue
                response = {
                    'malformed': b'not-a-status\n', 'status-suffix': b'0garbage\n',
                    'negative': b'-1 failed\n', 'error-status': b'10 failed\n',
                    'nul': b'0 \x00invalid\n', 'crlf': b'0\r\n',
                    'oversized': b'0 ' + b'x' * 65_536 + b'\n', 'back': b'30\n',
                }.get(mode, b'0\n')
            elif mode in ('half-close', 'truncated') and 'GO' in observed:
                continue
            elif command == 'CAPB':
                response = b'0 backup\n'
            elif command == 'GET':
                response = b'0 false\n' if mode == 'no' else b'0 true\n'
            elif mode == 'back-during-data' and command == 'DATA':
                response = b'30\n'
            else:
                response = b'0\n'
            try:
                wire.write(response)
            except (BrokenPipeError, ConnectionResetError):
                break
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=2)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=2)
        wire.close()
        frontend.close()
    forced = reap_config_scripts()
    written = (case / 'fake-write-marker').is_file()
    went_back = (case / 'back-marker').is_file()
    normal = mode in ('yes', 'no', 'back')
    passed = (loaded and not timed_out and not forced
              and process.returncode == 0 and written == (mode == 'yes')
              and went_back == (mode == 'back')) if normal else (
                  loaded and not timed_out and not forced and process.returncode != 0
                  and not written and not went_back and 'GET' not in observed)
    return {'case': mode, 'passed': passed, 'backend_exit': process.returncode,
            'synthetic_write_marker': written, 'back_marker': went_back,
            'commands': observed, 'module_observed': loaded,
            'timed_out': timed_out, 'forced_child_cleanup': forced,
            'seconds': round(time.monotonic() - started, 3)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture', type=Path)
    parser.add_argument('package_root', type=Path)
    parser.add_argument('--module', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--config-file', type=Path, default=Path('/etc/cdebconf.conf'))
    parser.add_argument('--observe', action='store_true', help='Retain original failures without declaring a pass')
    args = parser.parse_args()
    if os.geteuid() == 0 or not (args.fixture / 'WROS-SYNTHETIC-ONLY').is_file():
        raise SystemExit('Requires a prepared private non-root synthetic fixture.')
    if (args.fixture / 'cdebconf.conf').read_bytes() != args.config_file.read_bytes():
        raise SystemExit('Private backend configuration differs from the prepared fixture.')
    # Adopt only the config scripts this harness launches if cdebconf exits first.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        raise SystemExit('Cannot supervise synthetic child lifetime.')
    source = fingerprint()
    results = [run_case(args.fixture, args.package_root, args.module, mode) for mode in CASES]
    record = {'recorded_at_utc': datetime.now(timezone.utc).isoformat(),
              'scope': 'Actual non-root Debian cdebconf with private RAM confirmation markers; no target disk, user account or privileged live launcher',
              'observation_only': args.observe,
              'source_fingerprint': source, 'module_sha256': hashlib.sha256(args.module.read_bytes()).hexdigest(),
              'source_unchanged': source == fingerprint(), 'cases': results,
              'module_debug_formats_absent': all(value not in args.module.read_bytes() for value in
                                                 (b'----> %s', b'<---- %s', b'Got "%s" for %s')),
              'passed': all(result['passed'] for result in results)}
    args.report.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({'passed': record['passed'], 'passed_cases': sum(c['passed'] for c in results), 'cases': len(results)}))
    if not args.observe and not (record['passed'] and record['source_unchanged'] and record['module_debug_formats_absent']):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
