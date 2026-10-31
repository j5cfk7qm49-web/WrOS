"""Pure live-session planning and private files only; never host accounts/auth."""
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import stat
import shutil
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader('wros_live_session', str(ROOT/'os/scripts/wros-live-session'))
spec = importlib.util.spec_from_loader(loader.name, loader)
live = importlib.util.module_from_spec(spec)
loader.exec_module(live)
BASE = (ROOT/'os/config/greetd.toml').read_bytes()
PASSWD = 'root:x:0:0:root:/root:/bin/bash\nwros-live:x:1000:1000:Live:/home/wros-live:/bin/bash\n'
BOOT = '01234567-89ab-cdef-0123-456789abcdef'


def plan(**overrides):
    values = dict(base=BASE, passwd=PASSWD, user_status='wros-live P', root_status='root L',
                  cmdline='boot=live components wros.live=1 quiet', boot_id=BOOT)
    values.update(overrides)
    return live.render(**values)


class LiveSession(unittest.TestCase):
    def test_temporary_initial_entry_retains_original_authenticated_default(self):
        files = plan()
        config = tomllib.loads(files['greetd.toml'][0].decode())
        self.assertEqual(config['initial_session'], {'user':'wros-live','command':'/usr/libexec/writingdeck-session'})
        del config['initial_session']
        self.assertEqual(config, tomllib.loads(BASE.decode()))
        self.assertEqual((ROOT/'os/config/greetd.toml').read_bytes(), BASE)
        marker = json.loads(files['session.json'][0])
        self.assertEqual((marker['uid'], marker['user'], marker['boot_id']), (1000, 'wros-live', BOOT))
        self.assertTrue(marker['automatic'])
        self.assertNotIn(b'password', b''.join(value[0] for value in files.values()).lower())

    def test_normal_and_incomplete_boot_selection_have_no_plan(self):
        for cmdline in ('quiet', 'boot=live', 'wros.live=1', 'boot=normal wros.live=1', 'boot=live wros.live=10'):
            with self.subTest(cmdline=cmdline):
                self.assertIsNone(plan(cmdline=cmdline))

    def test_conflicting_boot_flags_are_rejected(self):
        for extra in ('boot=normal','boot=live','wros.live=0','wros.live=1'):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                plan(cmdline='boot=live wros.live=1 '+extra)

    def test_all_documented_manual_options_preserve_base_config_exactly(self):
        for option in live.MANUAL:
            with self.subTest(option=option):
                files = plan(cmdline='boot=live wros.live=1 '+option)
                self.assertEqual(files['greetd.toml'][0], BASE)
                self.assertFalse(json.loads(files['session.json'][0])['automatic'])
        self.assertEqual(plan(manual=True)['greetd.toml'][0], BASE)

    def test_personal_root_duplicate_or_invalid_accounts_cannot_become_live_entries(self):
        for passwd in (PASSWD.replace('wros-live:', 'writer:'),
                       PASSWD.replace('1000:1000','0:0'),
                       PASSWD.replace('1000:1000','1001:1001'),
                       PASSWD+'other:x:1000:1000:Other:/home/other:/bin/bash\n',
                       PASSWD.replace('/home/wros-live','/home/personal'),
                       PASSWD.replace(':/bin/bash\n',':/usr/sbin/nologin\n')):
            with self.subTest(passwd=passwd), self.assertRaises(ValueError):
                plan(passwd=passwd)
        for values in ({'root_status':'root P'}, {'user_status':'wros-live L'},
                       {'user_status':'writer P'}, {'boot_id':'invalid'}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                plan(**values)

    def test_preexisting_shipping_initial_entry_is_rejected(self):
        with self.assertRaises(ValueError):
            plan(base=BASE+b'\n[initial_session]\nuser="writer"\ncommand="/bin/sh"\n')

    def test_runtime_files_are_private_idempotent_and_repair_only_missing_expected_files(self):
        with tempfile.TemporaryDirectory(prefix='wros-live-files-') as directory:
            target = Path(directory)/'run'
            files = plan()
            live.install_runtime(target, files, owner=os.geteuid())
            before = {name: (target/name).read_bytes() for name in files}
            live.install_runtime(target, files, owner=os.geteuid())
            self.assertEqual(before, {name: (target/name).read_bytes() for name in files})
            for name, (_, mode) in files.items():
                self.assertEqual(stat.S_IMODE((target/name).stat().st_mode), mode)
            (target/'greetd.env').unlink()
            live.install_runtime(target, files, owner=os.geteuid())
            self.assertEqual((target/'greetd.env').read_bytes(), files['greetd.env'][0])
            self.assertEqual(set(p.name for p in target.iterdir()), set(files))

    def test_unknown_existing_or_symlink_files_are_preserved_before_any_publish(self):
        for symlink in (False, True):
            with self.subTest(symlink=symlink), tempfile.TemporaryDirectory(prefix='wros-live-preserve-') as directory:
                root = Path(directory); target = root/'run'; target.mkdir()
                original = root/'original'; original.write_bytes(b'personal settings')
                if symlink:
                    (target/'greetd.env').symlink_to(original)
                else:
                    (target/'greetd.env').write_bytes(b'unknown existing settings')
                with self.assertRaises(ValueError):
                    live.install_runtime(target, plan(), owner=os.geteuid())
                self.assertEqual(original.read_bytes(), b'personal settings')
                self.assertFalse((target/'greetd.toml').exists())
                self.assertFalse((target/'session.json').exists())

    def test_writable_runtime_directory_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix='wros-live-permissions-') as directory:
            target = Path(directory)/'run'; target.mkdir(); target.chmod(0o777)
            with self.assertRaises(ValueError):
                live.install_runtime(target, plan(), owner=os.geteuid())
            self.assertEqual(list(target.iterdir()), [])


class LiveSessionImage(unittest.TestCase):
    @staticmethod
    def fixture(root):
        from test_native_image import fixture
        fixture(root)
        files = {
            'os/scripts/writingdeck-session': 'usr/libexec/writingdeck-session',
            'os/scripts/wros-live-session': 'usr/libexec/wros-live-session',
            'os/scripts/wros-install-session': 'usr/libexec/wros-install-session',
            'os/config/live/9998-wros-session': 'lib/live/config/9998-wros-session',
            'os/config/live/live.conf': 'etc/live/config.conf.d/90-wros.conf',
            'os/config/live/greetd-live.conf': 'etc/systemd/system/greetd.service.d/90-wros-live.conf',
            'os/config/50-wros-installer.rules': 'etc/polkit-1/rules.d/50-wros-installer.rules',
            'os/config/org.wros.installer.policy': 'usr/share/polkit-1/actions/org.wros.installer.policy',
            'os/config/installer.preseed': 'etc/wros/installer.preseed',
            'os/config/installer/native-crypto-passphrase': 'usr/lib/wros-installer/native-crypto-passphrase',
        }
        exec_sources = (
            'writingdeck-session',
            'wros-live-session',
            'wros-install-session',
            '9998-wros-session',
            'native-crypto-passphrase',
        )
        for source, target in files.items():
            path = root/target; path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT/source, path)
            path.chmod(0o755 if source.endswith(exec_sources) else 0o644)
        for target in ('usr/bin/wros-installer','usr/bin/live-config','usr/lib/user-setup/user-setup-apply'):
            path = root/target; path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'fixture executable'); path.chmod(0o755)
        mask = root/'etc/systemd/system-generators/live-config-getty-generator'
        mask.parent.mkdir(parents=True); mask.symlink_to('/dev/null')
        for unit in ('getty@tty7.service', 'autovt@tty7.service'):
            (root/'etc/systemd/system'/unit).symlink_to('/dev/null')

    @staticmethod
    def audit(root):
        spec = importlib.util.spec_from_file_location('wros_live_image_audit', ROOT/'tools/audit-live-session.py')
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        return module.audit(root)

    def test_complete_immutable_profile_has_no_runtime_account_or_initial_session(self):
        with tempfile.TemporaryDirectory(prefix='wros-live-image-') as directory:
            root = Path(directory); self.fixture(root)
            result = self.audit(root)
            self.assertTrue(result['passed'])
            self.assertTrue(result['temporary_account_absent'])
            self.assertTrue(result['installer_tty7_masked'])
            self.assertFalse(result['native_session']['autologin'])
            self.assertNotIn('initial_session', tomllib.loads((root/'etc/greetd/config.toml').read_text()))

    def test_partial_altered_or_runtime_profiles_are_rejected(self):
        def runtime(root):
            (root/'run/wros-live').mkdir(parents=True)
        def installer_runtime(root):
            (root/'run/wros-installer').mkdir(parents=True)
        def sudo(root):
            (root/'etc/sudoers.d').mkdir()
            (root/'etc/sudoers.d/live').write_text('fixture only\n')
        mutations = {
            'runtime': runtime, 'installer runtime': installer_runtime, 'sudo': sudo,
            'live user': lambda r: (r/'etc/passwd').write_text((r/'etc/passwd').read_text()+'wros-live:x:1000:1000::/home/wros-live:/bin/bash\n'),
            'reserved uid': lambda r: (r/'etc/passwd').write_text((r/'etc/passwd').read_text()+'other:x:1000:1000::/home/other:/bin/bash\n'),
            'active getty generator': lambda r: (r/'etc/systemd/system-generators/live-config-getty-generator').unlink(),
            'unmasked tty7': lambda r: (r/'etc/systemd/system/getty@tty7.service').unlink(),
            'altered helper': lambda r: (r/'usr/libexec/wros-live-session').write_text('fixture\n'),
            'altered installer supervisor': lambda r: (r/'usr/libexec/wros-install-session').write_text('fixture\n'),
            'altered preseed': lambda r: (r/'etc/wros/installer.preseed').write_text('d-i partman/confirm boolean true\n'),
            'nonexecutable helper': lambda r: (r/'usr/libexec/wros-live-session').chmod(0o644),
            'unexpected override': lambda r: (r/'etc/systemd/system/greetd.service.d/99-other.conf').write_text('[Service]\nExecStart=/bin/sh\n'),
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory(prefix='wros-live-reject-') as directory:
                root = Path(directory); self.fixture(root); mutation(root)
                with self.assertRaises((ValueError, FileNotFoundError)):
                    self.audit(root)
