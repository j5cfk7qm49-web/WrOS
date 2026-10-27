#!/usr/bin/python3 -I
"""Synthetic PAM accounts only, inside the marked disposable VM overlay."""
import os
from pathlib import Path
import signal
import json
import re
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


def keycode(code):
    text = command('dumpkeys', '--console=/dev/tty1', '--keys-only', '--numeric').stdout.decode()
    match = re.search(r'^\s*keycode\s+' + str(code) + r'\s*=\s*\+?(0x[0-9a-fA-F]+)', text, re.MULTILINE)
    assert match
    return int(match[1], 16) & 0xff


def keyboard_acceptance():
    greeter = processes('wros-greeter')[0]
    command('pkcheck', '--action-id', 'org.writingdeck.keyboard', '--process', greeter.name)
    for action in ('org.wros.display', 'org.freedesktop.policykit.exec'):
        denied = subprocess.run(['pkcheck', '--action-id', action, '--process', greeter.name], capture_output=True)
        assert denied.returncode != 0
    command('chvt', '2')
    denied = subprocess.run(['pkcheck', '--action-id', 'org.writingdeck.keyboard', '--process', greeter.name], capture_output=True)
    assert denied.returncode != 0
    command('chvt', '1')
    command('pkcheck', '--action-id', 'org.writingdeck.keyboard', '--process', greeter.name)
    marker('KEYBOARD_AUTHORIZATION_PASS')
    config = Path('/etc/writingdeck/keyboard.json')
    original = config.read_bytes()
    pending = Path('/run/writingdeck/keyboard-preview.json')
    marker('KEYBOARD_READY')
    wait(lambda: 'Login keyboard' in screen(), 'keyboard menu')
    for index, layout in enumerate(('qwerty', 'colemak', 'colemak-dh-ansi', 'colemak-dh-iso')):
        marker('LAYOUT_READY_' + str(index))
        wait(lambda: pending.exists() and json.loads(pending.read_text())['layout'] == layout
             and 'Preview active' in screen(), 'layout preview ' + str(index))
        assert keycode(18) == (ord('e') if index == 0 else ord('f'))
        assert keycode(35) == (ord('h') if index < 2 else ord('m'))
        assert no_echo() and not processes('grimoire') and config.read_bytes() == original
        marker('LAYOUT_ACTIVE_' + str(index))
        wait(lambda: not pending.exists() and 'Saved layout restored' in screen(), 'layout cancellation')
        assert keycode(18) == ord('e') and config.read_bytes() == original
    marker('FOUR_LAYOUT_CANCEL_PASS')
    marker('TIMEOUT_READY')
    wait(lambda: pending.exists() and 'Preview active' in screen(), 'timed preview')
    wait(lambda: not pending.exists() and 'Saved layout restored' in screen(), 'preview expiry', 30)
    assert keycode(18) == ord('e') and config.read_bytes() == original
    marker('TIMEOUT_PASS')
    marker('CRASH_PREVIEW_READY')
    wait(lambda: pending.exists() and 'Preview active' in screen(), 'crash preview')
    old_pid = processes('wros-greeter')[0].name
    os.kill(int(old_pid), signal.SIGKILL)
    wait(lambda: processes('wros-greeter') and all(p.name != old_pid for p in processes('wros-greeter'))
         and 'Local account' in screen(), 'preview crash recovery')
    wait(lambda: not pending.exists(), 'rollback without greeter', 30)
    assert keycode(18) == ord('e') and config.read_bytes() == original and not processes('grimoire')
    marker('CRASH_ROLLBACK_PASS')
    marker('COMMIT_MENU_READY')
    wait(lambda: 'Login keyboard' in screen(), 'reopened keyboard')
    marker('COMMIT_READY')
    wait(lambda: pending.exists() and 'Preview active' in screen(), 'commit preview')
    marker('COMMIT_ACTIVE')
    wait(lambda: not pending.exists() and 'Local account' in screen(), 'confirmed layout')
    assert json.loads(config.read_text())['layout'] == 'colemak-dh-iso'
    assert keycode(18) == ord('f') and keycode(35) == ord('m')
    marker('COMMIT_PASS')
    marker('RESTORE_READY')
    wait(lambda: pending.exists() and json.loads(pending.read_text())['layout'] == 'qwerty'
         and 'Preview active' in screen(), 'qwerty recovery preview')
    marker('RESTORE_ACTIVE')
    wait(lambda: not pending.exists() and 'Local account' in screen(), 'qwerty recovery commit')
    assert json.loads(config.read_text())['layout'] == 'qwerty' and keycode(18) == ord('e')
    marker('KEYBOARD_PASS')


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
             'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules': 0o644,
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
    keyboard_acceptance()
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
    readers = processes('wros-input')
    assert len(readers) == 1
    reader_status = (readers[0] / 'status').read_text()
    assert '\nUid:\t1000\t' in reader_status and ('\nPPid:\t' + app.name + '\n') in reader_status
    marker('INPUT_READER_UID1000_PASS')
    assert not processes('wros-greeter')
    marker('SESSION')
    # Return to a new login, never an authenticated shell, on session exit.
    os.kill(int(app.name), signal.SIGKILL)
    wait(lambda: 'Local account' in screen() and processes('wros-greeter') and not processes('grimoire')
         and not processes('wros-input'), 'crashed session returns to login without an orphan reader')
    marker('INPUT_READER_CRASH_EXIT_PASS')
    marker('SWITCH_READY')
    wait(lambda: '[hidden]' in screen() and no_echo(), 'second account prompt')
    marker('SWITCH_PROMPT')
    wait(lambda: 'Open writing session' in screen(), 'second account authentication')
    marker('SWITCH_AUTHENTICATED')
    wait(lambda: 'Resume writing' in screen() and processes('grimoire'), 'second account writing session', 70)
    app = processes('grimoire')[0]
    assert '\nUid:\t1001\t' in (app / 'status').read_text()
    readers = processes('wros-input')
    assert len(readers) == 1 and '\nUid:\t1001\t' in (readers[0] / 'status').read_text()
    marker('INPUT_READER_UID1001_PASS')
    assert not processes('wros-greeter')
    os.kill(int(app.name), signal.SIGTERM)
    wait(lambda: 'Local account' in screen() and processes('wros-greeter') and not processes('grimoire')
         and not processes('wros-input'), 'second account logout')
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
            if 'The value for the SHELL variable was not found in the /etc/shells file' in line:
                marker('DIAGNOSTIC_PKEXEC_REJECTED_INHERITED_SHELL')
            if 'wros-greeter: ' in line:
                # Greeter diagnostics are static internal strings only.
                marker('DIAGNOSTIC_' + line.split('wros-greeter: ', 1)[1])
        for label in ('Local account', '[hidden]', 'Please wait', 'Login was not accepted', 'Open writing session',
                      'Login service read failed or timed out', 'Login service could not complete the request',
                      'Keyboard helper failed', 'Keyboard authorization unavailable', 'Keyboard process unavailable',
                      'Invalid login service response', 'Login service disconnected'):
            if label in current:
                marker('OBSERVED_' + label.replace(' ', '_'))
        marker('FAIL')
    finally:
        time.sleep(3)
        subprocess.run(['systemctl', 'poweroff'], check=False)
