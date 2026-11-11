"""Start-screen choices use fixed files; rebuilding never blocks the writer."""
import importlib.machinery
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def helper():
    loader = importlib.machinery.SourceFileLoader('boot_settings', str(ROOT / 'os/scripts/wros-boot-settings'))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class BootSettings(unittest.TestCase):
    def setUp(self):
        self.module = helper()
        self.tmp = tempfile.TemporaryDirectory(prefix='wros-boot-settings-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.calls = []
        self.build_result = 0
        self.during_build = lambda: None

        def run(argv, **kwargs):
            self.calls.append(argv)
            if argv[0] == '/usr/sbin/update-initramfs':
                self.during_build()
                return subprocess.CompletedProcess(argv, self.build_result)
            return subprocess.CompletedProcess(argv, 0)

        self.settings = self.module.Settings(self.root / 'etc', self.root / 'state', run)

    def test_the_default_is_graphical_and_a_theme_preserves_the_choice(self):
        self.assertEqual(self.settings.status(), {'enabled': True, 'pending': False, 'failed': False})
        theme = 'name = "Nord"\n'
        status = self.settings.change('theme', theme)
        self.assertEqual((self.root / 'etc/boot-theme.toml').read_text(), theme)
        self.assertEqual((self.root / 'etc/boot-theme.toml').stat().st_mode & 0o777, 0o644)
        self.assertTrue(status['enabled'])
        self.assertTrue(status['pending'])
        self.assertFalse((self.root / 'etc/boot-screen').exists())
        self.assertEqual(self.calls, [['/usr/bin/systemctl', 'start', '--no-block', 'wros-boot-rebuild.path']])

    def test_the_choice_and_theme_reach_the_existing_boot_image_hook(self):
        theme = 'name = "Custom"\naccent = "#123456"\nborder = "#654321"\n'
        self.assertTrue(self.settings.change('screen', 'ratatui', theme)['enabled'])
        self.assertEqual((self.root / 'etc/boot-screen').read_text(), 'ratatui\n')
        self.settings.rebuild()
        self.assertEqual(self.calls[-1], ['/usr/sbin/update-initramfs', '-u', '-k', 'all'])
        self.assertEqual(self.settings.status(), {'enabled': True, 'pending': False, 'failed': False})
        self.assertFalse(self.settings.change('screen', 'text', theme)['enabled'])
        self.assertEqual((self.root / 'etc/boot-screen').read_text(), 'text\n')
        self.settings.change('theme', theme)
        self.assertFalse(self.settings.status()['enabled'])

    def test_changes_during_a_rebuild_remain_queued_for_the_next_build(self):
        self.settings.change('theme', 'name = "Nord"\n')
        self.during_build = lambda: self.settings.change('theme', 'name = "Dracula"\n')
        self.settings.rebuild()
        self.assertTrue(self.settings.status()['pending'])
        self.during_build = lambda: None
        self.settings.rebuild()
        self.assertFalse(self.settings.status()['pending'])
        self.assertEqual((self.root / 'etc/boot-theme.toml').read_text(), 'name = "Dracula"\n')

    def test_a_failed_rebuild_is_reported_and_can_be_retried(self):
        self.settings.change('screen', 'ratatui', 'name = "Nord"\n')
        self.build_result = 1
        with self.assertRaises(RuntimeError):
            self.settings.rebuild()
        self.assertEqual(self.settings.status(), {'enabled': True, 'pending': False, 'failed': True})
        self.settings.change('screen', 'ratatui', 'name = "Nord"\n')
        self.assertTrue(self.settings.status()['pending'])
        self.build_result = 0
        self.settings.rebuild()
        self.assertFalse(self.settings.status()['failed'])

    def test_an_interrupted_rebuild_is_resumed_from_its_marker(self):
        self.settings.change('theme', 'name = "Nord"\n')
        (self.root / 'state/pending').rename(self.root / 'state/building')
        self.settings.rebuild()
        self.assertFalse(self.settings.status()['pending'])
        self.assertEqual(self.calls[-1], ['/usr/sbin/update-initramfs', '-u', '-k', 'all'])

    def test_invalid_data_and_links_cannot_change_any_files_or_run_a_command(self):
        self.settings.change('theme', 'name = "Nord"\n')
        original = (self.root / 'etc/boot-theme.toml').read_bytes()
        self.calls.clear()
        for value in ('name = "Nord"\npath = "/etc/shadow"', 'name = "Custom"\naccent = "oops"',
                      'name = "a\\nb"', 'name = "Nord"\nname = "Dracula"', 'a' * 4097):
            with self.subTest(value=value[:80]), self.assertRaises(ValueError):
                self.settings.change('theme', value)
        with self.assertRaises(ValueError):
            self.settings.change('screen', '../../oops', 'name = "Nord"\n')
        self.assertEqual((self.root / 'etc/boot-theme.toml').read_bytes(), original)
        self.assertEqual(self.calls, [])
        victim = self.root / 'untouched'
        victim.write_text('unchanged')
        (self.root / 'etc/boot-theme.toml').unlink()
        (self.root / 'etc/boot-theme.toml').symlink_to(victim)
        with self.assertRaises(ValueError):
            self.settings.change('theme', 'name = "Dracula"\n')
        self.assertEqual(victim.read_text(), 'unchanged')
        self.assertEqual(self.calls, [])

    def test_polkit_and_background_units_ship_without_enabling_the_start_screen(self):
        import json
        from xml.etree import ElementTree
        policy = ElementTree.parse(ROOT / 'os/config/org.wros.boot-settings.policy')
        action = policy.find('action')
        self.assertEqual(action.attrib['id'], 'org.wros.boot-settings')
        self.assertEqual(action.find('annotate').text, '/usr/libexec/wros-boot-settings')
        self.assertTrue(all(item.text == 'no' for item in action.find('defaults')))
        rule = (ROOT / 'os/config/50-wros-boot-settings.rules').read_text()
        for term in ('subject.local', 'subject.active', 'subject.isInGroup("wros")',
                     'subject.isInGroup("writingdeck")'):
            self.assertIn(term, rule)
        service = (ROOT / 'os/systemd/wros-boot-rebuild.service').read_text()
        path = (ROOT / 'os/systemd/wros-boot-rebuild.path').read_text()
        self.assertIn('ExecStart=/usr/libexec/wros-boot-settings rebuild', service)
        self.assertIn('PathExists=/var/lib/wros/boot-screen/pending', path)
        self.assertNotIn('Conflicts=greetd', service + path)
        manifest = json.loads((ROOT / 'packaging/manifest.json').read_text())
        files = {entry.get('dest') for entry in manifest['files']}
        for dest in ('usr/libexec/wros-boot-settings', 'etc/systemd/system/wros-boot-rebuild.service',
                     'etc/systemd/system/wros-boot-rebuild.path'):
            self.assertIn(dest, files)
        self.assertNotIn('etc/wros/boot-screen', files)

    def test_pkexec_cannot_rebuild_or_change_settings_without_both_groups(self):
        with mock.patch.object(self.module.os, 'geteuid', return_value=0), mock.patch.dict(
                self.module.os.environ, {'PKEXEC_UID': '1000'}), mock.patch.object(
                self.module, 'Settings', return_value=self.settings), mock.patch.object(
                self.module.pwd, 'getpwuid', return_value=mock.Mock(pw_name='writer', pw_gid=1000)), mock.patch.object(
                self.module.grp, 'getgrnam', side_effect=lambda name: mock.Mock(gr_gid={'wros': 998, 'writingdeck': 999}[name])
        ), mock.patch.object(self.module.os, 'getgrouplist', return_value=[1000, 998]) as groups:
            with self.assertRaisesRegex(ValueError, 'needs authorization'):
                self.module.main(['theme', 'name = "Nord"'])
            groups.return_value.append(999)
            with self.assertRaisesRegex(ValueError, 'needs authorization'):
                self.module.main(['rebuild'])
            self.assertEqual(self.calls, [])


if __name__ == '__main__':
    unittest.main()
