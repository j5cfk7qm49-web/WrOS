"""App-only WrOS updates: installed now, restarting only the writing app.

An update qualifies only when it is the wros package alone, its relationships,
config-file list and (absent) maintainer scripts are unchanged, and every file it
adds, changes or removes is the app or a helper the app itself runs. Anything else
waits for the offline restart as before.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_update_support as support  # noqa: E402

update = support.load_engine()

BASE = {
    'usr/bin/grimoire': b'grimoire 1\n',
    'usr/libexec/wros-sync': b'sync 1\n',
    'usr/libexec/wros-update': b'engine 1\n',
    'usr/share/doc/writingdeck/README.md': b'readme 1\n',
    'etc/writingdeck/release': b'WRITINGDECK_VERSION=1\n',
    'etc/writingdeck/keyboard.json': b'{}\n',
    'usr/share/polkit-1/actions/org.wros.update.policy': b'<policy/>\n',
}
CONFFILES = ['/etc/writingdeck/keyboard.json']


def md5sums(files):
    return ''.join(f'{hashlib.md5(data).hexdigest()}  {path}\n' for path, data in sorted(files.items()))


def wros_deb(directory, version, files, *, depends='libc6', conffiles=CONFFILES, scripts=None, name='wros'):
    directory = Path(directory)
    tree = directory / f'tree-{name}-{version}'
    (tree / 'DEBIAN').mkdir(parents=True)
    (tree / 'DEBIAN').chmod(0o755)
    control = (f'Package: {name}\nVersion: {version}\nArchitecture: amd64\n'
               f'Maintainer: QA <qa@wros.invalid>\nDescription: app-only fixture\n')
    if depends:
        control += f'Depends: {depends}\n'
    (tree / 'DEBIAN/control').write_text(control)
    (tree / 'DEBIAN/md5sums').write_text(md5sums(files))
    if conffiles:
        (tree / 'DEBIAN/conffiles').write_text(''.join(p + '\n' for p in conffiles))
    for script, body in (scripts or {}).items():
        (tree / 'DEBIAN' / script).write_text(body)
        (tree / 'DEBIAN' / script).chmod(0o755)
    for path, data in files.items():
        (tree / path).parent.mkdir(parents=True, exist_ok=True)
        (tree / path).write_bytes(data)
    out = directory / f'{name}_{version}_amd64.deb'
    subprocess.run(['/usr/bin/dpkg-deb', '--build', '--root-owner-group', str(tree), str(out)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return out


def install(system, files=BASE, *, version='0.1.0~rc3', depends='libc6', conffiles=CONFFILES):
    """Record wros as installed the way dpkg does: status stanza plus info/ files."""
    extra = f'Depends: {depends}\n' if depends else ''
    system.status.write_text(support.status_stanza('libc6', '2.41-12', arch='amd64')
                             + support.status_stanza('wros', version, arch='amd64', extra=extra))
    info = system.dpkg / 'info'
    info.mkdir(exist_ok=True)
    (info / 'wros.md5sums').write_text(md5sums(files))
    (info / 'wros.conffiles').write_text(''.join(p + '\n' for p in conffiles))


class LiveApplyCheck(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()

    def setUp(self):
        self.system = support.SystemFixture(self.gpg)
        self.addCleanup(self.system.cleanup)
        install(self.system)
        self.tmp = Path(tempfile.mkdtemp(prefix='wros-live-', dir=self.system.root))

    def check(self, deb, name='wros', action='upgrade'):
        packages = [{'name': name, 'version': '0.1.0~rc4', 'action': action}]
        return update.live_apply_check(packages, [deb], self.system.dpkg)

    def test_only_the_app_and_its_own_helpers_changing_installs_without_a_reboot(self):
        app_only = BASE | {'usr/bin/grimoire': b'grimoire 2\n', 'usr/libexec/wros-sync': b'sync 2\n',
                           'usr/share/doc/writingdeck/NEW.md': b'new\n',
                           'etc/writingdeck/release': b'WRITINGDECK_VERSION=2\n'}
        del app_only['usr/share/doc/writingdeck/README.md']
        self.assertEqual(self.check(wros_deb(self.tmp, '0.1.0~rc4', app_only)), (True, ''))

    def test_anything_privileged_or_structural_waits_for_the_restart(self):
        cases = {
            'the update engine': BASE | {'usr/libexec/wros-update': b'engine 2\n'},
            'polkit policy': BASE | {'usr/share/polkit-1/actions/org.wros.update.policy': b'<policy2/>\n'},
            'a config file': BASE | {'etc/writingdeck/keyboard.json': b'{"layout": 1}\n'},
            'a new systemd unit': BASE | {'etc/systemd/system/wros-new.service': b'[Unit]\n'},
        }
        for label, files in cases.items():
            with self.subTest(label):
                live, reason = self.check(wros_deb(self.tmp / label.replace(' ', '-'), '0.1.0~rc4', files))
                self.assertFalse(live)
                self.assertTrue(reason)
        removed = dict(BASE)
        del removed['usr/libexec/wros-update']
        self.assertFalse(self.check(wros_deb(self.tmp / 'removed', '0.1.0~rc4', removed))[0])

    def test_package_shape_changes_wait_for_the_restart(self):
        files = BASE | {'usr/bin/grimoire': b'grimoire 2\n'}
        shapes = {
            'dependencies': wros_deb(self.tmp / 'deps', '0.1.0~rc4', files, depends='libc6, python3'),
            'config-file list': wros_deb(self.tmp / 'conf', '0.1.0~rc4', files, conffiles=[]),
            'a maintainer script': wros_deb(self.tmp / 'script', '0.1.0~rc4', files,
                                            scripts={'postinst': '#!/bin/sh\nexit 0\n'}),
        }
        for label, deb in shapes.items():
            with self.subTest(label):
                self.assertFalse(self.check(deb)[0])
        # Only an upgrade of the installed wros qualifies, never another or a new package.
        self.assertFalse(self.check(shapes['dependencies'], action='install')[0])
        other = wros_deb(self.tmp / 'other', '1.0', {'usr/bin/other': b'x\n'}, name='wros-extra', conffiles=[])
        self.assertFalse(self.check(other, name='wros-extra')[0])
        live, _ = update.live_apply_check(
            [{'name': 'wros', 'version': '0.1.0~rc4', 'action': 'upgrade'},
             {'name': 'wros-extra', 'version': '1.0', 'action': 'install'}],
            [wros_deb(self.tmp / 'pair', '0.1.0~rc4', files), other], self.system.dpkg)
        self.assertFalse(live)

    def test_an_unreadable_installed_record_is_never_taken_as_app_only(self):
        (self.system.dpkg / 'info' / 'wros.md5sums').unlink()
        deb = wros_deb(self.tmp, '0.1.0~rc4', BASE | {'usr/bin/grimoire': b'grimoire 2\n'})
        self.assertFalse(self.check(deb)[0])


class LiveApply(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()

    def setUp(self):
        self.system = support.SystemFixture(self.gpg)
        self.addCleanup(self.system.cleanup)
        install(self.system)
        self.tmp = Path(tempfile.mkdtemp(prefix='wros-live-apply-', dir=self.system.root))

    def stage(self, files):
        deb = wros_deb(self.tmp / 'debs', '0.1.0~rc4', files)
        bundle = support.write_bundle(self.tmp / 'bundle', [deb], self.gpg)
        kwargs = self.system.kwargs()
        review = update.verify_bundle(bundle, keyring=kwargs['keyring'], signers=kwargs['signers'],
                                      dpkg_dir=kwargs['dpkg_dir'], host_arch='amd64')['review_id']
        with mock.patch.object(update, 'plan_with_apt', return_value={}):
            return update.stage_update(bundle, review, allowed_prefixes=None, **kwargs)

    def apply_now(self, runner):
        kwargs = self.system.kwargs()
        return update.apply_staged_now(state_dir=kwargs['state_dir'], trigger_path=kwargs['trigger_path'],
                                       dpkg_dir=kwargs['dpkg_dir'], keyring=kwargs['keyring'],
                                       signers=kwargs['signers'], host_arch='amd64', runner=runner)

    def fake_apt(self, version='0.1.0~rc4'):
        calls = []

        def run(cmd):
            calls.append(cmd)
            self.system.status.write_text(support.status_stanza('libc6', '2.41-12', arch='amd64')
                                          + support.status_stanza('wros', version, arch='amd64',
                                                                  extra='Depends: libc6\n'))
            return subprocess.CompletedProcess(cmd, 0, stdout='', stderr='')
        return run, calls

    def test_an_app_only_update_is_staged_as_no_restart_and_installs_now_through_the_same_transaction(self):
        staged = self.stage(BASE | {'usr/bin/grimoire': b'grimoire 2\n'})
        self.assertFalse(staged['restart_required'])
        manifest = json.loads((self.system.state / 'staged' / 'manifest.json').read_text())
        self.assertTrue(manifest['live_apply'])
        run, calls = self.fake_apt()
        result = self.apply_now(run)
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], '/usr/bin/apt-get')
        self.assertFalse(self.system.trigger.is_symlink(), 'the boot trigger must not apply it again')
        states = [r['state'] for r in update.Store(self.system.state).records()]
        self.assertEqual(states[-3:], ['staged', 'applying', 'completed'])

    def test_the_polkit_launcher_installs_an_app_only_update(self):
        self.stage(BASE | {'usr/bin/grimoire': b'grimoire 2\n'})
        run, calls = self.fake_apt()
        paths = dict(self.system.kwargs())
        result = update.launcher_main('apply', [], '1000', euid=0, runner=run, **paths)
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(len(calls), 1)

    def test_an_update_that_needs_a_restart_is_never_installed_now(self):
        staged = self.stage(BASE | {'usr/libexec/wros-update': b'engine 2\n'})
        self.assertTrue(staged['restart_required'])
        run, calls = self.fake_apt()
        with self.assertRaisesRegex(ValueError, 'restart'):
            self.apply_now(run)
        self.assertEqual(calls, [])
        self.assertTrue(self.system.trigger.is_symlink(), 'still staged for the restart')

    def test_a_system_that_changed_since_staging_is_checked_again(self):
        self.stage(BASE | {'usr/bin/grimoire': b'grimoire 2\n'})
        # Something else replaced the engine meanwhile: the staged update now changes it back.
        install(self.system, BASE | {'usr/libexec/wros-update': b'engine 0\n'})
        run, calls = self.fake_apt()
        with self.assertRaisesRegex(ValueError, 'restart'):
            self.apply_now(run)
        self.assertEqual(calls, [])
        self.assertTrue(self.system.trigger.is_symlink())


if __name__ == '__main__':
    unittest.main()
