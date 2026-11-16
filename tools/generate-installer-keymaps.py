#!/usr/bin/env python3
"""Generate the WrOS installer's Colemak keymap overlay from Debian's own data.

Debian Installer's keymap question (keyboard-configuration/xkb-keymap in
console-setup-udeb) offers Dvorak but not Colemak, and its precompiled console
maps (pc105.ekmap.gz in console-setup-pc-ekmap) have no Colemak entries. This
writes, into OUTPUT_DIR:

  xkb-keymap.templates  the maintained question with us(colemak), us(colemak_dh)
                        and us(colemak_dh_iso) inserted after Chinese, in every
                        Choices field at the same position
  colemak.ekmap.gz      those three maps in ekmap form, from Debian's ckbcomp
  keymaps.sha256        the exact ckbcomp-mini output each map must produce

Usage: generate-installer-keymaps.py CONSOLE_SETUP_UDEB OUTPUT_DIR
"""
import gzip
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import tempfile

TEMPLATE = 'keyboard-configuration/xkb-keymap'
AFTER = 'cn'
VARIANTS = (('colemak', 'Colemak'), ('colemak_dh', 'Colemak Mod-DH (ANSI)'),
            ('colemak_dh_iso', 'Colemak Mod-DH (ISO)'))
# ckbcomp-mini wraps the selected ekmap lines in exactly these two lines.
MINI_HEADER, MINI_FOOTER = 'keymaps 0-4,6,8,10,12,14', 'strings as usual'


def stanza(templates_text, name):
    for block in re.split(r'\n\s*\n', templates_text):
        if block.startswith(f'Template: {name}\n'):
            return block.rstrip('\n')
    raise ValueError(f'{name} is not in the console-setup-udeb templates')


def split_choices(value):
    # debconf separates choices with ", " and escapes literal commas as "\,".
    return re.split(r'(?<!\\), ', value)


def overlay(templates_text):
    """The xkb-keymap stanza with the Colemak choices added; every other field unchanged."""
    lines = stanza(templates_text, TEMPLATE).split('\n')
    fields = [line for line in lines if re.match(r'(__)?Choices(-[^:]+)?: ', line)]
    keys = {line.split(': ', 1)[0]: split_choices(line.split(': ', 1)[1]) for line in fields}
    codes = keys.get('Choices-C')
    if codes is None or AFTER not in codes or any(f'us({v})' in codes for v, _ in VARIANTS):
        raise ValueError('unexpected Choices-C in the maintained keymap question')
    if any(len(choices) != len(codes) for choices in keys.values()):
        raise ValueError('maintained keymap choices are not aligned')
    at = codes.index(AFTER) + 1
    out = []
    for line in lines:
        key, _, value = line.partition(': ')
        if line in fields:
            added = [f'us({v})' for v, _ in VARIANTS] if key == 'Choices-C' else [label for _, label in VARIANTS]
            choices = split_choices(value)
            line = f'{key}: ' + ', '.join(choices[:at] + added + choices[at:])
        out.append(line)
    return '\n'.join(out) + '\n'


def ekmap_lines(variant, ckbcomp='ckbcomp'):
    output = subprocess.run([ckbcomp, '-compact', '-model', 'pc105', 'us', variant],
                            check=True, capture_output=True, text=True).stdout
    keycodes = [line for line in output.splitlines() if line.startswith('keycode ')]
    if len(keycodes) < 100 or output.splitlines()[0] != MINI_HEADER:
        raise ValueError(f'ckbcomp produced an unexpected map for us({variant})')
    return keycodes


def generate(udeb, output_dir, ckbcomp='ckbcomp'):
    output_dir = Path(output_dir)
    with tempfile.TemporaryDirectory(prefix='wros-keymaps-') as scratch:
        subprocess.run(['dpkg-deb', '-e', str(udeb), scratch], check=True)
        templates_text = (Path(scratch) / 'templates').read_text(encoding='utf-8')
    (output_dir / 'xkb-keymap.templates').write_text(overlay(templates_text), encoding='utf-8')
    entries, sums = [], []
    for variant, _ in VARIANTS:
        keycodes = ekmap_lines(variant, ckbcomp)
        entries += [f'us:{variant}::{line}' for line in keycodes]
        mini = '\n'.join([MINI_HEADER, *keycodes, MINI_FOOTER]) + '\n'
        sums.append(f'{hashlib.sha256(mini.encode()).hexdigest()}  {variant}')
    data = ('\n'.join(entries) + '\n').encode()
    (output_dir / 'colemak.ekmap.gz').write_bytes(gzip.compress(data, compresslevel=9, mtime=0))
    (output_dir / 'keymaps.sha256').write_text('\n'.join(sums) + '\n', encoding='ascii')


if __name__ == '__main__':
    if len(sys.argv) != 3:
        sys.exit(__doc__.strip().splitlines()[-1])
    generate(sys.argv[1], sys.argv[2])
