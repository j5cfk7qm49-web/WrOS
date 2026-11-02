"""CI publishing of online WrOS updates: the workflow's fixed inputs and tools/publish-updates.py.

Real gpg/gpgv and a local HTTP server stand in for GitHub Pages; nothing is published.
"""
import functools
import http.server
import importlib.util
import io
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import threading
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_update_support as support  # noqa: E402

ROOT = support.ROOT
_spec = importlib.util.spec_from_file_location('publish_updates', ROOT / 'tools/publish-updates.py')
publish = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(publish)
WORKFLOW = (ROOT / '.github/workflows/release-updates.yml').read_text()
PIN = (ROOT / 'os/config/update-trust/update-signers').read_text().split()[0]


class Workflow(unittest.TestCase):
    def test_triggers_secrets_and_signing_inputs_are_fixed(self):
        for needle in ("tags: ['v*']", 'schedule:', 'workflow_dispatch:', 'scratch:',
                       'secrets.WROS_UPDATE_SIGNING_SUBKEY', 'actions/deploy-pages',
                       'tools/build-update-bundle.py', '--valid-days 21',
                       '--expect-fingerprint "$PIN"', 'os/config/update-trust/update-signers',
                       'tools/publish-updates.py version', 'tools/publish-updates.py published',
                       'updates-scratch'):
            self.assertIn(needle, WORKFLOW)
        # Only the signing subkey is ever imported; no primary secret or passphrase-free primary.
        self.assertNotIn('PRIMARY', WORKFLOW.upper().replace('PRIMARY FINGERPRINT', ''))
        self.assertEqual(set(re.findall(r'secrets\.(\w+)', WORKFLOW)), {'WROS_UPDATE_SIGNING_SUBKEY'})
        # Without the subkey nothing is signed in CI: a release's own bundle is checked, then served as it is.
        for needle in ("if: steps.plan.outputs.sign == 'true'", "if: steps.plan.outputs.release != ''",
                       'tools/publish-updates.py check-bundle', 'releases/download/$RELEASE/wros-updates.tar'):
            self.assertIn(needle, WORKFLOW)
        self.assertEqual(WORKFLOW.count("if: steps.plan.outputs.sign == 'true'"), 2)
        self.assertEqual(WORKFLOW.count("if: steps.plan.outputs.deploy == 'true'"), 2)
        # Pages serves exactly what this run built: a folder the repository does not have.
        served = re.search(r'upload-pages-artifact@v3\n.*\n\s+with:\n\s+path: (\S+)', WORKFLOW).group(1)
        self.assertEqual(served, 'out/pages')
        self.assertFalse((ROOT / served).exists())
        self.assertNotIn(' site', WORKFLOW.replace('the whole site', '').replace('into the site', ''))

    def test_apt_source_points_at_the_fixed_url_with_the_package_keyring(self):
        self.assertEqual((ROOT / 'os/config/apt/wros.sources').read_text(), (
            'Types: deb\n'
            'URIs: https://j5cfk7qm49-web.github.io/WrOS/updates/\n'
            'Suites: ./\n'
            'Signed-By: /usr/share/wros/update-keyring.gpg\n'))
        self.assertIn(publish.UPDATE_URL, WORKFLOW)


TREE = (ROOT / 'os/VERSION').read_text().splitlines()[0].strip()
TAG = 'v' + TREE.replace('~', '-')


def plan(event, *, ref='', signing=False, scratch=False, release=''):
    """Run the workflow's 'Plan this run' step as GitHub would, returning its outputs."""
    block = WORKFLOW.split('- name: Plan this run', 1)[1].split('\n      - ', 1)[0]
    script = textwrap.dedent(block.split('run: |\n', 1)[1])
    with tempfile.TemporaryDirectory(prefix='wros-plan-') as tmp:
        output = Path(tmp) / 'output'
        output.touch()
        env = {'PATH': os.environ['PATH'], 'GITHUB_EVENT_NAME': event, 'GITHUB_REF_NAME': ref,
               'GITHUB_OUTPUT': str(output), 'SIGNING': 'true' if signing else 'false',
               'SCRATCH': 'true' if scratch else 'false', 'RELEASE': release}
        result = subprocess.run(['bash', '-c', script], cwd=ROOT, env=env, capture_output=True, text=True)
        if result.returncode != 0:
            raise ValueError(result.stderr)
        return dict(line.split('=', 1) for line in output.read_text().splitlines())


class Plan(unittest.TestCase):
    def test_without_the_subkey_a_release_is_served_from_its_asset_and_nothing_else_deploys(self):
        cases = [
            # event, options -> build, release, sign, deploy
            (('push', dict(ref=TAG, signing=True)), ('updates', '', 'true', 'true')),
            (('push', dict(ref=TAG)), ('', TAG, 'false', 'true')),
            (('schedule', dict(signing=True)), ('', '', 'true', 'true')),
            (('schedule', {}), ('', '', 'false', 'false')),
            (('workflow_dispatch', dict(scratch=True, signing=True)), ('updates-scratch', '', 'true', 'true')),
            (('workflow_dispatch', dict(scratch=True)), ('', '', 'false', 'false')),
            (('workflow_dispatch', dict(release=TAG, signing=True)), ('', TAG, 'false', 'true')),
            (('workflow_dispatch', dict(release=TAG)), ('', TAG, 'false', 'true')),
        ]
        for (event, options), expected in cases:
            with self.subTest(event=event, **options):
                out = plan(event, **options)
                self.assertEqual((out['build'], out['release'], out['sign'], out['deploy']), expected)
        self.assertEqual(plan('push', ref=TAG)['version'], TREE)
        # A release must name the version in the tree it is published from.
        with self.assertRaisesRegex(ValueError, 'os/VERSION'):
            plan('workflow_dispatch', release='v9.9.9')


class CheckBundle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()
        cls.tmp = tempfile.TemporaryDirectory(prefix='wros-check-bundle-')
        base = Path(cls.tmp.name)
        (base / 'debs').mkdir()
        cls.deb = support.make_deb(base / 'debs', 'wros', '0.1.0~rc3', arch='amd64')
        support.write_bundle(base / 'good', [cls.deb], cls.gpg)
        support.write_bundle(base / 'foreign', [cls.deb], cls.gpg, signer='other')
        cls.keyring = base / 'keyring.gpg'
        cls.gpg.export(['archive'], cls.keyring)
        cls.pin = base / 'update-signers'
        cls.pin.write_text(cls.gpg.keys['archive']['primary'].upper() + '\n')
        cls.base = base

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def tar(self, bundle, name, *, replace=None, extra=None):
        path = self.base / name
        with tarfile.open(path, 'w') as tar:
            for file in sorted(Path(bundle).rglob('*')):
                rel = file.relative_to(bundle).as_posix()
                if replace and rel in replace:
                    info = tarfile.TarInfo(rel)
                    info.size = len(replace[rel])
                    tar.addfile(info, io.BytesIO(replace[rel]))
                else:
                    tar.add(file, arcname=rel, recursive=False)
            for info, data in extra or ():
                tar.addfile(info, io.BytesIO(data) if data is not None else None)
        return path

    def check(self, tar, version='0.1.0~rc3'):
        out = Path(tempfile.mkdtemp(prefix='site-', dir=self.base)) / 'updates'
        return publish.check_bundle(tar, out, version, keyring=self.keyring, pin=self.pin), out

    def test_a_release_bundle_is_served_only_as_a_wros_computer_would_accept_it(self):
        good = self.tar(self.base / 'good', 'good.tar')
        _, out = self.check(good)
        # The workflow names its output relative to the checkout.
        here = os.getcwd()
        try:
            os.chdir(tempfile.mkdtemp(prefix='checkout-', dir=self.base))
            publish.check_bundle(good, Path('site/updates'), '0.1.0~rc3', keyring=self.keyring, pin=self.pin)
            self.assertTrue(Path('site/updates/InRelease').is_file())
        finally:
            os.chdir(here)
        # The checkout's owner and modes are the runner's business: the committed trust still applies.
        loose = self.base / 'loose-keyring.gpg'
        loose.write_bytes(self.keyring.read_bytes())
        loose.chmod(0o666)
        out = Path(tempfile.mkdtemp(prefix='site-', dir=self.base)) / 'updates'
        publish.check_bundle(good, out, '0.1.0~rc3', keyring=loose, pin=self.pin)
        self.assertEqual((out / 'pool' / self.deb.name).read_bytes(), self.deb.read_bytes())
        self.assertTrue((out / 'InRelease').is_file())
        with self.assertRaisesRegex(ValueError, 'publishes'):
            self.check(good, version='0.1.0~rc4')
        with self.assertRaises(ValueError):
            self.check(self.tar(self.base / 'foreign', 'foreign.tar'))
        with self.assertRaises(ValueError):
            self.check(self.tar(self.base / 'good', 'tampered.tar',
                                replace={f'pool/{self.deb.name}': self.deb.read_bytes() + b'x'}))
        escape = tarfile.TarInfo('../escape')
        escape.size = 1
        with self.assertRaises(ValueError):
            self.check(self.tar(self.base / 'good', 'escape.tar', extra=[(escape, b'x')]))
        self.assertFalse((self.base / 'escape').exists())
        link = tarfile.TarInfo('Packages.link')
        link.type, link.linkname = tarfile.SYMTYPE, 'Packages'
        with self.assertRaises(ValueError):
            self.check(self.tar(self.base / 'good', 'link.tar', extra=[(link, None)]))


class TagVersion(unittest.TestCase):
    def test_a_tag_must_name_the_version_in_the_tree(self):
        self.assertEqual(publish.tag_version('v0.1.0-rc3', '0.1.0~rc3'), '0.1.0~rc3')
        self.assertEqual(publish.tag_version('v0.2.0', '0.2.0'), '0.2.0')
        for tag, tree in (('v0.1.0-rc3', '0.1.0~rc2'), ('0.1.0-rc3', '0.1.0~rc3'), ('v0.1.0-rc3;x', '0.1.0~rc3')):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                publish.tag_version(tag, tree)


class BundleSigningKey(unittest.TestCase):
    def test_ci_signs_with_the_subkey_and_use_primary_signs_with_the_pinned_primary(self):
        import subprocess
        gpg = support.shared_gpg()
        # Shaped like the real WrOS key: a primary that can sign, plus a CI signing subkey.
        if 'release' not in gpg.keys:
            key = gpg.make_key('release', with_subkey=False)
            gpg.run('--passphrase', '', '--pinentry-mode', 'loopback', '--quick-add-key', key['primary'],
                    'rsa2048', 'sign', '0')
            listing = gpg.run('--with-colons', '--list-keys', '--with-subkey-fingerprint', key['email'],
                              stdout=subprocess.PIPE, text=True).stdout
            key['subkey'] = [line.split(':')[9] for line in listing.splitlines() if line.startswith('fpr:')][1]
        key = gpg.keys['release']
        with tempfile.TemporaryDirectory(prefix='wros-bundle-key-') as tmp:
            tmp = Path(tmp)
            deb = support.make_deb(tmp, 'wros', '0.1.0~rc3', arch='amd64')
            keyring = tmp / 'keyring.gpg'
            gpg.export(['release'], keyring)
            for extra, expected in (([], key['subkey']), (['--use-primary'], key['primary'])):
                with self.subTest(extra=extra):
                    out = tmp / ('bundle' + ''.join(extra))
                    subprocess.run([sys.executable, '-I', str(ROOT / 'tools/build-update-bundle.py'), str(deb),
                                    '--out', str(out), '--gnupg-home', str(gpg.home), '--key-id', key['primary'],
                                    '--expect-fingerprint', key['primary'], *extra],
                                   check=True, capture_output=True, env=gpg.env)
                    status = subprocess.run(['/usr/bin/gpgv', '--status-fd', '1', '--keyring', str(keyring),
                                             'InRelease'], cwd=out, capture_output=True, text=True).stdout
                    signing = next(line.split()[2] for line in status.splitlines()
                                   if line.startswith('[GNUPG:] VALIDSIG'))
                    self.assertEqual(signing.upper(), expected.upper())


class PublishedPackage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gpg = support.shared_gpg()
        cls.tmp = tempfile.TemporaryDirectory(prefix='wros-publish-')
        base = Path(cls.tmp.name)
        (base / 'debs').mkdir()
        cls.deb = support.make_deb(base / 'debs', 'wros', '0.1.0~rc3', arch='amd64')
        cls.site = base / 'site'
        support.write_bundle(cls.site / 'updates', [cls.deb], cls.gpg)
        support.write_bundle(cls.site / 'foreign', [cls.deb], cls.gpg, signer='other')
        cls.keyring = base / 'keyring.gpg'
        cls.gpg.export(['archive'], cls.keyring)
        cls.pin = cls.gpg.keys['archive']['primary'].upper()

        class Quiet(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args):
                pass

        cls.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(cls.site)))
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.url = f'http://127.0.0.1:{cls.server.server_port}/'

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.tmp.cleanup()

    def test_only_a_published_package_signed_by_the_pinned_key_is_re_signed(self):
        with tempfile.TemporaryDirectory(prefix='wros-publish-out-') as out:
            deb = publish.published_deb(self.url + 'updates/', Path(out), keyring=self.keyring, pin=self.pin)
            self.assertEqual(deb.read_bytes(), self.deb.read_bytes())
            with self.assertRaisesRegex(ValueError, 'pinned'):
                publish.published_deb(self.url + 'foreign/', Path(out), keyring=self.keyring, pin=self.pin)
            # Nothing published yet: nothing to re-sign, not an error.
            self.assertIsNone(publish.published_deb(self.url + 'empty/', Path(out), keyring=self.keyring, pin=self.pin))


if __name__ == '__main__':
    unittest.main()
