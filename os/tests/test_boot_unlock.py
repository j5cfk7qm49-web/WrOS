"""The Ratatui disk password in the boot image (docs/RATATUI_BOOT.md): the askpass
wrapper always ends at Debian's prompt unless wros-boot gave a password, and the
hook never breaks the boot image."""
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
WRAPPER = ROOT / 'os/initramfs/askpass'
HOOK = ROOT / 'os/initramfs/wros-boot-hook'


def script(path, body):
    path.write_text('#!/bin/sh\n' + body)
    path.chmod(0o755)
    return path


class Wrapper(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='wros-askpass-'))
        self.addCleanup(shutil.rmtree, self.tmp)
        self.ran = self.tmp / 'wros-boot-ran'
        self.debian = script(self.tmp / 'askpass.debian', 'printf "debian-password:%s" "$1"\n')
        self.cmdline = self.tmp / 'cmdline'
        self.cmdline.write_text('BOOT_IMAGE=/vmlinuz root=/dev/mapper/root ro quiet splash\n')

    def boot(self, body):
        return script(self.tmp / 'wros-boot', f'touch "{self.ran}"\n' + body)

    def run_wrapper(self, boot=None, prompt='Please unlock disk root_crypt: '):
        env = dict(os.environ, WROS_CMDLINE=str(self.cmdline), WROS_ASKPASS_DEBIAN=str(self.debian),
                   WROS_BOOT_BINARY=str(boot or self.tmp / 'missing'))
        return subprocess.run(['sh', str(WRAPPER), prompt], env=env, capture_output=True, timeout=20)

    def test_a_password_from_the_start_screen_passes_through_unchanged(self):
        boot = self.boot('[ "$1" = unlock ] && [ "$2" = "Please unlock disk root_crypt: " ] || exit 30\n'
                         'printf "pässwort with spaces"\n')
        done = self.run_wrapper(boot)
        self.assertEqual((done.returncode, done.stdout), (0, 'pässwort with spaces'.encode()))

    def test_every_other_ending_goes_to_debians_prompt(self):
        endings = {
            'esc': 'exit 10\n',
            'could not run': 'exit 20\n',
            'crash': 'kill -SEGV $$\n',
            'unexpected code': 'exit 1\n',
        }
        for name, body in endings.items():
            with self.subTest(name):
                done = self.run_wrapper(self.boot(body))
                self.assertEqual(done.stdout, b'debian-password:Please unlock disk root_crypt: ')
                self.assertTrue(self.ran.exists())
                self.ran.unlink()

    def test_a_missing_or_unrunnable_start_screen_goes_to_debians_prompt(self):
        self.assertEqual(self.run_wrapper().stdout, b'debian-password:Please unlock disk root_crypt: ')
        broken = self.tmp / 'not-executable'
        broken.write_text('#!/bin/sh\nprintf nope\n')
        self.assertEqual(self.run_wrapper(broken).stdout, b'debian-password:Please unlock disk root_crypt: ')
        garbage = self.tmp / 'garbage'
        garbage.write_bytes(b'\x7fELF not really')
        garbage.chmod(0o755)
        self.assertEqual(self.run_wrapper(garbage).stdout, b'debian-password:Please unlock disk root_crypt: ')

    def test_wros_unlock_text_skips_the_start_screen(self):
        boot = self.boot('printf should-not-run\n')
        self.cmdline.write_text('root=/dev/mapper/root ro wros.unlock=text quiet\n')
        done = self.run_wrapper(boot)
        self.assertEqual(done.stdout, b'debian-password:Please unlock disk root_crypt: ')
        self.assertFalse(self.ran.exists())
        # Only the exact word: wros.unlock=textual is not a request for the text prompt.
        self.cmdline.write_text('wros.unlock=textual\n')
        self.assertEqual(self.run_wrapper(boot).stdout, b'should-not-run')


class Hook(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='wros-hook-'))
        self.addCleanup(shutil.rmtree, self.tmp)
        self.dest = self.tmp / 'image'
        (self.dest / 'usr/lib/cryptsetup').mkdir(parents=True)
        self.askpass = self.dest / 'usr/lib/cryptsetup/askpass'
        self.askpass.write_bytes(b'DEBIAN ASKPASS BINARY')
        self.askpass.chmod(0o755)
        self.switch = self.tmp / 'boot-screen'
        self.binary = script(self.tmp / 'wros-boot', 'exit 0\n')
        self.theme = self.tmp / 'boot-theme.toml'
        self.theme.write_text('name = "Gruvbox"\n')
        self.copy_fails = self.tmp / 'copy-fails'
        # Stand-ins for initramfs-tools' copy_exec: copies into the image, or fails on demand.
        self.functions = self.tmp / 'hook-functions'
        self.functions.write_text(
            'copy_exec() {\n'
            f'    [ -e "{self.copy_fails}" ] && return 1\n'
            '    mkdir -p "$DESTDIR$(dirname "$2")" && cp "$1" "$DESTDIR$2"\n'
            '}\n')

    def run_hook(self, *args):
        env = dict(os.environ, DESTDIR=str(self.dest), WROS_HOOK_FUNCTIONS=str(self.functions),
                   WROS_BOOT_SWITCH=str(self.switch), WROS_BOOT_BINARY=str(self.binary),
                   WROS_BOOT_WRAPPER=str(WRAPPER), WROS_BOOT_THEME=str(self.theme))
        return subprocess.run(['sh', str(HOOK), *args], env=env, capture_output=True, text=True, timeout=20)

    def unchanged(self):
        self.assertEqual(self.askpass.read_bytes(), b'DEBIAN ASKPASS BINARY')
        self.assertFalse((self.dest / 'usr/lib/cryptsetup/askpass.debian').exists())
        self.assertFalse((self.dest / 'usr/lib/wros/wros-boot').exists())

    def test_it_runs_after_cryptsetups_hook(self):
        self.assertEqual(self.run_hook('prereqs').stdout.strip(), 'cryptroot')

    def test_with_the_switch_off_the_image_is_debians(self):
        for content in (None, '', 'text\n', 'Ratatui\n'):
            with self.subTest(content=content):
                if content is None:
                    self.switch.unlink(missing_ok=True)
                else:
                    self.switch.write_text(content)
                done = self.run_hook()
                self.assertEqual(done.returncode, 0)
                self.unchanged()

    def test_with_the_switch_on_the_wrapper_stands_in_front_of_debians_prompt(self):
        self.switch.write_text('ratatui\n')
        for _ in range(2):  # a second run over the same image changes nothing more
            done = self.run_hook()
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual((self.dest / 'usr/lib/cryptsetup/askpass.debian').read_bytes(), b'DEBIAN ASKPASS BINARY')
            self.assertEqual(self.askpass.read_bytes(), WRAPPER.read_bytes())
            self.assertTrue(self.askpass.stat().st_mode & stat.S_IXUSR)
            self.assertTrue((self.dest / 'usr/lib/wros/wros-boot').is_file())
            self.assertEqual((self.dest / 'etc/wros/boot-theme.toml').read_text(), 'name = "Gruvbox"\n')
        self.assertEqual(done.stderr, '')

    def test_any_missing_piece_leaves_debians_prompt_and_succeeds(self):
        self.switch.write_text('ratatui\n')
        cases = {
            'no start screen': lambda: self.binary.unlink(),
            'copy fails': lambda: self.copy_fails.touch(),
        }
        for name, breaks in cases.items():
            with self.subTest(name):
                self.setUp()
                self.switch.write_text('ratatui\n')
                breaks()
                done = self.run_hook()
                self.assertEqual(done.returncode, 0)
                self.assertIn("keeps Debian's disk prompt", done.stderr)
                self.unchanged()

    def test_an_image_without_an_encrypted_disk_is_left_alone(self):
        self.switch.write_text('ratatui\n')
        self.askpass.unlink()
        done = self.run_hook()
        self.assertEqual((done.returncode, done.stderr), (0, ''))
        self.assertFalse((self.dest / 'usr/lib/wros').exists())

    def test_without_a_theme_the_start_screen_uses_the_default(self):
        self.switch.write_text('ratatui\n')
        self.theme.unlink()
        self.assertEqual(self.run_hook().returncode, 0)
        self.assertFalse((self.dest / 'etc/wros/boot-theme.toml').exists())
        self.assertEqual(self.askpass.read_bytes(), WRAPPER.read_bytes())


class ProgressScreen(unittest.TestCase):
    """wros-boot-splash.service hands tty1 to the login screen and stays out of plain starts."""

    def setUp(self):
        self.unit = {}
        for line in (ROOT / 'os/systemd/wros-boot-splash.service').read_text().splitlines():
            if '=' in line and not line.startswith('#'):
                key, value = line.split('=', 1)
                self.unit.setdefault(key, []).append(value)

    def words(self, key):
        return ' '.join(self.unit.get(key, [])).split()

    def test_the_login_screen_stops_it_and_nothing_conflicts_with_the_login(self):
        # Seen in the VM (vm4): Conflicts=greetd.service let systemd drop the login screen's start
        # job at boot, and tty1 never showed the login.
        self.assertEqual(self.words('Conflicts'), ['shutdown.target'])
        # Seen in the VM too: ordered Before=greetd.service, the stop in greetd's ExecStartPre
        # was refused and the progress screen outlived the login's start.
        self.assertEqual(self.words('Before'), ['shutdown.target'])
        greetd = (ROOT / 'os/config/greetd.service').read_text()
        self.assertIn('ExecStartPre=-/usr/bin/systemctl stop wros-boot-splash.service', greetd)
        self.assertNotIn('wros-boot-splash', greetd.split('[Service]')[0])

    def test_it_runs_only_with_the_switch_on_and_never_on_a_plain_text_start(self):
        self.assertEqual(self.unit['ConditionPathExists'], ['/etc/wros/boot-screen'])
        self.assertEqual(self.unit['ConditionKernelCommandLine'], ['!wros.unlock=text'])
        self.assertIn('= ratatui', self.unit['ExecCondition'][0])
        self.assertEqual(self.unit['ExecStart'], ['/usr/lib/wros/wros-boot splash --tty /dev/tty1'])
        self.assertEqual(self.unit['WantedBy'], ['sysinit.target'])
        self.assertEqual(self.unit['DefaultDependencies'], ['no'])


if __name__ == '__main__':
    unittest.main()
