#!/usr/bin/env python3
"""No-disk refusal and final-confirmation cancellation in disposable QEMU.

No physical target argument exists. A cancellation target is a newly created
regular sparse file whose entire logical contents are hashed before/after.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import tempfile
import time

from installed_boot import qmp
from terminal_screen import Screen

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / 'build/installer'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('no-disk', 'cancel'))
    parser.add_argument('--media-build', type=Path, default=BASE,
                        help='existing installer build directory; read-only media inputs, never a target disk')
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise RuntimeError('QEMU tests must run as a non-root builder')
    media = args.media_build.resolve(strict=True)
    iso = media / 'wros-installer-amd64.hybrid.iso'
    if not iso.is_file() or iso.is_symlink():
        raise RuntimeError('Build the regular installer ISO first')
    run = Path(tempfile.mkdtemp(prefix='safety-', dir=BASE / 'acceptance'))
    # Synthetic credentials are only transient installer input. No account is
    # created on a target in either successful safety outcome.
    password_hash = subprocess.check_output(['openssl', 'passwd', '-6', '-stdin'],
                                            input=secrets.token_hex(16) + '\n', text=True).strip()
    seed = (REPO / 'os/config/installer.preseed').read_text() + f'''
d-i debian-installer/locale string en_US.UTF-8
d-i keyboard-configuration/xkb-keymap select us
d-i netcfg/enable boolean false
d-i netcfg/get_hostname string wros-safety
d-i netcfg/get_domain string invalid
d-i clock-setup/ntp boolean false
d-i clock-setup/utc boolean true
d-i time/zone string Etc/UTC
d-i passwd/user-fullname string Disposable safety fixture
d-i passwd/username string fixture
d-i passwd/user-password-crypted password {password_hash}
'''
    if args.mode == 'cancel':
        # Choose only this disposable target, then leave BOTH final destructive
        # confirmations interactive. The controller will answer No, never Yes.
        seed += '''
d-i partman-auto/disk string /dev/vda
d-i partman-auto/method string regular
d-i partman-auto/choose_recipe select atomic
d-i partman-partitioning/confirm_write_new_label boolean true
'''
    (run / 'preseed.cfg').write_text(seed)
    extra = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'],
                                    cwd=run, input=b'preseed.cfg\n')
    initrd = run / 'test-initrd.gz'
    initrd.write_bytes((media / 'binary/install/initrd.gz').read_bytes() + gzip.compress(extra))
    firmware = run / 'OVMF_VARS.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    disk = run / 'cancel-target.raw'
    before = None
    if args.mode == 'cancel':
        with disk.open('xb') as stream:
            stream.truncate(12 * 1024**3)
            stream.seek(12 * 1024**3 - 4096)
            stream.write(b'WrOS disposable cancellation sentinel\n')
        before = digest(disk)
    serial, control = run / 'serial.sock', run / 'qmp.sock'
    command = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
               '-m', '1024', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
               '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
               '-drive', 'if=pflash,format=raw,file=' + str(firmware), '-cdrom', str(iso),
               '-kernel', str(media / 'binary/install/vmlinuz'), '-initrd', str(initrd),
               '-append', 'auto=true priority=critical DEBIAN_FRONTEND=text --- console=ttyS0,115200n8',
               '-serial', 'unix:' + str(serial) + ',server=on,wait=on',
               '-qmp', 'unix:' + str(control) + ',server=on,wait=off']
    if args.mode == 'cancel':
        command += ['-drive', 'if=virtio,format=raw,file=' + str(disk)]
    result = {'schema_version': 1, 'mode': args.mode, 'passed': False,
              'iso_sha256': digest(iso), 'host_disks_exposed': 0, 'network_devices': 0,
              'target_is_new_regular_file': args.mode == 'cancel', 'write_confirmation_seen': False,
              'negative_answer_sent': False, 'no_disk_refusal_seen': False,
              'finish_without_disk_attempted': False,
              'partition_finish_requested': False,
              'partition_menu_returned': False,
              'virtual_target_sha256_before': before}
    started = time.monotonic()
    screen = Screen(80, 24)
    content = bytearray()
    with (run / 'qemu.log').open('w') as log, (run / 'serial.log').open('wb') as capture:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while not serial.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Safety VM serial endpoint did not start')
                time.sleep(0.1)
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(0.5)
                connection.connect(str(serial))
                deadline = started + 900
                answered_at = None
                answer_offset = None
                while process.poll() is None and time.monotonic() < deadline:
                    try:
                        data = connection.recv(65536)
                    except socket.timeout:
                        data = None
                    if data == b'':
                        break
                    if data:
                        capture.write(data)
                        capture.flush()
                        content.extend(data)
                        screen.feed(data)
                        (run / 'screen.txt').write_text(screen.text)
                    if args.mode == 'no-disk' and not result['finish_without_disk_attempted'] and all(
                        marker in content for marker in (b'This is an overview of your currently configured partitions',
                                                        b'4: Finish partitioning and write changes to disk',
                                                        b"Prompt: '?' for help>")):
                        # There are no target drives in this VM. Ask the installer
                        # to finish the empty plan and require an actual refusal.
                        connection.sendall(b'4\n')
                        result['finish_without_disk_attempted'] = True
                    if args.mode == 'no-disk' and any(marker in content for marker in (
                        b'No disk drive was detected', b'No partitionable media', b'No hard disk drive was detected',
                        b'No root file system is defined')):
                        result['no_disk_refusal_seen'] = True
                        result['passed'] = True
                        break
                    if args.mode == 'cancel' and not result['partition_finish_requested']:
                        choice = re.search(rb'(\d+): Finish partitioning and write changes to disk', content)
                        if choice and b"Prompt: '?' for help" in content[choice.end():]:
                            connection.sendall(choice[1] + b'\n')
                            result['partition_finish_requested'] = True
                    if args.mode == 'cancel' and not result['negative_answer_sent'] and b'Write the changes to disks?' in content:
                        result['write_confirmation_seen'] = True
                        # The Debian text frontend accepts numbered choices;
                        # spelling "no" merely redisplays this question.
                        connection.sendall(b'2\n')
                        result['negative_answer_sent'] = True
                        answered_at = time.monotonic()
                        answer_offset = len(content)
                    if answered_at is not None and time.monotonic() - answered_at > 3:
                        # Require newly received menu text after the answer.
                        # The minimal ANSI tracker does not implement every
                        # scrolling escape and can retain an older menu title.
                        returned = content[answer_offset:]
                        if all(marker in returned for marker in (
                            b'This is an overview of your currently configured partitions',
                            b'Finish partitioning and write changes to disk', b"Prompt: '?' for help")):
                            result['partition_menu_returned'] = True
                            break
                    if args.mode == 'cancel' and b'Installing the system' in content:
                        break
                qmp(control, 'screendump', {'filename': str(run / 'safety-screen.ppm')})
                qmp(control, 'quit')
                process.wait(timeout=10)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if args.mode == 'cancel':
                result['virtual_target_sha256_after'] = digest(disk)
                result['target_unchanged'] = before == result['virtual_target_sha256_after']
                result['passed'] = (result['negative_answer_sent'] and result['partition_menu_returned'] and result['target_unchanged']
                                    and b'Installing the system' not in content)
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)
    print('Private safety artifacts: ' + str(run), flush=True)
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
