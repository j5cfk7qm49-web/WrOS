import gzip
import hashlib
import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
KEYMAPS = ROOT / 'os/config/installer/keymaps'
spec = importlib.util.spec_from_file_location('installer_keymaps', ROOT / 'tools/generate-installer-keymaps.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

STANZA = '''Template: keyboard-configuration/xkb-keymap
Type: select
Choices-C: us, ca(multix), cn, hr, us(dvorak)
Choices: American English, Canadian Multilingual, Chinese, Croatian, Dvorak
Choices-de.UTF-8: Amerikanisches Englisch, Kanadisch\\, mehrsprachig, Chinesisch, Kroatisch, Dvorak
Description: Keymap to use:
'''
# The upstream ckbcomp-mini selection, reduced to what one flat map needs.
CKBCOMP_MINI = '''#!/bin/sh
echo 'keymaps 0-4,6,8,10,12,14'
zcat "$EKMAP" | grep "^$1:$2::" | sed "s/^$1:$2:://"
echo 'strings as usual'
'''


class Overlay(unittest.TestCase):
    def test_colemak_choices_follow_chinese_in_every_choices_field(self):
        stanza = module.overlay('Template: other/question\nType: note\n\n' + STANZA)
        fields = {line.split(': ', 1)[0]: module.split_choices(line.split(': ', 1)[1])
                  for line in stanza.splitlines() if line.startswith('Choices')}
        self.assertEqual(fields['Choices-C'], ['us', 'ca(multix)', 'cn', 'us(colemak)', 'us(colemak_dh)',
                                               'us(colemak_dh_iso)', 'hr', 'us(dvorak)'])
        self.assertEqual(fields['Choices'][3:6], ['Colemak', 'Colemak Mod-DH (ANSI)', 'Colemak Mod-DH (ISO)'])
        self.assertEqual(fields['Choices-de.UTF-8'][1], 'Kanadisch\\, mehrsprachig')
        self.assertEqual(fields['Choices-de.UTF-8'][6], 'Kroatisch')
        self.assertTrue(stanza.startswith('Template: keyboard-configuration/xkb-keymap\n'))
        self.assertIn('Description: Keymap to use:\n', stanza)
        self.assertNotIn('other/question', stanza)

    def test_misaligned_already_extended_or_missing_questions_are_refused(self):
        for broken in (STANZA.replace(', Dvorak\n', '\n', 1),
                       STANZA.replace('us(dvorak)', 'us(colemak)'),
                       STANZA.replace(' cn,', ' tw,'),
                       STANZA.replace('xkb-keymap', 'layoutcode')):
            with self.subTest(broken=broken.splitlines()[2][:40]), self.assertRaises(ValueError):
                module.overlay(broken)

    def test_committed_overlay_keeps_upstream_alignment(self):
        stanza = (KEYMAPS / 'xkb-keymap.templates').read_text(encoding='utf-8')
        fields = [module.split_choices(line.split(': ', 1)[1]) for line in stanza.splitlines()
                  if line.startswith('Choices')]
        self.assertGreater(len(fields), 50)
        self.assertEqual({len(choices) for choices in fields}, {len(fields[0])})
        codes = next(module.split_choices(line.split(': ', 1)[1]) for line in stanza.splitlines()
                     if line.startswith('Choices-C: '))
        at = codes.index('us(colemak)')
        self.assertEqual(codes[at - 1:at + 4], ['cn', 'us(colemak)', 'us(colemak_dh)', 'us(colemak_dh_iso)', 'hr'])
        entries = gzip.decompress((KEYMAPS / 'colemak.ekmap.gz').read_bytes()).decode().splitlines()
        self.assertEqual({line.split('::', 1)[0] for line in entries},
                         {'us:colemak', 'us:colemak_dh', 'us:colemak_dh_iso'})
        # Physical key D types s in all three.
        for variant in ('colemak', 'colemak_dh', 'colemak_dh_iso'):
            self.assertIn(f'us:{variant}::keycode 32 = +U+0073 +U+0053', '\n'.join(entries))


class StartupHook(unittest.TestCase):
    def run_hook(self, root, sums=None):
        bin_dir = root / 'bin'
        ekmap = root / 'usr/share/console-setup/pc105.ekmap.gz'
        if not ekmap.exists():
            ekmap.parent.mkdir(parents=True, exist_ok=True)
            ekmap.write_bytes(gzip.compress(b'us:::keycode 32 = +U+0064 +U+0044\n'))
        data = root / 'usr/share/wros-keymaps'
        if not data.exists():
            shutil.copytree(KEYMAPS, data)
        if sums is not None:
            (data / 'keymaps.sha256').write_text(sums)
        bin_dir.mkdir(exist_ok=True)
        for name, body in (('ckbcomp-mini', CKBCOMP_MINI),
                           ('debconf-loadtemplate', f'#!/bin/sh\necho "$*" >> {root}/loaded\n'),
                           ('logger', f'#!/bin/sh\necho "$*" >> {root}/log\n')):
            (bin_dir / name).write_text(body)
            (bin_dir / name).chmod(0o755)
        env = {'PATH': f'{bin_dir}:/usr/bin:/bin', 'WROS_KEYMAPS_ROOT': str(root),
               'EKMAP': str(root / 'usr/share/console-setup/pc105.ekmap.gz')}
        subprocess.run(['sh', str(KEYMAPS / 'S21wros-keymaps')], env=env, check=True)
        loaded = (root / 'loaded').read_text() if (root / 'loaded').exists() else ''
        return loaded, (root / 'log').read_text()

    def test_offers_colemak_only_when_every_map_loads_as_generated(self):
        with tempfile.TemporaryDirectory(prefix='wros-keymaps-hook-') as tmp:
            root = Path(tmp)
            ekmap = root / 'usr/share/console-setup/pc105.ekmap.gz'
            loaded, log = self.run_hook(root)
            self.assertEqual(loaded, f'd-i {root}/usr/share/wros-keymaps/xkb-keymap.templates\n')
            self.assertIn('Colemak keymaps offered', log)
            lines = gzip.decompress(ekmap.read_bytes()).decode().splitlines()
            self.assertEqual(lines[0], 'us:::keycode 32 = +U+0064 +U+0044')
            count = len(lines)
            self.run_hook(root)
            self.assertEqual(len(gzip.decompress(ekmap.read_bytes()).decode().splitlines()), count)

    def test_a_map_that_does_not_load_byte_for_byte_is_never_offered(self):
        with tempfile.TemporaryDirectory(prefix='wros-keymaps-refuse-') as tmp:
            root = Path(tmp)
            sums = (KEYMAPS / 'keymaps.sha256').read_text().replace('colemak_dh\n', 'colemak_dh_iso\n', 1)
            loaded, log = self.run_hook(root, sums=sums)
            self.assertEqual(loaded, '')
            self.assertIn('us(colemak_dh_iso) does not load as generated', log)

    def test_committed_sums_match_the_committed_maps(self):
        entries = gzip.decompress((KEYMAPS / 'colemak.ekmap.gz').read_bytes()).decode().splitlines()
        for line in (KEYMAPS / 'keymaps.sha256').read_text().splitlines():
            digest, variant = line.split('  ')
            body = [entry.split('::', 1)[1] for entry in entries if entry.startswith(f'us:{variant}::')]
            mini = '\n'.join(['keymaps 0-4,6,8,10,12,14', *body, 'strings as usual']) + '\n'
            self.assertEqual(hashlib.sha256(mini.encode()).hexdigest(), digest)


class FinishHook(unittest.TestCase):
    def test_installed_initramfs_loads_the_console_keymap(self):
        with tempfile.TemporaryDirectory(prefix='wros-keymaps-finish-') as tmp:
            subprocess.run(['sh', str(KEYMAPS / '13wros-initramfs-keymap')], env={'WROS_KEYMAPS_ROOT': tmp,
                           'PATH': '/usr/bin:/bin'}, check=True)
            conf = Path(tmp) / 'target/etc/initramfs-tools/conf.d/wros-keymap'
            self.assertIn('\nKEYMAP=y\n', '\n' + conf.read_text())


if __name__ == '__main__':
    unittest.main()
