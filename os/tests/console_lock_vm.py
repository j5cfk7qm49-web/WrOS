#!/usr/bin/env python3
"""Manual lock, PAM rejection, VT ownership and crash faults in no-NIC QEMU."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import zipfile
from installed_boot import qmp

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / 'build/appliance'
sys.path.insert(0, str(REPO / 'tools'))
from writingdeck_source import fingerprint


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('greetd', 'vlock', 'greeter', 'editor', 'artifacts-directory'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() != 0, 'QEMU must run as an unprivileged builder'
    image = BASE / 'disk.img'
    assert image.is_file() and not image.is_symlink()
    assert (BASE / 'rootfs/etc/writingdeck/VM-DEMO-ONLY').is_file()
    kernels = sorted((BASE / 'rootfs/boot').glob('vmlinuz-*'))
    assert len(kernels) == 1
    kernel = kernels[0]
    initrd = kernel.with_name(kernel.name.replace('vmlinuz-', 'initrd.img-'))
    assert 'scripts/local-bottom/ORDER' not in subprocess.check_output(['lsinitramfs', str(initrd)], text=True).splitlines()
    args.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='run-', dir=args.artifacts_directory)); run.chmod(0o700)
    source, before, secret = fingerprint(), digest(image), secrets.token_hex(12)
    overlay = run / 'overlay.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b', str(image.resolve()), str(overlay)], check=True, stdout=subprocess.DEVNULL)
    extra = run / 'extra'
    fixtures = extra / 'wros-lock-test'; fixtures.mkdir(parents=True)
    copies = {'probe': REPO / 'os/tests/guest-console-lock.py',
              'greeter_probe.py': REPO / 'os/tests/guest-greeter-probe.py',
              'greetd.deb': args.greetd, 'vlock.deb': args.vlock}
    for name, path in copies.items():
        shutil.copyfile(path, fixtures / name)
    (fixtures / 'lock-secret').write_text(secret); (fixtures / 'lock-secret').chmod(0o600)
    files = {'usr/bin/wros-greeter': (args.greeter, 0o755), 'usr/bin/grimoire': (args.editor, 0o755),
             'usr/libexec/writingdeck-session': (REPO / 'os/scripts/writingdeck-session', 0o755),
             'usr/libexec/wros-lock': (REPO / 'os/scripts/wros-lock', 0o755),
             'etc/pam.d/vlock': (REPO / 'os/config/vlock.pam', 0o644),
             'etc/systemd/logind.conf.d/60-wros-power-guard.conf': (REPO / 'os/config/60-wros-power-guard.conf', 0o644),
             'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf': (REPO / 'os/config/60-wros-sleep-guard.conf', 0o644),
             'etc/polkit-1/rules.d/50-wros-lock.rules': (REPO / 'os/config/50-wros-lock.rules', 0o644),
             'usr/share/polkit-1/actions/org.wros.lock.policy': (REPO / 'os/config/org.wros.lock.policy', 0o644),
             'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules': (REPO / 'os/config/51-wros-greeter-keyboard.rules', 0o644),
             'etc/greetd/config.toml': (REPO / 'os/config/greetd.toml', 0o644),
             'etc/systemd/system/greetd.service': (REPO / 'os/config/greetd.service', 0o644)}
    (fixtures / 'files.json').write_text(json.dumps({name: mode for name, (_, mode) in files.items()}))
    with zipfile.ZipFile(fixtures / 'fixture.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, (path, _) in files.items():
            archive.write(path, name)
    hook = extra / 'scripts/local-bottom/wros-lock-test'; hook.parent.mkdir(parents=True)
    hook.write_text('''#!/bin/sh
case "$1" in prereqs) exit 0;; esac
set -eu
test -f "$rootmnt/etc/writingdeck/VM-DEMO-ONLY"
mount -o remount,rw /dev/vda2 "$rootmnt"
cat /wros-lock-test/probe > "$rootmnt/usr/libexec/writingdeck-boot-probe"
umask 077
for name in greeter_probe.py greetd.deb vlock.deb files.json fixture.zip lock-secret; do
    cat "/wros-lock-test/$name" > "$rootmnt/root/$name"
done
mount -o remount,ro /dev/vda2 "$rootmnt"
''')
    hook.chmod(0o755)
    hook.with_name('ORDER').write_text('/scripts/local-bottom/wros-lock-test "$@"\n')
    names = [str(path.relative_to(extra)) for path in sorted(extra.rglob('*'))]
    archive = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'], cwd=extra, input=('\n'.join(names) + '\n').encode())
    test_initrd = run / 'test-initrd.gz'; test_initrd.write_bytes(initrd.read_bytes() + gzip.compress(archive)); test_initrd.chmod(0o600)
    firmware = run / 'OVMF_VARS.fd'; shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    serial, control, handshake = run / 'serial.log', run / 'qmp.sock', run / 'handshake.sock'
    arguments = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
                 '-m', '512', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
                 '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
                 '-drive', 'if=pflash,format=raw,file=' + str(firmware),
                 '-drive', 'if=virtio,format=qcow2,file=' + str(overlay), '-kernel', str(kernel), '-initrd', str(test_initrd),
                 # The long-running test oneshot otherwise causes PID 1 to
                 # write a ticking startup-status line onto the blanked tty1.
                 '-append', 'root=UUID=3224a43a-692d-4cd6-a526-96b9a630fa24 ro console=ttyS0,115200 console=tty1 systemd.show_status=no panic=1',
                 '-serial', 'file:' + str(serial), '-serial', 'unix:' + str(handshake) + ',server=on,wait=off',
                 '-qmp', 'unix:' + str(control) + ',server=on,wait=off']
    result = {'scope': 'Manual console lock in generated VM-demo COW; synthetic PAM credentials; no hardware/suspend qualification',
              'source_fingerprint': source, 'editor_binary_sha256': digest(args.editor), 'greeter_binary_sha256': digest(args.greeter),
              'greetd_package_sha256': digest(args.greetd), 'vlock_package_sha256': digest(args.vlock),
              'network_devices': 0, 'host_disks_exposed': 0, 'base_image_sha256_before': before, 'passed': False}
    completed, started, failure_screen = set(), time.monotonic(), False
    with (run / 'qemu.log').open('w') as log:
        process = subprocess.Popen(arguments, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            while process.poll() is None and time.monotonic() - started < 600:
                content = serial.read_text(errors='replace') if serial.exists() else ''
                if 'WROS_LOCK_FAIL' in content or 'WROS_GREETER_TIMEOUT' in content or 'Kernel panic' in content:
                    if not failure_screen:
                        qmp(control, 'screendump', {'filename': str(run / 'failed-screen.ppm')})
                        failure_screen = True
                    if 'Kernel panic' in content:
                        break
                    # Retain the guest's bounded static diagnostics and normal
                    # poweroff instead of terminating it at the first failure.
                    time.sleep(0.15)
                    continue
                def keys(values, enter=False):
                    for value in [*values, *(['ret'] if enter else [])]:
                        qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'spc' if value == ' ' else value}], 'hold-time': 40})
                        time.sleep(0.09)
                def chord(*values):
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': value} for value in values], 'hold-time': 40})
                    time.sleep(0.3)
                def ack():
                    with socket.socket(socket.AF_UNIX) as channel:
                        channel.connect(str(handshake)); channel.sendall(b'H\n')
                def palette(text):
                    chord('ctrl', 'k'); keys(text, True)
                actions = {
                    'LOGIN': lambda: keys('writer', True), 'PASSWORD': lambda: keys(secret, True),
                    'AUTHENTICATED': lambda: keys([], True),
                    'WRITING_READY': lambda: (keys(['esc']), palette('scene one'), keys(['home']), keys('locking fixture original')),
                    'SETTINGS_READY': lambda: palette('wros settings'),
                    'SYSTEM_READY': lambda: keys(['down', 'down'], True),
                    'POWER_READY': lambda: keys(['down', 'down'], True),
                    'LOCK_READY': lambda: keys(['end'], True),
                    'FIRST_PROTECTED': lambda: (chord('ctrl', 'alt', 'f1'), chord('ctrl', 'alt', 'f2')),
                    'FIRST_SWITCH_CHECKPOINT': ack,
                    'WRONG_PASSWORD_READY': lambda: keys([], True),
                    'WRONG_PROMPT': lambda: keys('wrongfixture', True),
                    'WRONG_DENIED': lambda: keys([], True),
                    'RETRY_CHECKPOINT': ack,
                    'CORRECT_PROMPT': lambda: keys(secret),
                    'CORRECT_HIDDEN_CHECKPOINT': ack,
                    'CORRECT_HIDDEN_PASS': lambda: keys([], True),
                    'UNLOCKED': lambda: (keys(['esc', 'esc', 'esc']), keys(' suffix')),
                    'CONTEXT_PASS': lambda: None,
                    'RELOCK_READY': lambda: (palette('wros settings'), keys(['home', 'down', 'down'], True), keys(['home', 'down', 'down'], True)),
                    'RELOCK': lambda: keys(['end'], True),
                    'SECOND_PROTECTED': lambda: (chord('ctrl', 'alt', 'f1'), chord('ctrl', 'alt', 'f2')),
                    'SECOND_SWITCH_CHECKPOINT': ack,
                }
                for label in ('LOCKER_FAILURE', 'HELPER_FAILURE', 'WORKER_FAILURE'):
                    actions.update({
                        label + '_LOGIN': lambda: keys('writer', True),
                        label + '_PASSWORD': lambda: keys(secret, True),
                        label + '_AUTHENTICATED': lambda: keys([], True),
                        label + '_NAVIGATE': lambda: (keys(['esc']), palette('wros settings'), keys(['home', 'down', 'down'], True), keys(['home', 'down', 'down'], True)),
                        label + '_LOCK': lambda: keys(['end'], True),
                        label + '_PROTECTED': lambda: (chord('ctrl', 'alt', 'f1'), chord('ctrl', 'alt', 'f2')),
                        label + '_SWITCH_CHECKPOINT': ack,
                    })
                for name, action in actions.items():
                    if 'WROS_LOCK_' + name + '\n' in content and name not in completed:
                        action(); completed.add(name)
                time.sleep(0.15)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait()
            content = serial.read_text(errors='replace') if serial.exists() else ''
            result['markers'] = [line[line.index('WROS_'):].strip() for line in content.splitlines() if 'WROS_LOCK_' in line or 'WROS_GREETER_TIMEOUT' in line]
            result['completed_controller_steps'] = sorted(completed)
            result['qemu_exit_code'] = process.returncode
            result['base_image_sha256_after'] = digest(image)
            result['base_image_unchanged'] = before == result['base_image_sha256_after']
            result['source_unchanged'] = source == fingerprint()
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            result['passed'] = (process.returncode == 0 and 'WROS_LOCK_PASS' in content and 'WROS_LOCK_FAIL' not in content
                                and result['source_unchanged'] and result['base_image_unchanged'] and secret not in content
                                and 'WROS_LOCK_BINARY_' + result['editor_binary_sha256'] in content)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            if result['passed']:
                (fixtures / 'lock-secret').unlink(); test_initrd.unlink()
    print(json.dumps(result, indent=2)); print('Private virtual artifacts: ' + str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
