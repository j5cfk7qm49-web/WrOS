#!/usr/bin/python3 -I
"""Retain the previous console configuration and entry points after unpack."""
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile


def atomic_file(path, contents, mode=0o644):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix='.wros-migrate-', delete=False) as output:
        temp = Path(output.name)
        try:
            os.fchmod(output.fileno(), mode)
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def alias(old, new, *, replace_file=False):
    old.parent.mkdir(parents=True, exist_ok=True)
    target = os.path.relpath(new, old.parent)
    if old.is_symlink() and os.readlink(old) == target:
        return
    if old.exists() or old.is_symlink():
        if not replace_file or old.is_symlink() or not old.is_file():
            return  # Preserve independent administrator paths.
        with tempfile.TemporaryDirectory(dir=old.parent, prefix='.wros-migrate-') as directory:
            temp = Path(directory) / old.name
            temp.symlink_to(target)
            os.replace(temp, old)
        return
    old.symlink_to(target)


def groups(root):
    path = root / 'etc/group'
    if not path.is_file():
        return

    def records():
        return dict((line[0], line[2:]) for line in
                    (row.split(':') for row in path.read_text().splitlines()) if len(line) == 4)

    def change(program, *args):
        subprocess.run([program, '--root', str(root), *args], check=True)

    found = records()
    migration = root / 'etc/wros/writer-group'
    if 'writingdeck' in found and 'wros' in found and not migration.is_file():
        raise ValueError('Both writer group names already exist. Review their members before updating.')
    if 'writingdeck' in found and 'wros' not in found:
        change('/usr/sbin/groupmod', '--new-name', 'wros', 'writingdeck')
        found = records()
    if 'wros' not in found:
        return
    gid = found['wros'][0]
    if migration.is_file() and migration.read_text().strip() != gid:
        raise ValueError('The WrOS writer group ID changed. Review its members before updating.')
    atomic_file(migration, (gid + '\n').encode())
    if 'writingdeck' not in found:
        # A distinct group lets old polkit versions still resolve the old name.
        change('/usr/sbin/groupadd', '--system', 'writingdeck')
        found = records()
    gid, current = found['wros']
    members = set(filter(None, current.split(',')))
    passwd = root / 'etc/passwd'
    if passwd.is_file():
        for row in passwd.read_text().splitlines():
            user = row.split(':')
            if len(user) == 7 and user[3] == gid and user[2] != '0':
                members.add(user[0])
    if members != set(filter(None, found['writingdeck'][1].split(','))):
        change('/usr/bin/gpasswd', '--members', ','.join(sorted(members)), 'writingdeck')


def migrate(root):
    old = root / 'etc/writingdeck'
    current = root / 'etc/wros'
    if old.is_symlink() or current.is_symlink():
        raise ValueError('WrOS configuration directories must be independent')
    groups(root)
    old.mkdir(parents=True, exist_ok=True)
    for name in ('keyboard.json', 'image-profile', 'installer-keyboard-pending', 'VM-DEMO-ONLY'):
        legacy, canonical = old / name, current / name
        if legacy.is_file() and not legacy.is_symlink():
            mode = stat.S_IMODE(legacy.stat().st_mode) & 0o644
            atomic_file(canonical, legacy.read_bytes(), mode)
        alias(legacy, canonical, replace_file=True)
    release = (current / 'release').read_bytes()
    atomic_file(old / 'release', release.replace(b'WROS_VERSION=', b'WRITINGDECK_VERSION=', 1))
    for directory, legacy, canonical in (
            ('usr/libexec', 'writingdeck-keyboard', 'wros-keyboard'),
            ('usr/libexec', 'writingdeck-session', 'wros-session'),
            ('etc/profile.d', 'writingdeck.sh', 'wros.sh'),
            ('usr/share', 'writingdeck/keymaps', 'wros/keymaps'),
            ('usr/share/doc', 'writingdeck', 'wros'),
            *[('etc/systemd/system', 'writingdeck-keyboard' + suffix, 'wros-keyboard' + suffix)
              for suffix in ('.service', '-revert.service', '-revert.timer')]):
        alias(root / directory / legacy, root / directory / canonical)


if __name__ == '__main__':
    migrate(Path(sys.argv[1] or '/'))
