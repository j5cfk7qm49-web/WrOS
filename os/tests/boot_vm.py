#!/usr/bin/env python3
"""One-shot, networkless UEFI QEMU acceptance test; uses a disposable COW layer.

Regular artifacts in build/ only. No root requirement, host block devices, KVM,
network interface, firmware-variable modification, or manuscript sharing.
"""
import json
import hashlib
from datetime import datetime, timezone
import sys
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import time

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / 'build/appliance'
EVIDENCE = REPO / 'docs/evidence'
sys.path.insert(0, str(REPO/'tools'))
from writingdeck_source import fingerprint


def qmp_screenshot(path):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(5)
        sock.connect(str(BASE / 'qmp.sock'))
        stream = sock.makefile('rwb', buffering=0)
        json.loads(stream.readline())
        for message in ({'execute': 'qmp_capabilities'},
                        {'execute': 'screendump', 'arguments': {'filename': str(path)}}):
            stream.write(json.dumps(message).encode() + b'\n')
            while True:
                reply = json.loads(stream.readline())
                if 'error' in reply:
                    raise RuntimeError(reply['error'])
                if 'return' in reply:
                    break


def main():
    image = BASE / 'disk.img'
    if not image.is_file() or image.is_symlink():
        raise RuntimeError('Build the VM image first; refusing non-regular disk artifact.')
    BASE.mkdir(exist_ok=True, parents=True)
    EVIDENCE.mkdir(exist_ok=True, parents=True)
    vars_path = BASE / 'OVMF_VARS-test.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', vars_path)
    serial = BASE / 'serial.log'
    qmp = BASE / 'qmp.sock'
    qmp.unlink(missing_ok=True)
    serial.write_text('')
    args = [
        'qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35',
        '-m', '512', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
        '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
        '-drive', 'if=pflash,format=raw,file=' + str(vars_path),
        '-drive', 'if=virtio,format=raw,snapshot=on,file=' + str(image),
        '-serial', 'file:' + str(serial), '-qmp', 'unix:' + str(qmp) + ',server=on,wait=off',
    ]
    start = time.monotonic()
    screenshot = False
    with open(BASE / 'qemu.log', 'w') as log:
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            deadline = start + int(os.environ.get('WRITINGDECK_VM_TIMEOUT', '300'))
            while process.poll() is None:
                content = serial.read_text(errors='replace')
                if ('WRITINGDECK_VM_PASS' in content or 'WRITINGDECK_VM_FAIL' in content or time.monotonic() - start > 70) and not screenshot:
                    qmp_screenshot(BASE / 'dashboard.ppm')
                    screenshot = True
                if time.monotonic() > deadline:
                    raise TimeoutError('VM boot acceptance deadline exceeded; inspect build/appliance/serial.log and qemu.log')
                time.sleep(0.2)
            process.wait(timeout=5)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    content = serial.read_text(errors='replace')
    markers = [line.strip() for line in content.splitlines() if 'WRITINGDECK_VM_' in line]
    result = {
        'kind': 'QEMU virtual hardware, NOT physical hardware verification',
        'recorded_at_utc': datetime.now(timezone.utc).isoformat(),
        'source_fingerprint': fingerprint(),
        'image_sha256': (BASE/'SHA256SUMS').read_text().split()[0],
        'installed_stripped_binary_sha256': hashlib.sha256((BASE/'rootfs/usr/bin/grimoire').read_bytes()).hexdigest(),
        'firmware': 'OVMF UEFI, Secure Boot disabled', 'acceleration': 'TCG',
        'memory_mib': 512, 'vcpus': 2, 'network_devices': 0,
        'wall_seconds': round(time.monotonic() - start, 2), 'markers': markers,
        'passed': all(s in content for s in [
            'WRITINGDECK_VM_UI_PASS', 'WRITINGDECK_VM_POLKIT_PASS',
            'WRITINGDECK_VM_KEYBOARD_ROLLBACK_PASS', 'WRITINGDECK_VM_KEYBOARD_COMMIT_PASS',
            'WRITINGDECK_VM_GRAMMAR_PASS', 'WRITINGDECK_VM_PASS']) and 'WRITINGDECK_VM_FAIL' not in content,
    }
    grammar_lines = [line.split('WRITINGDECK_VM_GRAMMAR_RESULT ',1)[1] for line in content.splitlines() if 'WRITINGDECK_VM_GRAMMAR_RESULT ' in line]
    if grammar_lines:
        result['grammar'] = json.loads(grammar_lines[-1])
        (EVIDENCE/'vm-grammar.txt').write_text(result['grammar']['frame']+'\n')
    (EVIDENCE / 'vm-boot.json').write_text(json.dumps(result, indent=2) + '\n')
    # Persist small evidence, never the multi-gigabyte VM image in a source snapshot.
    (EVIDENCE / 'vm-serial.log').write_text(content)
    if screenshot:
        try:
            from PIL import Image
            Image.open(BASE / 'dashboard.ppm').save(EVIDENCE / 'vm-dashboard.png')
        except ImportError:
            print('Pillow is optional; raw screenshot remains in build/appliance/dashboard.ppm')
    print(json.dumps(result, indent=2))
    if not result['passed']:
        print('\nLast serial output:\n' + content[-7000:])
        raise SystemExit(1)


if __name__ == '__main__':
    main()
