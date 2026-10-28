#!/usr/bin/python3 -I
"""Synthetic credentials and destructive faults in a marked QEMU COW only."""
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import select
import struct
import subprocess
import termios
import time
import traceback
import tty
import zipfile

spec = importlib.util.spec_from_file_location('probe', '/root/greeter_probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
command, processes, screen, wait = probe.command, probe.processes, probe.screen, probe.wait


def marker(value):
    with open('/dev/ttyS0', 'w') as output:
        output.write('WROS_LOCK_' + value + '\n')


def active():
    fd = os.open('/dev/tty1', os.O_RDONLY | os.O_NOCTTY)
    try:
        return struct.unpack('HHH', fcntl.ioctl(fd, 0x5603, bytes(6)))[0]
    finally:
        os.close(fd)


def protected():
    fd = os.open('/dev/tty8', os.O_RDONLY | os.O_NOCTTY)
    try:
        mode = struct.unpack('BBhhh', fcntl.ioctl(fd, 0x5601, bytes(8)))[0]
        return (active() == 8 and mode == 1 and not termios.tcgetattr(fd)[3] & termios.ECHO
                and Path('/proc/sys/kernel/sysrq').read_text().strip() == '0')
    finally:
        os.close(fd)


def checkpoint(label):
    control = os.open('/dev/ttyS1', os.O_RDONLY | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        # Ignore modem carrier while opening this test-only control UART.
        # The host connects only after our ready marker; ordinary open() can
        # wait for that connection forever before it can publish the marker.
        attributes = termios.tcgetattr(control)
        attributes[2] |= termios.CLOCAL | termios.CREAD
        termios.tcsetattr(control, termios.TCSANOW, attributes)
        tty.setraw(control, when=termios.TCSANOW)
        marker(label)
        for expected in b'H\n':
            assert select.select([control], [], [], 20)[0], 'controller acknowledgment timeout'
            assert os.read(control, 1) == bytes([expected])
    finally:
        os.close(control)


def locked(label, app):
    wait(lambda: processes('vlock-main') and protected() and not processes('wros-input'), 'protected ' + label)
    assert app.exists() and '\nUid:\t1000\t' in (app / 'status').read_text()
    assert 'locking fixture original' not in screen()
    blank = Path('/dev/vcs1').read_bytes()
    assert not blank.strip(b' '), 'old console must be completely blank'
    assert (app / 'wchan').read_text().strip() == 'do_wait', 'editor must wait without terminal operations'
    marker(label + '_PROTECTED')
    time.sleep(2)
    if Path('/dev/vcs1').read_bytes() != blank:
        marker(label + '_OLD_CONSOLE_CHANGED')
        now = Path('/dev/vcs1').read_bytes()
        marker(label + '_OLD_SIZE_' + str(len(blank)) + '_NEW_SIZE_' + str(len(now)))
        marker(label + '_OLD_NONSPACE_' + str(sum(c != 32 for c in blank)) + '_NEW_NONSPACE_' + str(sum(c != 32 for c in now)))
        marker(label + '_ACTIVE_AT_CHANGE_' + str(active()))
        differences = [(i, a, b) for i, (a, b) in enumerate(zip(blank, now)) if a != b]
        marker(label + '_CHANGED_BYTE_COUNT_' + str(len(differences)))
        marker(label + '_CHANGED_BYTE_VALUES_' + json.dumps(differences[:12], separators=(',', ':')))
        for value in (b'WrOS', b'locked', b'Password', b'Local account', b'Console lock', b'locking fixture original'):
            if value in now:
                marker(label + '_OLD_HAS_' + value.decode().replace(' ', '_'))
    if processes('wros-input'):
        marker(label + '_READER_RETURNED')
    assert Path('/dev/vcs1').read_bytes() == blank and not processes('wros-input')
    checkpoint(label + '_SWITCH_CHECKPOINT')
    marker(label + '_ACTIVE_VT_' + str(active()))
    assert protected() and active() == 8
    marker(label + '_SWITCH_DENIED')


def main():
    assert Path('/etc/writingdeck/VM-DEMO-ONLY').is_file()
    assert 'root=UUID=3224a43a-692d-4cd6-a526-96b9a630fa24' in Path('/proc/cmdline').read_text()
    command('systemctl', 'stop', 'getty@tty1.service')
    for proc in processes('grimoire'):
        os.kill(int(proc.name), signal.SIGTERM)
    policy = Path('/usr/sbin/policy-rc.d')
    policy.write_text('#!/bin/sh\nexit 101\n'); policy.chmod(0o755)
    command('dpkg', '-i', '/root/greetd.deb', '/root/vlock.deb')
    policy.unlink()
    files = json.loads(Path('/root/files.json').read_text())
    with zipfile.ZipFile('/root/fixture.zip') as archive:
        assert sorted(archive.namelist()) == sorted(files)
        for name, mode in files.items():
            path = Path('/') / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if path.is_symlink():
                path.unlink()
            path.write_bytes(archive.read(name)); path.chmod(mode)
    for name in ('getty@tty1.service', 'getty@tty8.service', 'autovt@tty8.service'):
        command('systemctl', 'mask', name)
    for name in ('sleep', 'suspend', 'hibernate', 'hybrid-sleep', 'suspend-then-hibernate'):
        command('systemctl', 'mask', name + '.target')
    assert command('passwd', '-S', 'root').stdout.split()[1] == b'L'
    secret = Path('/root/lock-secret').read_text().strip()
    assert len(secret) == 24 and secret.isalnum()
    command('chpasswd', input=('writer:' + secret + '\n').encode())
    Path('/root/lock-secret').unlink()
    Path('/etc/writingdeck/VM-DEMO-ONLY').unlink()  # This generated COW only.
    command('systemctl', 'daemon-reload')
    command('systemctl', 'start', 'greetd')
    wait(lambda: 'Local account' in screen() and processes('wros-greeter'), 'login')
    marker('LOGIN')
    wait(lambda: '[hidden]' in screen() and probe.no_echo(), 'password')
    marker('PASSWORD')
    wait(lambda: 'Open writing session' in screen(), 'authenticated')
    marker('AUTHENTICATED')
    wait(lambda: 'Resume writing' in screen() and processes('grimoire'), 'editor', 70)
    app = processes('grimoire')[0]
    assert '\nUid:\t1000\t' in (app / 'status').read_text()
    identity = hashlib.file_digest((app / 'exe').open('rb'), 'sha256').hexdigest()
    assert identity == hashlib.file_digest(Path('/usr/bin/grimoire').open('rb'), 'sha256').hexdigest()
    command('pkcheck', '--action-id', 'org.wros.lock', '--process', app.name)
    command('chvt', '2')
    assert subprocess.run(['pkcheck', '--action-id', 'org.wros.lock', '--process', app.name], capture_output=True).returncode != 0
    command('chvt', '1')
    marker('ACTIVE_LOCAL_POLICY_PASS')
    # Actual command palette, manuscript, Settings navigation and lock action.
    marker('WRITING_READY')
    wait(lambda: 'locking fixture original' in screen(), 'original manuscript')
    marker('SETTINGS_READY')
    wait(lambda: 'Accounts' in screen() and 'Settings' in screen(), 'settings')
    marker('SYSTEM_READY')
    wait(lambda: 'Battery and power' in screen() and 'Recovery' in screen(), 'system')
    marker('POWER_READY')
    wait(lambda: 'Lock writing session' in screen(), 'lock available')
    marker('LOCK_READY')
    locked('FIRST', app)
    marker('WRONG_PASSWORD_READY')
    wait(lambda: "writer's Password:" in Path('/dev/vcs8').read_bytes().decode('latin1'), 'locker prompt')
    marker('WRONG_PROMPT')
    # PAM denial delays vary. Require the next cleared lock screen, rather
    # than matching an old prompt or typing during the previous PAM request.
    wait(lambda: 'Press Enter to unlock.' in Path('/dev/vcs8').read_bytes().decode('latin1')
         and 'Password:' not in Path('/dev/vcs8').read_bytes().decode('latin1'), 'rejected attempt completed')
    assert protected() and not processes('wros-input') and app.exists()
    assert "root's Password:" not in Path('/dev/vcs8').read_bytes().decode('latin1')
    marker('ROOT_FALLBACK_DENIED_WITHOUT_PROMPT')
    marker('WRONG_DENIED')
    checkpoint('RETRY_CHECKPOINT')
    wait(lambda: "writer's Password:" in Path('/dev/vcs8').read_bytes().decode('latin1'), 'retry prompt')
    assert protected()
    marker('CORRECT_PROMPT')
    checkpoint('CORRECT_HIDDEN_CHECKPOINT')
    assert protected() and secret not in Path('/dev/vcs8').read_bytes().decode('latin1')
    assert not processes('wros-input')
    marker('CORRECT_HIDDEN_PASS')
    wait(lambda: active() == 1 and len(processes('wros-input')) == 1 and not processes('vlock-main'), 'unlock', 50)
    assert app.exists() and processes('grimoire')[0].name == app.name
    reader = processes('wros-input')[0]
    assert '\nUid:\t1000\t' in (reader / 'status').read_text()
    assert '\nPPid:\t' + app.name + '\n' in (reader / 'status').read_text()
    marker('UNLOCKED')
    wait(lambda: 'locking fixture original suffix' in screen(), 'retained cursor and original')
    marker('CONTEXT_PASS')
    # Kill the editor with the independent locker holding a separate VT.
    marker('RELOCK_READY')
    wait(lambda: 'Lock writing session' in screen(), 'relock settings')
    marker('RELOCK')
    locked('SECOND', app)
    originals = [p for p in Path('/home/writer').rglob('*.md')
                 if b'locking fixture original suffix' in p.read_bytes()]
    assert len(originals) == 1
    saved = originals[0].read_bytes()
    os.kill(int(app.name), signal.SIGKILL)
    time.sleep(3)
    assert not processes('grimoire') and not processes('wros-input')
    assert 'locking fixture original' not in screen()
    # greetd may terminate the old session locker or leave it protected. Both
    # require authentication; an old authenticated editor must never reappear.
    assert protected() if processes('vlock-main') else ('Local account' in screen() and processes('wros-greeter'))
    marker('EDITOR_DEATH_FAIL_CLOSED')
    for proc in processes('vlock-main'):
        os.kill(int(proc.name), signal.SIGKILL)
    wait(lambda: 'Local account' in screen() and processes('wros-greeter') and not processes('grimoire')
         and not processes('wros-input'), 'fresh login after locker death')
    marker('LOCKER_DEATH_LOGIN_PASS')
    assert originals[0].read_bytes() == saved
    for label in ('LOCKER_FAILURE', 'HELPER_FAILURE'):
        marker(label + '_LOGIN')
        wait(lambda: '[hidden]' in screen() and probe.no_echo(), label + ' login prompt')
        marker(label + '_PASSWORD')
        wait(lambda: 'Open writing session' in screen(), label + ' PAM')
        marker(label + '_AUTHENTICATED')
        wait(lambda: 'Resume writing' in screen() and processes('grimoire'), label + ' editor', 70)
        app = processes('grimoire')[0]
        assert '\nUid:\t1000\t' in (app / 'status').read_text()
        marker(label + '_NAVIGATE')
        wait(lambda: 'Lock writing session' in screen(), label + ' power page')
        marker(label + '_LOCK')
        locked(label, app)
        if label == 'LOCKER_FAILURE':
            for proc in processes('vlock-main'):
                os.kill(int(proc.name), signal.SIGTERM)
        else:
            children = (app / 'task' / app.name / 'children').read_text().split()
            helpers = [Path('/proc') / pid for pid in children
                       if b'/usr/libexec/wros-lock\0' in (Path('/proc') / pid / 'cmdline').read_bytes()]
            assert len(helpers) == 1 and '\nUid:\t0\t0\t0\t0\n' in (helpers[0] / 'status').read_text()
            os.kill(int(helpers[0].name), signal.SIGKILL)
        wait(lambda: not processes('grimoire') and not processes('wros-input'), label + ' editor closes')
        assert 'locking fixture original' not in screen()
        if processes('vlock-main'):
            assert protected()
            for proc in processes('vlock-main'):
                os.kill(int(proc.name), signal.SIGKILL)
        wait(lambda: active() == 1 and 'Local account' in screen() and processes('wros-greeter')
             and not processes('grimoire') and not processes('wros-input'), label + ' fresh login')
        assert originals[0].read_bytes() == saved
        marker(label + '_FAIL_CLOSED_PASS')
    # All guarded sleep modes remain unavailable; no actual suspend is invoked.
    for name in ('sleep', 'suspend', 'hibernate', 'hybrid-sleep', 'suspend-then-hibernate'):
        assert os.readlink('/etc/systemd/system/' + name + '.target') == '/dev/null'
    journal = command('journalctl', '-b', '--no-pager').stdout
    assert secret.encode() not in journal
    for path in Path('/home/writer').rglob('*'):
        if path.is_file() and path.stat().st_size < 8 * 1024 * 1024:
            assert secret.encode() not in path.read_bytes()
    marker('SECRET_SCAN_AND_SLEEP_GUARD_PASS')
    marker('BINARY_' + identity)
    marker('PASS')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Static type only; never echo authentication, command output or screens.
        marker('FAIL_' + type(error).__name__ + '_LINE_' + str(traceback.extract_tb(error.__traceback__)[-1].lineno))
        marker('DIAGNOSTIC_ACTIVE_VT_' + str(active()))
        for name in ('grimoire', 'wros-input', 'wros-greeter', 'greetd', 'vlock-main', 'openvt', 'wros-lock', 'python3'):
            found = processes(name)
            marker('DIAGNOSTIC_' + name + '_COUNT_' + str(len(found)))
            for proc in found:
                try:
                    marker('DIAGNOSTIC_' + name + '_WAIT_' + (proc / 'wchan').read_text().strip())
                except OSError:
                    pass
        for label in ('Local account', 'Open writing session', 'Resume writing', 'locking fixture original'):
            if label in screen():
                marker('DIAGNOSTIC_TTY1_HAS_' + label.replace(' ', '_'))
        for line in command('systemctl', 'show', 'greetd', '-p', 'ActiveState', '-p', 'SubState', '-p', 'Result').stdout.decode().splitlines():
            marker('DIAGNOSTIC_UNIT_' + line)
    finally:
        time.sleep(2)
        subprocess.run(['systemctl', 'poweroff'], check=False)
