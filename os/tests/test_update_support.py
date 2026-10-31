"""Shared fixtures for the update engine tests (no tests of its own).

Real /usr/bin/gpg, gpgv, dpkg-deb, dpkg --compare-versions and apt-get --simulate against
throwaway keys and temporary directories.  Nothing here touches host packages or state.
"""
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
GPG_ENV_BASE = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}


def load_script(name, relative):
    loader = importlib.machinery.SourceFileLoader(name, str(ROOT / relative))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def load_engine():
    engine = load_script('wros_update_engine_under_test', 'os/scripts/wros-update-engine.py')
    engine.TRUSTED_UID = os.geteuid()  # fixtures are owned by the invoking user, not root
    return engine


def rfc2822(moment):
    return format_datetime(moment.astimezone(timezone.utc), usegmt=True).replace('GMT', 'UTC')


def fresh_headers(now=None, date_ago=timedelta(hours=1), valid_for=timedelta(days=7)):
    now = now or datetime.now(timezone.utc)
    return (f'Date: {rfc2822(now - date_ago)}\n'
            f'Valid-Until: {rfc2822(now - date_ago + valid_for)}\n')


class Gpg:
    """A throwaway GNUPGHOME with named keys; each key = certify-only primary + signing subkey."""

    def __init__(self):
        self.temp = tempfile.TemporaryDirectory(prefix='wros-update-gpg-')
        self.home = Path(self.temp.name) / 'gnupg'
        self.home.mkdir(mode=0o700)
        self.env = {**GPG_ENV_BASE, 'GNUPGHOME': str(self.home)}
        self.keys = {}

    def run(self, *args, **kwargs):
        return subprocess.run(['/usr/bin/gpg', '--batch', '--yes', *args], env=self.env, check=True,
                              stdout=kwargs.pop('stdout', subprocess.DEVNULL),
                              stderr=subprocess.DEVNULL, **kwargs)

    def make_key(self, name, with_subkey=True):
        """with_subkey: certify-only primary plus a signing subkey; else one sign+cert primary."""
        email = f'{name}@wros.invalid'
        usage = 'cert' if with_subkey else 'sign,cert'
        self.run('--passphrase', '', '--pinentry-mode', 'loopback', '--quick-generate-key', email, 'rsa2048', usage, '0')
        listing = self.run('--with-colons', '--list-keys', email, stdout=subprocess.PIPE, text=True).stdout
        primary = next(line.split(':')[9] for line in listing.splitlines() if line.startswith('fpr:'))
        subkey = None
        if with_subkey:
            self.run('--passphrase', '', '--pinentry-mode', 'loopback', '--quick-add-key', primary, 'rsa2048', 'sign', '0')
            listing = self.run('--with-colons', '--list-keys', '--with-subkey-fingerprint', email,
                               stdout=subprocess.PIPE, text=True).stdout
            subkey = [line.split(':')[9] for line in listing.splitlines() if line.startswith('fpr:')][1]
        self.keys[name] = {'email': email, 'primary': primary, 'subkey': subkey}
        return self.keys[name]

    def export(self, names, path):
        emails = [self.keys[n]['email'] for n in names]
        self.run('--output', str(path), '--export', *emails)

    def sign_clear(self, text, name, subkey=True):
        key = self.keys[name]
        signer = key['subkey'] + '!' if subkey and key['subkey'] else key['primary']
        return subprocess.run(['/usr/bin/gpg', '--batch', '--yes', '--pinentry-mode', 'loopback', '--passphrase', '',
                               '--local-user', signer, '--clearsign'],
                              input=text, text=True, env=self.env, check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout

    def sign_detached(self, data, name, subkey=True):
        key = self.keys[name]
        signer = key['subkey'] + '!' if subkey and key['subkey'] else key['primary']
        return subprocess.run(['/usr/bin/gpg', '--batch', '--yes', '--pinentry-mode', 'loopback', '--passphrase', '',
                               '--local-user', signer, '--detach-sign'],
                              input=data, env=self.env, check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout

    def cleanup(self):
        self.temp.cleanup()


_SHARED = {}


def shared_gpg():
    """Two keys (archive, other) generated once per test process."""
    if 'gpg' not in _SHARED:
        gpg = Gpg()
        gpg.make_key('archive')
        gpg.make_key('other')
        gpg.make_key('solo', with_subkey=False)
        _SHARED['gpg'] = gpg
    return _SHARED['gpg']


def make_deb(directory, name, version, arch='all', depends='', provides='', extra=''):
    directory = Path(directory)
    tree = directory / f'tree-{name}-{version.replace(":", "_")}'
    (tree / 'DEBIAN').mkdir(parents=True)
    control = (f'Package: {name}\nVersion: {version}\nArchitecture: {arch}\n'
               'Maintainer: QA <qa@wros.invalid>\nDescription: disposable update fixture\n')
    if depends:
        control += f'Depends: {depends}\n'
    if provides:
        control += f'Provides: {provides}\n'
    (tree / 'DEBIAN/control').write_text(control + extra)
    (tree / 'usr/share/wros-fixture').mkdir(parents=True)
    (tree / f'usr/share/wros-fixture/{name}').write_text(f'{name} {version}\n')
    path = directory / f'{name}_{version.replace(":", "_")}_{arch}.deb'
    subprocess.run(['/usr/bin/dpkg-deb', '--build', '--root-owner-group', str(tree), str(path)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return path


def control_fields(deb):
    out = subprocess.run(['/usr/bin/dpkg-deb', '-f', str(deb)], check=True, stdout=subprocess.PIPE, text=True).stdout
    fields = {}
    key = None
    for line in out.splitlines():
        if line.startswith((' ', '\t')) and key:
            fields[key] += ' ' + line.strip()
        else:
            key, _, value = line.partition(':')
            fields[key] = value.strip()
    return fields


def write_bundle(bundle_dir, debs, gpg, signer='archive', *, headers=None, suite='wros-updates', origin='WrOS',
                 detached=False, subkey=True, pool_names=None, now=None):
    """Hand-rolled bundle writer (independent of tools/build-update-bundle.py)."""
    bundle = Path(bundle_dir)
    (bundle / 'pool').mkdir(parents=True, exist_ok=True)
    stanzas = []
    for deb in sorted(debs, key=lambda d: control_fields(d)['Package']):
        fields = control_fields(deb)
        rel = f'pool/{deb.name}'
        data = Path(deb).read_bytes()
        (bundle / rel).write_bytes(data)
        lines = [f'Package: {fields["Package"]}', f'Version: {fields["Version"]}',
                 f'Architecture: {fields["Architecture"]}']
        for key in ('Pre-Depends', 'Depends', 'Breaks', 'Conflicts', 'Provides'):
            if key in fields:
                lines.append(f'{key}: {fields[key]}')
        lines += [f'Filename: {rel}', f'Size: {len(data)}', f'SHA256: {hashlib.sha256(data).hexdigest()}']
        stanzas.append('\n'.join(lines) + '\n')
    packages = ('\n'.join(stanzas)).encode()
    (bundle / 'Packages').write_bytes(packages)
    if headers is None:
        headers = fresh_headers(now)
    release = (f'Origin: {origin}\nSuite: {suite}\nCodename: trixie\n{headers}'
               f'SHA256:\n {hashlib.sha256(packages).hexdigest()} {len(packages)} Packages\n')
    if detached:
        (bundle / 'Release').write_text(release)
        (bundle / 'Release.gpg').write_bytes(gpg.sign_detached(release.encode(), signer, subkey))
    else:
        (bundle / 'InRelease').write_text(gpg.sign_clear(release, signer, subkey))
    return bundle


class SystemFixture:
    """A fake root: dpkg dir with status, state dir, trigger path, keyring and pin file."""

    def __init__(self, gpg, status='', signers=None, keyring_names=('archive',)):
        self.temp = tempfile.TemporaryDirectory(prefix='wros-update-sys-')
        self.root = Path(self.temp.name)
        self.dpkg = self.root / 'dpkg'
        (self.dpkg / 'updates').mkdir(parents=True)
        self.status = self.dpkg / 'status'
        self.status.write_text(status)
        self.state = self.root / 'state'
        self.trigger = self.root / 'system-update'
        self.keyring = self.root / 'wros-archive-keyring.gpg'
        gpg.export(keyring_names, self.keyring)
        self.signers = self.root / 'update-signers'
        pins = signers if signers is not None else [gpg.keys['archive']['primary']]
        self.signers.write_text('# pinned primary fingerprints\n' + ''.join(f'{p}  # test key\n' for p in pins))
        for path in (self.keyring, self.signers):
            path.chmod(0o644)

    def kwargs(self):
        return {'keyring': self.keyring, 'signers': self.signers, 'dpkg_dir': self.dpkg,
                'state_dir': self.state, 'trigger_path': self.trigger, 'host_arch': 'amd64'}

    def cleanup(self):
        self.temp.cleanup()


def status_stanza(name, version, state='install ok installed', arch='all', extra=''):
    return (f'Package: {name}\nStatus: {state}\nArchitecture: {arch}\nVersion: {version}\n'
            f'Description: fixture\n{extra}\n')
