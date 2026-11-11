#!/usr/bin/env python3
"""Check the live-build installer tree and the REAL ISO artifact built from it.

``audit(base)`` checks a root filesystem and the offline package pool. The
root tree gate extracts the built SquashFS and records its digest. The
release gate ``audit_release`` additionally opens the finished ISO with
xorriso and the real squashfs with unsquashfs, so what is checked is what a
person would flash: boot records, the native-installer capsule, kernel ABI,
shipped binaries and helpers, update-trust inputs and a deny-list of QA
leftovers. An ISO without its native-installer capsule fails the live profile.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

from debian.debfile import DebFile
from debian.deb822 import Release

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import media_constants

# Actual runtime files copied to an encrypted target, not a request-list check.
# The shared cryptsetup shell functions are data; the other roles execute.
ENCRYPTION_PAYLOAD = {
    'cryptsetup': (('usr/lib/cryptsetup/functions', False),),
    'cryptsetup-bin': (('usr/sbin/cryptsetup', True),),
    'cryptsetup-initramfs': (('usr/share/initramfs-tools/hooks/cryptroot', True),
                           ('usr/share/initramfs-tools/scripts/local-top/cryptroot', True)),
    'dmsetup': (('usr/sbin/dmsetup', True),),
    'lvm2': (('usr/sbin/lvm', True),),
    'systemd-cryptsetup': (('usr/lib/systemd/systemd-cryptsetup', True),),
    # Debian also queues this target EFI filesystem tool during partitioning.
    'dosfstools': (('usr/sbin/mkfs.fat', True),),
}

PROFILES = ('live-session', 'installer')
# Binaries shipped from cargo, and where they land in the image.
BINARY_PATHS = media_constants.BINARY_PATHS
# Helper scripts copied verbatim from os/scripts/<name> to usr/libexec/<name>.
# 'required' helpers must exist in the image; the update launchers become
# required automatically as soon as their source file exists in the repository.
HELPERS_REQUIRED = ('wros-keyboard', 'wros-session', 'wros-display', 'wros-lock',
                    'wros-backup', 'wros-update', 'wros-live-session', 'wros-install-session')
HELPERS_IF_SOURCE = ('wros-update-stage', 'wros-update-cancel', 'wros-update-repair', 'wros-update-media')
STRIP_VARIANTS = ((), ('--strip-unneeded',), ('--strip-all',))

# Names (anywhere in the image) that exist only for QEMU/QA fixtures.
QA_NAMES = re.compile(
    r'(boot-probe|native[-_]probe|wros-native-qa|native_installer_guest|fixture_storage|'
    r'guest-[a-z]+-probe|wros-vm\.conf|VM-DEMO-ONLY|qa[-_]launcher|wros-installer-qa)')
# Configuration text that must not appear on the shipped surfaces.
QA_TEXT = re.compile(r'(wros\.native_probe|wros-native-qa|native-probe|boot-probe|fixture-token|wros-qa-|WROS_NATIVE_QA)')
AUTOLOGIN_ACTIVE = re.compile(
    r'(?i)(--autologin\b|\bautologin[-_ ]user\b|\bautologin\s*=|\bautomaticlogin(enable)?\s*=\s*true|'
    r'^\s*\[initial_session\]|(?<![a-z])autologin\b)')
BOOT_AUTOLOGIN = re.compile(r'(?<![A-Za-z])autologin\b')
NOPASSWD = re.compile(r'^\s*[^#\s].*\bNOPASSWD\b')
FIXTURE_KEY_WORDS = re.compile(r'(?i)\b(test|fixture|throwaway|insecure|example\.invalid|do not use)\b')
# Directories whose configuration decides who can log in without a password.
CONFIG_SCAN_ROOTS = ('etc', 'usr/lib/systemd/system', 'usr/lib/systemd/system-generators',
                     'usr/lib/systemd/user', 'usr/share/polkit-1', 'usr/lib/wros-installer',
                     'usr/lib/wros-update', 'usr/share/wros')
# Package-owned systemd units and generators and the /etc entry that masks each one.
SYSTEMD_MASKABLE = {'usr/lib/systemd/system-generators': 'etc/systemd/system-generators',
                    'usr/lib/systemd/system': 'etc/systemd/system',
                    'usr/lib/systemd/user': 'etc/systemd/user'}
MAX_TEXT = 2 * 1024 * 1024


def encryption_inventory(root):
    try:
        query = subprocess.check_output(['dpkg-query', '--admindir='+str(root/'var/lib/dpkg'),
            '-W', '-f=${Package}\t${Version}\t${Status}\n', *sorted(ENCRYPTION_PAYLOAD)], text=True)
    except subprocess.CalledProcessError as error:
        raise ValueError('encrypted-target package inventory incomplete') from error
    versions = {}
    for line in query.splitlines():
        name, version, status = line.split('\t', 2)
        if name not in ENCRYPTION_PAYLOAD or name in versions or status != 'install ok installed':
            raise ValueError('encrypted-target package is not installed: '+name)
        versions[name] = version
    if set(versions) != set(ENCRYPTION_PAYLOAD):
        raise ValueError('encrypted-target package inventory incomplete')
    for paths in ENCRYPTION_PAYLOAD.values():
        for relative, executable in paths:
            path = root/relative
            if not path.is_file() or path.stat().st_size == 0 or (executable and not path.stat().st_mode & 0o111):
                raise ValueError('encrypted-target runtime missing or not executable: '+relative)
    return versions


def load_tool(name, filename):
    spec = importlib.util.spec_from_file_location(name, REPO / 'tools' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def audit(base, root=None):
    """Check a root filesystem together with the live-build offline package pool."""
    base = Path(base)
    root = base / 'chroot' if root is None else Path(root)
    wifi = load_tool('wifi_audit', 'audit-wifi-image.py')
    # This invokes dpkg-query against the staged database and checks real files.
    result = wifi.audit(root)
    encryption = encryption_inventory(root)
    native = load_tool('native_audit', 'audit-native-image.py')
    native_result = native.audit(root)
    for relative in ('etc/wros/VM-DEMO-ONLY',
                     'etc/systemd/system/getty@tty1.service.d/wros-vm.conf',
                     'etc/systemd/system/wros-boot-probe.service'):
        if (root / relative).exists():
            raise ValueError('VM-only integration present: ' + relative)
    for unit in (root / 'etc/systemd/system').rglob('*'):
        if unit.is_file() and not unit.is_symlink() and 'autologin' in unit.read_text(errors='replace'):
            raise ValueError('autologin configured in installed system: ' + str(unit.relative_to(root)))
    root_password = next(line.split(':')[1] for line in (root / 'etc/shadow').read_text().splitlines()
                         if line.startswith('root:'))
    if not root_password.startswith(('!', '*')):
        raise ValueError('root account must be locked before installer account setup')
    if any(line.split(':')[0] in ('writer', 'user', 'builder') for line in (root / 'etc/passwd').read_text().splitlines()):
        raise ValueError('build/demo account present before installer account setup')
    for relative in ('binary/live/filesystem.squashfs', 'binary/install/vmlinuz',
                     'binary/install/initrd.gz'):
        if not (base / relative).is_file() or (base / relative).stat().st_size == 0:
            raise ValueError('missing installer payload: ' + relative)
    if not (root / 'usr/bin/grimoire').is_file() or (root / 'usr/bin/grimoire').stat().st_size == 0:
        raise ValueError('missing installer payload: ' + str(root / 'usr/bin/grimoire'))
    expected = {row['package'] for row in json.loads((REPO / 'os/config/wifi-firmware.json').read_text())['firmware']}
    packages = set()
    encryption_archives = {}
    for archive in sorted((base / 'binary/pool').rglob('*.deb')):
        if archive.is_symlink() or not archive.is_file():
            raise ValueError('offline package must be a regular archive: '+archive.name)
        with DebFile(str(archive)) as package:
            control = package.debcontrol()
            name = control['Package']
            if name in expected:
                packages.add(name)
            if name not in encryption or control['Version'] != encryption[name]:
                continue
            for relative, _ in ENCRYPTION_PAYLOAD[name]:
                # Read the regular payload directly from the maintained archive
                # parser, without extracting it into the staged filesystem.
                if (not package.data.has_file(relative)
                        or package.data.get_content(relative) != (root/relative).read_bytes()):
                    raise ValueError('encrypted-target payload differs from offline archive: '+relative)
            with archive.open('rb') as stream:
                checksum = sha256_stream(stream)
            encryption_archives[name] = {'version': control['Version'],
                'file': archive.relative_to(base/'binary').as_posix(), 'sha256': checksum}
    missing = expected - packages
    if missing:
        raise ValueError('installer media missing offline firmware packages: ' + ', '.join(sorted(missing)))
    missing = set(encryption) - encryption_archives.keys()
    if missing:
        raise ValueError('installer media missing matching offline encryption packages: '+', '.join(sorted(missing)))
    return {'schema_version': 1, 'passed': True, 'build_tree': str(base.resolve()), 'installed_rootfs_wifi': result,
            'native_session': native_result,
            'installer_firmware_packages': sorted(packages), 'vm_profile_absent': True,
            'encrypted_target_packages': encryption, 'offline_encryption_archives': encryption_archives,
            'physical_hardware_verified': False, 'installation_verified': False}


# ---------------------------------------------------------------- real artifacts

def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256_stream(stream):
    if hasattr(hashlib, 'file_digest'):
        return hashlib.file_digest(stream, 'sha256').hexdigest()
    hasher = hashlib.sha256()
    while chunk := stream.read(65536):
        hasher.update(chunk)
    return hasher.hexdigest()


def sha256_file(path):
    with Path(path).open('rb') as stream:
        return sha256_stream(stream)


def run_tool(command, **kwargs):
    try:
        return subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    except FileNotFoundError as error:
        raise ValueError('audit tool not installed: ' + command[0]) from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.decode('utf-8', 'replace').strip().splitlines()[-3:]
        raise ValueError(f'{command[0]} failed on the real artifact: ' + ' | '.join(detail)) from error


def xorriso(iso, *arguments):
    return run_tool(['xorriso', '-abort_on', 'FAILURE', '-indev', str(iso), *arguments], timeout=1800)


def parse_boot_report(text):
    """El Torito entries and the MBR/GPT/APM summary from xorriso's plain reports."""
    entries = {}
    for line in text.splitlines():
        found = re.match(r'El Torito boot img :\s+(\d+)\s+(\w+)\s+([yn])\s+(\w+)\s+(\S+)\s+(\S+)\s+(\d+)\s+(\d+)\s*$', line)
        if found:
            number = int(found.group(1))
            entries.setdefault(number, {}).update(platform=found.group(2), bootable=found.group(3) == 'y',
                emulation=found.group(4), load_segment=found.group(5), system_type=found.group(6),
                load_size=int(found.group(7)))
            continue
        found = re.match(r'El Torito img path :\s+(\d+)\s+(\S.*?)\s*$', line)
        if found:
            entries.setdefault(int(found.group(1)), {})['path'] = found.group(2)
            continue
        found = re.match(r'El Torito img opts :\s+(\d+)\s+(\S.*?)\s*$', line)
        if found:
            entries.setdefault(int(found.group(1)), {})['options'] = found.group(2)
    summary = next((m.group(1) for m in (re.match(r'System area summary:\s*(.*?)\s*$', l) for l in text.splitlines()) if m), '')
    gpt = [m.group(1) for m in (re.match(r'GPT partition path :\s+\d+\s+(\S.*?)\s*$', l) for l in text.splitlines()) if m]
    gpt_count = sum(1 for l in text.splitlines() if l.startswith('GPT start and size'))
    return {'el_torito': [entries[k] for k in sorted(entries)], 'system_area': summary.split(),
            'gpt_partitions': gpt_count, 'gpt_paths': gpt}


def boot_structure(iso):
    report = xorriso(iso, '-report_el_torito', 'plain', '-report_system_area', 'plain')
    parsed = parse_boot_report(report.stdout.decode('utf-8', 'replace') + report.stderr.decode('utf-8', 'replace'))
    platforms = {entry.get('platform'): entry for entry in parsed['el_torito']}
    require('BIOS' in platforms and platforms['BIOS'].get('path'), 'ISO has no BIOS El Torito boot image')
    require('UEFI' in platforms and platforms['UEFI'].get('path'), 'ISO has no UEFI El Torito boot image')
    require({'MBR', 'GPT'} <= set(parsed['system_area']),
            'ISO lost its hybrid MBR/GPT system area: ' + ' '.join(parsed['system_area']))
    require(parsed['gpt_partitions'] >= 2, 'ISO GPT lacks the EFI system partition')
    return parsed


def same_boot_structure(current, baseline):
    """Compare boot entries and partition types across a remaster."""
    def shape(parsed):
        # Cylinder alignment describes the image size, which changes with the capsule.
        area = [flag for flag in parsed['system_area'] if flag not in ('cyl-align-on', 'cyl-align-off')]
        return ([{k: v for k, v in entry.items() if k != 'lba'} for entry in parsed['el_torito']],
                area, parsed['gpt_partitions'], parsed['gpt_paths'])
    require(shape(current) == shape(baseline),
            'Re-mastered ISO boot records differ from the original live-build ISO')


def iso_paths(iso, *arguments):
    output = xorriso(iso, *arguments).stdout.decode('utf-8', 'surrogateescape')
    return [shlex.split(line)[0] for line in output.splitlines() if line.startswith("'")]


def iso_extract(iso, mapping):
    command = ['-osirrox', 'on:auto_chmod_on']
    for source, target in mapping:
        Path(target).parent.mkdir(parents=True, exist_ok=True)
        command += ['-extract', source, str(target)]
    xorriso(iso, *command)


def squash_listing(squashfs):
    """Every entry of the real squashfs: {path: (kind, target)}."""
    output = run_tool(['unsquashfs', '-ll', str(squashfs)], timeout=1800).stdout.decode('utf-8', 'surrogateescape')
    entries = {}
    for line in output.splitlines():
        found = re.match(r'^([-dlbcps])\S{9}\s+\S+\s+(?:\d+|\d+,\s*\d+)\s+\d{4}-\d\d-\d\d \d\d:\d\d squashfs-root/(.*)$', line)
        if not found:
            continue
        rest = found.group(2)
        target = None
        if found.group(1) == 'l' and ' -> ' in rest:
            rest, target = rest.split(' -> ', 1)
        entries[rest] = (found.group(1), target)
    require(entries, 'unsquashfs listed no entries from the real squashfs')
    return entries


def squash_cat(squashfs, path):
    return run_tool(['unsquashfs', '-cat', str(squashfs), path], timeout=600).stdout


def squash_mode(squashfs, path):
    """(permissions, owner/group) of one entry as stored in the real squashfs, e.g. ('-rwsr-xr-x', 'root/root')."""
    output = run_tool(['unsquashfs', '-ll', str(squashfs), path], timeout=600).stdout.decode('utf-8', 'surrogateescape')
    for line in output.splitlines():
        fields = line.split(None, 2)
        if len(fields) == 3 and line.endswith(' squashfs-root/' + path):
            return fields[0], fields[1]
    raise ValueError('squashfs has no entry for ' + path)


def squash_extract(squashfs, destination):
    """Extract the whole filesystem; device nodes cannot be made unprivileged and are not needed."""
    process = subprocess.run(['unsquashfs', '-no-progress', '-ignore-errors', '-no-xattrs', '-d', str(destination), str(squashfs)],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3600)
    errors = process.stderr.decode('utf-8', 'replace').splitlines()
    fatal = [e for e in errors if e.strip() and 'mknod' not in e and 'dev/' not in e and 'failed to' in e.lower()
             and 'xattr' not in e.lower()]
    require(process.returncode == 0 or not fatal, 'unsquashfs extraction failed: ' + ' | '.join(fatal[:3]))
    require((destination / 'etc/os-release').exists() or (destination / 'etc/debian_version').exists(),
            'extracted squashfs is not a Debian root')


def squash_kernel_abi(entries):
    kernels = sorted(p.removeprefix('boot/vmlinuz-') for p, (kind, _) in entries.items()
                     if re.fullmatch(r'boot/vmlinuz-[^/]+', p) and kind == '-')
    require(len(kernels) == 1, 'squashfs must contain exactly one kernel image, found: ' + ', '.join(kernels))
    modules = sorted({p.split('/')[-1] for p, (kind, _) in entries.items()
                      if kind == 'd' and re.fullmatch(r'(usr/)?lib/modules/[^/]+', p)})
    require(modules == kernels, f'squashfs kernel modules {modules} do not match its kernel {kernels}')
    return kernels[0]


def strip_variants(path, scratch):
    """Hashes of the input binary as strip(1) leaves it, for images that strip at install time."""
    hashes = {}
    for arguments in STRIP_VARIANTS:
        copy = Path(scratch) / ('strip-' + '-'.join(a.strip('-') for a in arguments) + '-' + path.name)
        shutil.copyfile(path, copy)
        try:
            run_tool(['strip', *arguments, str(copy)], timeout=120)
        except ValueError:
            continue
        hashes['strip ' + ' '.join(arguments)] = sha256_file(copy)
    return hashes


def compare_binaries(squashfs, binaries, scratch):
    result = {}
    for name, path in sorted(binaries.items()):
        require(name in BINARY_PATHS, 'unknown shipped binary: ' + name)
        path = Path(path)
        require(path.is_file() and not path.is_symlink(), f'input binary missing: {name} ({path})')
        shipped = hashlib.sha256(squash_cat(squashfs, BINARY_PATHS[name])).hexdigest()
        original = sha256_file(path)
        matched = 'as built' if shipped == original else None
        if matched is None:
            for label, digest in strip_variants(path, scratch).items():
                if digest == shipped:
                    matched = label
                    break
        require(matched is not None,
                f'squashfs {BINARY_PATHS[name]} ({shipped}) is not the input binary ({original}) or a strip of it')
        result[name] = {'input_sha256': original, 'squashfs_sha256': shipped, 'relation': matched}
    return result


def compare_helpers(rootfs):
    result = {}
    wanted = list(HELPERS_REQUIRED) + [n for n in HELPERS_IF_SOURCE if (REPO / 'os/scripts' / n).is_file()]
    for name in wanted:
        source = REPO / 'os/scripts' / name
        require(source.is_file(), 'helper source missing from the repository: ' + name)
        shipped = rootfs / 'usr/libexec' / name
        require(shipped.is_file() and not shipped.is_symlink(), 'helper missing from the image: usr/libexec/' + name)
        require(shipped.read_bytes() == source.read_bytes(), 'helper differs from its source: usr/libexec/' + name)
        require(os.access(shipped, os.X_OK) and not shipped.stat().st_mode & 0o022,
                'helper is not executable or is group/world writable: usr/libexec/' + name)
        result[name] = sha256_file(shipped)
    return result


def read_status(rootfs):
    packages = {}
    text = (rootfs / 'var/lib/dpkg/status').read_text(errors='replace')
    for block in text.split('\n\n'):
        fields = dict(line.split(': ', 1) for line in block.splitlines() if ': ' in line and not line.startswith(' '))
        if 'Package' in fields:
            packages[fields['Package']] = fields
    return packages


def check_launcher_package(rootfs):
    status = read_status(rootfs).get(media_constants.LAUNCHER_PACKAGE)
    require(status is not None and status.get('Status') == 'install ok installed',
            media_constants.LAUNCHER_PACKAGE + ' is not installed in the live image')
    require(status.get('Version') == media_constants.LAUNCHER_VERSION,
            f"{media_constants.LAUNCHER_PACKAGE} {status.get('Version')} differs from the pinned {media_constants.LAUNCHER_VERSION}")
    return status['Version']


def text_files(root, relatives):
    for relative in relatives:
        base = root / relative
        if not base.is_dir() or base.is_symlink():
            continue
        for path in sorted(base.rglob('*')):
            if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_TEXT:
                continue
            data = path.read_bytes()
            if b'\0' in data[:4096]:
                continue
            yield path.relative_to(root).as_posix(), data.decode('utf-8', 'replace')


def masked_by_etc(rootfs, relative):
    """True only for a top-level package unit or generator that /etc masks with a /dev/null symlink."""
    parent, _, name = relative.rpartition('/')
    override = SYSTEMD_MASKABLE.get(parent)
    if override is None:
        return False
    mask = rootfs / override / name
    return mask.is_symlink() and os.readlink(mask) == '/dev/null'


STOCK_INSTALLER_MENUS = ('boot/grub/install.cfg', 'boot/grub/install_start.cfg', 'isolinux/install.cfg')


def check_live_boot_menu(boot_configs):
    """The live medium's menus boot only the live system (tools/live-boot-menu.py): WrOS installs
    from the live session, and a stock Debian Installer entry would install plain Debian."""
    for name in STOCK_INSTALLER_MENUS:
        require(name not in boot_configs, f'stock Debian Installer menu file on the live medium: {name}')
    entries = []
    for name, text in sorted(boot_configs.items()):
        for marker in ('/install/', 'file=/cdrom', 'preseed'):
            require(marker not in text, f'{name} boots the stock Debian Installer ({marker})')
        entries += re.findall(r'^menuentry "([^"]+)"', text, re.MULTILINE)
        entries += re.findall(r'^\s*menu label \^?(.+)$', text, re.MULTILINE)
    require(any(entry.startswith('Start WrOS') for entry in entries), 'no "Start WrOS" boot entry')
    return {'entries': entries}


def deny_list(rootfs, listing, boot_configs):
    """Every QA/test leftover the shipped image must not carry. Returns nothing or raises."""
    problems = []
    for path in sorted(listing):
        # Name migration retains this alias even on images with no VM marker.
        if (path == 'etc/writingdeck/VM-DEMO-ONLY'
                and listing[path] == ('l', '../wros/VM-DEMO-ONLY')
                and 'etc/wros/VM-DEMO-ONLY' not in listing
                and not (rootfs / 'etc/wros').is_symlink()):
            continue
        if QA_NAMES.search(path):
            problems.append('QA/test file shipped: ' + path)
    if 'etc/sudoers.d/live' in listing:
        problems.append('live NOPASSWD sudoers file shipped: etc/sudoers.d/live')
    for relative, text in text_files(rootfs, CONFIG_SCAN_ROOTS):
        active = [l for l in text.splitlines() if l.strip() and not l.lstrip().startswith(('#', ';'))]
        if relative.startswith(('etc/sudoers',)) and any(NOPASSWD.match(l) for l in active):
            problems.append('passwordless sudo configured: ' + relative)
        if any(AUTOLOGIN_ACTIVE.search(l) for l in active) and not masked_by_etc(rootfs, relative):
            problems.append('autologin/initial-session configured outside /run/wros-live/greetd.toml: ' + relative)
        if QA_TEXT.search(text):
            problems.append('QA marker or fixture credential text in: ' + relative)
    sudoers = rootfs / 'etc/sudoers'
    if sudoers.is_file() and any(NOPASSWD.match(l) for l in sudoers.read_text(errors='replace').splitlines()):
        problems.append('passwordless sudo configured: etc/sudoers')
    shadow_path = rootfs / 'etc/shadow'
    passwd_path = rootfs / 'etc/passwd'
    require(shadow_path.is_file(), 'missing /etc/shadow in rootfs')
    require(passwd_path.is_file(), 'missing /etc/passwd in rootfs')
    shadow = {}
    for line in shadow_path.read_text().splitlines():
        parts = line.split(':')
        if len(parts) > 1:
            shadow[parts[0]] = parts[1]
    if not shadow.get('root', '').startswith(('!', '*')):
        problems.append('root account is not locked')
    for line in passwd_path.read_text().splitlines():
        parts = line.split(':')
        if len(parts) < 4 or not parts[2].isdigit():
            continue
        name, uid = parts[0], int(parts[2])
        if name in ('builder', 'writer', 'user', 'live', 'debian', 'wros-live') or 1000 <= uid < 65534:
            problems.append(f'builder/demo/runtime account shipped in the image: {name} (uid {uid})')
    for name, secret in sorted(shadow.items()):
        if not secret.startswith(('!', '*')):
            problems.append('account with a usable or empty password shipped: ' + name)
    for relative, text in boot_configs.items():
        for line in text.splitlines():
            if line.lstrip().startswith(('#', ';')):
                continue
            if QA_TEXT.search(line):
                problems.append('QA marker in boot configuration: ' + relative)
            if BOOT_AUTOLOGIN.search(line):
                problems.append('autologin requested in boot configuration: ' + relative)
            if re.search(r'(^|\s)(init|rd\.break|systemd\.unit|single|emergency)(=|\s|$)', line) and 'append' in line.lower():
                problems.append('shell-escape kernel parameter in boot configuration: ' + relative)
    require(not problems, '; '.join(problems[:8]) + (f' (+{len(problems) - 8} more)' if len(problems) > 8 else ''))


def pgp_packets(data):
    """Minimal OpenPGP packet walker for binary keyrings: yields (tag, body)."""
    index = 0
    while index < len(data):
        header = data[index]
        require(header & 0x80, 'update keyring is not a binary OpenPGP keyring')
        index += 1
        if header & 0x40:
            tag = header & 0x3f
            require(index < len(data), 'truncated update keyring')
            first = data[index]
            if first < 192:
                length, index = first, index + 1
            elif first < 224:
                require(index + 1 < len(data), 'truncated update keyring')
                length, index = ((first - 192) << 8) + data[index + 1] + 192, index + 2
            elif first == 255:
                require(index + 5 <= len(data), 'truncated update keyring')
                length, index = int.from_bytes(data[index + 1:index + 5], 'big'), index + 5
            else:
                raise ValueError('partial-length OpenPGP packets are not supported in the update keyring')
        else:
            tag = (header >> 2) & 0xf
            kind = header & 3
            require(kind != 3, 'indeterminate-length OpenPGP packets are not supported')
            size = (1, 2, 4)[kind]
            require(index + size <= len(data), 'truncated update keyring')
            length, index = int.from_bytes(data[index:index + size], 'big'), index + size
        require(index + length <= len(data), 'truncated update keyring')
        yield tag, data[index:index + length]
        index += length


def keyring_inventory(data):
    primaries, secret = [], False
    for tag, body in pgp_packets(data):
        if tag in (5, 7):
            secret = True
        elif tag == 6:
            require(body[:1] == b'\x04', 'update keyring contains a non-v4 primary key')
            fingerprint = hashlib.sha1(b'\x99' + len(body).to_bytes(2, 'big') + body).hexdigest().upper()
            primaries.append({'fingerprint': fingerprint, 'uids': []})
        elif tag == 13 and primaries:
            primaries[-1]['uids'].append(body.decode('utf-8', 'replace'))
    return primaries, secret


def verify_update_trust(keyring, signers, required=True):
    keyring, signers = Path(keyring), Path(signers)
    present = [keyring.is_file() and not keyring.is_symlink(), signers.is_file() and not signers.is_symlink()]
    if not any(present) and not required:
        return {'checked': False, 'reason': 'absent in an interim build'}
    require(all(present), f'update trust inputs missing: {keyring} and {signers} are both required')
    for path in (keyring, signers):
        mode = path.stat().st_mode
        require(not mode & 0o022, 'update trust input is group/world writable: ' + path.name)
    primaries, secret = keyring_inventory(keyring.read_bytes())
    require(not secret, 'update keyring contains PRIVATE key material')
    require(primaries, 'update keyring contains no keys')
    pinned = []
    for line in signers.read_text().splitlines():
        line = line.split('#', 1)[0].strip()
        if not line:
            continue
        require(re.fullmatch(r'[0-9A-Fa-f]{40}', line) is not None, 'malformed update signer fingerprint')
        pinned.append(line.upper())
    require(pinned, 'update-signers pins no fingerprint')
    known = {p['fingerprint'] for p in primaries}
    require(set(pinned) <= known, 'a pinned update signer is absent from the update keyring')
    for primary in primaries:
        require(not any(FIXTURE_KEY_WORDS.search(uid) for uid in primary['uids']),
                'test/fixture key in production update trust: ' + primary['fingerprint'])
    debian = [Path('/usr/share/keyrings') / name for name in ('debian-archive-keyring.pgp', 'debian-archive-keyring.gpg')]
    for candidate in debian:
        if candidate.is_file():
            require(keyring.read_bytes() != candidate.read_bytes(),
                    'the Debian archive keyring must not be the WrOS update keyring')
    return {'checked': True, 'keyring_sha256': sha256_file(keyring), 'signers_sha256': sha256_file(signers),
            'pinned_fingerprints': sorted(set(pinned)), 'keys_in_keyring': sorted(known),
            'secret_key_material': False}


def check_update_trust(rootfs, required=True):
    """Image inputs from decision 2 of docs/archive/RC_CONTRACT.md; public material only."""
    return verify_update_trust(rootfs / 'usr/share/keyrings/wros-archive-keyring.gpg',
                               rootfs / 'usr/share/wros/update-signers',
                               required=required)


def audit_capsule(capsule, iso_kernel_sha256, abi):
    """The extracted native-installer tree, byte-checked against its own receipts."""
    manifest_path = capsule / 'manifest.json'
    require(manifest_path.is_file() and not manifest_path.is_symlink(),
            'native-installer capsule missing from the ISO (native-installer/manifest.json)')
    manifest = json.loads(manifest_path.read_bytes())
    module = media_constants.module_sha256()
    require(manifest.get('passed') is True and manifest.get('architecture') == 'amd64'
            and manifest.get('media_subdirectory') == 'native-installer',
            'capsule manifest is not a passing amd64 native-installer receipt')
    require(manifest.get('kernel_release') == abi,
            f"capsule kernel_release {manifest.get('kernel_release')} differs from the squashfs kernel ABI {abi}")
    require(abi == media_constants.qualified_kernel_abi(),
            f'squashfs kernel ABI {abi} is not the qualified {media_constants.qualified_kernel_abi()}')
    require(manifest.get('module_sha256') == module,
            'capsule module_sha256 differs from the constant enforced by wros-install-session')
    for flag in ('module_observed_in_compressed_initrd', 'backend_kernel_matches_live_medium',
                 'pool_independently_revalidated'):
        require(manifest.get(flag) is True, 'capsule receipt does not attest ' + flag)
    require(manifest.get('generated_media_release_debian_signed') is False, 'capsule media Release must not claim a Debian signature')
    listed = manifest.get('file_sha256') or {}
    require(listed, 'capsule manifest lists no files')
    actual = {}
    for path in sorted(capsule.rglob('*')):
        require(not path.is_symlink(), 'capsule contains a symlink: ' + path.relative_to(capsule).as_posix())
        if path.is_file():
            actual[path.relative_to(capsule).as_posix()] = path
    actual.pop('manifest.json')
    require(set(actual) == set(listed), 'capsule files differ from its manifest: '
            + ', '.join(sorted(set(actual) ^ set(listed))[:5]))
    for name, expected in listed.items():
        require(sha256_file(actual[name]) == expected, 'capsule file hash differs from its manifest: ' + name)
    backend = json.loads((capsule / 'backend-manifest.json').read_bytes())
    require(backend.get('passed') is True and backend.get('module_sha256') == module
            and backend.get('kernel_version') == abi, 'backend receipt does not match the module/kernel')
    require(backend.get('kernel_sha256') == sha256_file(capsule / 'vmlinuz') == iso_kernel_sha256,
            'capsule kernel differs from the live medium kernel')
    require(backend.get('initrd_sha256') == sha256_file(capsule / 'initrd.gz'), 'capsule initrd differs from its receipt')
    cpio = shutil.which('cpio')
    require(cpio is not None, 'audit tool not installed: cpio')
    import gzip
    with gzip.open(capsule / 'initrd.gz', 'rb') as stream:
        image = stream.read(512 * 1024 * 1024 + 1)
    found = run_tool([cpio, '-i', '--to-stdout', '--quiet', 'usr/lib/cdebconf/frontend/passthrough.so'], input=image, timeout=120).stdout
    require(hashlib.sha256(found).hexdigest() == module, 'passthrough module inside the capsule initrd differs from the pinned module')
    pool = json.loads((capsule / 'pool-manifest.json').read_bytes())
    require(pool.get('passed') is True and pool.get('kernel_abi') == abi, 'pool receipt is not for this kernel ABI')
    archives = pool.get('archives') or []
    require(archives and pool.get('package_count') == len(archives), 'pool receipt package count is inconsistent')
    for row in archives:
        path = capsule / row['file']
        require(path.is_file() and sha256_file(path) == row['sha256'] and path.stat().st_size == row['bytes'],
                'pool archive differs from its receipt: ' + row['file'])
    pooled = {n for n in actual if n.startswith('pool/')}
    require(pooled == {row['file'] for row in archives}, 'pool directory differs from the pool receipt')
    packages = capsule / 'Packages'
    require(sha256_file(packages) == pool['generated_packages_sha256']
            and sha256_file(capsule / 'Packages.gz') == pool['generated_packages_gz_sha256'],
            'capsule Packages index differs from the pool receipt')
    indexed = re.findall(r'(?m)^Filename: native-installer/(pool/\S+)$', packages.read_text())
    require(sorted(indexed) == sorted(row['file'] for row in archives), 'Packages index does not list exactly the pool archives')
    release = Release((capsule / 'media-Release').read_text())
    index = 'main/debian-installer/binary-amd64/Packages'
    wanted = {index: sha256_file(packages), index + '.gz': sha256_file(capsule / 'Packages.gz')}
    signed = {row['name']: row['sha256'] for row in release.get('SHA256', [])}
    for name, digest in wanted.items():
        require(signed.get(name) == digest, 'media-Release does not hash the capsule index: ' + name)
    return {'kernel_release': abi, 'module_sha256': module, 'package_count': len(archives),
            'archive_bytes': sum(row['bytes'] for row in archives), 'files_verified': len(listed),
            'manifest_sha256': sha256_file(manifest_path), 'packages_sha256': sha256_file(packages),
            'packages_gz_sha256': sha256_file(capsule / 'Packages.gz'),
            'media_release_sha256': sha256_file(capsule / 'media-Release')}


def source_identity():
    from wros_source import fingerprint, is_dirty
    try:
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True, stderr=subprocess.DEVNULL).strip()
        dirty = is_dirty()
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError('cannot determine the git commit of the build source') from error
    return {'git_commit': commit, 'git_dirty': dirty, 'source_fingerprint': fingerprint()}


def tree_gate(base):
    """Audit the shipped root with its recorded ownership and modes."""
    require(os.geteuid() == 0, 'the tree gate reads the root-owned filesystem: run --tree-only as root')
    squashfs = Path(base) / 'binary/live/filesystem.squashfs'
    require(squashfs.is_file() and not squashfs.is_symlink(), 'missing or unsafe built squashfs')
    digest = sha256_file(squashfs)
    with tempfile.TemporaryDirectory(prefix='wros-tree-audit-') as directory:
        root = Path(directory) / 'rootfs'
        squash_extract(squashfs, root)
        result = audit(base, root=root)
    require(sha256_file(squashfs) == digest, 'squashfs changed during its tree audit')
    result['tree_gate_uid'] = 0
    result['squashfs_sha256'] = digest
    return result


def load_tree_result(path, base):
    """A tree-gate result written by a root --tree-only run of this tool for the same build tree."""
    require(path.is_file() and not path.is_symlink(), 'tree-gate result is missing or a symlink')
    tree = json.loads(path.read_text())
    require(isinstance(tree, dict) and tree.get('passed') is True and tree.get('tree_gate_uid') == 0,
            'tree-gate result did not pass as root')
    require(tree.get('build_tree') == str(Path(base).resolve()), 'tree-gate result is for a different build tree')
    require(isinstance(tree.get('squashfs_sha256'), str)
            and re.fullmatch(r'[0-9a-f]{64}', tree['squashfs_sha256']),
            'tree-gate result is missing its squashfs digest')
    require(tree['squashfs_sha256'] == sha256_file(Path(base) / 'binary/live/filesystem.squashfs'),
            'tree-gate result is for a different squashfs')
    return tree


def audit_release(base, iso, profile='live-session', binaries=None, baseline=None, scratch=None, interim=False,
                  identity=True, tree_result=None):
    """Tree gate plus the real ISO. Raises ValueError unless everything holds."""
    base, iso = Path(base), Path(iso)
    require(profile in PROFILES, 'unknown profile')
    require(iso.is_file() and not iso.is_symlink(), 'ISO to audit is missing or a symlink')
    live = profile == 'live-session'
    require(not live or BINARY_PATHS.keys() <= (binaries or {}).keys(),
            'the live profile must compare all shipped Rust binaries (--binary NAME=PATH)')
    tree = load_tree_result(Path(tree_result), base) if tree_result is not None else None
    own = scratch is None
    scratch = Path(tempfile.mkdtemp(prefix='wros-media-audit-')) if own else Path(scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    try:
        boot = boot_structure(iso)
        if baseline is not None:
            same_boot_structure(boot, boot_structure(baseline))
        top = set(iso_paths(iso, '-find', '/', '-maxdepth', '1'))
        require('/live' in top, 'ISO has no /live directory')
        squashfs = scratch / 'filesystem.squashfs'
        iso_extract(iso, [('/live/filesystem.squashfs', squashfs)])
        squashfs_digest = sha256_file(squashfs)
        if tree is not None:
            require(tree['squashfs_sha256'] == squashfs_digest,
                    'ISO squashfs differs from the filesystem that passed the tree audit')
        listing = squash_listing(squashfs)
        abi = squash_kernel_abi(listing)
        kernel_name = '/live/vmlinuz-' + abi
        medium_files = set(iso_paths(iso, '-find', '/live', '-type', 'f'))
        require(kernel_name in medium_files and '/live/initrd.img-' + abi in medium_files,
                f'ISO /live lacks the kernel/initrd for {abi}')
        iso_extract(iso, [(kernel_name, scratch / 'vmlinuz')])
        require(sha256_file(scratch / 'vmlinuz') == hashlib.sha256(squash_cat(squashfs, 'boot/vmlinuz-' + abi)).hexdigest(),
                'ISO /live kernel differs from the squashfs kernel')
        rootfs = scratch / 'rootfs'
        squash_extract(squashfs, rootfs)
        if tree is None:
            tree = audit(base, root=rootfs)
            tree['squashfs_sha256'] = squashfs_digest
        boot_configs = {}
        for directory in ('/isolinux', '/boot/grub', '/EFI'):
            wanted = [p for p in iso_paths(iso, '-find', directory, '-type', 'f') if p.endswith(('.cfg', '.conf'))]
            if wanted:
                targets = []
                for index, name in enumerate(wanted):
                    targets.append((name, scratch / 'boot-configs' / str(index) / Path(name).name))
                iso_extract(iso, targets)
                for name, target in targets:
                    boot_configs[name.lstrip('/')] = target.read_text(errors='replace')
        require(boot_configs, 'ISO has no readable boot configuration')
        deny_list(rootfs, listing, boot_configs)
        result = {'schema_version': 2, 'passed': True, 'profile': profile, 'interim': bool(interim),
                  'tree': tree, 'boot': boot,
                  'iso': {'name': iso.name, 'bytes': iso.stat().st_size, 'sha256': sha256_file(iso)},
                  'squashfs': {'sha256': squashfs_digest, 'kernel_abi': abi, 'entries': len(listing)},
                  'deny_list_clean': True, 'boot_tested': False}
        if binaries:
            result['binaries'] = compare_binaries(squashfs, binaries, scratch)
        if live:
            require(boot_configs and any('wros.live=1' in text and 'boot=live' in text for text in boot_configs.values()),
                    'no boot entry requests boot=live wros.live=1')
            result['boot_menu'] = check_live_boot_menu(boot_configs)
            live_audit = load_tool('live_audit', 'audit-live-session.py')
            result['live_session'] = live_audit.audit(rootfs)
            # The unprivileged extraction above cannot carry setuid; prove it from the squashfs bytes.
            require(squash_mode(squashfs, 'usr/sbin/vlock-main') == ('-rwsr-xr-x', 'root/root'),
                    'maintained vlock-main in the squashfs is not setuid root as the Debian package ships it')
            result['squashfs']['helpers'] = compare_helpers(rootfs)
            result['squashfs']['launcher_version'] = check_launcher_package(rootfs)
            result['update_trust'] = check_update_trust(rootfs, required=not interim)
            capsule = scratch / 'capsule'
            require('/native-installer' in top, 'native-installer capsule missing from the ISO root')
            iso_extract(iso, [('/native-installer', capsule)])
            result['capsule'] = audit_capsule(capsule, sha256_file(scratch / 'vmlinuz'), abi)
        else:
            result['capsule'] = {'required': False}
        if identity:
            result.update(source_identity())
            result['source_commit'] = result['git_commit']
        result['input_binaries_sha256'] = {n: sha256_file(p) for n, p in sorted((binaries or {}).items())}
        result['scope'] = ('Static audit of the finished ISO bytes: boot records, native-installer capsule, '
                           'kernel ABI, shipped binaries/helpers, deny-list and update-trust inputs. '
                           'Not a boot, installation, update, Secure Boot or hardware test (boot_tested is false).')
        return result
    finally:
        if own:
            shutil.rmtree(scratch, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build_tree', nargs='?', type=Path, help='live-build directory')
    parser.add_argument('--check-update-inputs', nargs=2, metavar=('KEYRING', 'SIGNERS'), type=Path,
                        help='verify raw update-trust inputs before building')
    parser.add_argument('--profile', choices=PROFILES, default='live-session')
    parser.add_argument('--iso', type=Path, help='finished ISO to open (required for the live profile)')
    parser.add_argument('--baseline-iso', type=Path, help='the capsule-less ISO whose boot records must be preserved')
    parser.add_argument('--binary', action='append', default=[], metavar='NAME=PATH',
                        help='input binary (grimoire, wros-greeter, wros-installer) the squashfs must contain')
    parser.add_argument('--scratch', type=Path, help='work directory (default: a temporary directory)')
    parser.add_argument('--interim', action='store_true', help='allow absent update-trust inputs (never for the RC)')
    parser.add_argument('--tree-only', action='store_true',
                        help='audit the built SquashFS with recorded ownership (must run as root)')
    parser.add_argument('--tree-result', type=Path,
                        help='use a passed root --tree-only result for this build tree instead of re-reading the chroot')
    args = parser.parse_args()
    try:
        if args.check_update_inputs:
            keyring, signers = args.check_update_inputs
            result = verify_update_trust(keyring, signers, required=True)
            print(json.dumps(result, indent=2))
            return
        if args.build_tree is None:
            parser.error('the following arguments are required: build_tree')
        binaries = {}
        for item in args.binary:
            name, _, path = item.partition('=')
            binaries[name] = Path(path)
        if args.tree_only:
            result = tree_gate(args.build_tree)
        elif args.iso is None:
            if args.profile == 'live-session':
                raise ValueError('the live profile audits the real ISO: pass --iso')
            result = audit(args.build_tree)
        else:
            result = audit_release(args.build_tree, args.iso, args.profile, binaries, args.baseline_iso,
                                   args.scratch, args.interim, identity=args.profile == 'live-session',
                                   tree_result=args.tree_result)
        print(json.dumps(result, indent=2))
    except (ValueError, OSError, subprocess.SubprocessError, KeyError, json.JSONDecodeError) as exc:
        raise SystemExit(str(exc)) from None


if __name__ == '__main__':
    main()
