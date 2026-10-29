#!/usr/bin/python3 -I
"""Actual driver/service/polkit probe, only in a disposable VM-demo overlay."""
import json
import os
from pathlib import Path
import re
import resource
import subprocess
import time
import zipfile


def marker(value):
    with open('/dev/ttyS0', 'w') as output:
        output.write(value + '\n')


def command(*arguments):
    return subprocess.check_output(arguments, text=True, stderr=subprocess.STDOUT, timeout=12)


def helper(*arguments):
    return json.loads(command('/usr/libexec/wros-display', *arguments))


def screen():
    return Path('/dev/vcs1').read_bytes().decode('latin1')


def main():
    assert Path('/etc/writingdeck/VM-DEMO-ONLY').is_file()
    # This closed list contains only current source/binaries and units. The
    # archive is a private initrd fixture, never a shipping installation path.
    paths = ['usr/libexec/wros-display', 'usr/libexec/wros-display-hw',
             'usr/lib/x86_64-linux-gnu/libdrm.so.2',
             'etc/polkit-1/rules.d/50-wros-display.rules',
             'usr/share/polkit-1/actions/org.wros.display.policy',
             *['etc/systemd/system/' + name for name in (
                 'wros-display.service', 'wros-display-revert.service', 'wros-display-revert.timer')]]
    with zipfile.ZipFile('/root/wros-display-fixture.zip') as archive:
        assert sorted(archive.namelist()) == sorted(paths)
        for name in paths:
            path = Path('/') / name
            path.parent.mkdir(parents=True, exist_ok=True)
            # A symlink from the base must not redirect fixture writes.
            if path.is_symlink():
                path.unlink()
            path.write_bytes(archive.read(name))
            path.chmod(0o755 if name.startswith('usr/libexec/') else 0o644)
    Path('/etc/wros').mkdir(exist_ok=True)
    command('/usr/bin/systemctl', 'daemon-reload')
    assert command('/usr/bin/systemctl', 'show', '-p', 'LoadState', 'wros-display-revert.timer').strip() == 'LoadState=loaded'
    assert command('/usr/bin/systemctl', 'show', '-p', 'LoadState', 'wros-display.service').strip() == 'LoadState=loaded'
    before_usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    status = helper('status')
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    marker('WROS_DISPLAY_STATUS_RESOURCES ' + json.dumps({
        'wall_seconds': round(time.monotonic() - started, 3),
        'cpu_seconds': round(usage.ru_utime + usage.ru_stime - before_usage.ru_utime - before_usage.ru_stime, 3),
        'max_child_rss_kib_so_far': usage.ru_maxrss,
        'scope': 'Status on this emulated driver; max RSS includes earlier short service commands, not a positive gamma or battery benchmark'}))
    marker('WROS_DISPLAY_DRIVER ' + json.dumps(status, sort_keys=True))
    if status['available']:
        original = subprocess.check_output(['/usr/libexec/wros-display-hw', 'capture'])
        preview = helper('preview', 'on', '1800', '100')
        adjusted = subprocess.check_output(['/usr/libexec/wros-display-hw', 'capture'])
        assert adjusted != original, 'driver must actually expose changed gamma'
        time.sleep(16)
        restored = subprocess.check_output(['/usr/libexec/wros-display-hw', 'capture'])
        assert restored == original, 'independent systemd timer must restore exact calibration'
        assert not Path('/run/wros-display/preview.json').exists()
        preview = helper('preview', 'on', '2200', '100')
        helper('confirm', preview['token'])
        command('/usr/bin/systemctl', 'restart', 'wros-display.service')
        assert helper('status')['applied']
        preview = helper('preview', 'off', '2200', '100')
        helper('confirm', preview['token'])
        assert subprocess.check_output(['/usr/libexec/wros-display-hw', 'capture']) == original
        marker('WROS_DISPLAY_GAMMA_TIMER_COMMIT_OFF_PASS')
    else:
        assert not status['applied']
        rejected = subprocess.run(['/usr/libexec/wros-display', 'preview', 'on', '2200', '100'],
                                  capture_output=True, timeout=12)
        assert rejected.returncode != 0
        assert not Path('/run/wros-display/preview.json').exists()
        assert not Path('/etc/wros/display.json').exists()
        marker('WROS_DISPLAY_UNAVAILABLE_NO_CHANGE_PASS')
    deadline = time.monotonic() + 100
    app = None
    while time.monotonic() < deadline:
        for path in Path('/proc').glob('[0-9]*'):
            try:
                if (path / 'comm').read_text().strip() == 'grimoire':
                    app = path
            except OSError:
                pass
        if app and 'Resume writing' in screen():
            break
        time.sleep(0.25)
    assert app and 'Resume writing' in screen(), 'normal unprivileged dashboard required'
    details = (app / 'status').read_text()
    assert re.search(r'^Uid:\s+1000\s', details, re.MULTILINE)
    command('/usr/bin/pkcheck', '--action-id', 'org.wros.display', '--process', app.name)
    denied = subprocess.run(['/usr/sbin/runuser', '-u', 'nobody', '--', '/usr/bin/pkexec',
                             '--disable-internal-agent', '/usr/libexec/wros-display', 'status'],
                            capture_output=True, timeout=12)
    assert denied.returncode != 0, 'inactive nonmember must not receive display privilege'
    root_only = subprocess.run(['/usr/sbin/runuser', '-u', 'writer', '--', '/usr/libexec/wros-display-hw', 'capture'],
                              capture_output=True, timeout=12)
    assert root_only.returncode != 0, 'hardware bridge is not setuid'
    marker('WROS_DISPLAY_POLKIT_BOUNDARY_PASS')
    marker('WROS_DISPLAY_UI_READY')
    # Each marker is a barrier for actual visible child controls. Fixed sleeps
    # alone can type into an earlier frame under slow software emulation.
    for name, labels in (
            ('SETTINGS', ('Writing', 'Interface', 'Accounts')),
            ('SYSTEM', ('Wi-Fi and networking', 'Date and time', 'Recovery')),
            ('DISPLAY', ('Brightness -10%', 'Warm screen'))):
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline:
            if all(label in screen() for label in labels):
                break
            time.sleep(0.25)
        assert all(label in screen() for label in labels), 'missing actual UI stage ' + name
        marker('WROS_DISPLAY_UI_' + name)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        current = screen()
        if 'Check display support' in current and ('unavailable' in current or 'calibration is retained' in current):
            break
        time.sleep(0.25)
    assert 'Check display support' in current, 'QMP must enter the actual native settings'
    assert ('unavailable' in current) if not status['available'] else ('calibration is retained' in current)
    assert '1000' in current and '6500' in current, 'temperature bounds are visible'
    marker('WROS_DISPLAY_NATIVE_UI_PASS rss_kib=' + re.search(r'^VmRSS:\s+(\d+)', (app / 'status').read_text(), re.MULTILINE).group(1))
    marker('WROS_DISPLAY_PASS')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        marker('WROS_DISPLAY_FAILED_SCREEN ' + repr(screen()[:10000]))
        marker('WROS_DISPLAY_FAIL ' + repr(error))
    finally:
        # Give the controller time to retain the actual screen.
        time.sleep(3)
        subprocess.run(['/usr/bin/systemctl', 'poweroff'], check=False)
