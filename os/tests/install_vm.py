#!/usr/bin/env python3
"""Install offline into a NEW disposable QEMU file, never a host disk.

The extra test initrd supplies synthetic credentials and disk answers. These
answers are NOT added to the shipping ISO. No network or host directory share.
"""
import gzip
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile
import time
from installed_boot import verify as verify_boot

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / 'build/installer'


def main():
    if os.geteuid() == 0:
        raise RuntimeError('Run QEMU acceptance as a non-root builder')
    iso = BASE / 'wros-installer-amd64.hybrid.iso'
    if iso.is_symlink() or not iso.is_file():
        raise RuntimeError('Build the regular installer ISO first')
    trust = json.loads((BASE / 'installer-input-trust.json').read_text())
    if not trust['passed']:
        raise RuntimeError('Authenticate Debian Installer inputs before boot')
    kernel = BASE / 'binary/install/vmlinuz'
    if hashlib.sha256(kernel.read_bytes()).hexdigest() != trust['images']['cdrom/vmlinuz']['sha256']:
        raise RuntimeError('Installer kernel changed after trust verification')
    run = Path(tempfile.mkdtemp(prefix='install-', dir=BASE / 'acceptance'))
    secret = secrets.token_hex(16)
    password_hash = subprocess.check_output(['openssl', 'passwd', '-6', '-stdin'], input=secret + '\n', text=True).strip()
    preseed = (REPO / 'os/config/installer.preseed').read_text() + f'''
# Disposable no-NIC QEMU fixture only; never distributed in ISO.
d-i debian-installer/locale string en_US.UTF-8
d-i keyboard-configuration/xkb-keymap select us
d-i netcfg/enable boolean false
d-i netcfg/get_hostname string wros-fixture
d-i netcfg/get_domain string invalid
d-i clock-setup/utc boolean true
d-i clock-setup/ntp boolean false
d-i time/zone string Etc/UTC
d-i passwd/user-fullname string Disposable VM Fixture
d-i passwd/username string fixture
d-i passwd/user-password-crypted password {password_hash}
d-i partman-auto/disk string /dev/vda
d-i partman-auto/method string regular
d-i partman-auto/choose_recipe select atomic
d-i partman-partitioning/confirm_write_new_label boolean true
d-i partman/choose_partition select finish
d-i partman/confirm boolean true
d-i partman/confirm_nooverwrite boolean true
d-i apt-setup/use_mirror boolean false
d-i apt-setup/services-select multiselect
d-i pkgsel/upgrade select none
tasksel tasksel/first multiselect
popularity-contest popularity-contest/participate boolean false
d-i grub-installer/only_debian boolean true
d-i grub-installer/with_other_os boolean false
d-i grub-installer/bootdev string /dev/vda
d-i cdrom-detect/eject boolean false
d-i finish-install/keep-consoles boolean true
d-i finish-install/reboot_in_progress note
d-i debian-installer/exit/poweroff boolean true
d-i preseed/late_command string in-target --pass-stdout /bin/sh -c 'test -x /usr/bin/grimoire && test -x /opt/writingdeck/python/bin/python3 && test ! -e /etc/writingdeck/VM-DEMO-ONLY && id -nG fixture | grep -qw writingdeck && echo WROS_INSTALLED_PAYLOAD_PASS' > /dev/ttyS0 && in-target --pass-stdout /usr/sbin/runuser -u fixture -- /usr/bin/grimoire --version > /dev/ttyS0 && echo WROS_INSTALL_LATE_REACHED > /dev/ttyS0
'''
    (run / 'preseed.cfg').write_text(preseed)
    extra = subprocess.check_output(['cpio', '--create', '--format=newc', '--quiet'], cwd=run, input=b'preseed.cfg\n')
    initrd = run / 'test-initrd.gz'
    initrd.write_bytes((BASE / 'binary/install/initrd.gz').read_bytes() + gzip.compress(extra))
    # Only this freshly created qcow2 file is writable in the guest.
    disk = run / 'target.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', str(disk), '12G'], check=True, stdout=subprocess.DEVNULL)
    firmware = run / 'OVMF_VARS.fd'
    shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', firmware)
    serial = run / 'install-serial.log'
    args = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
            '-m', '1024', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
            '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
            '-drive', 'if=pflash,format=raw,file=' + str(firmware),
            '-drive', 'if=virtio,format=qcow2,file=' + str(disk), '-cdrom', str(iso),
            '-kernel', str(kernel), '-initrd', str(initrd),
            # Debian copies only parameters after --- into installed GRUB.
            # This fixture keeps serial maintenance AND the normal tty1 UI.
            '-append', 'auto=true priority=critical DEBIAN_FRONTEND=text --- console=ttyS0,115200n8',
            '-serial', 'file:' + str(serial)]
    started = time.monotonic()
    result = {'schema_version': 1, 'scope': 'Disposable no-network QEMU install, not hardware qualification',
              'iso_sha256': hashlib.sha256(iso.read_bytes()).hexdigest(),
              'installed_binary_sha256': hashlib.sha256((BASE / 'chroot/usr/bin/grimoire').read_bytes()).hexdigest(),
              'network_devices': 0, 'host_disks_exposed': 0, 'test_answers_in_shipping_iso': False,
              'installed_boot_verified': False, 'authentication_verified': False, 'passed': False}
    with (run / 'qemu.log').open('w') as log:
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            process.wait(timeout=1200)
            content = serial.read_text(errors='replace')
            result['exit_code'] = process.returncode
            result['late_payload_verified'] = 'WROS_INSTALLED_PAYLOAD_PASS' in content
            result['late_stage_reached'] = 'WROS_INSTALL_LATE_REACHED' in content
            if secret in content:
                raise RuntimeError('Synthetic password appeared in output; output must remain private')
            result['passed'] = (process.returncode == 0 and result['late_payload_verified']
                                and result['late_stage_reached'] and 'Power down' in content)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            result['wall_seconds'] = round(time.monotonic() - started, 2)
            (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    result['installation_copy_verified'] = result['passed']
    result['passed'] = False
    try:
        if result['installation_copy_verified']:
            result.update(verify_boot(run, secret))
            result['passed'] = result['authentication_verified'] and result['tty1_nonroot_session_verified']
    finally:
        (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)
    print('Private test artifacts retained in ' + str(run), flush=True)
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
