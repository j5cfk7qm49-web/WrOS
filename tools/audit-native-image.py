#!/usr/bin/env python3
"""Read-only native login, manual-lock payload and power-policy image gate."""
import argparse
import configparser
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ElementTree

REPO = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('wros_packaging', REPO / 'packaging/wros_packaging.py')
packaging = sys.modules.get('wros_packaging')
if packaging is None or getattr(packaging, '__file__', None) != str(REPO / 'packaging/wros_packaging.py'):
    packaging = importlib.util.module_from_spec(_spec)
    sys.modules['wros_packaging'] = packaging
    _spec.loader.exec_module(packaging)
SLEEP_TARGETS = ('hibernate', 'hybrid-sleep', 'suspend-then-hibernate')


def regular(root, relative):
    path = root / relative
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError('missing or unsafe native session file: ' + relative)
    return path


LOGIND_POLICY = {key: 'ignore' for key in (
    'HandlePowerKey', 'HandlePowerKeyLongPress', 'HandleRebootKey', 'HandleRebootKeyLongPress',
    'HandleSuspendKey', 'HandleSuspendKeyLongPress', 'HandleHibernateKey', 'HandleHibernateKeyLongPress',
    'HandleLidSwitch', 'HandleLidSwitchExternalPower', 'HandleLidSwitchDocked', 'IdleAction')}
SLEEP_POLICY = {'AllowSuspend': 'yes', 'AllowHibernation': 'no',
                'AllowHybridSleep': 'no', 'AllowSuspendThenHibernate': 'no'}
# (staged path, section, required values). Checked on the staged bytes themselves, so a
# weakened source file cannot pass merely by being copied faithfully.
POLICY_VALUES = (
    ('etc/systemd/logind.conf.d/60-wros-power-guard.conf', 'Login', LOGIND_POLICY),
    ('etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf', 'Sleep', SLEEP_POLICY),
    ('etc/systemd/system.conf.d/60-wros-reboot-guard.conf', 'Manager', {'CtrlAltDelBurstAction': 'none'}),
    ('etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf', 'connectivity', {'enabled': 'false'}),
)


def policy_values(root):
    for relative, section, required in POLICY_VALUES:
        parser = configparser.ConfigParser(strict=False, interpolation=None)
        parser.optionxform = str
        parser.read_string(regular(root, relative).read_text())
        actual = dict(parser.items(section)) if parser.has_section(section) else {}
        for key, value in required.items():
            if actual.get(key, '').strip().lower() != value:
                raise ValueError(f'native policy value {key}={value} missing or weakened in {relative}')
    sysctl = {}
    for relative in ('etc/sysctl.d/60-wros-sysrq.conf', 'etc/sysctl.d/60-wros-quiet-console.conf'):
        for row in regular(root, relative).read_text().splitlines():
            row = row.strip()
            if row and not row.startswith(('#', ';')) and '=' in row:
                key, _, value = row.partition('=')
                sysctl[key.strip()] = value.strip()
    if sysctl.get('kernel.sysrq') != '0':
        raise ValueError('native policy value kernel.sysrq=0 missing or weakened')
    console_level = sysctl.get('kernel.printk', '').split()[:1]
    if not console_level or not console_level[0].isdigit() or int(console_level[0]) > 3:
        raise ValueError('native policy value kernel.printk console level 3 or less missing or weakened')


UPDATE_FILES = ('usr/libexec/wros-update', 'usr/libexec/wros-update-stage', 'usr/libexec/wros-update-cancel',
                'usr/libexec/wros-update-repair', 'usr/libexec/wros-update-media', 'usr/libexec/wros-update-fetch',
                'usr/libexec/wros-update-apply', 'usr/libexec/wros-update-rollback',
                'usr/libexec/wros-update-healthy', 'usr/lib/wros-update/engine.py')
UPDATE_UNITS = ('etc/systemd/system/wros-offline-update.service', 'etc/systemd/system/wros-update-reconcile.service')
# Marker (PEM/ASCII-armour) of private key material followed by key-sized body text. Binary files
# are excluded for this pattern because TLS libraries embed the bare marker as a string constant.
PRIVATE_BLOCK = re.compile(rb'-----BEGIN (?:[A-Z0-9]+ )*(?:PRIVATE|SECRET) KEY(?: BLOCK)?-----.{0,400}?[A-Za-z0-9+/]{48}',
                           re.DOTALL)
PRIVATE_NAMES = re.compile(r'^(?:secring\.gpg|id_(?:rsa|dsa|ecdsa|ed25519)|.*\.sec|.*\.secret|.*\.p12|.*\.pfx)$')
KEY_SUFFIXES = ('.gpg', '.pgp', '.key', '.keys', '.kbx')
KEY_BEARING_DIRS = ('usr/share/keyrings', 'usr/share/wros', 'etc/apt', 'etc/wros', 'var/lib/wros-update',
                    'root', 'home', 'etc/gnupg', 'etc/ssl/private')
SKIP_TREES = ('proc', 'sys', 'dev', 'run')
SCAN_LIMIT = 32 * 1024 * 1024


def strict_ownership():
    # Ownership is only meaningful when the audit itself runs as (pseudo-)root.
    return os.geteuid() == 0


def package_listing(root):
    admindir = root / 'var/lib/dpkg'
    out = subprocess.check_output(['dpkg-query', '--admindir=' + str(admindir), '-L', 'wros'], text=True)
    return [row for row in out.splitlines() if row and row != '/.']


def audit_package(root):
    """The 'wros' package must be installed at os/VERSION and own exactly the manifest's files,
    unmodified, with safe modes. Returns a summary dict."""
    admindir = root / 'var/lib/dpkg'
    expected = packaging.read_version(REPO)
    try:
        fields = subprocess.check_output(['dpkg-query', '--admindir=' + str(admindir), '-W',
                                          '-f=${Version}\t${Status}\t${Architecture}', 'wros'],
                                         text=True, stderr=subprocess.DEVNULL).split('\t')
    except subprocess.CalledProcessError:
        raise ValueError('WrOS package wros is not recorded in the image dpkg database') from None
    if len(fields) != 3 or fields[1] != 'install ok installed':
        raise ValueError('WrOS package wros is not installed')
    if fields[0] != expected:
        raise ValueError('wros package version %s does not match os/VERSION %s' % (fields[0], expected))
    if fields[2] != 'amd64':
        raise ValueError('wros package architecture must be amd64')
    manifest = packaging.load_manifest(REPO)
    specs = packaging.expand(manifest, REPO)
    listing = package_listing(root)
    owned = set()
    for row in listing:
        path = root / row.lstrip('/')
        try:
            info = path.lstat()
        except PermissionError:
            raise ValueError('package-owned path cannot be read by this audit (permission denied; '
                             'audit a root-owned tree as root): ' + row) from None
        except OSError:
            raise ValueError('package-owned path is missing on disk: ' + row) from None
        if stat.S_ISDIR(info.st_mode):
            continue
        if not stat.S_ISREG(info.st_mode):
            raise ValueError('package-owned path is not a regular file: ' + row)
        owned.add(row.lstrip('/'))
    if packaging.GOOGLE_CLIENTS_DEST in owned:
        specs.append(packaging.Spec(packaging.GOOGLE_CLIENTS_DEST, 0o644, 'config'))
    wanted = {spec.dest for spec in specs}
    if owned - wanted:
        raise ValueError('wros package owns a file the manifest does not list: ' + sorted(owned - wanted)[0])
    if wanted - owned:
        raise ValueError('wros package is missing a required file: ' + sorted(wanted - owned)[0])
    try:
        sums = {}
        for row in (admindir / 'info/wros.md5sums').read_text().splitlines():
            digest, _, name = row.partition('  ')
            sums[name] = digest
    except OSError:
        raise ValueError('wros package has no md5sums record') from None
    if set(sums) != owned:
        raise ValueError('wros package md5sums do not cover exactly its files')
    for spec in specs:
        path = root / spec.dest
        try:
            info = path.lstat()
        except OSError:
            raise ValueError('package file is missing on disk: ' + spec.dest) from None
        if info.st_mode & 0o7000 or info.st_mode & 0o022:
            raise ValueError('unsafe package file permissions: ' + spec.dest)
        if stat.S_IMODE(info.st_mode) != spec.mode:
            raise ValueError('package file mode differs from the manifest: ' + spec.dest)
        if strict_ownership() and (info.st_uid, info.st_gid) != (0, 0):
            raise ValueError('package file is not owned by root:root: ' + spec.dest)
        data = path.read_bytes()
        if hashlib.md5(data, usedforsecurity=False).hexdigest() != sums[spec.dest]:
            raise ValueError('package file differs from its dpkg md5sum: ' + spec.dest)
        # Pending (update-engine) files are covered by md5sums only until the flag is cleared.
        if spec.source is not None and not spec.pending and spec.klass in ('code', 'config'):
            if data != spec.source.read_bytes():
                raise ValueError('package file differs from the qualified source: ' + spec.dest)
        if spec.dest == 'etc/wros/release':
            if ('WROS_VERSION=' + expected + '\n').encode() not in data:
                raise ValueError('/etc/wros/release does not carry the package version')
        if spec.dest == packaging.GOOGLE_CLIENTS_DEST:
            if data != packaging.google_clients_bytes(path):
                raise ValueError('packaged Google clients differ from the canonical build format')
    for conffile in packaging.conffile_paths(specs):
        listed = (admindir / 'info/wros.conffiles').read_text().split() if (admindir / 'info/wros.conffiles').exists() else []
        if conffile not in listed:
            raise ValueError('writer-editable file is not a registered conffile: ' + conffile)
    for relative in (packaging.KEYRING_DEST, packaging.SIGNERS_DEST):
        if relative in owned:
            raise ValueError('update trust anchor must not be package-owned: ' + relative)
    return {'name': 'wros', 'version': fields[0], 'files': len(owned)}


def audit_update_engine(root):
    """Launchers must be plain root-owned 0755 files that never consult the Debian archive keyring."""
    for relative in UPDATE_FILES:
        path = regular(root, relative)
        if path.read_bytes().find(b'debian-archive-keyring') != -1:
            raise ValueError('update engine must not use the Debian archive keyring: ' + relative)
    for relative in UPDATE_FILES[:-1]:
        if stat.S_IMODE((root / relative).stat().st_mode) != 0o755:
            raise ValueError('update launcher must be mode 0755: ' + relative)


def audit_trust(root, fixture_fingerprints=None):
    """Keyring and signer pin: public-only, pin == keyring primaries, not world-writable, no fixture keys."""
    keyring = regular(root, packaging.KEYRING_DEST)
    pin = regular(root, packaging.SIGNERS_DEST)
    for path, label in ((keyring, 'update keyring'), (pin, 'update signer pin'), (pin.parent, 'update trust directory')):
        info = path.stat()
        if info.st_mode & 0o022:
            raise ValueError(label + ' is group/world-writable')
        if strict_ownership() and info.st_uid != 0:
            raise ValueError(label + ' is not owned by root')
    try:
        pinned, uids = packaging.check_trust_material(keyring.read_bytes(), pin.read_text(encoding='utf-8'),
                                                      fixture_fingerprints)
    except (packaging.PackagingError, UnicodeDecodeError) as exc:
        raise ValueError('update trust: ' + str(exc)) from None
    # The wros package may bring a signing subkey, never another root of trust.
    if os.path.lexists(root / packaging.PACKAGE_KEYRING_DEST):
        packaged = regular(root, packaging.PACKAGE_KEYRING_DEST).read_bytes()
        try:
            primaries = packaging.inspect_keyring(packaged)['primaries']
        except packaging.PackagingError as exc:
            raise ValueError('packaged update keyring: ' + str(exc)) from None
        if not primaries or set(primaries) - set(pinned):
            raise ValueError('update-keyring.gpg holds a primary key the image pin does not name')
    return pinned


def audit_units(root):
    manifest = packaging.load_manifest(REPO)
    enabled = []
    for item in manifest['enable']:
        unit, target = item['unit'], item['target']
        unit_path = regular(root, 'etc/systemd/system/' + unit)
        if item.get('verify_install_section'):
            declared = packaging.install_section(unit_path.read_text(encoding='utf-8'))
            if target not in declared['WantedBy'] + declared['RequiredBy']:
                raise ValueError('%s does not declare [Install] for %s' % (unit, target))
        link = root / 'etc/systemd/system' / (target + '.wants') / unit
        if not link.is_symlink() or os.readlink(link) not in ('../' + unit, '/etc/systemd/system/' + unit):
            raise ValueError('unit is not enabled for %s: %s' % (target, unit))
        enabled.append(unit)
    return enabled


def audit_polkit(root):
    manifest = packaging.load_manifest(REPO)
    expected = manifest['update_polkit_actions']
    found = {}
    for policy in sorted((root / 'usr/share/polkit-1/actions').glob('*.policy')):
        try:
            tree = ElementTree.parse(policy)
        except ElementTree.ParseError:
            raise ValueError('unparseable polkit policy: ' + policy.name) from None
        for action in tree.getroot().iter('action'):
            identifier = action.get('id', '')
            if not identifier.startswith('org.wros.update.'):
                continue
            if identifier in found:
                raise ValueError('duplicate update polkit action: ' + identifier)
            paths = [a.text for a in action.iter('annotate') if a.get('key') == 'org.freedesktop.policykit.exec.path']
            defaults = action.find('defaults')
            implicit = {child.tag: (child.text or '').strip() for child in defaults} if defaults is not None else {}
            if implicit != {'allow_any': 'no', 'allow_inactive': 'no', 'allow_active': 'no'}:
                raise ValueError('update polkit action must default to no for every class: ' + identifier)
            found[identifier] = paths
    if set(found) != set(expected):
        raise ValueError('update polkit actions are not exactly %s' % sorted(expected))
    for identifier, paths in found.items():
        if paths != [expected[identifier]]:
            raise ValueError('update polkit action has the wrong exec path: ' + identifier)
    rules = regular(root, 'etc/polkit-1/rules.d/50-wros-update.rules').read_text(encoding='utf-8')
    for needle in ('org.wros.update.', 'subject.local', 'subject.active', 'isInGroup("wros")'):
        if needle not in rules:
            raise ValueError('update polkit rules do not restrict to the local active wros group: ' + needle)
    return sorted(found)


def audit_no_private_keys(root):
    """No private/secret key material anywhere in the image."""
    root = root.resolve(strict=True)
    for current, directories, names in os.walk(root, followlinks=False):
        here = Path(current)
        relative = here.relative_to(root).as_posix()
        if relative == '.':
            directories[:] = [d for d in directories if d not in SKIP_TREES]
        directories[:] = sorted(directories)
        if any(d == 'private-keys-v1.d' for d in directories):
            raise ValueError('private key store present: ' + (here / 'private-keys-v1.d').relative_to(root).as_posix())
        for name in names:
            path = here / name
            shown = path.relative_to(root).as_posix()
            if PRIVATE_NAMES.match(name):
                raise ValueError('private key file present: ' + shown)
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_size == 0 or info.st_size > SCAN_LIMIT:
                continue
            try:
                with path.open('rb') as handle:
                    data = handle.read(SCAN_LIMIT)
            except PermissionError:
                raise ValueError(f'cannot scan {shown} for private keys; run with access to the staged root') from None
            if b'\0' not in data[:8192] and PRIVATE_BLOCK.search(data):
                raise ValueError('private key material present: ' + shown)
            key_like = name.endswith(KEY_SUFFIXES) or shown.startswith(tuple(d + '/' for d in KEY_BEARING_DIRS))
            if key_like and data[0] & 0x80 and len(data) <= packaging.MAX_KEYRING:
                try:
                    secret = packaging.inspect_keyring(data)['secret']
                except packaging.PackagingError:
                    continue
                if secret:
                    raise ValueError('OpenPGP secret key material present: ' + shown)


DEBIAN_SOURCES_TEMPLATE = 'usr/share/wros/apt/debian.sources'


def audit_network_sources(root):
    """Debian's online sources template, byte for byte, and network time for installs."""
    if regular(root, DEBIAN_SOURCES_TEMPLATE).read_bytes() != (REPO / 'os/config/apt/debian.sources').read_bytes():
        raise ValueError('image debian.sources template differs from os/config/apt/debian.sources')
    regular(root, 'usr/share/keyrings/debian-archive-keyring.pgp')
    status = subprocess.run([
        'dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-W',
        '-f=${Status}', 'systemd-timesyncd'], capture_output=True, text=True).stdout
    if status != 'install ok installed':
        raise ValueError('systemd-timesyncd is not installed in the image')


def audit(root, fixture_fingerprints=None):
    root = Path(root)
    if root.is_symlink() or root.resolve() == Path('/'):
        raise ValueError('inspect a staged image directory, never the live root')
    root = root.resolve(strict=True)
    package = subprocess.check_output([
        'dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-W',
        '-f=${Version}\t${Status}', 'greetd'], text=True).split('\t')
    if len(package) != 2 or package[1] != 'install ok installed':
        raise ValueError('maintained Debian greetd package is not installed')
    locker = subprocess.check_output([
        'dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-W',
        '-f=${Version}\t${Status}', 'vlock'], text=True).split('\t')
    if len(locker) != 2 or locker[1] != 'install ok installed':
        raise ValueError('maintained Debian vlock package is not installed')
    binaries = {}
    for relative in ('usr/sbin/greetd', 'usr/bin/wros-greeter', 'usr/bin/grimoire',
                     'usr/libexec/wros-session', 'usr/libexec/wros-keyboard',
                     'usr/libexec/wros-lock', 'usr/libexec/wros-backup', 'usr/libexec/wros-update',
                     'usr/libexec/wros-recovery',
                     'usr/bin/restic', 'usr/bin/rclone', 'usr/bin/gpgv',
                     'usr/bin/openvt', 'usr/bin/systemd-run', 'usr/bin/chvt'):
        path = regular(root, relative)
        if not path.stat().st_mode & 0o111 or path.stat().st_mode & 0o6022:
            raise ValueError('unsafe native session executable permissions: ' + relative)
        binaries[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    regular(root, 'etc/pam.d/greetd')
    regular(root, 'usr/share/doc/greetd/copyright')
    for relative in ('usr/sbin/vlock-main', 'usr/lib/vlock/modules/all.so',
                     'usr/lib/vlock/modules/nosysrq.so'):
        path = regular(root, relative)
        mode = path.stat().st_mode
        if not mode & 0o111 or mode & 0o022 or (relative != 'usr/sbin/vlock-main' and mode & 0o6000):
            raise ValueError('unsafe maintained locker permissions: ' + relative)
        # An unprivileged extraction (unsquashfs as a normal user) drops setuid; the media
        # audit then proves the bit from the squashfs listing instead.
        allowed = (0o4755,) if strict_ownership() else (0o4755, 0o755)
        if relative == 'usr/sbin/vlock-main' and mode & 0o7777 not in allowed:
            raise ValueError('maintained vlock-main mode must match the Debian package')
        binaries[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    regular(root, 'usr/share/doc/vlock/copyright')
    pam_module = regular(root, 'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so')
    if pam_module.stat().st_mode & 0o6022:
        raise ValueError('unsafe maintained PAM guard permissions')
    binaries['usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so'] = hashlib.sha256(pam_module.read_bytes()).hexdigest()
    # Staged values first: a faithful copy of a weakened policy file is refused for its values,
    # not merely for failing to match the package record.
    policy_values(root)
    # WrOS-owned installed files (lock/backup/update helpers, policy, units, release) are the
    # dpkg package 'wros': installed at os/VERSION, owning exactly the manifest, bytes matching
    # both dpkg's md5sums and the qualified repository sources.
    package_summary = audit_package(root)
    audit_update_engine(root)
    update_actions = audit_polkit(root)
    update_units = audit_units(root)
    # APT still needs the Debian archive keyring for the base system; the update engine must not.
    keyring_candidate = root / 'usr/share/keyrings/debian-archive-keyring.gpg'
    if keyring_candidate.is_symlink():
        target = keyring_candidate.readlink()
        if target != Path('debian-archive-keyring.pgp') and target != (root / 'usr/share/keyrings/debian-archive-keyring.pgp'):
            raise ValueError('missing or unsafe native session file: usr/share/keyrings/debian-archive-keyring.gpg')
        regular(root, 'usr/share/keyrings/debian-archive-keyring.pgp')
    else:
        regular(root, 'usr/share/keyrings/debian-archive-keyring.gpg')
    update_signers = audit_trust(root, fixture_fingerprints)
    audit_no_private_keys(root)
    for name in ('restic', 'rclone'):
        state = subprocess.check_output(['dpkg-query', '--admindir=' + str(root/'var/lib/dpkg'),
                                         '-W', '-f=${Status}', name], text=True)
        if state != 'install ok installed':
            raise ValueError('maintained Debian backup package is not installed: ' + name)
        regular(root, 'usr/share/doc/' + name + '/copyright')
    config = tomllib.loads(regular(root, 'etc/greetd/config.toml').read_text())
    if config != {'terminal': {'vt': 1, 'switch': True},
                  'general': {'source_profile': False},
                  'default_session': {'command': '/usr/bin/wros-greeter', 'user': '_greetd'}}:
        raise ValueError('native login must use the fixed unprivileged tty1 greeter without autologin')
    accounts = [row.split(':') for row in (root / 'etc/passwd').read_text().splitlines()
                if row.startswith('_greetd:')]
    passwords = [row.split(':')[1] for row in (root / 'etc/shadow').read_text().splitlines()
                 if row.startswith('_greetd:')]
    if (len(accounts) != 1 or len(accounts[0]) != 7 or not accounts[0][2].isdigit()
            or int(accounts[0][2]) == 0 or not accounts[0][6].endswith(('/nologin', '/false'))
            or len(passwords) != 1 or not passwords[0].startswith(('!', '*'))):
        raise ValueError('native greeter service account must be non-root, locked and nonlogin')
    for source, relative in (
            ('greetd.service', 'etc/systemd/system/greetd.service'),
            ('51-wros-greeter-keyboard.rules', 'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules'),
            ('50-wros-lock.rules', 'etc/polkit-1/rules.d/50-wros-lock.rules'),
            ('org.wros.lock.policy', 'usr/share/polkit-1/actions/org.wros.lock.policy'),
            ('vlock.pam', 'etc/pam.d/vlock'),
            ('wros-recovery.pam', 'etc/pam.d/wros-recovery'),
            ('60-wros-power-guard.conf', 'etc/systemd/logind.conf.d/60-wros-power-guard.conf'),
            ('60-wros-sleep-guard.conf', 'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf'),
            ('wros-suspend.conf', 'etc/systemd/system/systemd-suspend.service.d/60-wros-lock.conf'),
            ('60-wros-no-connectivity-check.conf', 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf'),
            ('60-wros-reboot-guard.conf', 'etc/systemd/system.conf.d/60-wros-reboot-guard.conf'),
            ('60-wros-sysrq.conf', 'etc/sysctl.d/60-wros-sysrq.conf'),
            ('60-wros-quiet-console.conf', 'etc/sysctl.d/60-wros-quiet-console.conf')):
        if regular(root, relative).read_bytes() != (REPO / 'os/config' / source).read_bytes():
            raise ValueError('native session policy differs from qualified source: ' + relative)
    # The recovery boot: its menu entry, the failed-boot check and the boot-menu settings.
    for source, relative in (('06_wros_boot_check', 'etc/grub.d/06_wros_boot_check'),
                             ('42_wros_recovery', 'etc/grub.d/42_wros_recovery'),
                             ('60-wros.cfg', 'etc/default/grub.d/60-wros.cfg')):
        if regular(root, relative).read_bytes() != (REPO / 'os/grub' / source).read_bytes():
            raise ValueError('recovery boot file differs from qualified source: ' + relative)
    enabled = root / 'etc/systemd/system/multi-user.target.wants/greetd.service'
    if not enabled.is_symlink() or os.readlink(enabled) not in (
            '../greetd.service', '/etc/systemd/system/greetd.service'):
        raise ValueError('native greetd service is not enabled')
    for name in ('getty@tty1.service', 'getty@tty8.service', 'autovt@tty8.service', 'ctrl-alt-del.target',
                 *(target + '.target' for target in SLEEP_TARGETS)):
        path = root / 'etc/systemd/system' / name
        if not path.is_symlink() or os.readlink(path) != '/dev/null':
            raise ValueError('unsafe native image: unmasked ' + name)
    maintenance = root / 'etc/systemd/system/getty@tty2.service'
    if maintenance.is_symlink() and os.readlink(maintenance) == '/dev/null':
        raise ValueError('authenticated tty2 maintenance must remain available')
    audit_network_sources(root)
    return {'schema_version': 3, 'scope': 'Staged native session/manual-lock payload/power policy/update trust; not runtime locking or update qualification',
            'passed': True, 'greetd_package_version': package[0], 'binary_sha256': binaries,
            'wros_package': package_summary, 'update_signers': update_signers,
            'update_units_enabled': update_units, 'update_polkit_actions': update_actions,
            'update_engine_uses_debian_keyring': False, 'private_key_material_absent': True,
            'vlock_package_version': locker[0], 'manual_lock_payload_present': True,
            'native_tty1_login': True, 'autologin': False, 'greeter_account_locked': True,
            'sleep_targets_masked': list(SLEEP_TARGETS), 'automatic_power_actions_disabled': True,
            'keyboard_reboot_chords_guarded': True, 'unsolicited_network_checks_disabled': True,
            'secure_locking_verified': False, 'physical_hardware_verified': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(audit(args.root), indent=2))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc)) from None
