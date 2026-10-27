#!/usr/bin/env python3
"""Real greetd/PAM acceptance in a disposable networkless QEMU overlay."""
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
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--editor', type=Path, required=True)
    parser.add_argument('--artifacts-directory', type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise RuntimeError('Run QEMU as a normal user')
    image = BASE / 'disk.img'
    if not image.is_file() or image.is_symlink() or not (BASE / 'rootfs/etc/writingdeck/VM-DEMO-ONLY').is_file():
        raise RuntimeError('Requires the isolated marked VM-demo artifact')
    kernels = sorted((BASE / 'rootfs/boot').glob('vmlinuz-*'))
    assert len(kernels) == 1
    kernel = kernels[0]
    initrd = kernel.with_name(kernel.name.replace('vmlinuz-', 'initrd.img-'))
    assert 'scripts/local-bottom/ORDER' not in subprocess.check_output(['lsinitramfs', str(initrd)], text=True).splitlines()
    args.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='run-', dir=args.artifacts_directory))
    run.chmod(0o700)
    secret = secrets.token_hex(12)
    source, before = fingerprint(), digest(image)
    overlay = run / 'overlay.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b', str(image.resolve()), str(overlay)], check=True, stdout=subprocess.DEVNULL)
    extra = run / 'extra'
    fixtures = extra / 'wros-greeter-test'
    fixtures.mkdir(parents=True)
    shutil.copyfile(REPO / 'os/tests/guest-greeter-probe.py', fixtures / 'probe')
    shutil.copyfile(args.package, fixtures / 'package.deb')
    (fixtures / 'secret').write_text(secret)
    (fixtures / 'secret').chmod(0o600)
    files = {'usr/bin/wros-greeter': args.binary, 'usr/bin/grimoire': args.editor,
             'usr/libexec/writingdeck-session': REPO / 'os/scripts/writingdeck-session',
             'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules': REPO / 'os/config/51-wros-greeter-keyboard.rules',
             'etc/greetd/config.toml': REPO / 'os/config/greetd.toml',
             'etc/systemd/system/greetd.service': REPO / 'os/config/greetd.service'}
    with zipfile.ZipFile(fixtures / 'fixture.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, path in files.items():
            archive.write(path, name)
    hook = extra / 'scripts/local-bottom/wros-greeter-test'
    hook.parent.mkdir(parents=True)
    hook.write_text('''#!/bin/sh
case "$1" in prereqs) exit 0;; esac
set -eu
test -f "$rootmnt/etc/writingdeck/VM-DEMO-ONLY"
mount -o remount,rw /dev/vda2 "$rootmnt"
cat /wros-greeter-test/probe > "$rootmnt/usr/libexec/writingdeck-boot-probe"
cat /wros-greeter-test/package.deb > "$rootmnt/root/greeter-package.deb"
cat /wros-greeter-test/fixture.zip > "$rootmnt/root/greeter-fixture.zip"
umask 077
cat /wros-greeter-test/secret > "$rootmnt/root/greeter-secret"
mount -o remount,ro /dev/vda2 "$rootmnt"
''')
    hook.chmod(0o755)
    hook.with_name('ORDER').write_text('/scripts/local-bottom/wros-greeter-test "$@"\n')
    names = [str(path.relative_to(extra)) for path in sorted(extra.rglob('*'))]
    archive = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'], cwd=extra, input=('\n'.join(names) + '\n').encode())
    test_initrd = run / 'test-initrd.gz'
    test_initrd.write_bytes(initrd.read_bytes() + gzip.compress(archive))
    test_initrd.chmod(0o600)
    firmware = run / 'OVMF_VARS.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    serial, control, handshake = run / 'serial.log', run / 'qmp.sock', run / 'handshake.sock'
    command = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
               '-m', '512', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
               '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
               '-drive', 'if=pflash,format=raw,file=' + str(firmware),
               '-drive', 'if=virtio,format=qcow2,file=' + str(overlay), '-kernel', str(kernel), '-initrd', str(test_initrd),
               '-append', 'root=UUID=3224a43a-692d-4cd6-a526-96b9a630fa24 ro console=ttyS0,115200 console=tty1 panic=1',
               '-serial', 'file:' + str(serial), '-serial', 'unix:' + str(handshake) + ',server=on,wait=off',
               '-qmp', 'unix:' + str(control) + ',server=on,wait=off']
    result = {'scope': 'Experimental greeter with real Debian greetd/PAM in VM-demo COW layer; not installer/locker/hardware qualification',
              'source_fingerprint': source, 'binary_sha256': digest(args.binary), 'greetd_package_sha256': digest(args.package),
              'editor_binary_sha256': digest(args.editor),
              'network_devices': 0, 'host_disks_exposed': 0, 'passed': False, 'base_image_sha256_before': before}
    started, stage, retry_sent, keyboard_stage = time.monotonic(), 0, False, 0
    with (run / 'qemu.log').open('w') as output:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        try:
            while process.poll() is None and time.monotonic() - started < 480:
                content = serial.read_text(errors='replace') if serial.exists() else ''
                if 'WROS_GREETER_FAIL' in content or 'Kernel panic' in content:
                    qmp(control, 'screendump', {'filename': str(run / 'failed-screen.ppm')})
                    break
                def keys(text, enter=True):
                    for value in [*text, *(['ret'] if enter else [])]:
                        qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': value}], 'hold-time': 40})
                        time.sleep(0.09)
                if 'WROS_GREETER_KEYBOARD_READY' in content and keyboard_stage == 0:
                    keys(['f2'], enter=False); keyboard_stage = 1
                for index in range(4):
                    if f'WROS_GREETER_LAYOUT_READY_{index}' in content and keyboard_stage == 1 + index * 2:
                        if index:
                            keys(['down'], enter=False)
                        keys(''); keyboard_stage += 1
                    if f'WROS_GREETER_LAYOUT_ACTIVE_{index}' in content and keyboard_stage == 2 + index * 2:
                        if index == 3:
                            qmp(control, 'screendump', {'filename': str(run / 'keyboard-preview.ppm')})
                        keys(['esc'], enter=False); keyboard_stage += 1
                if 'WROS_GREETER_TIMEOUT_READY' in content and keyboard_stage == 9:
                    keys(''); keyboard_stage = 10
                if 'WROS_GREETER_CRASH_PREVIEW_READY' in content and keyboard_stage == 10:
                    keys(''); keyboard_stage = 11
                if 'WROS_GREETER_COMMIT_MENU_READY' in content and keyboard_stage == 11:
                    keys(['f2', 'down', 'down', 'down'], enter=False); keyboard_stage = 12
                if 'WROS_GREETER_COMMIT_READY' in content and keyboard_stage == 12:
                    keys(''); keyboard_stage = 13
                if 'WROS_GREETER_COMMIT_ACTIVE' in content and keyboard_stage == 13:
                    keys(''); keyboard_stage = 14
                if 'WROS_GREETER_RESTORE_READY' in content and keyboard_stage == 14:
                    keys(['f2', 'up', 'up', 'up']); keyboard_stage = 15
                if 'WROS_GREETER_RESTORE_ACTIVE' in content and keyboard_stage == 15:
                    keys(''); keyboard_stage = 16
                if 'WROS_GREETER_READY' in content and stage == 0:
                    qmp(control, 'screendump', {'filename': str(run / 'login.ppm')})
                    keys('writer'); stage = 1
                if 'WROS_GREETER_PROMPT1' in content and stage == 1:
                    keys('invalidfixture'); stage = 2
                if 'WROS_GREETER_RETRY_REQUIRED' in content and stage == 2 and not retry_sent:
                    keys(''); retry_sent = True
                if 'WROS_GREETER_DENIED' in content and stage == 2:
                    keys('writer'); stage = 3
                if 'WROS_GREETER_PROMPT2' in content and stage == 3:
                    keys(secret, enter=False)
                    with socket.socket(socket.AF_UNIX) as ready:
                        ready.connect(str(handshake)); ready.sendall(b'H\n')
                    stage = 4
                if 'WROS_GREETER_HIDDEN' in content and stage == 4:
                    keys(''); stage = 5
                if 'WROS_GREETER_AUTHENTICATED' in content and stage == 5:
                    keys(''); stage = 6
                if 'WROS_GREETER_SWITCH_READY' in content and stage == 6:
                    # The last selectable ordinary account is writerother;
                    # exercise the actual picker rather than typing its name.
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'up'}], 'hold-time': 40})
                    time.sleep(0.2)
                    keys(''); stage = 7
                if 'WROS_GREETER_SWITCH_PROMPT' in content and stage == 7:
                    keys(secret); stage = 8
                if 'WROS_GREETER_SWITCH_AUTHENTICATED' in content and stage == 8:
                    keys(''); stage = 9
                if 'WROS_GREETER_PASS' in content:
                    stage = 10
                time.sleep(0.15)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait()
            content = serial.read_text(errors='replace') if serial.exists() else ''
            result['markers'] = [line[line.index('WROS_GREETER_'):].strip() for line in content.splitlines() if 'WROS_GREETER_' in line]
            result['base_image_sha256_after'] = digest(image)
            result['source_unchanged'] = fingerprint() == source
            result['base_image_unchanged'] = before == result['base_image_sha256_after']
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            result['passed'] = (process.returncode == 0 and stage == 10 and keyboard_stage == 16 and result['base_image_unchanged'] and result['source_unchanged']
                                and secret not in content and 'WROS_GREETER_FAIL' not in content)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            if result['passed']:
                (fixtures / 'secret').unlink()
                test_initrd.unlink()
    print(json.dumps(result, indent=2))
    print('Private virtual artifacts: ' + str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
