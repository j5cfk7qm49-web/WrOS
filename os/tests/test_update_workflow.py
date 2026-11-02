"""CI publishing of online WrOS updates: the workflow's fixed inputs and tools/publish-updates.py.

Real gpg/gpgv and a local HTTP server stand in for GitHub Pages; nothing is published.
"""
import functools
import http.server
import importlib.util
from pathlib import Path
import sys
import tempfile
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
        self.assertEqual(WORKFLOW.count('secrets.'), 1)

    def test_apt_source_points_at_the_fixed_url_with_the_package_keyring(self):
        self.assertEqual((ROOT / 'os/config/apt/wros.sources').read_text(), (
            'Types: deb\n'
            'URIs: https://j5cfk7qm49-web.github.io/WrOS/updates/\n'
            'Suites: ./\n'
            'Signed-By: /usr/share/wros/update-keyring.gpg\n'))
        self.assertIn(publish.UPDATE_URL, WORKFLOW)


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
