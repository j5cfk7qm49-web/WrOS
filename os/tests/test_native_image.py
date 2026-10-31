"""Real staged-dpkg and policy mutations; no root, services or host state."""
import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('native_image', ROOT / 'tools/audit-native-image.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture(root):
    database = root / 'var/lib/dpkg/status'
    database.parent.mkdir(parents=True, exist_ok=True)
    with database.open('a') as output:
        output.write('\n\nPackage: greetd\nStatus: install ok installed\nArchitecture: amd64\n'
                     'Version: 0.10.3-4\nMaintainer: Fixture <fixture@example.invalid>\n'
                     'Description: synthetic policy fixture\n')
        for name in ('restic', 'rclone'):
            output.write('\nPackage: ' + name + '\nStatus: install ok installed\nArchitecture: amd64\n'
                         'Version: 1\nMaintainer: Fixture <fixture@example.invalid>\n'
                         'Description: synthetic backup policy fixture\n')
        output.write('\nPackage: vlock\nStatus: install ok installed\nArchitecture: amd64\n'
                     'Version: 2.2.2-12\nMaintainer: Fixture <fixture@example.invalid>\n'
                     'Description: synthetic policy fixture\n')
    for relative in ('usr/sbin/greetd', 'usr/bin/wros-greeter', 'usr/bin/grimoire',
                     'usr/libexec/writingdeck-session', 'usr/libexec/writingdeck-keyboard',
                     'etc/pam.d/greetd', 'usr/share/doc/greetd/copyright', 'usr/share/doc/vlock/copyright',
                     'usr/libexec/wros-lock', 'usr/libexec/wros-backup', 'usr/bin/restic', 'usr/bin/rclone',
                     'usr/share/doc/restic/copyright', 'usr/share/doc/rclone/copyright',
                     'usr/bin/openvt', 'usr/bin/systemd-run', 'usr/bin/chvt', 'usr/sbin/vlock-main',
                     'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so',
                     'usr/lib/vlock/modules/all.so', 'usr/lib/vlock/modules/nosysrq.so'):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('synthetic policy fixture; not an executable or PAM stack\n')
        path.chmod(0o755 if relative.startswith('usr/') and 'copyright' not in relative else 0o644)
    (root / 'usr/sbin/vlock-main').chmod(0o4755)
    shutil.copyfile(ROOT / 'os/scripts/wros-lock', root / 'usr/libexec/wros-lock')
    shutil.copyfile(ROOT / 'os/scripts/wros-backup', root / 'usr/libexec/wros-backup')
    for source, relative in (
            ('greetd.toml', 'etc/greetd/config.toml'),
            ('greetd.service', 'etc/systemd/system/greetd.service'),
            ('51-wros-greeter-keyboard.rules', 'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules'),
            ('50-wros-lock.rules', 'etc/polkit-1/rules.d/50-wros-lock.rules'),
            ('org.wros.lock.policy', 'usr/share/polkit-1/actions/org.wros.lock.policy'),
            ('vlock.pam', 'etc/pam.d/vlock'),
            ('60-wros-power-guard.conf', 'etc/systemd/logind.conf.d/60-wros-power-guard.conf'),
            ('60-wros-sleep-guard.conf', 'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf'),
            ('60-wros-no-connectivity-check.conf', 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf'),
            ('60-wros-reboot-guard.conf', 'etc/systemd/system.conf.d/60-wros-reboot-guard.conf'),
            ('60-wros-sysrq.conf', 'etc/sysctl.d/60-wros-sysrq.conf')):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / 'os/config' / source, path)
    for name in ('passwd', 'shadow'):
        with (root / 'etc' / name).open('a') as output:
            output.write('_greetd:x:995:995::/nonexistent:/usr/sbin/nologin\n' if name == 'passwd'
                         else '_greetd:!:1:0:99999:7:::\n')
    enabled = root / 'etc/systemd/system/multi-user.target.wants/greetd.service'
    enabled.parent.mkdir(parents=True)
    enabled.symlink_to('../greetd.service')
    for name in ('getty@tty1.service', 'getty@tty8.service', 'autovt@tty8.service',
                 *(target + '.target' for target in module.SLEEP_TARGETS)):
        (root / 'etc/systemd/system' / name).symlink_to('/dev/null')


class NativeImage(unittest.TestCase):
    def test_complete_policy_has_no_autologin_and_does_not_claim_secure_locking(self):
        with tempfile.TemporaryDirectory(prefix='wros-native-policy-') as tmp:
            root = Path(tmp)
            fixture(root)
            result = module.audit(root)
            self.assertTrue(result['passed'])
            self.assertFalse(result['autologin'])
            self.assertFalse(result['secure_locking_verified'])
            self.assertEqual(result['greetd_package_version'], '0.10.3-4')
            self.assertEqual(result['vlock_package_version'], '2.2.2-12')
            self.assertTrue(result['manual_lock_payload_present'])
            self.assertEqual(len(result['binary_sha256']), 16)

    def test_unsafe_native_session_cannot_pass_even_with_all_requested_packages(self):
        mutations = {
            'autologin': lambda r: (r / 'etc/greetd/config.toml').write_text(
                (r / 'etc/greetd/config.toml').read_text() + '\n[initial_session]\nuser="root"\ncommand="sh"\n'),
            'root greeter': lambda r: (r / 'etc/passwd').write_text(
                '_greetd:x:0:0::/root:/bin/bash\n'),
            'unlocked greeter': lambda r: (r / 'etc/shadow').write_text('_greetd::1:0:99999:7:::\n'),
            'login shell': lambda r: (r / 'etc/passwd').write_text('_greetd:x:995:995::/tmp:/bin/bash\n'),
            'setuid editor': lambda r: (r / 'usr/bin/grimoire').chmod(0o4755),
            'writable greeter': lambda r: (r / 'usr/bin/wros-greeter').chmod(0o777),
            'missing PAM': lambda r: (r / 'etc/pam.d/greetd').unlink(),
            'missing notice': lambda r: (r / 'usr/share/doc/greetd/copyright').unlink(),
            'missing backup engine': lambda r: (r / 'usr/bin/restic').unlink(),
            'missing cloud engine': lambda r: (r / 'usr/bin/rclone').unlink(),
            'missing backup notice': lambda r: (r / 'usr/share/doc/restic/copyright').unlink(),
            'changed backup integration': lambda r: (r / 'usr/libexec/wros-backup').write_text('not the qualified integration'),
            'disabled service': lambda r: (r / 'etc/systemd/system/multi-user.target.wants/greetd.service').unlink(),
            'no tty1 mask': lambda r: (r / 'etc/systemd/system/getty@tty1.service').unlink(),
            'unavailable maintenance': lambda r: (r / 'etc/systemd/system/getty@tty2.service').symlink_to('/dev/null'),
            'altered service': lambda r: (r / 'etc/systemd/system/greetd.service').write_text('[Service]\nExecStart=/bin/sh\n'),
            'unprotected lid': lambda r: (r / 'etc/systemd/logind.conf.d/60-wros-power-guard.conf').unlink(),
            'reboot burst allowed': lambda r: (r / 'etc/systemd/system.conf.d/60-wros-reboot-guard.conf').unlink(),
            'sysrq left on': lambda r: (r / 'etc/sysctl.d/60-wros-sysrq.conf').unlink(),
            'network probe enabled': lambda r: (r / 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf').unlink(),
            'changed network probe': lambda r: (r / 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf').write_text(
                '[connectivity]\nenabled=true\nuri=http://network-test.debian.org/nm\n'),
            'missing locker': lambda r: (r / 'usr/sbin/vlock-main').unlink(),
            'writable locker module': lambda r: (r / 'usr/lib/vlock/modules/all.so').chmod(0o777),
            'missing lock PAM': lambda r: (r / 'etc/pam.d/vlock').unlink(),
            'missing PAM guard': lambda r: (r / 'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so').unlink(),
            'changed lock PAM': lambda r: (r / 'etc/pam.d/vlock').write_text('auth sufficient pam_permit.so\n'),
            'changed lock helper': lambda r: (r / 'usr/libexec/wros-lock').write_text('#!/bin/sh\nexit 0\n'),
            'missing lock policy': lambda r: (r / 'usr/share/polkit-1/actions/org.wros.lock.policy').unlink(),
            'unreserved tty8': lambda r: (r / 'etc/systemd/system/getty@tty8.service').unlink(),
            'unmasked automatic tty8': lambda r: (r / 'etc/systemd/system/autovt@tty8.service').unlink(),
        }
        for target in module.SLEEP_TARGETS:
            mutations['unmasked ' + target] = lambda r, t=target: (r / 'etc/systemd/system' / (t + '.target')).unlink()
        for name, mutate in mutations.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory(prefix='wros-native-refusal-') as tmp:
                root = Path(tmp)
                fixture(root)
                mutate(root)
                with self.assertRaises((ValueError, OSError)):
                    module.audit(root)

    def test_weakened_policy_values_are_refused_even_when_the_file_is_present(self):
        # A faithful copy of a weakened source file must still fail: values are checked, not just bytes.
        weaken = {
            'etc/systemd/logind.conf.d/60-wros-power-guard.conf': [
                ('HandleLidSwitchDocked=ignore', 'HandleLidSwitchDocked=suspend'),
                ('HandleHibernateKey=ignore', ''),
                ('IdleAction=ignore', 'IdleAction=poweroff'),
                ('HandlePowerKeyLongPress=ignore', 'HandlePowerKeyLongPress=poweroff')],
            'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf': [
                ('AllowHybridSleep=no', 'AllowHybridSleep=yes'),
                ('AllowSuspend=no', '[Sleep]')],
            'etc/systemd/system.conf.d/60-wros-reboot-guard.conf': [
                ('CtrlAltDelBurstAction=none', 'CtrlAltDelBurstAction=reboot-force')],
            'etc/sysctl.d/60-wros-sysrq.conf': [('kernel.sysrq = 0', 'kernel.sysrq = 1')],
            'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf': [('enabled=false', 'enabled=true')],
        }
        for relative, edits in weaken.items():
            for old, new in edits:
                with self.subTest(relative=relative, old=old), tempfile.TemporaryDirectory(prefix='wros-native-weak-') as tmp:
                    root = Path(tmp)
                    fixture(root)
                    path = root / relative
                    text = path.read_text()
                    self.assertIn(old, text)
                    path.write_text(text.replace(old, new))
                    with self.assertRaisesRegex(ValueError, 'policy value'):
                        module.audit(root)

    def test_live_root_and_symlinked_image_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'never the live root'):
            module.audit(Path('/'))
        with tempfile.TemporaryDirectory(prefix='wros-native-symlink-') as tmp:
            link = Path(tmp) / 'image'
            link.symlink_to('/')
            with self.assertRaisesRegex(ValueError, 'never the live root'):
                module.audit(link)
