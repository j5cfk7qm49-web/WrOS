#!/usr/bin/python3 -I
"""Synthetic PAM accounts only, inside the marked disposable VM overlay."""
import os
from pathlib import Path
import signal
import subprocess
import termios
import time
import zipfile


def marker(name):
    with open('/dev/ttyS0', 'w') as output:
        output.write('WROS_GREETER_' + name + '\n')


def command(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=30, **kwargs)


def screen():
    return Path('/dev/vcs1').read_bytes().decode('latin1')


def wait(predicate, description, seconds=45):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.15)
    marker('TIMEOUT_' + description.replace(' ', '_'))
    raise RuntimeError(description)


def processes(name):
    result = []
    for proc in Path('/proc').glob('[0-9]*'):
        try:
            if (proc / 'comm').read_text().strip() == name:
                result.append(proc)
        except OSError:
            pass
    return result


def no_echo():
    fd = os.open('/dev/tty1', os.O_RDONLY | os.O_NOCTTY)
    try:
        return not termios.tcgetattr(fd)[3] & termios.ECHO
    finally:
        os.close(fd)


def main():
    assert Path('/etc/writingdeck/VM-DEMO-ONLY').is_file()
    command('systemctl', 'stop', 'getty@tty1.service')
    for proc in processes('grimoire'):
        os.kill(int(proc.name), signal.SIGTERM)
    # Install the maintained package in this disposable guest only, with service
    # auto-start suppressed until our explicit configuration is staged.
    policy = Path('/usr/sbin/policy-rc.d')
    policy.write_text('#!/bin/sh\nexit 101\n')
    policy.chmod(0o755)
    command('dpkg', '-i', '/root/greeter-package.deb')
    policy.unlink()
    files = {'usr/bin/wros-greeter': 0o755, 'usr/bin/grimoire': 0o755,
             'usr/libexec/writingdeck-session': 0o755,
             'etc/greetd/config.toml': 0o644,
             'etc/systemd/system/greetd.service': 0o644}
    with zipfile.ZipFile('/root/greeter-fixture.zip') as archive:
        assert sorted(archive.namelist()) == sorted(files)
        for name, mode in files.items():
            path = Path('/') / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if path.is_symlink():
                path.unlink()
            path.write_bytes(archive.read(name))
            path.chmod(mode)
    secret = Path('/root/greeter-secret').read_text().strip()
    assert len(secret) == 24 and secret.isalnum()
    command('useradd', '-m', '-u', '1001', '-G', 'writingdeck', '-s', '/bin/bash', 'writerother')
    command('chpasswd', input=('writer:' + secret + '\nwriterother:' + secret + '\n').encode())
    Path('/root/greeter-secret').unlink()
    command('systemctl', 'daemon-reload')
    command('systemctl', 'mask', 'getty@tty1.service')
    command('systemctl', 'enable', 'greetd.service')
    assert Path('/etc/systemd/system/multi-user.target.wants/greetd.service').is_symlink()
    conflicts = command('systemctl', 'show', '--value', '-p', 'Conflicts', 'greetd.service').stdout.decode().split()
    assert 'getty@tty1.service' in conflicts and 'getty@tty2.service' not in conflicts and 'getty@tty7.service' not in conflicts
    command('systemctl', 'start', 'getty@tty2.service')
    assert command('systemctl', 'is-active', 'getty@tty2.service').stdout.strip() == b'active'
    marker('SERVICE_BOUNDARIES_PASS')
    command('systemctl', 'start', 'greetd')
    wait(lambda: 'Local account' in screen() and processes('wros-greeter'), 'initial greeter')
    first = processes('wros-greeter')[0]
    assert '\nUid:\t0\t' not in (first / 'status').read_text()
    os.kill(int(first.name), signal.SIGKILL)
    wait(lambda: processes('wros-greeter') and all(p.name != first.name for p in processes('wros-greeter'))
         and 'Local account' in screen(), 'crash recovery')
    assert not processes('grimoire')
    marker('CRASH_RECOVERED')
    marker('READY')
    wait(lambda: '[hidden]' in screen() and no_echo(), 'first hidden prompt')
    login_pid = processes('wros-greeter')[0].name
    marker('PROMPT1')
    wait(lambda: ('Login was not accepted' in screen() and ('Local account' in screen() or 'Enter to restart login' in screen()))
         or not any(p.name == login_pid for p in processes('wros-greeter')), 'wrong password refusal')
    assert any(p.name == login_pid for p in processes('wros-greeter')), 'unexpected restart during ordinary rejection'
    assert not processes('grimoire')
    if 'Enter to restart login' in screen():
        marker('RETRY_REQUIRED')
        wait(lambda: 'Local account' in screen() and processes('wros-greeter'), 'fresh login after denial')
    marker('DENIED')
    wait(lambda: '[hidden]' in screen() and no_echo(), 'second hidden prompt')
    marker('PROMPT2')
    # Controller types the synthetic password without Enter, then gives us a
    # dedicated serial /dev/ttyS1 marker to check the screen before submission.
    ready = Path('/dev/ttyS1')
    with ready.open('rb', buffering=0) as control:
        assert control.read(1) == b'H'
    assert no_echo() and secret not in screen()
    marker('HIDDEN')
    wait(lambda: 'Open writing session' in screen(), 'PAM authentication')
    marker('AUTHENTICATED')
    wait(lambda: 'Resume writing' in screen() and processes('grimoire'), 'native writing session', 70)
    app = processes('grimoire')[0]
    assert '\nUid:\t1000\t' in (app / 'status').read_text()
    assert not processes('wros-greeter')
    marker('SESSION')
    # Return to a new login, never an authenticated shell, on session exit.
    os.kill(int(app.name), signal.SIGTERM)
    wait(lambda: 'Local account' in screen() and processes('wros-greeter') and not processes('grimoire'), 'logout returns to login')
    marker('SWITCH_READY')
    wait(lambda: '[hidden]' in screen() and no_echo(), 'second account prompt')
    marker('SWITCH_PROMPT')
    wait(lambda: 'Open writing session' in screen(), 'second account authentication')
    marker('SWITCH_AUTHENTICATED')
    wait(lambda: 'Resume writing' in screen() and processes('grimoire'), 'second account writing session', 70)
    app = processes('grimoire')[0]
    assert '\nUid:\t1001\t' in (app / 'status').read_text()
    assert not processes('wros-greeter')
    os.kill(int(app.name), signal.SIGTERM)
    wait(lambda: 'Local account' in screen() and processes('wros-greeter') and not processes('grimoire'), 'second account logout')
    marker('ACCOUNT_SWITCH_PASS')
    journal = command('journalctl', '-b', '--no-pager').stdout
    assert secret.encode() not in journal
    for home in ('/home/writer', '/home/writerother'):
        for path in Path(home).rglob('*'):
            if path.is_file() and path.stat().st_size < 8 * 1024 * 1024:
                assert secret.encode() not in path.read_bytes()
    marker('LOGOUT_AND_SECRET_SCAN_PASS')
    marker('PASS')


if __name__ == '__main__':
    try:
        main()
    except Exception:
        # Deliberately no raw exceptions/screens/process output: PAM data may
        # occur there. Private VM artifacts support separate investigation.
        current = screen()
        journal = command('journalctl', '-b', '--no-pager').stdout.decode(errors='replace')
        for line in journal.splitlines():
            if 'wros-greeter: ' in line:
                # Greeter diagnostics are static internal strings only.
                marker('DIAGNOSTIC_' + line.split('wros-greeter: ', 1)[1])
        for label in ('Local account', '[hidden]', 'Please wait', 'Login was not accepted', 'Open writing session',
                      'Login service read failed or timed out', 'Login service could not complete the request',
                      'Invalid login service response', 'Login service disconnected'):
            if label in current:
                marker('OBSERVED_' + label.replace(' ', '_'))
        marker('FAIL')
    finally:
        time.sleep(3)
        subprocess.run(['systemctl', 'poweroff'], check=False)
