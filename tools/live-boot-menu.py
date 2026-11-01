#!/usr/bin/env python3
"""Rewrite live-build's boot menus on the WrOS live medium before its ISO is made.

`--debian-installer live` puts Debian Installer's kernel, initrd and pool on the
medium, which the native installer needs, but it also adds GRUB and isolinux
entries that boot the stock Debian Installer. WrOS installs from the live
session, so those entries only lead to a plain Debian install (and to a preseed
path that does not exist on the medium). This keeps the two live entries, under
names that say what they do, and the Utilities submenu; it removes every
installer entry and refuses if live-build's menus are not shaped as expected.

Usage: live-boot-menu.py BINARY_DIR   (live-build's binary/ tree)
"""
import re
import sys
from pathlib import Path

LIVE_LABEL = 'Start WrOS (try it, or install it from the dashboard)'
FAILSAFE_LABEL = 'Start WrOS (fail-safe mode)'
INSTALLER_FILES = ('boot/grub/install.cfg', 'boot/grub/install_start.cfg', 'isolinux/install.cfg')
MENU_FILES = ('boot/grub/grub.cfg', 'boot/grub/loopback.cfg', 'isolinux/isolinux.cfg',
              'isolinux/menu.cfg', 'isolinux/live.cfg', 'isolinux/utilities.cfg')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def one(pattern, text, what):
    found = re.findall(pattern, text, re.MULTILINE | re.DOTALL)
    require(len(found) == 1, f'expected exactly one {what} in the live-build menu, found {len(found)}')
    return found[0]


def grub_menu(text):
    """Only the two live entries (bodies verbatim) and the Utilities submenu (verbatim)."""
    live = one(r'^menuentry "Live system \([^)"]*\)" --hotkey=l \{\n(.*?)^\}$', text, 'live GRUB entry')
    failsafe = one(r'^menuentry "Live system \([^)"]*fail-safe mode\)" \{\n(.*?)^\}$', text,
                   'fail-safe GRUB entry')
    utilities = one(r"^(submenu 'Utilities\.\.\.' --hotkey=u \{\n.*?^\}$)", text, 'Utilities submenu')
    for body in (live, failsafe):
        require(re.search(r'^\tlinux\t/live/vmlinuz\S* boot=live ', body, re.MULTILINE)
                and re.search(r'^\tinitrd\t/live/initrd\.img', body, re.MULTILINE),
                'a live GRUB entry does not boot the live kernel')
    return (f'source /boot/grub/config.cfg\n\n'
            f'menuentry "{LIVE_LABEL}" --hotkey=s {{\n{live}}}\n'
            f'menuentry "{FAILSAFE_LABEL}" {{\n{failsafe}}}\n\n'
            f'{utilities}\n')


def isolinux_live(text):
    require(text.count('menu label ^Live system (amd64)\n') == 1
            and text.count('menu label Live system (amd64 fail-safe mode)\n') == 1,
            'isolinux live.cfg labels are not the ones live-build writes')
    return (text.replace('menu label ^Live system (amd64)\n', f'menu label ^{LIVE_LABEL}\n')
            .replace('menu label Live system (amd64 fail-safe mode)\n', f'menu label {FAILSAFE_LABEL}\n'))


def isolinux_menu(text):
    # live-build ends the title with a BEL (the menu's beep); keep whatever follows the words.
    titled, titles = re.subn(r'^menu title Boot menu(\x07?)$', r'menu title WrOS\1', text, flags=re.MULTILINE)
    require(text.count('include install.cfg\n') == 1 and titles == 1,
            'isolinux menu.cfg is not the one live-build writes')
    return titled.replace('include install.cfg\n', '')


def check(binary):
    """Every remaining menu boots the live system; nothing starts the stock installer."""
    for name in INSTALLER_FILES:
        require(not (binary / name).exists(), f'installer menu file left on the medium: {name}')
    for name in MENU_FILES:
        path = binary / name
        if not path.exists():
            continue
        text = path.read_text()
        for marker in ('/install/', 'install.cfg', 'install_start.cfg', 'file=/cdrom', 'preseed'):
            require(marker not in text, f'{name} still refers to the stock installer ({marker})')


def main(argv):
    require(len(argv) == 2, __doc__.strip().splitlines()[-1])
    binary = Path(argv[1])
    grub, live, menu = binary / 'boot/grub/grub.cfg', binary / 'isolinux/live.cfg', binary / 'isolinux/menu.cfg'
    new = {grub: grub_menu(grub.read_text()), live: isolinux_live(live.read_text()),
           menu: isolinux_menu(menu.read_text())}
    for path, text in new.items():
        path.write_text(text)
    for name in INSTALLER_FILES:
        (binary / name).unlink()
    check(binary)
    print(f'boot menus: two live entries ({LIVE_LABEL!r}), no stock installer entries')


if __name__ == '__main__':
    try:
        main(sys.argv)
    except ValueError as error:
        sys.exit(f'live-boot-menu: {error}')
