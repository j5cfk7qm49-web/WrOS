"""The wros package builder, its determinism, dpkg installation into a staging root,
the install-files.sh staging path and the update-trust validators. Real dpkg-deb, dpkg,
gpg and bash; no host state. Staging runs need the pseudo-root the Debian test chroot gives."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_native_image as native  # noqa: E402  (fixture helpers: synthetic keyrings, packets)

packaging = native.packaging
EPOCH = 1700000000
ENV = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'LC_ALL': 'C', 'TZ': 'UTC'}
GRIMOIRE = b'#!/bin/sh\necho grimoire stub\n'
DISPLAY_HW = b'#!/bin/sh\necho display helper stub\n'
SIGNER_UID = 'WrOS RC Update Signer <rc-update@wros.invalid>'


def run(*command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, text=True, **kwargs)


def tool_available(name):
    return shutil.which(name) is not None


def pseudo_root_prefix():
    """Command prefix that runs install-files.sh as (pseudo-)root: nothing when already root, otherwise an
    unprivileged user namespace, so the non-root qualification account exercises staging too. None when
    neither is possible."""
    if os.geteuid() == 0:
        return []
    # A chrooted caller may not create a user namespace (EPERM), so fakeroot is the fallback; the
    # script only needs file ownership and dpkg --root, which fakeroot fakes.
    for prefix in (['unshare', '--user', '--map-root-user', '--'], ['fakeroot', '--']):
        if tool_available(prefix[0]) and subprocess.run(prefix + ['sh', '-c', 'test "$(id -u)" = 0'],
                                                        capture_output=True).returncode == 0:
            return prefix
    return None


PSEUDO_ROOT = pseudo_root_prefix()


def copy_repo(destination):
    """The subset of the checkout the packaging path reads, as an editable copy."""
    destination = Path(destination)
    for relative in ('packaging', 'os/config', 'os/systemd', 'os/grub', 'os/scripts', 'os/helpers', 'os/licenses',
                     'tools/build-wros-debs.py', 'tools/stage-prose-runtime.py', 'tools/prepare-prose-tools.py',
                     'tools/audit-native-image.py',
                     'assets/dict/DICTIONARY-LICENSE', 'LICENSE', 'os/VERSION'):
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target, symlinks=True, ignore=shutil.ignore_patterns('__pycache__'))
        else:
            shutil.copyfile(source, target)
    (destination / 'docs').mkdir()
    for source in (ROOT / 'docs').iterdir():
        if source.is_file() and source.suffix in ('.md', '.json', '.html'):
            shutil.copyfile(source, destination / 'docs' / source.name)
    (destination / 'os/scripts/install-files.sh').chmod(0o755)
    return destination


def add_update_engine(repo):
    """Stand-ins for the update-engine workstream's files, inside a throwaway copy only."""
    for name in ('wros-update', 'wros-update-stage', 'wros-update-cancel', 'wros-update-repair', 'wros-update-media',
                 'wros-update-fetch', 'wros-update-apply', 'wros-update-rollback', 'wros-update-healthy'):
        (repo / 'os/scripts' / name).write_bytes(('#!/usr/bin/python3 -I\n# stand-in %s\n' % name).encode())
    (repo / 'os/scripts/wros-update-engine.py').write_bytes(b'# stand-in engine\n')
    (repo / 'os/config/org.wros.update.policy').write_bytes(native.UPDATE_POLICY)
    (repo / 'os/config/50-wros-update.rules').write_bytes(native.UPDATE_RULES)
    (repo / 'os/systemd/wros-offline-update.service').write_bytes(
        native.PLACEHOLDERS['etc/systemd/system/wros-offline-update.service'])
    (repo / 'os/systemd/wros-update-reconcile.service').write_bytes(
        native.PLACEHOLDERS['etc/systemd/system/wros-update-reconcile.service'])


def stub_inputs(base):
    base = Path(base)
    binaries, keymaps = base / 'binaries', base / 'keymaps'
    binaries.mkdir()
    keymaps.mkdir()
    (binaries / 'grimoire').write_bytes(GRIMOIRE)
    (binaries / 'wros-display-hw').write_bytes(DISPLAY_HW)
    for name in ('qwerty', 'colemak', 'colemak-dh-ansi', 'colemak-dh-iso'):
        (keymaps / (name + '.map')).write_bytes(('keymap ' + name + '\n').encode())
    return binaries, keymaps


TREE = (ROOT / 'os/VERSION').read_text().splitlines()[0]


def build(repo, out, binaries, keymaps, version=TREE, commit='a' * 40, epoch=EPOCH):
    return packaging.build_deb(binaries_dir=binaries, version=version, output_dir=out, repo=repo,
                               keymaps_dir=keymaps, commit=commit, epoch=epoch)


def dpkg_skeleton(root, extra_status=''):
    root = Path(root)
    for sub in ('var/lib/dpkg/info', 'var/lib/dpkg/updates', 'var/log', 'etc'):
        (root / sub).mkdir(parents=True, exist_ok=True)
    (root / 'var/lib/dpkg/available').write_text('')
    (root / 'etc/debian_version').write_text('13.0\n')
    stanzas = ''.join('Package: %s\nStatus: install ok installed\nArchitecture: amd64\nVersion: 1.0\n'
                      'Maintainer: Fixture <fixture@example.invalid>\nDescription: dependency fixture\n\n' % name
                      for name in packaging.load_manifest(ROOT)['package']['depends'])
    (root / 'var/lib/dpkg/status').write_text(stanzas + extra_status)
    return root


def dpkg_root_install(root, deb):
    command = ['dpkg', '--root=' + str(root), '--force-confold', '--force-unsafe-io', '--no-triggers',
               '--log=' + str(Path(root) / 'var/log/dpkg.log')]
    if os.geteuid() != 0:
        command.append('--force-not-root')
    # The postinst honours DPKG_ROOT, so it runs here without a chroot, as in image staging.
    command.append('--force-script-chrootless')
    return run(*command, '--install', str(deb), env=ENV)


def installed_version(root):
    return run('dpkg-query', '--admindir=' + str(Path(root) / 'var/lib/dpkg'), '-W', '-f=${Version}',
               'wros', env=ENV).stdout


class GpgKeys:
    """Throwaway real OpenPGP keys, generated once per test class."""

    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='wros-gpg-')
        self.home = Path(self.tmp.name) / 'home'
        self.home.mkdir(mode=0o700)
        self.env = {**ENV, 'GNUPGHOME': str(self.home)}

    def generate(self, uid):
        run('gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase', '', '--quick-generate-key', uid,
            'ed25519', 'sign', 'never', env=self.env)
        out = run('gpg', '--batch', '--with-colons', '--fingerprint', uid, env=self.env).stdout
        return next(row.split(':')[9] for row in out.splitlines() if row.startswith('fpr:'))

    def public(self, *fingerprints):
        return subprocess.run(['gpg', '--batch', '--export', *fingerprints], check=True, capture_output=True,
                              env=self.env).stdout

    def secret(self, fingerprint):
        return subprocess.run(['gpg', '--batch', '--pinentry-mode', 'loopback', '--passphrase', '',
                               '--export-secret-keys', fingerprint], check=True, capture_output=True,
                              env=self.env).stdout

    def close(self):
        subprocess.run(['gpgconf', '--kill', 'all'], env=self.env, capture_output=True)
        self.tmp.cleanup()


@unittest.skipUnless(tool_available('dpkg-deb') and tool_available('gpg'), 'needs the Debian packaging tools')
class Builder(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix='wros-builder-')
        cls.base = Path(cls.tmp.name)
        cls.binaries, cls.keymaps = stub_inputs(cls.base)
        cls.deb = build(ROOT, cls.base / 'out', cls.binaries, cls.keymaps)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_identical_inputs_give_identical_bytes_whatever_the_path_or_time(self):
        with tempfile.TemporaryDirectory(prefix='wros-again-') as other:
            binaries, keymaps = stub_inputs(other)
            again = build(ROOT, Path(other) / 'elsewhere', binaries, keymaps)
            self.assertEqual(again.name, self.deb.name)
            self.assertEqual(again.read_bytes(), self.deb.read_bytes())
        with tempfile.TemporaryDirectory(prefix='wros-later-') as other:
            binaries, keymaps = stub_inputs(other)
            later = build(ROOT, Path(other) / 'out', binaries, keymaps, epoch=EPOCH + 86400)
            self.assertNotEqual(later.read_bytes(), self.deb.read_bytes())

    def test_package_identity_and_honest_dependencies(self):
        fields = {name: packaging.deb_field(self.deb, name)
                  for name in ('Package', 'Version', 'Architecture', 'Maintainer', 'Depends', 'Section', 'Priority')}
        self.assertEqual(fields['Package'], 'wros')
        self.assertEqual(fields['Version'], packaging.read_version(ROOT))
        self.assertEqual(fields['Architecture'], 'amd64')
        self.assertIn('invalid', fields['Maintainer'])
        provided = {line.strip() for line in (ROOT / 'os/config/packages.txt').read_text().splitlines()}
        provided |= {'apt', 'dpkg', 'libgcc-s1', 'passwd', 'util-linux', 'systemd'}
        for dependency in fields['Depends'].split(', '):
            self.assertIn(dependency, provided, 'depends on something the image does not provide')
        self.assertEqual(self.deb.name, f'wros_{TREE}_amd64.deb')
        archive = run('ar', 't', str(self.deb)).stdout.split()
        self.assertEqual(archive, ['debian-binary', 'control.tar.xz', 'data.tar.xz'])

    def test_every_entry_is_owned_by_root_and_none_is_privileged_or_writable(self):
        listing = run('dpkg-deb', '--contents', str(self.deb)).stdout.splitlines()
        self.assertGreater(len(listing), 100)
        for row in listing:
            permissions, owner, _, date, _, name = row.split(None, 5)
            self.assertEqual(owner, 'root/root', row)
            self.assertFalse(set(permissions) & set('sStT'), row)
            self.assertEqual((permissions[5], permissions[8]), ('-', '-'), 'group/world writable: ' + row)
            self.assertEqual(date, '2023-11-14', row)
            self.assertIn(permissions[0], 'd-', 'no symlinks or devices expected: ' + row)

    def test_modes_follow_the_manifest_and_launchers_are_0755(self):
        extract = self.base / 'extract'
        run('dpkg-deb', '-x', str(self.deb), str(extract))
        specs = packaging.expand(packaging.load_manifest(ROOT), ROOT)
        for spec in specs:
            if spec.missing:
                continue
            self.assertEqual((extract / spec.dest).stat().st_mode & 0o7777, spec.mode, spec.dest)
        self.assertEqual((extract / 'usr/libexec/wros-lock').stat().st_mode & 0o7777, 0o755)
        self.assertEqual((extract / 'usr/bin/grimoire').read_bytes(), GRIMOIRE)
        for path in extract.rglob('*'):
            self.assertEqual(path.lstat().st_mtime, EPOCH, str(path))

    def test_md5sums_conffiles_and_only_the_generated_postinst(self):
        control = self.base / 'control'
        run('dpkg-deb', '-e', str(self.deb), str(control))
        self.assertEqual(sorted(p.name for p in control.iterdir()), ['conffiles', 'control', 'md5sums', 'postinst'])
        self.assertEqual(stat.S_IMODE((control / 'postinst').stat().st_mode), 0o755)
        run('sh', '-n', str(control / 'postinst'))
        self.assertEqual((control / 'conffiles').read_text(), '/etc/writingdeck/keyboard.json\n')
        extract = self.base / 'extract2'
        run('dpkg-deb', '-x', str(self.deb), str(extract))
        rows = (control / 'md5sums').read_text().splitlines()
        self.assertEqual(rows, sorted(rows, key=lambda row: row.split('  ', 1)[1]))
        for row in rows:
            digest, name = row.split('  ', 1)
            self.assertEqual(hashlib.md5((extract / name).read_bytes()).hexdigest(), digest, name)
        files = {p.relative_to(extract).as_posix() for p in extract.rglob('*') if p.is_file()}
        self.assertEqual({row.split('  ', 1)[1] for row in rows}, files)

    def test_postinst_enables_every_manifest_unit_inside_dpkg_root_and_leaves_grub_alone(self):
        control = self.base / 'control-postinst'
        run('dpkg-deb', '-e', str(self.deb), str(control))
        manifest = packaging.load_manifest(ROOT)
        with tempfile.TemporaryDirectory(prefix='wros-postinst-') as tmp:
            root = Path(tmp)
            masked = root / 'etc/systemd/system/multi-user.target.wants/wros-display.service'
            masked.parent.mkdir(parents=True)
            masked.symlink_to('/dev/null')  # an existing link is the administrator's choice
            marker = root / 'update-grub-ran'
            fake = root / 'bin'
            fake.mkdir()
            (fake / 'update-grub').write_text(f'#!/bin/sh\ntouch {marker}\n')
            (fake / 'update-grub').chmod(0o755)
            env = dict(ENV, DPKG_ROOT=str(root), PATH=f'{fake}:' + ENV['PATH'])
            for _ in range(2):  # an upgrade runs it again
                run('sh', str(control / 'postinst'), 'configure', '0.1.0~rc4', env=env)
            for item in manifest['enable']:
                link = root / 'etc/systemd/system' / (item['target'] + '.wants') / item['unit']
                if link == masked:
                    self.assertEqual(os.readlink(link), '/dev/null')
                else:
                    self.assertEqual(os.readlink(link), '../' + item['unit'])
            for target in ('multi-user.target', 'system-update.target'):  # an update boot is a good boot
                self.assertIn({'unit': 'wros-boot-ok.service', 'target': target,
                               'verify_install_section': True}, manifest['enable'])
            self.assertFalse(marker.exists())  # staging leaves the boot menu to the installer
            self.assertEqual(sorted(p.name for p in root.iterdir()), ['bin', 'etc'])

    def test_googles_client_is_packaged_only_when_the_build_is_given_it(self):
        listing = run('dpkg-deb', '-c', str(self.deb)).stdout
        self.assertNotIn('usr/share/wros/google-client.json', listing)  # never from the repository
        self.assertIn('./usr/lib/wros-backup/link.py', listing)
        clients = self.base / 'google-client.json'
        clients.write_text(json.dumps({'linking': {'client_id': 'web-id', 'client_secret': 'web-secret'},
                                       'mailbox': {'client_id': 'tv-id', 'client_secret': 'tv-secret'}}))
        out = self.base / 'with-client'
        deb = packaging.build_deb(binaries_dir=self.binaries, version=TREE, output_dir=out, repo=ROOT,
                                  keymaps_dir=self.keymaps, commit='a' * 40, epoch=EPOCH, google_clients=clients)
        extract = self.base / 'with-client-x'
        run('dpkg-deb', '-x', str(deb), str(extract))
        packaged = extract / 'usr/share/wros/google-client.json'
        self.assertEqual(json.loads(packaged.read_text()), json.loads(clients.read_text()))
        self.assertEqual(stat.S_IMODE(packaged.stat().st_mode), 0o644)  # the writer's helper reads it
        clients.write_text('{"linking": {}}')
        with self.assertRaises(packaging.PackagingError):
            packaging.build_deb(binaries_dir=self.binaries, version=TREE, output_dir=self.base / 'bad', repo=ROOT,
                                keymaps_dir=self.keymaps, commit='a' * 40, epoch=EPOCH, google_clients=clients)

    def test_release_file_derives_from_version_commit_and_source_date(self):
        extract = self.base / 'extract3'
        run('dpkg-deb', '-x', str(self.deb), str(extract))
        self.assertEqual((extract / 'etc/writingdeck/release').read_text(),
                         f'WRITINGDECK_VERSION={TREE}\nWROS_COMMIT=' + 'a' * 40 + '\nBUILD_DATE=2023-11-14\n'
                         'GRIMOIRE_BASE=0.7.5\nBASE_COMMIT=f3ca1e8\n')

    def test_a_newer_version_builds_from_modified_inputs_and_orders_after_the_baseline(self):
        with tempfile.TemporaryDirectory(prefix='wros-next-') as other:
            other = Path(other)
            repo = copy_repo(other / 'repo')
            (repo / 'os/config/60-wros-sysrq.conf').write_bytes(
                (ROOT / 'os/config/60-wros-sysrq.conf').read_bytes() + b'# next release\n')
            binaries, keymaps = stub_inputs(other)
            (binaries / 'grimoire').write_bytes(GRIMOIRE + b'# next\n')
            newer = build(repo, other / 'out', binaries, keymaps, version='0.1.0~rc2', commit='b' * 40,
                          epoch=EPOCH + 3600)
            self.assertEqual(newer.name, 'wros_0.1.0~rc2_amd64.deb')
            self.assertEqual(packaging.deb_field(newer, 'Version'), '0.1.0~rc2')
            run('dpkg', '--compare-versions', '0.1.0~rc1', 'lt', '0.1.0~rc2', env=ENV)
            run('dpkg', '--compare-versions', '0.1.0~rc2', 'lt', '0.1.0', env=ENV)
            run('dpkg', '--compare-versions', '0.1.0', 'lt', '0.1.1~dev1', env=ENV)
            run('dpkg', '--compare-versions', '0.1.1~dev1', 'lt', '0.1.1', env=ENV)
            new_x, old_x = other / 'new', self.base / 'old-x'
            run('dpkg-deb', '-x', str(newer), str(new_x))
            run('dpkg-deb', '-x', str(self.deb), str(old_x))
            changed = sorted(p.relative_to(new_x).as_posix() for p in new_x.rglob('*') if p.is_file()
                             and (not (old_x / p.relative_to(new_x)).exists()
                                  or (old_x / p.relative_to(new_x)).read_bytes() != p.read_bytes()))
            self.assertEqual(changed, ['etc/sysctl.d/60-wros-sysrq.conf', 'etc/writingdeck/release', 'usr/bin/grimoire'])
            self.assertIn('WRITINGDECK_VERSION=0.1.0~rc2', (new_x / 'etc/writingdeck/release').read_text())

    def test_invalid_versions_and_missing_inputs_are_refused(self):
        for version in ('', '1:0.1.0', 'x1', '0.1 0', '../0.1', '0.1.0\n', '0.1.0;rm'):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as out:
                with self.assertRaises(packaging.PackagingError):
                    build(ROOT, out, self.binaries, self.keymaps, version=version)
        with tempfile.TemporaryDirectory() as other:
            binaries, keymaps = stub_inputs(other)
            (binaries / 'grimoire').unlink()
            with self.assertRaisesRegex(packaging.PackagingError, 'missing build input: grimoire'):
                build(ROOT, Path(other) / 'out', binaries, keymaps)
            (binaries / 'grimoire').write_bytes(b'')
            with self.assertRaisesRegex(packaging.PackagingError, 'missing build input'):
                build(ROOT, Path(other) / 'out', binaries, keymaps)
            (binaries / 'grimoire').write_bytes(GRIMOIRE)
            (keymaps / 'colemak.map').unlink()
            with self.assertRaisesRegex(packaging.PackagingError, 'colemak.map'):
                build(ROOT, Path(other) / 'out', binaries, keymaps)
            with self.assertRaisesRegex(packaging.PackagingError, 'binaries-dir'):
                packaging.build_deb(binaries_dir=Path(other) / 'nope', version='0.1.0~rc1', output_dir=other,
                                    repo=ROOT, keymaps_dir=keymaps, commit='a' * 40, epoch=EPOCH)

    def test_every_missing_repository_source_is_an_error_including_the_update_engine(self):
        with tempfile.TemporaryDirectory(prefix='wros-missing-') as other:
            other = Path(other)
            repo = copy_repo(other / 'repo')
            binaries, keymaps = stub_inputs(other)
            full = build(repo, other / 'full', binaries, keymaps)
            names = {row.split()[-1] for row in run('dpkg-deb', '--contents', str(full)).stdout.splitlines()}
            for packaged in ('./usr/libexec/wros-update-stage', './usr/lib/wros-update/engine.py',
                             './etc/systemd/system/wros-update-reconcile.service'):
                self.assertIn(packaged, names)
            for index, relative in enumerate(('os/config/60-wros-sysrq.conf', 'os/scripts/wros-update-stage',
                                              'os/scripts/wros-update-engine.py',
                                              'os/systemd/wros-update-reconcile.service')):
                with self.subTest(missing=relative):
                    saved = (repo / relative).read_bytes()
                    (repo / relative).unlink()
                    with self.assertRaisesRegex(packaging.PackagingError, 'missing source file'):
                        build(repo, other / ('out%d' % index), binaries, keymaps)
                    (repo / relative).write_bytes(saved)

    @unittest.skipUnless(tool_available('cc') and tool_available('pkg-config')
                         and subprocess.run(['pkg-config', '--exists', 'libdrm']).returncode == 0,
                         'needs a C compiler and libdrm development files')
    def test_display_helper_is_compiled_when_not_supplied_and_elf_inputs_are_stripped(self):
        with tempfile.TemporaryDirectory(prefix='wros-elf-') as other:
            other = Path(other)
            binaries, keymaps = stub_inputs(other)
            (binaries / 'wros-display-hw').unlink()
            source = other / 'g.c'
            source.write_text('int main(void) { return 0; }\n')
            run('cc', '-g', '-o', str(binaries / 'grimoire'), str(source))
            unstripped = (binaries / 'grimoire').stat().st_size
            deb = build(ROOT, other / 'out', binaries, keymaps)
            extract = other / 'x'
            run('dpkg-deb', '-x', str(deb), str(extract))
            for name in ('usr/bin/grimoire', 'usr/libexec/wros-display-hw'):
                self.assertTrue((extract / name).read_bytes().startswith(b'\x7fELF'), name)
            self.assertLess((extract / 'usr/bin/grimoire').stat().st_size, unstripped)
            self.assertNotIn(b'.symtab', (extract / 'usr/bin/grimoire').read_bytes())

    def test_command_line_prints_the_package_and_matches_the_library(self):
        with tempfile.TemporaryDirectory(prefix='wros-cli-') as out:
            result = subprocess.run(
                [sys.executable, str(ROOT / 'tools/build-wros-debs.py'), '--binaries-dir', str(self.binaries),
                 '--keymaps-dir', str(self.keymaps), '--display-helper', str(self.binaries / 'wros-display-hw'),
                 '--output', out, '--commit', 'a' * 40, '--source-date-epoch', str(EPOCH)],
                check=True, capture_output=True, text=True)
            info = json.loads(result.stdout)
            self.assertEqual((info['package'], info['version'], info['architecture']), ('wros', TREE, 'amd64'))
            self.assertEqual(Path(info['path']).read_bytes(), self.deb.read_bytes())
            self.assertEqual(info['sha256'], hashlib.sha256(self.deb.read_bytes()).hexdigest())
            newer = subprocess.run(
                [sys.executable, str(ROOT / 'tools/build-wros-debs.py'), '--binaries-dir', str(self.binaries),
                 '--keymaps-dir', str(self.keymaps), '--display-helper', str(self.binaries / 'wros-display-hw'),
                 '--output', out, '--commit', 'a' * 40, '--source-date-epoch', str(EPOCH), '--version', '0.1.0~rc2'],
                check=True, capture_output=True, text=True)
            self.assertEqual(json.loads(newer.stdout)['version'], '0.1.0~rc2')
            bad = subprocess.run([sys.executable, str(ROOT / 'tools/build-wros-debs.py'), '--binaries-dir',
                                  str(self.binaries), '--output', out, '--version', 'bad version'],
                                 capture_output=True, text=True)
            self.assertNotEqual(bad.returncode, 0)
            self.assertIn('not a Debian', bad.stderr)


@unittest.skipUnless(tool_available('dpkg-deb') and tool_available('dpkg'), 'needs dpkg')
class DpkgRootInstall(unittest.TestCase):
    """The package installs, upgrades, verifies and preserves the writer's layout under dpkg --root."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix='wros-dpkg-')
        cls.base = Path(cls.tmp.name)
        binaries, keymaps = stub_inputs(cls.base)
        cls.rc1 = build(ROOT, cls.base / 'out1', binaries, keymaps, version='0.1.0~rc1')
        repo = copy_repo(cls.base / 'repo2')
        (repo / 'os/config/60-wros-sysrq.conf').write_bytes(
            (ROOT / 'os/config/60-wros-sysrq.conf').read_bytes() + b'# rc2\n')
        (repo / 'os/config/keyboard.json').write_bytes((ROOT / 'os/config/keyboard.json').read_bytes() + b'\n')
        (binaries / 'grimoire').write_bytes(GRIMOIRE + b'# rc2\n')
        cls.rc2 = build(repo, cls.base / 'out2', binaries, keymaps, version='0.1.0~rc2', commit='b' * 40)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_install_records_the_package_and_verifies_clean(self):
        root = dpkg_skeleton(self.base / 'root-a')
        dpkg_root_install(root, self.rc1)
        self.assertEqual(installed_version(root), '0.1.0~rc1')
        status = (root / 'var/lib/dpkg/status').read_text()
        self.assertIn('Package: wros\nStatus: install ok installed', status)
        self.assertIn('Version: 0.1.0~rc1\n', status)
        listing = run('dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-L', 'wros', env=ENV).stdout.split()
        self.assertIn('/usr/libexec/wros-lock', listing)
        self.assertIn('/etc/writingdeck/release', listing)
        verify = subprocess.run(['dpkg', '--root=' + str(root), '--verify', 'wros'], capture_output=True, text=True, env=ENV)
        self.assertEqual((verify.returncode, verify.stdout), (0, ''), verify.stderr)
        self.assertIn('/etc/writingdeck/keyboard.json', (root / 'var/lib/dpkg/info/wros.conffiles').read_text())
        for path in (root / 'usr/libexec').iterdir():
            self.assertEqual(path.stat().st_mode & 0o7000, 0)

    def test_upgrade_replaces_policy_and_release_but_keeps_the_writers_layout(self):
        root = dpkg_skeleton(self.base / 'root-b')
        dpkg_root_install(root, self.rc1)
        layout = root / 'etc/writingdeck/keyboard.json'
        layout.write_text('{"layout": "writer-chose-this"}\n')
        dpkg_root_install(root, self.rc2)
        self.assertEqual(installed_version(root), '0.1.0~rc2')
        self.assertEqual(layout.read_text(), '{"layout": "writer-chose-this"}\n')
        self.assertTrue((root / 'etc/sysctl.d/60-wros-sysrq.conf').read_bytes().endswith(b'# rc2\n'))
        self.assertIn('WRITINGDECK_VERSION=0.1.0~rc2', (root / 'etc/writingdeck/release').read_text())
        self.assertEqual((root / 'usr/bin/grimoire').read_bytes(), GRIMOIRE + b'# rc2\n')

    def test_policy_files_are_not_conffiles_so_local_drift_is_replaced(self):
        root = dpkg_skeleton(self.base / 'root-c')
        dpkg_root_install(root, self.rc1)
        (root / 'etc/sysctl.d/60-wros-sysrq.conf').write_text('kernel.sysrq = 1\n')
        dpkg_root_install(root, self.rc2)
        self.assertTrue((root / 'etc/sysctl.d/60-wros-sysrq.conf').read_bytes().endswith(b'# rc2\n'))

    def test_unsatisfied_dependencies_stop_the_install(self):
        root = dpkg_skeleton(self.base / 'root-d')
        status = root / 'var/lib/dpkg/status'
        status.write_text(status.read_text().replace('Package: vlock\n', 'Package: vlock-absent\n'))
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            dpkg_root_install(root, self.rc1)
        self.assertIn('vlock', raised.exception.stderr)
        status_text = status.read_text()
        self.assertIn('Package: wros\nStatus: install ok unpacked', status_text)
        self.assertNotIn('Package: wros\nStatus: install ok installed', status_text)


def add_gpg_environment(case):
    case.keys = GpgKeys()
    case.addClassCleanup(case.keys.close)


@unittest.skipUnless(tool_available('gpg'), 'needs gpg')
class UpdateTrustValidation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        add_gpg_environment(cls)
        cls.fpr = cls.keys.generate(SIGNER_UID)
        cls.other = cls.keys.generate('WrOS Backup Signer <backup@wros.invalid>')
        cls.test_fpr = cls.keys.generate('WrOS Test Archive <archive@wros.invalid>')

    def check(self, keyring, pin, **kwargs):
        return packaging.check_trust_material(keyring, pin, **kwargs)

    def test_parser_agrees_with_gpg_about_real_keys(self):
        public = self.keys.public(self.fpr)
        info = packaging.inspect_keyring(public)
        self.assertEqual(info['primaries'], [self.fpr])
        self.assertEqual(info['uids'], [SIGNER_UID])
        self.assertFalse(info['secret'])
        both = packaging.inspect_keyring(self.keys.public(self.fpr, self.other))
        self.assertEqual(sorted(both['primaries']), sorted([self.fpr, self.other]))
        self.assertTrue(packaging.inspect_keyring(self.keys.secret(self.fpr))['secret'])
        self.assertTrue(packaging.inspect_keyring(public + self.keys.secret(self.other))['secret'])

    def test_a_matching_public_keyring_and_pin_are_accepted(self):
        public = self.keys.public(self.fpr)
        pinned, uids = self.check(public, '# signer\n%s  # primary\n\n' % self.fpr.lower(), fixtures=set())
        self.assertEqual(pinned, [self.fpr])
        self.assertEqual(uids, [SIGNER_UID])
        two = self.keys.public(self.fpr, self.other)
        self.assertEqual(sorted(self.check(two, self.fpr + '\n' + self.other + '\n', fixtures=set())[0]),
                         sorted([self.fpr, self.other]))

    def test_refusals(self):
        public = self.keys.public(self.fpr)
        both = self.keys.public(self.fpr, self.other)
        cases = [
            ('empty keyring', b'', self.fpr, 'empty'),
            ('garbage keyring', b'this is not a keyring at all', self.fpr, 'binary OpenPGP|truncated|keyring'),
            ('armored keyring', b'-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nAAAA\n', self.fpr, 'binary'),
            ('secret key material', self.keys.secret(self.fpr), self.fpr, 'secret key material'),
            ('public plus secret', public + self.keys.secret(self.other), self.fpr, 'secret key material'),
            ('empty pin', public, '# nothing\n', 'pin is empty'),
            ('pin of an absent key', public, self.other, 'not a primary key'),
            ('unpinned extra key', both, self.fpr, 'does not name'),
            ('pin is a subkey-length string', public, self.fpr[:39], 'not a 40-hex'),
            ('pin with junk', public, self.fpr + ' extra', 'not a 40-hex'),
            ('64-hex pin', public, self.fpr + '00' * 12, 'not a 40-hex'),
            ('test-named key', self.keys.public(self.test_fpr), self.test_fpr, 'test/fixture'),
        ]
        for name, keyring, pin, pattern in cases:
            with self.subTest(name=name), self.assertRaisesRegex(packaging.PackagingError, pattern):
                self.check(keyring, pin)

    def test_known_fixture_fingerprints_are_refused_case_insensitively(self):
        public = self.keys.public(self.fpr)
        with self.assertRaisesRegex(packaging.PackagingError, 'known test/fixture key'):
            self.check(public, self.fpr, fixtures={self.fpr.lower()})
        self.check(public, self.fpr, fixtures={self.other})

    def test_the_committed_fixture_list_is_well_formed(self):
        listed = packaging.load_fixture_fingerprints()
        self.assertTrue(all(len(item) == 40 for item in listed))

    def install(self, root, keyring, pin, **env):
        directory = Path(root)
        (directory / 'in').mkdir(exist_ok=True)
        environ = {}
        if keyring is not None:
            (directory / 'in/keyring.gpg').write_bytes(keyring)
            environ['WROS_UPDATE_KEYRING'] = str(directory / 'in/keyring.gpg')
        if pin is not None:
            (directory / 'in/signers').write_bytes(pin)
            environ['WROS_UPDATE_SIGNERS'] = str(directory / 'in/signers')
        environ.update(env)
        (directory / 'image').mkdir(exist_ok=True)
        return packaging.install_trust(directory / 'image', environ=environ, fixtures=set())

    def test_install_writes_modes_and_normalised_pin_and_nothing_on_refusal(self):
        with tempfile.TemporaryDirectory(prefix='wros-trust-') as tmp:
            public = self.keys.public(self.fpr)
            self.assertEqual(self.install(tmp, public, ('# c\n' + self.fpr.lower() + '\n').encode()), [self.fpr])
            image = Path(tmp) / 'image'
            self.assertEqual((image / packaging.KEYRING_DEST).read_bytes(), public)
            self.assertEqual((image / packaging.SIGNERS_DEST).read_text().splitlines()[1], self.fpr)
            for relative in (packaging.KEYRING_DEST, packaging.SIGNERS_DEST):
                self.assertEqual((image / relative).stat().st_mode & 0o7777, 0o644)
            self.assertEqual((image / 'usr/share/wros').stat().st_mode & 0o7777, 0o755)
        refusals = {
            'keyring unset': (None, b'x', 'WROS_UPDATE_KEYRING is not set'),
            'signers unset': (self.keys.public(self.fpr), None, 'WROS_UPDATE_SIGNERS is not set'),
            'keyring empty': (b'', b'x', 'WROS_UPDATE_KEYRING is empty'),
            'signers empty': (self.keys.public(self.fpr), b'', 'WROS_UPDATE_SIGNERS is empty'),
            'signers not text': (self.keys.public(self.fpr), b'\xff\xfe', 'UTF-8'),
            'secret keyring': (self.keys.secret(self.fpr), self.fpr.encode(), 'secret key material'),
            'mismatched pin': (self.keys.public(self.fpr), self.other.encode(), 'not a primary key'),
        }
        for name, (keyring, pin, pattern) in refusals.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory(prefix='wros-trust-') as tmp:
                with self.assertRaisesRegex(packaging.PackagingError, pattern):
                    self.install(tmp, keyring, pin)
                self.assertEqual(list((Path(tmp) / 'image').rglob('*')), [])

    def test_symlinked_or_missing_input_files_are_refused(self):
        with tempfile.TemporaryDirectory(prefix='wros-trust-') as tmp:
            tmp = Path(tmp)
            (tmp / 'real').write_bytes(self.keys.public(self.fpr))
            (tmp / 'link').symlink_to(tmp / 'real')
            (tmp / 'pin').write_text(self.fpr)
            for keyring in (tmp / 'link', tmp / 'absent', tmp):
                with self.subTest(keyring=keyring.name), self.assertRaisesRegex(packaging.PackagingError, 'regular file'):
                    packaging.install_trust(tmp / 'image', environ={
                        'WROS_UPDATE_KEYRING': str(keyring), 'WROS_UPDATE_SIGNERS': str(tmp / 'pin')}, fixtures=set())

    def test_symlinked_destination_directory_is_refused(self):
        with tempfile.TemporaryDirectory(prefix='wros-trust-') as tmp:
            tmp = Path(tmp)
            (tmp / 'elsewhere').mkdir()
            (tmp / 'image/usr/share').mkdir(parents=True)
            (tmp / 'image/usr/share/wros').symlink_to(tmp / 'elsewhere')
            with self.assertRaisesRegex(packaging.PackagingError, 'symlinked'):
                self.install(tmp, self.keys.public(self.fpr), self.fpr.encode())
            self.assertEqual(list((tmp / 'elsewhere').iterdir()), [])


class UnitEnablement(unittest.TestCase):
    MANIFEST = {'enable': [
        {'unit': 'a.service', 'target': 'multi-user.target', 'verify_install_section': False},
        {'unit': 'b.service', 'target': 'system-update.target', 'verify_install_section': True},
        {'unit': 'c.service', 'target': 'multi-user.target', 'verify_install_section': True, 'pending': True}]}

    def stage(self, tmp, **units):
        system = Path(tmp) / 'etc/systemd/system'
        system.mkdir(parents=True, exist_ok=True)
        for name, text in units.items():
            (system / (name + '.service')).write_text(text)
        return Path(tmp)

    def test_links_are_created_only_for_units_that_declare_their_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self.stage(tmp, a='[Unit]\nDescription=a\n', b='[Install]\nWantedBy=system-update.target\n',
                              c='[Install]\nWantedBy=graphical.target multi-user.target\n')
            log = []
            self.assertEqual(packaging.enable_units(root, self.MANIFEST, log.append),
                             ['a.service', 'b.service', 'c.service'])
            system = root / 'etc/systemd/system'
            self.assertEqual(os.readlink(system / 'multi-user.target.wants/a.service'), '../a.service')
            self.assertEqual(os.readlink(system / 'system-update.target.wants/b.service'), '../b.service')
            self.assertEqual(packaging.enable_units(root, self.MANIFEST, log.append)[:2], ['a.service', 'b.service'])

    def test_a_unit_without_the_matching_install_section_is_not_enabled(self):
        for text in ('[Service]\nExecStart=/bin/true\n', '[Install]\nWantedBy=multi-user.target\n',
                     '[Unit]\nWantedBy=system-update.target\n', '# [Install]\nWantedBy=system-update.target\n'):
            with self.subTest(text=text), tempfile.TemporaryDirectory() as tmp:
                root = self.stage(tmp, a='[Unit]\n', b=text)
                with self.assertRaisesRegex(packaging.PackagingError, 'no \\[Install\\]'):
                    packaging.enable_units(root, self.MANIFEST, lambda message: None)
                self.assertFalse((root / 'etc/systemd/system/system-update.target.wants/b.service').exists())

    def test_pending_units_may_be_absent_but_required_ones_may_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self.stage(tmp, a='[Unit]\n', b='[Install]\nWantedBy=system-update.target\n')
            log = []
            self.assertEqual(packaging.enable_units(root, self.MANIFEST, log.append), ['a.service', 'b.service'])
            self.assertTrue(any('c.service' in line for line in log))
        with tempfile.TemporaryDirectory() as tmp:
            root = self.stage(tmp, a='[Unit]\n')
            with self.assertRaisesRegex(packaging.PackagingError, 'missing: b.service'):
                packaging.enable_units(root, self.MANIFEST, lambda message: None)

    def test_a_stale_or_masking_link_is_replaced_and_a_symlinked_wants_directory_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self.stage(tmp, a='[Unit]\n', b='[Install]\nWantedBy=system-update.target\n',
                              c='[Install]\nWantedBy=multi-user.target\n')
            wants = root / 'etc/systemd/system/system-update.target.wants'
            wants.mkdir()
            (wants / 'b.service').symlink_to('/dev/null')
            packaging.enable_units(root, self.MANIFEST, lambda message: None)
            self.assertEqual(os.readlink(wants / 'b.service'), '../b.service')
        with tempfile.TemporaryDirectory() as tmp:
            root = self.stage(tmp, a='[Unit]\n', b='[Install]\nWantedBy=system-update.target\n')
            (root / 'etc/systemd/system/system-update.target.wants').symlink_to(Path(tmp))
            with self.assertRaisesRegex(packaging.PackagingError, 'symlinked'):
                packaging.enable_units(root, self.MANIFEST, lambda message: None)


class ManifestData(unittest.TestCase):
    def test_the_package_keyring_carries_only_the_pinned_primary(self):
        specs = {spec.dest: spec for spec in packaging.expand(packaging.load_manifest(ROOT), ROOT)}
        self.assertEqual(specs[packaging.PACKAGE_KEYRING_DEST].mode, 0o644)
        self.assertNotIn(packaging.KEYRING_DEST, specs)
        self.assertNotIn(packaging.SIGNERS_DEST, specs)
        self.assertEqual(packaging.check_package_keyring(ROOT), ['B9F5B97C9370239CC07B425F2CB426FC6D27CA65'])
        with tempfile.TemporaryDirectory(prefix='wros-pkg-keyring-') as tmp:
            repo = Path(tmp)
            trust = repo / 'os/config/update-trust'
            trust.mkdir(parents=True)
            shutil.copyfile(ROOT / 'os/config/update-trust/update-signers', trust / 'update-signers')
            extra, _ = native.synthetic_keyring(seed=b'extra')
            (trust / 'wros-archive-keyring.gpg').write_bytes(
                (ROOT / 'os/config/update-trust/wros-archive-keyring.gpg').read_bytes() + extra)
            with self.assertRaisesRegex(packaging.PackagingError, 'signer pin does not name'):
                packaging.check_package_keyring(repo)

    def test_the_video_group_may_write_only_the_screen_backlight(self):
        specs = {spec.dest: spec for spec in packaging.expand(packaging.load_manifest(ROOT), ROOT)}
        rule = specs['usr/lib/udev/rules.d/90-wros-backlight.rules']
        self.assertEqual(rule.mode, 0o644)
        lines = [line for line in (ROOT / 'os/config/90-wros-backlight.rules').read_text().splitlines()
                 if line and not line.startswith('#')]
        self.assertEqual(lines, [
            'ACTION=="add", SUBSYSTEM=="backlight", '
            'RUN+="/usr/bin/chgrp video /sys/class/backlight/%k/brightness", '
            'RUN+="/usr/bin/chmod g+w /sys/class/backlight/%k/brightness"'])
        for groups in ((ROOT / 'os/config/live/live.conf').read_text(),
                       (ROOT / 'os/config/installer.preseed').read_text()):
            self.assertRegex(groups, r'(user-default-groups|DEFAULT_GROUPS)\S*[ ="][^\n]*\bvideo\b')

    def test_manifest_is_internally_consistent(self):
        manifest = packaging.load_manifest(ROOT)
        specs = packaging.expand(manifest, ROOT)
        dests = [spec.dest for spec in specs]
        self.assertEqual(len(dests), len(set(dests)))
        self.assertEqual(dests, sorted(dests))
        for spec in specs:
            self.assertFalse(spec.mode & 0o7022, spec.dest)
            self.assertNotIn(spec.dest, (packaging.KEYRING_DEST, packaging.SIGNERS_DEST))
            self.assertFalse(spec.dest.startswith(('run/', 'tmp/', 'var/lib/wros-update/', 'root/', 'home/')), spec.dest)
            if not spec.pending:
                self.assertFalse(spec.missing, spec.dest)
        self.assertEqual([s.dest for s in specs if s.conffile], ['etc/writingdeck/keyboard.json'])
        by_dest = set(dests)
        for item in manifest['enable']:
            self.assertIn('etc/systemd/system/' + item['unit'], by_dest)
        for path in manifest['update_polkit_actions'].values():
            self.assertIn(path.lstrip('/'), by_dest)
        self.assertEqual(sorted(manifest['update_polkit_actions']), [
            'org.wros.update.apply', 'org.wros.update.cancel', 'org.wros.update.fetch', 'org.wros.update.healthy',
                'org.wros.update.media', 'org.wros.update.repair', 'org.wros.update.rollback',
                'org.wros.update.stage'])

    def test_every_repository_unit_policy_and_dropin_is_packaged(self):
        dests = {spec.dest for spec in packaging.expand(packaging.load_manifest(ROOT), ROOT)}
        for unit in (ROOT / 'os/systemd').iterdir():
            self.assertIn('etc/systemd/system/' + unit.name, dests, 'unpackaged unit ' + unit.name)
        for script in ('06_wros_boot_check', '42_wros_recovery'):
            self.assertIn('etc/grub.d/' + script, dests)
        self.assertIn('etc/default/grub.d/60-wros.cfg', dests)
        # Files deliberately left out of the package: live-session, installer and greeter pieces, the
        # PAM stack that Debian's vlock package owns, and the default-profile inputs of other tools.
        image_only = {'51-wros-greeter-keyboard.rules', 'greetd.service', 'greetd.toml', 'vlock.pam',
                      '50-wros-installer.rules', 'org.wros.installer.policy', 'installer.preseed',
                      'packages.txt', 'encrypted-target-packages.txt', 'wifi-firmware.json'}
        packaged_sources = {Path(str(spec.source)).name for spec in packaging.expand(packaging.load_manifest(ROOT), ROOT)
                            if spec.source}
        for entry in (ROOT / 'os/config').iterdir():
            if entry.is_file() and entry.name not in image_only:
                self.assertIn(entry.name, packaged_sources, 'unpackaged config ' + entry.name)

    def test_version_is_the_single_source(self):
        self.assertEqual((ROOT / 'os/VERSION').read_text(), '0.1.0~rc4\n')
        self.assertEqual(packaging.read_version(ROOT), '0.1.0~rc4')
        for bad in ('', '1:2', 'v1', '1 2', '1\t2'):
            with self.assertRaises(packaging.PackagingError):
                packaging.check_version(bad)
        self.assertEqual(packaging.check_version('0.1.0~rc2'), '0.1.0~rc2')

    def test_source_date_epoch_and_commit_come_from_the_environment_first(self):
        self.assertEqual(packaging.source_date_epoch(ROOT, {'SOURCE_DATE_EPOCH': '123'}), 123)
        with self.assertRaises(packaging.PackagingError):
            packaging.source_date_epoch(ROOT, {'SOURCE_DATE_EPOCH': 'yesterday'})
        self.assertEqual(packaging.source_commit(ROOT, {'WROS_SOURCE_COMMIT': 'abc1234'}), 'abc1234')
        with self.assertRaises(packaging.PackagingError):
            packaging.source_commit(ROOT, {'WROS_SOURCE_COMMIT': 'not a commit'})
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(packaging.PackagingError, 'WROS_SOURCE_COMMIT'):
                packaging.source_commit(tmp, {})
            self.assertEqual(packaging.source_commit(tmp, {'WROS_ALLOW_UNKNOWN_COMMIT': '1'}), 'unknown')
            with self.assertRaisesRegex(packaging.PackagingError, 'SOURCE_DATE_EPOCH'):
                packaging.source_date_epoch(tmp, {})


@unittest.skipUnless(PSEUDO_ROOT is not None, 'install-files.sh stages into a root and needs root or a user namespace')
@unittest.skipUnless(tool_available('dpkg-deb') and tool_available('gpg'), 'needs the Debian packaging tools')
class StageIntoRoot(unittest.TestCase):
    """os/scripts/install-files.sh against a throwaway Debian-shaped root and a throwaway repository copy."""

    @classmethod
    def setUpClass(cls):
        add_gpg_environment(cls)
        cls.fpr = cls.keys.generate(SIGNER_UID)
        cls.test_fpr = cls.keys.generate('WrOS Test Archive <archive@wros.invalid>')
        cls.other = cls.keys.generate('WrOS Backup Signer <backup@wros.invalid>')
        cls.tmp = tempfile.TemporaryDirectory(prefix='wros-stage-')
        cls.addClassCleanup(cls.tmp.cleanup)
        cls.base = Path(cls.tmp.name)
        cls.counter = 0

    def prepare(self, engine=True):
        """-> (repo copy, staging root, environment) with stubs for the parts that need ckbcomp/prose data."""
        type(self).counter += 1
        work = self.base / ('case%d' % self.counter)
        work.mkdir()
        repo = copy_repo(work / 'repo')
        if engine:
            add_update_engine(repo)
        (repo / 'os/scripts/generate-keymaps.sh').write_text(
            '#!/bin/sh\nset -eu\nmkdir -p "$1"\nfor n in qwerty colemak colemak-dh-ansi colemak-dh-iso; do echo "map $n" > "$1/$n.map"; done\n')
        (repo / 'os/scripts/generate-keymaps.sh').chmod(0o755)
        (repo / 'tools/stage-prose-runtime.py').write_text(
            'import sys\nfrom pathlib import Path\nPath(sys.argv[1], "opt/writingdeck/python").mkdir(parents=True, exist_ok=True)\n')
        root = dpkg_skeleton(work / 'root')
        (root / 'etc/pam.d').mkdir(parents=True)
        (work / 'grimoire').write_bytes(GRIMOIRE)
        (work / 'grimoire').chmod(0o755)
        (work / 'display-hw').write_bytes(DISPLAY_HW)
        keyring = work / 'keyring.gpg'
        keyring.write_bytes(self.keys.public(self.fpr))
        signers = work / 'signers'
        signers.write_text('# RC signer\n' + self.fpr + '\n')
        # The repository copy's package keyring belongs to the same signer the image pins.
        shutil.copyfile(keyring, repo / 'os/config/update-trust/wros-archive-keyring.gpg')
        shutil.copyfile(signers, repo / 'os/config/update-trust/update-signers')
        env = {**ENV, 'WROS_UPDATE_KEYRING': str(keyring), 'WROS_UPDATE_SIGNERS': str(signers),
               'WROS_DISPLAY_HW_BINARY': str(work / 'display-hw'), 'SOURCE_DATE_EPOCH': str(EPOCH),
               'WROS_SOURCE_COMMIT': 'c' * 40, 'HOME': str(work)}
        return repo, root, env, work

    def stage(self, repo, root, env, work, profile='installer'):
        return subprocess.run([*PSEUDO_ROOT, 'bash', str(repo / 'os/scripts/install-files.sh'), str(root), str(work / 'grimoire'),
                               profile], capture_output=True, text=True, env=env)

    def test_staging_installs_the_package_trust_anchors_and_enabled_units(self):
        repo, root, env, work = self.prepare()
        result = self.stage(repo, root, env, work)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installed_version(root), TREE)
        release = (root / 'etc/writingdeck/release').read_text()
        self.assertEqual(release, f'WRITINGDECK_VERSION={TREE}\nWROS_COMMIT=' + 'c' * 40
                         + '\nBUILD_DATE=2023-11-14\nGRIMOIRE_BASE=0.7.5\nBASE_COMMIT=f3ca1e8\n')
        self.assertEqual((root / 'etc/writingdeck/image-profile').read_text(), 'IMAGE_PROFILE=installer\n')
        self.assertTrue((root / 'etc/writingdeck/installer-keyboard-pending').is_file())
        system = root / 'etc/systemd/system'
        for target, unit in (('system-update.target', 'wros-offline-update.service'),
                             ('multi-user.target', 'wros-update-reconcile.service'),
                             ('multi-user.target', 'writingdeck-keyboard.service'),
                             ('multi-user.target', 'wros-display.service'),
                             ('multi-user.target', 'wros-boot-ok.service'),
                             ('system-update.target', 'wros-boot-ok.service')):
            self.assertEqual(os.readlink(system / (target + '.wants') / unit), '../' + unit)
        for unit in ('sleep.target', 'getty@tty8.service'):
            self.assertEqual(os.readlink(system / unit), '/dev/null')
        self.assertEqual(packaging.inspect_keyring((root / packaging.KEYRING_DEST).read_bytes())['primaries'], [self.fpr])
        self.assertEqual(packaging.parse_pin((root / packaging.SIGNERS_DEST).read_text()), [self.fpr])
        listing = run('dpkg-query', '--admindir=' + str(root / 'var/lib/dpkg'), '-L', 'wros', env=ENV).stdout.split()
        self.assertNotIn('/' + packaging.KEYRING_DEST, listing)
        self.assertEqual((root / 'usr/bin/grimoire').read_bytes(), GRIMOIRE)
        self.assertTrue((root / 'etc/pam.d/vlock').is_file())
        verify = subprocess.run(['dpkg', '--root=' + str(root), '--verify', 'wros'], capture_output=True, text=True, env=ENV)
        self.assertEqual((verify.returncode, verify.stdout), (0, ''), verify.stderr)

    def test_the_staged_root_satisfies_the_package_trust_unit_and_polkit_audits(self):
        repo, root, env, work = self.prepare()
        self.assertEqual(self.stage(repo, root, env, work).returncode, 0)
        loaded = importlib.util.spec_from_file_location('native_audit_copy', repo / 'tools/audit-native-image.py')
        audit = importlib.util.module_from_spec(loaded)
        # The audit reads sources from its own repository copy, whose engine files are the stand-ins.
        loaded.loader.exec_module(audit)
        self.assertEqual(audit.REPO, repo)
        self.assertEqual(audit.audit_package(root)['version'], TREE)
        audit.audit_update_engine(root)
        self.assertEqual(audit.audit_trust(root, fixture_fingerprints=set()), [self.fpr])
        self.assertEqual(audit.audit_units(root), ['writingdeck-keyboard.service', 'wros-display.service',
                                                   'wros-offline-update.service', 'wros-update-reconcile.service',
                                                   'wros-boot-ok.service', 'wros-boot-ok.service'])
        self.assertEqual(len(audit.audit_polkit(root)), 8)
        audit.audit_no_private_keys(root)

    def test_restaging_is_idempotent_and_keeps_the_writers_layout(self):
        repo, root, env, work = self.prepare()
        self.assertEqual(self.stage(repo, root, env, work).returncode, 0)
        layout = root / 'etc/writingdeck/keyboard.json'
        layout.write_text('{"layout": "colemak-dh-ansi"}\n')
        (root / 'etc/writingdeck/installer-keyboard-pending').unlink()
        self.assertEqual(self.stage(repo, root, env, work).returncode, 0)
        self.assertEqual(layout.read_text(), '{"layout": "colemak-dh-ansi"}\n')
        self.assertFalse((root / 'etc/writingdeck/installer-keyboard-pending').exists(),
                         'an existing layout must not be re-marked for first-boot adoption')

    def test_a_vm_demo_root_gets_no_first_boot_keyboard_marker(self):
        repo, root, env, work = self.prepare()
        self.assertEqual(self.stage(repo, root, env, work, profile='vm-demo').returncode, 0)
        self.assertFalse((root / 'etc/writingdeck/installer-keyboard-pending').exists())
        self.assertEqual((root / 'etc/writingdeck/image-profile').read_text(), 'IMAGE_PROFILE=vm-demo\n')

    def test_missing_or_bad_update_trust_fails_before_anything_is_staged(self):
        cases = {
            'keyring unset': (lambda e, w: e.pop('WROS_UPDATE_KEYRING'), 'WROS_UPDATE_KEYRING is not set'),
            'signers unset': (lambda e, w: e.pop('WROS_UPDATE_SIGNERS'), 'WROS_UPDATE_SIGNERS is not set'),
            'keyring empty': (lambda e, w: (w / 'keyring.gpg').write_bytes(b''), 'WROS_UPDATE_KEYRING is empty'),
            'signers empty': (lambda e, w: (w / 'signers').write_bytes(b''), 'WROS_UPDATE_SIGNERS is empty'),
            'secret key material': (lambda e, w: (w / 'keyring.gpg').write_bytes(self.keys.secret(self.fpr)),
                                    'secret key material'),
            'pin of another key': (lambda e, w: (w / 'signers').write_text(self.other + '\n'), 'not a primary key'),
            'pin names nothing': (lambda e, w: (w / 'signers').write_text('# none\n'), 'pin is empty'),
            'test-identity key': (lambda e, w: ((w / 'keyring.gpg').write_bytes(self.keys.public(self.test_fpr)),
                                                (w / 'signers').write_text(self.test_fpr + '\n')), 'test/fixture'),
        }
        for name, (mutate, message) in cases.items():
            with self.subTest(name=name):
                repo, root, env, work = self.prepare()
                mutate(env, work)
                result = self.stage(repo, root, env, work)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertNotIn('Package: wros\n', (root / 'var/lib/dpkg/status').read_text())
                self.assertFalse((root / 'usr/bin/grimoire').exists())
                self.assertFalse((root / packaging.KEYRING_DEST).exists())

    def test_listed_fixture_fingerprints_are_refused_in_staging(self):
        repo, root, env, work = self.prepare()
        with (repo / 'packaging/fixture-fingerprints.txt').open('a') as handle:
            handle.write(self.fpr + '\n')
        result = self.stage(repo, root, env, work)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('known test/fixture key', result.stderr)
        self.assertFalse((root / 'usr/bin/grimoire').exists())

    def test_offline_unit_without_an_install_section_is_not_enabled(self):
        repo, root, env, work = self.prepare()
        (repo / 'os/systemd/wros-offline-update.service').write_text(
            '[Unit]\nDescription=legacy, unenabled\n[Service]\nExecStart=/bin/true\n')
        result = self.stage(repo, root, env, work)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('no [Install]', result.stderr)
        self.assertFalse((root / 'etc/systemd/system/system-update.target.wants').exists())

    def test_missing_update_engine_files_are_refused_now_that_the_engine_is_integrated(self):
        for relative in ('os/scripts/wros-update-stage', 'os/scripts/wros-update-engine.py',
                         'os/systemd/wros-update-reconcile.service'):
            with self.subTest(missing=relative):
                repo, root, env, work = self.prepare(engine=False)
                (repo / relative).unlink()
                result = self.stage(repo, root, env, work)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertIn('missing source file', result.stderr)
                self.assertNotIn('pending', result.stderr)
                self.assertFalse((root / 'usr/libexec/wros-update-stage').exists())
                self.assertFalse((root / 'etc/systemd/system/multi-user.target.wants/wros-update-reconcile.service').exists())

    def test_downgrade_and_wrong_prebuilt_package_and_live_root_are_refused(self):
        repo, root, env, work = self.prepare()
        status = root / 'var/lib/dpkg/status'
        status.write_text(status.read_text() + 'Package: wros\nStatus: install ok installed\nArchitecture: amd64\n'
                          'Version: 9.0\nMaintainer: x <x@example.invalid>\nDescription: newer\n\n')
        result = self.stage(repo, root, env, work)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('newer wros', result.stderr)
        repo, root, env, work = self.prepare()
        binaries, keymaps = stub_inputs(work)
        wrong = build(ROOT, work / 'prebuilt', binaries, keymaps, version='0.1.0~rc9')
        result = self.stage(repo, root, {**env, 'WROS_PACKAGE_DEB': str(wrong)}, work)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f'not wros {TREE}', result.stderr)
        self.assertFalse((root / 'usr/bin/grimoire').exists())
        result = self.stage(repo, Path('/'), env, work)
        self.assertEqual(result.returncode, 2)

    def test_a_prebuilt_baseline_package_is_staged_unchanged(self):
        repo, root, env, work = self.prepare()
        binaries, keymaps = stub_inputs(work)
        prebuilt = build(repo, work / 'prebuilt', binaries, keymaps, commit='c' * 40)
        result = self.stage(repo, root, {**env, 'WROS_PACKAGE_DEB': str(prebuilt)}, work)
        self.assertEqual(result.returncode, 0, result.stderr)
        extract = work / 'extract'
        run('dpkg-deb', '-x', str(prebuilt), str(extract))
        for path in extract.rglob('*'):
            if path.is_file():
                self.assertEqual((root / path.relative_to(extract)).read_bytes(), path.read_bytes())

    def test_an_existing_foreign_locker_pam_stack_is_preserved_and_refused(self):
        repo, root, env, work = self.prepare()
        (root / 'etc/pam.d/vlock').write_text('auth sufficient pam_permit.so\n')
        result = self.stage(repo, root, env, work)
        self.assertEqual(result.returncode, 2)
        self.assertIn('Existing locker PAM configuration preserved', result.stderr)
        self.assertEqual((root / 'etc/pam.d/vlock').read_text(), 'auth sufficient pam_permit.so\n')
        self.assertFalse((root / 'usr/bin/grimoire').exists())


if __name__ == '__main__':
    unittest.main()
