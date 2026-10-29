#!/usr/bin/env python3
"""Current display integration in a new VM-demo COW layer; no host display/NIC.

An unsupported emulated driver is a valid refusal result, never a positive gamma
qualification. This does not qualify a rebuilt installer or a physical laptop.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
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
    parser.add_argument('--artifacts-directory', type=Path, default=REPO / 'build/display-vm')
    options = parser.parse_args()
    source = fingerprint()
    if os.geteuid() == 0:
        raise RuntimeError('Run QEMU as a normal user')
    image = BASE / 'disk.img'
    binary = Path(os.environ.get('GRIMOIRE_BINARY', REPO / 'target/debug/grimoire'))
    if not image.is_file() or image.is_symlink() or not (BASE / 'rootfs/etc/writingdeck/VM-DEMO-ONLY').is_file():
        raise RuntimeError('Only the regular isolated VM-demo artifact is accepted')
    kernels = sorted((BASE / 'rootfs/boot').glob('vmlinuz-*'))
    if len(kernels) != 1 or not binary.is_file():
        raise RuntimeError('Need an unambiguous VM-demo kernel and current compiled binary')
    kernel = kernels[0]
    initrd = kernel.with_name(kernel.name.replace('vmlinuz-', 'initrd.img-'))
    if 'scripts/local-bottom/ORDER' in subprocess.check_output(['lsinitramfs', str(initrd)], text=True).splitlines():
        raise RuntimeError('Fixture requires an empty local-bottom stage; preserve existing ordering')
    options.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='run-', dir=options.artifacts_directory))
    before = digest(image)
    overlay = run / 'overlay.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b', str(image.resolve()), str(overlay)], check=True, stdout=subprocess.DEVNULL)
    extra = run / 'extra'
    fixtures = extra / 'wros-display-test'
    fixtures.mkdir(parents=True)
    shutil.copyfile(REPO / 'os/tests/guest-display-probe.py', fixtures / 'probe')
    shutil.copyfile(binary, fixtures / 'grimoire')
    subprocess.run(['strip', str(fixtures / 'grimoire')], check=True)
    bridge = run / 'wros-display-hw'
    flags = subprocess.check_output(['pkg-config', '--cflags', '--libs', 'libdrm'], text=True).split()
    subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-D_FORTIFY_SOURCE=3', '-fstack-protector-strong',
                    str(REPO / 'os/helpers/wros-display-hw.c'), *flags, '-Wl,-z,relro,-z,now', '-o', str(bridge)], check=True)
    files = {'usr/libexec/wros-display': REPO / 'os/scripts/wros-display', 'usr/libexec/wros-display-hw': bridge,
             'usr/lib/x86_64-linux-gnu/libdrm.so.2': Path('/usr/lib/x86_64-linux-gnu/libdrm.so.2'),
             'etc/polkit-1/rules.d/50-wros-display.rules': REPO / 'os/config/50-wros-display.rules',
             'usr/share/polkit-1/actions/org.wros.display.policy': REPO / 'os/config/org.wros.display.policy'}
    for name in ('wros-display.service', 'wros-display-revert.service', 'wros-display-revert.timer'):
        files['etc/systemd/system/' + name] = REPO / 'os/systemd' / name
    with zipfile.ZipFile(fixtures / 'fixture.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, path in files.items():
            archive.write(path, name)
    hook = extra / 'scripts/local-bottom/wros-display-test'
    hook.parent.mkdir(parents=True)
    hook.write_text('''#!/bin/sh
case "$1" in prereqs) exit 0;; esac
exec >/dev/ttyS0 2>&1
set -eux
test -f "$rootmnt/etc/writingdeck/VM-DEMO-ONLY"
mount -o remount,rw /dev/vda2 "$rootmnt"
cat /wros-display-test/probe > "$rootmnt/usr/libexec/writingdeck-boot-probe"
cat /wros-display-test/grimoire > "$rootmnt/usr/bin/grimoire"
cat /wros-display-test/fixture.zip > "$rootmnt/root/wros-display-fixture.zip"
mount -o remount,ro /dev/vda2 "$rootmnt"
echo WROS_DISPLAY_FIXTURE_STAGED
''')
    hook.chmod(0o755)
    hook.with_name('ORDER').write_text('/scripts/local-bottom/wros-display-test "$@"\n')
    names = [str(path.relative_to(extra)) for path in sorted(extra.rglob('*'))]
    archive = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'], cwd=extra,
                                      input=('\n'.join(names) + '\n').encode())
    test_initrd = run / 'test-initrd.gz'
    test_initrd.write_bytes(initrd.read_bytes() + gzip.compress(archive))
    firmware = run / 'OVMF_VARS.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    serial, control = run / 'serial.log', run / 'qmp.sock'
    args = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
            '-m', '512', '-smp', '2', '-nic', 'none', '-display', 'none', '-vga', 'virtio', '-no-reboot',
            '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
            '-drive', 'if=pflash,format=raw,file=' + str(firmware),
            '-drive', 'if=virtio,format=qcow2,file=' + str(overlay), '-kernel', str(kernel), '-initrd', str(test_initrd),
            '-append', 'root=UUID=3224a43a-692d-4cd6-a526-96b9a630fa24 ro console=ttyS0,115200 console=tty1 panic=1',
            '-serial', 'file:' + str(serial), '-qmp', 'unix:' + str(control) + ',server=on,wait=off']
    recorded = [path for path in files.values() if path.is_relative_to(REPO)]
    recorded.extend(REPO / name for name in ('os/helpers/wros-display-hw.c', 'os/tests/display_vm.py', 'os/tests/guest-display-probe.py'))
    result = {'scope': 'Current binary/helper/units injected into a VM-demo COW layer; not current ISO or hardware qualification',
              'source_fingerprint': source,
              'network_devices': 0, 'host_disks_exposed': 0, 'passed': False, 'base_image_sha256_before': before,
              'binary_sha256': digest(binary), 'bridge_sha256': digest(bridge),
              'input_sha256': {str(path.relative_to(REPO)): digest(path) for path in recorded}}
    started, sent, screenshot = time.monotonic(), 0, False
    with (run / 'qemu.log').open('w') as output:
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        try:
            while process.poll() is None and time.monotonic() - started < 330:
                content = serial.read_text(errors='replace') if serial.exists() else ''
                if 'WROS_DISPLAY_FAIL' in content:
                    qmp(control, 'screendump', {'filename': str(run / 'failed-screen.ppm')})
                    break
                if 'Kernel panic' in content or 'WRITINGDECK_VM_' in content:
                    break
                def keys(*values):
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': value} for value in values], 'hold-time': 60})
                    time.sleep(0.25)
                if 'WROS_DISPLAY_UI_READY' in content and sent == 0:
                    # The dashboard owns its keys. Return to the retained
                    # writing view before invoking its command palette.
                    keys('esc')
                    time.sleep(0.5)
                    keys('ctrl', 'k')
                    # Exact existing palette query, typed as logical QWERTY.
                    for value in ('shift-w', 'r', 'shift-o', 'shift-s', 'spc', 's', 'e', 't', 't', 'i', 'n', 'g', 's'):
                        keys(*(['shift', value[6:]] if value.startswith('shift-') else [value]))
                    keys('ret')
                    sent = 1
                if 'WROS_DISPLAY_UI_SETTINGS' in content and sent == 1:
                    for value in ('down', 'down', 'ret'): keys(value)
                    sent = 2
                if 'WROS_DISPLAY_UI_SYSTEM' in content and sent == 2:
                    for value in ('down', 'ret'): keys(value)
                    sent = 3
                if 'WROS_DISPLAY_UI_DISPLAY' in content and sent == 3:
                    for value in ('end', 'ret'): keys(value)
                    sent = 4
                if 'WROS_DISPLAY_NATIVE_UI_PASS' in content and not screenshot:
                    qmp(control, 'screendump', {'filename': str(run / 'warm-screen.ppm')})
                    screenshot = True
                time.sleep(0.25)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            content = serial.read_text(errors='replace') if serial.exists() else ''
            result['markers'] = [line.strip() for line in content.splitlines() if line.startswith('WROS_DISPLAY_')]
            result['gamma_application_verified'] = 'WROS_DISPLAY_GAMMA_TIMER_COMMIT_OFF_PASS' in content
            result['unsupported_driver_refusal_verified'] = 'WROS_DISPLAY_UNAVAILABLE_NO_CHANGE_PASS' in content
            result['base_image_sha256_after'] = digest(image)
            result['base_image_unchanged'] = before == result['base_image_sha256_after']
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            result['source_unchanged'] = fingerprint() == source
            result['passed'] = (process.returncode == 0 and result['base_image_unchanged'] and screenshot and result['source_unchanged']
                                and 'WROS_DISPLAY_PASS' in content and 'WROS_DISPLAY_FAIL' not in content)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    print('Retained private virtual artifacts: ' + str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
