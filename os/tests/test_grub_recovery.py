"""The GRUB scripts for the recovery entry and the failed-boot menu, run with Debian's own
grub-mkconfig_lib and checked by grub-script-check. Only a fake /boot in a temporary
directory is used; no device is probed and no configuration is written."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIB = Path('/usr/share/grub/grub-mkconfig_lib')
CHECK = shutil.which('grub-script-check')


@unittest.skipUnless(LIB.is_file() and CHECK, 'needs grub-common (grub-mkconfig_lib, grub-script-check)')
class GrubRecovery(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='wros-grub-')
        self.root = Path(self.temporary.name)
        self.boot = self.root / 'boot'
        self.boot.mkdir()
        self.abstraction = ''
        self.uuid_dir = self.root / 'by-uuid'
        self.uuid_dir.mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def kernel(self, version, initrd=True):
        (self.boot / f'vmlinuz-{version}').write_bytes(b'kernel')
        if initrd:
            (self.boot / f'initrd.img-{version}').write_bytes(b'initrd')

    def generate(self, script, **env):
        # Debian's library, with the two device probes replaced: there is no real disk here.
        probe = self.root / 'grub-probe'
        probe.write_text('#!/bin/sh\ncase "$*" in *abstraction*) echo "$WROS_TEST_ABSTRACTION";;'
                         ' *) echo /dev/fake;; esac\n')
        probe.chmod(0o755)
        lib = self.root / 'lib'
        lib.write_text(f'. {LIB}\ngrub_probe={probe}\n'
                       'prepare_grub_to_access_device () { echo "search --no-floppy --fs-uuid --set=root BOOT-UUID"; }\n'
                       'make_system_path_relative_to_its_root () { echo /; }\n')
        values = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'WROS_GRUB_MKCONFIG_LIB': str(lib),
                  'WROS_GRUB_KERNEL_DIR': str(self.boot), 'WROS_TEST_ABSTRACTION': self.abstraction,
                  'GRUB_DEVICE': '/dev/mapper/root', 'GRUB_DEVICE_BOOT': '/dev/sda2',
                  'GRUB_DEVICE_UUID': 'ROOT-UUID', 'WROS_GRUB_UUID_DIR': str(self.uuid_dir),
                  'GRUB_CMDLINE_LINUX': 'resume=UUID=SWAP', 'GRUB_CMDLINE_LINUX_DEFAULT': 'quiet', **env}
        done = subprocess.run(['sh', str(ROOT / 'os/grub' / script)], env=values, capture_output=True,
                              text=True, check=True)
        if done.stdout.strip():
            checked = subprocess.run([CHECK], input=done.stdout, capture_output=True, text=True)
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        return done.stdout

    def test_recovery_entry_boots_the_newest_kernel_into_the_recovery_target(self):
        self.kernel('6.12.9+deb13-amd64')
        self.kernel('6.12.113+deb13-amd64')
        (self.uuid_dir / 'ROOT-UUID').touch()
        out = self.generate('42_wros_recovery')
        self.assertIn("menuentry 'WrOS recovery'", out)
        self.assertIn('search --no-floppy --fs-uuid --set=root BOOT-UUID', out)
        linux = next(line.split() for line in out.splitlines() if line.strip().startswith('linux'))
        self.assertEqual(linux[1], '/vmlinuz-6.12.113+deb13-amd64')
        self.assertIn('root=UUID=ROOT-UUID', linux)
        self.assertIn('resume=UUID=SWAP', linux)
        self.assertEqual(linux[-1], 'systemd.unit=wros-recovery.target')
        self.assertNotIn('quiet', linux)  # recovery keeps its boot messages
        self.assertIn('initrd\t/initrd.img-6.12.113+deb13-amd64', out)

    def test_without_a_by_uuid_link_the_device_path_is_used(self):
        self.kernel('6.12.113+deb13-amd64')
        out = self.generate('42_wros_recovery')
        self.assertIn('root=/dev/mapper/root', out)

    def test_no_entry_without_the_kernels_initramfs(self):
        # The encrypted root cannot be opened without it, so an entry would only fail.
        self.kernel('6.12.113+deb13-amd64', initrd=False)
        self.assertEqual(self.generate('42_wros_recovery').strip(), '')
        shutil.rmtree(self.boot)
        self.boot.mkdir()
        self.assertEqual(self.generate('42_wros_recovery').strip(), '')

    def test_failed_boot_check_marks_every_boot_and_shows_the_menu_after_one_that_never_finished(self):
        out = self.generate('06_wros_boot_check')
        self.assertIn('if [ "${wros_boot_pending}" = "1" ]; then', out)
        self.assertIn('set timeout_style=menu', out)
        self.assertIn('set timeout=30', out)
        self.assertIn('set wros_boot_pending=1', out)
        self.assertIn('save_env wros_boot_pending', out)
        self.assertLess(out.index('set timeout=30'), out.index('set wros_boot_pending=1'))

    def test_failed_boot_check_stays_out_where_grub_cannot_write_its_environment(self):
        for abstraction in ('lvm', 'diskfilter', 'cryptodisk luks2'):
            self.abstraction = abstraction
            self.assertEqual(self.generate('06_wros_boot_check').strip(), '', abstraction)

    def test_boot_settings_hide_the_menu_and_drop_debians_root_shell_entries(self):
        settings = (ROOT / 'os/grub/60-wros.cfg').read_text()
        lines = {line.split('=', 1)[0]: line.split('=', 1)[1] for line in settings.splitlines()
                 if line and not line.startswith('#')}
        self.assertEqual(lines, {'GRUB_TIMEOUT_STYLE': 'hidden', 'GRUB_TIMEOUT': '2',
                                 'GRUB_DISABLE_RECOVERY': 'true'})


if __name__ == '__main__':
    unittest.main()
