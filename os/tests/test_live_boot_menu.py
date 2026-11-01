import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('live_boot_menu', ROOT / 'tools/live-boot-menu.py')
menu = importlib.util.module_from_spec(spec)
spec.loader.exec_module(menu)
audit_spec = importlib.util.spec_from_file_location('installer_audit', ROOT / 'tools/audit-installer-media.py')
audit = importlib.util.module_from_spec(audit_spec)
audit_spec.loader.exec_module(audit)

KERNEL = '/live/vmlinuz-6.12.113+deb13-amd64'
INITRD = '/live/initrd.img-6.12.113+deb13-amd64'
PRESEED = 'file=/cdrom/install/home/builder/release/os/config/installer.preseed'
# What live-build 1:20250505+deb13u1 wrote on the 0.1.0~rc1 medium (trimmed comments kept).
GRUB = f'''source /boot/grub/config.cfg

# Live boot
menuentry "Live system (amd64)" --hotkey=l {{
\tlinux\t{KERNEL} boot=live components wros.live=1 findiso=${{iso_path}}
\tinitrd\t{INITRD}
}}
menuentry "Live system (amd64 fail-safe mode)" {{
\tlinux\t{KERNEL} boot=live components wros.live=1 memtest noapic noapm nodma nomce nosmp nosplash vga=788
\tinitrd\t{INITRD}
}}

# You can add more entries like this
# menuentry "Alternate textual installer" {{
# linux /install/vmlinuz vga=normal {PRESEED} --- quiet custom options here
# initrd /install/initrd.gz
# }}

# Installer (if any)
if true; then

source\t/boot/grub/install_start.cfg

submenu 'Advanced install options ...' --hotkey=a {{

\tsource /boot/grub/theme.cfg

\tsource\t/boot/grub/install.cfg

}}
fi

submenu 'Utilities...' --hotkey=u {{

\tsource /boot/grub/theme.cfg

\t# Firmware setup (UEFI)
\tif [ "${{grub_platform}}" = "efi" ]; then
\t\tmenuentry "UEFI Firmware Settings" --hotkey=f {{
\t\t\tfwsetup
\t\t}}
\tfi

\t# Verify the checksums
\tif true; then
\t\tmenuentry "Verify integrity of the boot medium" --hotkey=v {{
\t\t\tlinux\t{KERNEL} boot=live components wros.live=1 findiso=${{iso_path}} verify-checksums
\t\t\tinitrd\t{INITRD}
\t\t}}
\tfi
}}
'''
LIVE = '''label live-amd64
\tmenu label ^Live system (amd64)
\tmenu default
\tlinux /live/vmlinuz
\tinitrd /live/initrd.img
\tappend boot=live components wros.live=1

label live-amd64-failsafe
\tmenu label Live system (amd64 fail-safe mode)
\tlinux /live/vmlinuz
\tinitrd /live/initrd.img
\tappend boot=live components wros.live=1 memtest noapic noapm nodma nomce nosmp nosplash vga=788
'''
MENU = '''menu hshift 0
menu width 82

menu title Boot menu\x07
include stdmenu.cfg
include live.cfg
include install.cfg
menu begin utilities
\tmenu label ^Utilities
\tmenu title Utilities
\tinclude stdmenu.cfg
\tlabel mainmenu
\t\tmenu label ^Back..
\t\tmenu exit
\tinclude utilities.cfg
menu end

menu clear
'''
FILES = {
    'boot/grub/grub.cfg': GRUB,
    'boot/grub/loopback.cfg': 'source /boot/grub/grub.cfg\n',
    'boot/grub/install_start.cfg': f"menuentry 'Start installer' --hotkey=i {{\n\tlinux\t/install/vmlinuz {PRESEED}\n}}\n",
    'boot/grub/install.cfg': f"menuentry 'Install' --hotkey=i {{\n\tlinux\t/install/vmlinuz {PRESEED}\n}}\n",
    'isolinux/isolinux.cfg': 'include menu.cfg\ndefault vesamenu.c32\nprompt 0\ntimeout 0\n',
    'isolinux/menu.cfg': MENU,
    'isolinux/live.cfg': LIVE,
    'isolinux/install.cfg': f'label installstart\n\tmenu label Start ^installer\n\tlinux /install/vmlinuz\n\tappend {PRESEED}\n',
    'isolinux/utilities.cfg': 'label hdt\n\tmenu label ^Hardware Detection Tool (HDT)\n\tcom32 hdt.c32\n',
}


class LiveBootMenu(unittest.TestCase):
    def medium(self, tmp, files=FILES):
        binary = Path(tmp) / 'binary'
        for name, text in files.items():
            (binary / name).parent.mkdir(parents=True, exist_ok=True)
            (binary / name).write_text(text)
        (binary / 'install').mkdir()
        (binary / 'install/initrd.gz').write_bytes(b'kept for the native installer')
        return binary

    def configs(self, binary):
        return {str(p.relative_to(binary)): p.read_text() for p in binary.rglob('*.cfg')}

    def test_keeps_only_the_live_entries_and_utilities_and_the_audit_accepts_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = self.medium(tmp)
            menu.main(['live-boot-menu.py', str(binary)])
            grub = (binary / 'boot/grub/grub.cfg').read_text()
            self.assertEqual(grub.count('menuentry "Start WrOS'), 2)
            self.assertIn(f'menuentry "{menu.LIVE_LABEL}" --hotkey=s {{\n'
                          f'\tlinux\t{KERNEL} boot=live components wros.live=1 findiso=${{iso_path}}\n'
                          f'\tinitrd\t{INITRD}\n}}\n', grub)
            self.assertIn('menuentry "Verify integrity of the boot medium"', grub)
            self.assertIn('fwsetup', grub)
            self.assertNotIn('/install/', grub)
            self.assertNotIn('install.cfg', grub)
            self.assertNotIn('install_start', grub)
            for name in menu.INSTALLER_FILES:
                self.assertFalse((binary / name).exists(), name)
            self.assertIn(f'menu label ^{menu.LIVE_LABEL}\n\tmenu default\n', (binary / 'isolinux/live.cfg').read_text())
            self.assertNotIn('install.cfg', (binary / 'isolinux/menu.cfg').read_text())
            self.assertIn('menu title WrOS\x07\n', (binary / 'isolinux/menu.cfg').read_text())
            # The installer's own files stay: the native installer lays its capsule over them.
            self.assertTrue((binary / 'install/initrd.gz').is_file())
            entries = audit.check_live_boot_menu(self.configs(binary))['entries']
            self.assertIn(menu.LIVE_LABEL, entries)
            self.assertIn(menu.FAILSAFE_LABEL, entries)

    def test_the_audit_rejects_the_rc1_menus(self):
        with tempfile.TemporaryDirectory() as tmp:
            configs = self.configs(self.medium(tmp))
        with self.assertRaisesRegex(ValueError, 'stock Debian Installer menu file'):
            audit.check_live_boot_menu(configs)
        for name in audit.STOCK_INSTALLER_MENUS:
            del configs[name]
        with self.assertRaisesRegex(ValueError, r'boots the stock Debian Installer \(/install/\)'):
            audit.check_live_boot_menu(configs)

    def test_refuses_menus_live_build_did_not_write(self):
        for name, old, new in (('boot/grub/grub.cfg', '--hotkey=l', '--hotkey=x'),
                               ('isolinux/menu.cfg', 'include install.cfg\n', ''),
                               ('isolinux/live.cfg', '^Live system (amd64)', 'Live')):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                binary = self.medium(tmp, {**FILES, name: FILES[name].replace(old, new)})
                with self.assertRaises(ValueError):
                    menu.main(['live-boot-menu.py', str(binary)])
                # Nothing is half-rewritten on refusal.
                self.assertEqual((binary / 'boot/grub/grub.cfg').read_text(), FILES['boot/grub/grub.cfg']
                                 if name != 'boot/grub/grub.cfg' else FILES[name].replace(old, new))
                self.assertTrue((binary / 'isolinux/install.cfg').exists())


if __name__ == '__main__':
    unittest.main()
