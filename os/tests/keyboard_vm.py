#!/usr/bin/env python3
"""Actual boot/keymap test in a new COW layer over the isolated VM demo.

No host target argument, NIC, host sharing or account credentials. The test-only
initrd injects the current helper and fixture into the guest's disposable layer;
the original VM artifact and installer ISO are never modified. This qualifies
helper integration, not Debian Installer's keyboard question or a current ISO.
"""
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / 'build/appliance'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    if os.geteuid() == 0:
        raise RuntimeError('Run the virtual integration test as a non-root user')
    image = BASE / 'disk.img'
    if image.is_symlink() or not image.is_file() or not (BASE / 'rootfs/etc/writingdeck/VM-DEMO-ONLY').is_file():
        raise RuntimeError('Only the isolated regular VM-demo artifact is accepted')
    kernels = sorted((BASE / 'rootfs/boot').glob('vmlinuz-*'))
    if len(kernels) != 1:
        raise RuntimeError('Expected one unambiguous VM-demo kernel')
    kernel = kernels[0]
    initrd = kernel.with_name(kernel.name.replace('vmlinuz-', 'initrd.img-'))
    listing = subprocess.check_output(['lsinitramfs', str(initrd)], text=True).splitlines()
    if 'scripts/local-bottom/ORDER' in listing:
        raise RuntimeError('Preserve existing initramfs local-bottom ordering; this fixture requires an empty stage')
    output = REPO / 'build/keyboard-vm'
    output.mkdir(exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='run-', dir=output))
    before = digest(image)
    overlay = run / 'overlay.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b', str(image.resolve()), str(overlay)],
                   check=True, stdout=subprocess.DEVNULL)
    extra = run / 'initrd-extra'
    hook = extra / 'scripts/local-bottom/wros-keyboard-test'
    hook.parent.mkdir(parents=True)
    fixtures = extra / 'wros-keyboard-test'
    fixtures.mkdir()
    shutil.copyfile(REPO / 'os/scripts/writingdeck-keyboard', fixtures / 'helper')
    shutil.copyfile(REPO / 'os/tests/guest-keyboard-probe.py', fixtures / 'probe')
    hook.write_text('''#!/bin/sh
case "$1" in prereqs) exit 0;; esac
exec >/dev/ttyS0 2>&1
set -eux
test -f "$rootmnt/etc/writingdeck/VM-DEMO-ONLY"
mount -o remount,rw /dev/vda2 "$rootmnt"
cp /wros-keyboard-test/helper "$rootmnt/usr/libexec/writingdeck-keyboard"
cp /wros-keyboard-test/probe "$rootmnt/usr/libexec/writingdeck-boot-probe"
chmod 755 "$rootmnt/usr/libexec/writingdeck-keyboard" "$rootmnt/usr/libexec/writingdeck-boot-probe"
printf 'XKBLAYOUT="us"\nXKBVARIANT="colemak"\n' > "$rootmnt/etc/default/keyboard"
printf '{"version":1,"layout":"qwerty","generation":"initial"}\n' > "$rootmnt/etc/writingdeck/keyboard.json"
touch "$rootmnt/etc/writingdeck/installer-keyboard-pending"
mount -o remount,ro /dev/vda2 "$rootmnt"
''')
    hook.chmod(0o755)
    # Debian sources ORDER rather than discovering hooks dynamically. The base
    # initrd has no local-bottom stage; do not replace an existing order file.
    hook.with_name('ORDER').write_text('/scripts/local-bottom/wros-keyboard-test "$@"\n')
    names = [str(path.relative_to(extra)) for path in sorted(extra.rglob('*'))]
    archive = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'],
                                      cwd=extra, input=('\n'.join(names) + '\n').encode())
    test_initrd = run / 'test-initrd.gz'
    test_initrd.write_bytes(initrd.read_bytes() + gzip.compress(archive))
    firmware = run / 'OVMF_VARS.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    serial = run / 'serial.log'
    args = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
            '-m', '512', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
            '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
            '-drive', 'if=pflash,format=raw,file=' + str(firmware),
            '-drive', 'if=virtio,format=qcow2,file=' + str(overlay),
            '-kernel', str(kernel), '-initrd', str(test_initrd),
            '-append', 'root=UUID=3224a43a-692d-4cd6-a526-96b9a630fa24 ro console=tty1 console=ttyS0,115200 panic=1',
            '-serial', 'file:' + str(serial)]
    result = {'scope': 'Current helper injected into a disposable VM-demo overlay; not installer ISO qualification',
              'host_disks_exposed': 0, 'network_devices': 0, 'base_image_sha256_before': before,
              'input_sha256': {name: digest(REPO / name) for name in (
                  'os/scripts/writingdeck-keyboard', 'os/tests/guest-keyboard-probe.py', 'os/tests/keyboard_vm.py')},
              'passed': False}
    started = time.monotonic()
    with (run / 'qemu.log').open('w') as log:
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            process.wait(timeout=300)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            content = serial.read_text(errors='replace') if serial.exists() else ''
            result['markers'] = [line.strip() for line in content.splitlines() if 'WROS_KEYBOARD_' in line]
            result['base_image_sha256_after'] = digest(image)
            result['base_image_unchanged'] = before == result['base_image_sha256_after']
            expected = ('INSTALLER_BOOT', 'INSTALLER_ROLLBACK', 'COMMITTED_PRESERVED', 'SYSTEM_MAP_PRESERVED')
            result['passed'] = (process.returncode == 0 and result['base_image_unchanged']
                                and all('WROS_KEYBOARD_' + name + '_PASS' in content for name in expected)
                                and 'WROS_KEYBOARD_PASS' in content and 'WROS_KEYBOARD_FAIL' not in content)
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    print('Retained private virtual artifacts: ' + str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
