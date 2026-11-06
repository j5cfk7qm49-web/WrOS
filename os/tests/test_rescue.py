import os
from pathlib import Path
import pty
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RESCUE = ROOT / 'os/scripts/wros-rescue'
LUKS_UUID = '80b3e367-28a9-41c7-9aca-b7fdf2be9224'

# Each stub logs its command line; the disk tools model one LUKS partition holding the
# guided install's debian-vg with root and swap_1, and a root whose fstab has /boot and /boot/efi.
STUBS = {
    'id': 'echo 0',
    'lsblk': 'echo "/dev/vda3 crypto_LUKS part"',
    'cryptsetup': f'''case $1 in
    isLuks|open|close) exit 0 ;;
    status) exit 1 ;;
    luksUUID) echo {LUKS_UUID} ;;
esac''',
    'pvs': 'echo "  debian-vg"',
    'vgchange': 'exit 0',
    'lvs': 'printf "  /dev/debian-vg/root\\n  /dev/debian-vg/swap_1\\n"',
    'blkid': '[ "$5" = /dev/debian-vg/root ] && echo ext4 || echo swap',
    'mountpoint': 'exit 1',
    'dmsetup': '[ "$1" = info ] && exit 1; exit 0',
    'mount': '''last=$(eval echo \\${$#})
if [ "$1" = -t ] && [ "$2" = ext4 ] && [ "$last" = "$TARGET" ]; then cp -a "$FIXTURE/." "$TARGET"; fi''',
    'umount': '[ "$1" = "$TARGET" ] && find "$TARGET" -mindepth 1 -delete; exit 0',
    'chroot': '''case "$*" in
    *"/usr/bin/mount /boot") [ -e "$STATE/fail-boot" ] && exit 32; exit 0 ;;
    *rescue-repair*) echo '{"status": "clean"}' ;;
esac''',
}


class RescueTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='wros-rescue-')
        base = Path(self.directory.name)
        self.state, self.target, fixture, stubs = base / 'state', base / 'target', base / 'fixture', base / 'bin'
        for path in (self.state, fixture, stubs):
            path.mkdir()
        for relative in ('etc/wros', 'usr/lib/wros-update', 'usr/libexec', 'dev/pts', 'proc', 'sys', 'run',
                         'boot'):
            (fixture / relative).mkdir(parents=True)
        (fixture / 'etc/wros/release').write_text('WROS_VERSION=0.1.0~rc1\n')
        (fixture / 'etc/crypttab').write_text(f'vda3_crypt UUID={LUKS_UUID} none luks,discard\n')
        (fixture / 'etc/fstab').write_text('/dev/mapper/debian--vg-root / ext4 errors=remount-ro 0 1\n'
                                           'UUID=f3a68734 /boot ext4 defaults 0 2\n'
                                           'UUID=EC6E-66AF /boot/efi vfat umask=0077 0 1\n')
        (fixture / 'usr/lib/wros-update/engine.py').write_text('# engine\n')
        (fixture / 'usr/libexec/wros-update').write_text('#!/bin/sh\n')
        (fixture / 'usr/libexec/wros-update').chmod(0o755)
        for name, body in STUBS.items():
            (stubs / name).write_text(f'#!/bin/sh\necho "{name} $*" >> "$STATE/log"\n{body}\n')
            (stubs / name).chmod(0o755)
        (self.state / 'cmdline').write_text('BOOT_IMAGE=/live/vmlinuz boot=live components wros.live=1\n')
        self.env = {'WROS_RESCUE_TEST_PATH': f'{stubs}:/usr/bin:/bin', 'WROS_RESCUE_TARGET': str(self.target),
                    'WROS_RESCUE_CMDLINE': str(self.state / 'cmdline'), 'TARGET': str(self.target),
                    'FIXTURE': str(fixture), 'STATE': str(self.state), 'PATH': '/usr/bin:/bin'}

    def tearDown(self):
        self.directory.cleanup()

    def run_rescue(self):
        """Run the real script on a pseudo-terminal, as an operator would on the live ISO."""
        pid, fd = pty.fork()
        if pid == 0:
            os.execve('/bin/bash', ['/bin/bash', str(RESCUE)], self.env)
        output = b''
        while True:
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                break
            if not chunk:
                break
            output += chunk
        _, status = os.waitpid(pid, 0)
        return os.waitstatus_to_exitcode(status), output.decode(), (self.state / 'log').read_text().splitlines()

    def test_unlocks_under_the_crypttab_name_and_mounts_boot_before_the_esp(self):
        code, output, log = self.run_rescue()
        self.assertEqual(code, 0, output)
        target = str(self.target)
        # In this order (the group is activated once before the rename and again after it).
        position = 0
        for line in ('cryptsetup open --type luks --tries 3 /dev/vda3 wros-rescue-root',
                     'vgchange -ay debian-vg',
                     'vgchange -an debian-vg',
                     'dmsetup rename wros-rescue-root vda3_crypt',
                     'vgchange -ay debian-vg',
                     f'mount -t ext4 -o rw,nodev /dev/debian-vg/root {target}',
                     f'chroot {target} /usr/bin/mount /boot',
                     f'chroot {target} /usr/bin/mount /boot/efi'):
            self.assertIn(line, log[position:], f'missing or out of order: {line}')
            position = log.index(line, position) + 1
        self.assertTrue(any('rescue-repair' in line for line in log))
        self.assertEqual(log[-1], 'cryptsetup close vda3_crypt')
        self.assertIn(f'umount {target}/boot', log)
        self.assertLess(log.index(f'umount {target}/boot/efi'), log.index(f'umount {target}/boot'))

    def test_a_boot_partition_that_will_not_mount_stops_before_any_repair(self):
        (self.state / 'fail-boot').touch()
        code, output, log = self.run_rescue()
        self.assertEqual(code, 1)
        self.assertIn('could not mount the installed /boot', output)
        self.assertFalse(any('rescue-repair' in line or '/boot/efi' in line for line in log))
        self.assertEqual(log[-1], 'cryptsetup close vda3_crypt')


if __name__ == '__main__':
    unittest.main()
