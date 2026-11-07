# Boot screen (#89)

While the laptop starts, the screen shows only the WrOS logo and a loading bar.
When the disk asks for its password, the prompt takes the bar's place, with a dot
for each key. Esc shows the boot messages, as with any Plymouth theme.

The theme is a Plymouth script theme in `os/plymouth/wros`, installed to
`/usr/share/plymouth/themes/wros`. Its words are pictures, drawn by
`tools/make-bootscreen.py` from DejaVu Sans Bold: Plymouth draws text only with a
plugin the boot image may not contain. The logo is a placeholder wordmark until
WrOS has a real one; replace `logo.png` (any size) and rebuild the package.

## Where it shows

WrOS doesn't install Plymouth, because updates carry no Debian packages. On a
computer with Plymouth installed:

- the package's postinst makes `wros` the default theme once and rebuilds the boot
  image (`update-initramfs -u`), which takes a while on the laptop;
- `/etc/default/grub.d/60-wros.cfg` adds `splash` to the kernel command line.

Without Plymouth nothing changes: boot stays quiet text, as since rc6. The
recovery boot entry never shows the theme, so its messages stay readable.

## The keyboard at the password prompt

Plymouth reads the disk password through the boot image's keyboard setup, the
same one the text prompt uses, so the writer's Colemak layout should carry over.
This is unchecked on the laptop: see section 10 of the hardware checklist.
