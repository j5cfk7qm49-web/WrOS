"""Real staged-dpkg and policy mutations; no root, services or host state."""
import contextlib
import dataclasses
import hashlib
import importlib.util
from pathlib import Path
import shutil
import stat
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('native_image', ROOT / 'tools/audit-native-image.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
packaging = module.packaging

EPOCH = 1700000000
SIGNER_UID = 'WrOS RC Update Signer <rc-update@wros.invalid>'
UPDATE_POLICY = ('<?xml version="1.0" encoding="UTF-8"?>\n'
                 '<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" '
                 '"http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">\n<policyconfig>\n'
                 + ''.join('  <action id="%s">\n    <description>fixture</description>\n    <message>fixture</message>\n'
                           '    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive>'
                           '<allow_active>no</allow_active></defaults>\n'
                           '    <annotate key="org.freedesktop.policykit.exec.path">%s</annotate>\n  </action>\n' % item
                           for item in sorted(packaging.load_manifest(ROOT)['update_polkit_actions'].items()))
                 + '</policyconfig>\n').encode()
UPDATE_RULES = (b'// fixture rules\npolkit.addRule(function(action, subject) {\n'
                b'    if (action.id.indexOf("org.wros.update.") == 0 && subject.local && subject.active &&\n'
                b'        subject.isInGroup("writingdeck")) { return polkit.Result.YES; }\n});\n')
# Stand-ins for the update-engine workstream's files, which are not merged into this tree.
# They live only in fixtures; the repository carries the real ones once integrated.
PLACEHOLDERS = {
    'usr/share/polkit-1/actions/org.wros.update.policy': UPDATE_POLICY,
    'etc/polkit-1/rules.d/50-wros-update.rules': UPDATE_RULES,
    'etc/systemd/system/wros-offline-update.service':
        b'[Unit]\nDescription=fixture offline update\n[Service]\nType=oneshot\nExecStart=/usr/libexec/wros-update run-offline-boot\n'
        b'[Install]\nWantedBy=system-update.target\n',
    'etc/systemd/system/wros-update-reconcile.service':
        b'[Unit]\nDescription=fixture reconcile\n[Service]\nType=oneshot\nExecStart=/usr/libexec/wros-update reconcile\n'
        b'[Install]\nWantedBy=multi-user.target\n',
}


def packet(tag, body):
    length = len(body)
    if length < 192:
        header = bytes([length])
    elif length < 8384:
        header = bytes([((length - 192) >> 8) + 192, (length - 192) & 0xff])
    else:
        header = b'\xff' + length.to_bytes(4, 'big')
    return bytes([0xc0 | tag]) + header + body


def synthetic_keyring(uid=SIGNER_UID, seed=b'rc'):
    """A deterministic, public-only v4 RSA key packet + user id: enough for the audit's parser,
    not a usable signing key. Returns (keyring bytes, primary fingerprint)."""
    modulus = bytearray((hashlib.sha256(seed).digest() * 8)[:256])
    modulus[0] |= 0x80
    body = (bytes([4]) + EPOCH.to_bytes(4, 'big') + bytes([1]) + (2048).to_bytes(2, 'big') + bytes(modulus)
            + (17).to_bytes(2, 'big') + b'\x01\x00\x01')
    fingerprint = hashlib.sha1(b'\x99' + len(body).to_bytes(2, 'big') + body).hexdigest().upper()
    return packet(6, body) + packet(13, uid.encode()), fingerprint


def dpkg_stanza(name, version, extra=''):
    return ('\n\nPackage: %s\nStatus: install ok installed\nArchitecture: amd64\nVersion: %s\n'
            'Maintainer: Fixture <fixture@example.invalid>\n%sDescription: synthetic policy fixture\n'
            % (name, version, extra))


def record_package(root, files, version=None):
    """Write the dpkg info files (list, md5sums, conffiles) for the wros package from {dest: bytes}."""
    info = root / 'var/lib/dpkg/info'
    info.mkdir(parents=True, exist_ok=True)
    entries = {'.'}
    for dest in files:
        parts = Path(dest).parts
        for index in range(1, len(parts) + 1):
            entries.add('/'.join(parts[:index]))
    (info / 'wros.list').write_text('/.\n' + ''.join('/%s\n' % e for e in sorted(entries - {'.'})))
    (info / 'wros.md5sums').write_text(''.join(
        '%s  %s\n' % (hashlib.md5(files[d]).hexdigest(), d) for d in sorted(files)))
    (info / 'wros.conffiles').write_text('/etc/writingdeck/keyboard.json\n')


def fixture_bytes(spec, version, manifest):
    if spec.generated == 'release':
        return packaging.release_text(version, 'f' * 40, EPOCH, manifest)
    if spec.generated is not None:
        return ('synthetic %s fixture; not an executable\n' % spec.generated).encode()
    if spec.pending:
        return PLACEHOLDERS.get(spec.dest, ('#!/usr/bin/python3 -I\n# fixture placeholder for %s\n' % spec.dest).encode())
    if spec.klass == 'doc':
        return ('fixture documentation for %s\n' % spec.dest).encode()
    return spec.source.read_bytes()


def fixture(root, signer_uid=SIGNER_UID):
    """Stage a complete synthetic image under *root*, with the 'wros' package recorded in the
    dpkg database. Returns {'fingerprint': signer fingerprint, 'files': {dest: bytes}}."""
    root = Path(root)
    manifest = packaging.load_manifest(ROOT)
    version = packaging.read_version(ROOT)
    database = root / 'var/lib/dpkg/status'
    database.parent.mkdir(parents=True, exist_ok=True)
    files = {}
    for spec_ in packaging.expand(manifest, ROOT):
        data = fixture_bytes(spec_, version, manifest)
        path = root / spec_.dest
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(spec_.mode)
        files[spec_.dest] = data
    record_package(root, files, version)
    with database.open('a') as output:
        output.write(dpkg_stanza('greetd', '0.10.3-4'))
        for name in ('restic', 'rclone'):
            output.write(dpkg_stanza(name, '1').replace('synthetic policy fixture', 'synthetic backup policy fixture'))
        output.write(dpkg_stanza('vlock', '2.2.2-12'))
        output.write(dpkg_stanza('systemd-timesyncd', '257.9-1~deb13u1'))
        output.write(dpkg_stanza('wros', version, 'Depends: ' + ', '.join(manifest['package']['depends']) + '\n'
                                 'Conffiles:\n /etc/writingdeck/keyboard.json 0\n'))
    for relative in ('usr/sbin/greetd', 'usr/bin/wros-greeter',
                     'etc/pam.d/greetd', 'usr/share/doc/greetd/copyright', 'usr/share/doc/vlock/copyright',
                     'usr/bin/restic', 'usr/bin/rclone', 'usr/bin/gpgv',
                     'usr/share/keyrings/debian-archive-keyring.gpg', 'usr/share/keyrings/debian-archive-keyring.pgp',
                     'usr/share/doc/restic/copyright', 'usr/share/doc/rclone/copyright',
                     'usr/bin/openvt', 'usr/bin/systemd-run', 'usr/bin/chvt', 'usr/sbin/vlock-main',
                     'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so',
                     'usr/lib/vlock/modules/all.so', 'usr/lib/vlock/modules/nosysrq.so'):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('synthetic policy fixture; not an executable or PAM stack\n')
        path.chmod(0o755 if relative.startswith('usr/') and 'copyright' not in relative else 0o644)
    (root / 'usr/sbin/vlock-main').chmod(0o4755)
    for source, relative in (
            ('greetd.toml', 'etc/greetd/config.toml'),
            ('greetd.service', 'etc/systemd/system/greetd.service'),
            ('51-wros-greeter-keyboard.rules', 'etc/polkit-1/rules.d/51-wros-greeter-keyboard.rules'),
            ('vlock.pam', 'etc/pam.d/vlock')):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / 'os/config' / source, path)
    for name in ('passwd', 'shadow'):
        with (root / 'etc' / name).open('a') as output:
            output.write('_greetd:x:995:995::/nonexistent:/usr/sbin/nologin\n' if name == 'passwd'
                         else '_greetd:!:1:0:99999:7:::\n')
    enabled = root / 'etc/systemd/system/multi-user.target.wants/greetd.service'
    enabled.parent.mkdir(parents=True, exist_ok=True)
    enabled.symlink_to('../greetd.service')
    for item in manifest['enable']:
        wants = root / 'etc/systemd/system' / (item['target'] + '.wants')
        wants.mkdir(parents=True, exist_ok=True)
        (wants / item['unit']).symlink_to('../' + item['unit'])
    for name in ('getty@tty1.service', 'getty@tty8.service', 'autovt@tty8.service',
                 *(target + '.target' for target in module.SLEEP_TARGETS)):
        (root / 'etc/systemd/system' / name).symlink_to('/dev/null')
    template = root / 'usr/share/wros/apt/debian.sources'
    template.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / 'os/config/apt/debian.sources', template)
    keyring, fingerprint = synthetic_keyring(signer_uid)
    # As in a real image, the image pin also names the primary of the package's keyring.
    install_trust(root, keyring + RELEASE_KEYRING, [fingerprint, RELEASE_PRIMARY])
    return {'fingerprint': fingerprint, 'files': files}


RELEASE_KEYRING = (ROOT / 'os/config/update-trust/wros-archive-keyring.gpg').read_bytes()
RELEASE_PRIMARY = (ROOT / 'os/config/update-trust/update-signers').read_text().split()[0]


def install_trust(root, keyring, fingerprints):
    for relative, data, mode in ((packaging.KEYRING_DEST, keyring, 0o644),
                                 (packaging.SIGNERS_DEST, packaging.pin_file_text(fingerprints).encode(), 0o644)):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.parent.chmod(0o755)
        path.write_bytes(data)
        path.chmod(mode)


# dest -> bytes a test declares to be the qualified repository source (see qualify=True below).
QUALIFIED = {}


def replace_package_file(root, relative, data, qualify=False):
    """Change a package-owned file *and* its dpkg md5sum. With qualify=True the new bytes also stand in
    for the repository source, so a semantic rule (keyring use, polkit actions) is what must object
    rather than the byte comparison with the repository, which has its own tests."""
    if qualify:
        QUALIFIED[relative] = data
    path = root / relative
    path.write_bytes(data)
    sums = root / 'var/lib/dpkg/info/wros.md5sums'
    rows = [row for row in sums.read_text().splitlines() if not row.endswith('  ' + relative)]
    rows.append('%s  %s' % (hashlib.md5(data).hexdigest(), relative))
    sums.write_text('\n'.join(sorted(rows, key=lambda r: r.split('  ', 1)[1])) + '\n')


@contextlib.contextmanager
def qualified_sources(scratch):
    """Serve QUALIFIED bytes as the repository source of those package paths, for this audit only."""
    real = module.packaging.expand

    def expand(*args, **kwargs):
        specs = []
        for spec in real(*args, **kwargs):
            if spec.dest in QUALIFIED and spec.source is not None:
                source = scratch / ('qualified-' + spec.dest.replace('/', '_'))
                source.write_bytes(QUALIFIED[spec.dest])
                spec = dataclasses.replace(spec, source=source)
            specs.append(spec)
        return specs

    with mock.patch.object(module.packaging, 'expand', expand):
        yield


class NativeImage(unittest.TestCase):
    def test_complete_policy_has_no_autologin_and_does_not_claim_secure_locking(self):
        with tempfile.TemporaryDirectory(prefix='wros-native-policy-') as tmp:
            root = Path(tmp)
            fixture(root)
            result = module.audit(root)
            self.assertTrue(result['passed'])
            self.assertFalse(result['autologin'])
            self.assertFalse(result['secure_locking_verified'])
            self.assertEqual(result['greetd_package_version'], '0.10.3-4')
            self.assertEqual(result['vlock_package_version'], '2.2.2-12')
            self.assertTrue(result['manual_lock_payload_present'])
            self.assertEqual(len(result['binary_sha256']), 18)
            self.assertIn('usr/libexec/wros-update', result['binary_sha256'])
            self.assertIn('usr/bin/gpgv', result['binary_sha256'])

    def test_debian_sources_template_and_network_time_are_required(self):
        with tempfile.TemporaryDirectory(prefix='wros-native-sources-') as tmp:
            root = Path(tmp)
            fixture(root)
            self.assertTrue(module.audit(root)['passed'])
            template = root / 'usr/share/wros/apt/debian.sources'
            original = template.read_bytes()
            self.assertEqual(original, (ROOT / 'os/config/apt/debian.sources').read_bytes())
            template.write_bytes(original.replace(b'trixie-updates', b'trixie-backports'))
            with self.assertRaisesRegex(ValueError, 'debian.sources'):
                module.audit(root)
            template.unlink()
            with self.assertRaisesRegex(ValueError, 'debian.sources'):
                module.audit(root)
            template.write_bytes(original)
            (root / 'usr/share/keyrings/debian-archive-keyring.pgp').unlink()
            with self.assertRaisesRegex(ValueError, 'debian-archive-keyring.pgp'):
                module.audit(root)
            (root / 'usr/share/keyrings/debian-archive-keyring.pgp').write_text('synthetic\n')
            edit_status(root, lambda stanzas: [s for s in stanzas
                                               if not s.lstrip('\n').startswith('Package: systemd-timesyncd\n')])
            with self.assertRaisesRegex(ValueError, 'systemd-timesyncd'):
                module.audit(root)

    def test_locker_setuid_is_required_as_root_and_tolerated_only_when_dropped_by_extraction(self):
        with tempfile.TemporaryDirectory(prefix='wros-native-setuid-') as tmp:
            root = Path(tmp)
            fixture(root)
            locker = root / 'usr/sbin/vlock-main'
            locker.chmod(0o755)
            with mock.patch.object(module, 'strict_ownership', return_value=False):
                self.assertTrue(module.audit(root)['passed'])
            with mock.patch.object(module, 'strict_ownership', return_value=True):
                with self.assertRaisesRegex(ValueError, 'vlock-main mode must match'):
                    module.audit(root)
            for mode in (0o2755, 0o6755, 0o4711):
                locker.chmod(mode)
                with self.subTest(mode=oct(mode)), \
                        mock.patch.object(module, 'strict_ownership', return_value=False):
                    with self.assertRaisesRegex(ValueError, 'vlock-main mode must match'):
                        module.audit(root)

    def test_unsafe_native_session_cannot_pass_even_with_all_requested_packages(self):
        mutations = {
            'autologin': lambda r: (r / 'etc/greetd/config.toml').write_text(
                (r / 'etc/greetd/config.toml').read_text() + '\n[initial_session]\nuser="root"\ncommand="sh"\n'),
            'root greeter': lambda r: (r / 'etc/passwd').write_text(
                '_greetd:x:0:0::/root:/bin/bash\n'),
            'unlocked greeter': lambda r: (r / 'etc/shadow').write_text('_greetd::1:0:99999:7:::\n'),
            'login shell': lambda r: (r / 'etc/passwd').write_text('_greetd:x:995:995::/tmp:/bin/bash\n'),
            'setuid editor': lambda r: (r / 'usr/bin/grimoire').chmod(0o4755),
            'writable greeter': lambda r: (r / 'usr/bin/wros-greeter').chmod(0o777),
            'missing PAM': lambda r: (r / 'etc/pam.d/greetd').unlink(),
            'missing notice': lambda r: (r / 'usr/share/doc/greetd/copyright').unlink(),
            'missing backup engine': lambda r: (r / 'usr/bin/restic').unlink(),
            'missing cloud engine': lambda r: (r / 'usr/bin/rclone').unlink(),
            'missing backup notice': lambda r: (r / 'usr/share/doc/restic/copyright').unlink(),
            'changed backup integration': lambda r: (r / 'usr/libexec/wros-backup').write_text('not the qualified integration'),
            'disabled service': lambda r: (r / 'etc/systemd/system/multi-user.target.wants/greetd.service').unlink(),
            'no tty1 mask': lambda r: (r / 'etc/systemd/system/getty@tty1.service').unlink(),
            'unavailable maintenance': lambda r: (r / 'etc/systemd/system/getty@tty2.service').symlink_to('/dev/null'),
            'altered service': lambda r: (r / 'etc/systemd/system/greetd.service').write_text('[Service]\nExecStart=/bin/sh\n'),
            'unprotected lid': lambda r: (r / 'etc/systemd/logind.conf.d/60-wros-power-guard.conf').unlink(),
            'reboot burst allowed': lambda r: (r / 'etc/systemd/system.conf.d/60-wros-reboot-guard.conf').unlink(),
            'sysrq left on': lambda r: (r / 'etc/sysctl.d/60-wros-sysrq.conf').unlink(),
            'network probe enabled': lambda r: (r / 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf').unlink(),
            'changed network probe': lambda r: (r / 'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf').write_text(
                '[connectivity]\nenabled=true\nuri=http://network-test.debian.org/nm\n'),
            'missing locker': lambda r: (r / 'usr/sbin/vlock-main').unlink(),
            'writable locker module': lambda r: (r / 'usr/lib/vlock/modules/all.so').chmod(0o777),
            'missing lock PAM': lambda r: (r / 'etc/pam.d/vlock').unlink(),
            'missing PAM guard': lambda r: (r / 'usr/lib/x86_64-linux-gnu/security/pam_succeed_if.so').unlink(),
            'changed lock PAM': lambda r: (r / 'etc/pam.d/vlock').write_text('auth sufficient pam_permit.so\n'),
            'changed lock helper': lambda r: (r / 'usr/libexec/wros-lock').write_text('#!/bin/sh\nexit 0\n'),
            'missing lock policy': lambda r: (r / 'usr/share/polkit-1/actions/org.wros.lock.policy').unlink(),
            'unreserved tty8': lambda r: (r / 'etc/systemd/system/getty@tty8.service').unlink(),
            'unmasked automatic tty8': lambda r: (r / 'etc/systemd/system/autovt@tty8.service').unlink(),
        }
        for target in module.SLEEP_TARGETS:
            mutations['unmasked ' + target] = lambda r, t=target: (r / 'etc/systemd/system' / (t + '.target')).unlink()
        for name, mutate in mutations.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory(prefix='wros-native-refusal-') as tmp:
                root = Path(tmp)
                fixture(root)
                mutate(root)
                with self.assertRaises((ValueError, OSError)):
                    module.audit(root)

    def test_weakened_policy_values_are_refused_even_when_the_file_is_present(self):
        # A faithful copy of a weakened source file must still fail: values are checked, not just bytes.
        weaken = {
            'etc/systemd/logind.conf.d/60-wros-power-guard.conf': [
                ('HandleLidSwitchDocked=ignore', 'HandleLidSwitchDocked=suspend'),
                ('HandleHibernateKey=ignore', ''),
                ('IdleAction=ignore', 'IdleAction=poweroff'),
                ('HandlePowerKeyLongPress=ignore', 'HandlePowerKeyLongPress=poweroff')],
            'etc/systemd/sleep.conf.d/60-wros-sleep-guard.conf': [
                ('AllowHybridSleep=no', 'AllowHybridSleep=yes'),
                ('AllowSuspend=no', '[Sleep]')],
            'etc/systemd/system.conf.d/60-wros-reboot-guard.conf': [
                ('CtrlAltDelBurstAction=none', 'CtrlAltDelBurstAction=reboot-force')],
            'etc/sysctl.d/60-wros-sysrq.conf': [('kernel.sysrq = 0', 'kernel.sysrq = 1')],
            'etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf': [('enabled=false', 'enabled=true')],
        }
        for relative, edits in weaken.items():
            for old, new in edits:
                with self.subTest(relative=relative, old=old), tempfile.TemporaryDirectory(prefix='wros-native-weak-') as tmp:
                    root = Path(tmp)
                    fixture(root)
                    path = root / relative
                    text = path.read_text()
                    self.assertIn(old, text)
                    path.write_text(text.replace(old, new))
                    with self.assertRaisesRegex(ValueError, 'policy value'):
                        module.audit(root)

    def test_live_root_and_symlinked_image_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'never the live root'):
            module.audit(Path('/'))
        with tempfile.TemporaryDirectory(prefix='wros-native-symlink-') as tmp:
            link = Path(tmp) / 'image'
            link.symlink_to('/')
            with self.assertRaisesRegex(ValueError, 'never the live root'):
                module.audit(link)


def edit_status(root, transform):
    path = root / 'var/lib/dpkg/status'
    stanzas = path.read_text().split('\n\n')
    path.write_text('\n\n'.join(transform(stanzas)).rstrip('\n') + '\n')


def staged_image():
    tmp = tempfile.TemporaryDirectory(prefix='wros-package-audit-')
    root = Path(tmp.name)
    info = fixture(root)
    return tmp, root, info


def is_wros(stanza):
    return ('\n' + stanza).find('\nPackage: wros\n') != -1


class PackageAwareAudit(unittest.TestCase):
    def refused(self, mutate, pattern, **audit_arguments):
        tmp, root, info = staged_image()
        QUALIFIED.clear()
        with tmp, qualified_sources(Path(tmp.name)):
            mutate(root, info)
            with self.assertRaisesRegex(ValueError, pattern):
                module.audit(root, **audit_arguments)

    def test_complete_image_reports_package_trust_units_and_polkit(self):
        tmp, root, info = staged_image()
        with tmp:
            result = module.audit(root)
            self.assertEqual(result['wros_package']['name'], 'wros')
            self.assertEqual(result['wros_package']['version'], packaging.read_version(ROOT))
            self.assertEqual(result['update_signers'], [info['fingerprint'], RELEASE_PRIMARY])
            self.assertEqual(result['update_units_enabled'], [
                'writingdeck-keyboard.service', 'wros-display.service',
                'wros-offline-update.service', 'wros-update-reconcile.service'])
            self.assertEqual(result['update_polkit_actions'], [
                'org.wros.update.cancel', 'org.wros.update.media', 'org.wros.update.repair', 'org.wros.update.stage'])
            self.assertFalse(result['update_engine_uses_debian_keyring'])
            self.assertTrue(result['private_key_material_absent'])

    def test_package_must_be_installed_at_the_repository_version(self):
        version = packaging.read_version(ROOT)
        self.refused(lambda r, i: edit_status(r, lambda s: [x for x in s if not is_wros(x)]), 'not recorded')
        self.refused(lambda r, i: edit_status(r, lambda s: [
            x.replace('Version: ' + version, 'Version: 0.0.1') if is_wros(x) else x for x in s]),
            'does not match os/VERSION')
        self.refused(lambda r, i: edit_status(r, lambda s: [
            x.replace('install ok installed', 'install ok half-installed') if is_wros(x) else x for x in s]),
            'not installed')
        self.refused(lambda r, i: edit_status(r, lambda s: [
            x.replace('Architecture: amd64', 'Architecture: arm64') if is_wros(x) else x for x in s]),
            'architecture')

    def test_package_ownership_integrity_and_modes(self):
        def drop_from_list(root, info):
            listing = root / 'var/lib/dpkg/info/wros.list'
            listing.write_text(''.join(r + '\n' for r in listing.read_text().splitlines()
                                       if r != '/usr/libexec/wros-update-stage'))

        def own_extra(root, info):
            (root / 'usr/bin/extra').write_bytes(b'x')
            with (root / 'var/lib/dpkg/info/wros.list').open('a') as handle:
                handle.write('/usr/bin/extra\n')

        def symlink_over(root, info):
            (root / 'usr/libexec/wros-display').unlink()
            (root / 'usr/libexec/wros-display').symlink_to('/bin/true')

        cases = {
            'required file unlisted': (drop_from_list, 'missing a required file'),
            'extra owned file': (own_extra, 'does not list'),
            'bytes changed after install': (lambda r, i: (r / 'usr/libexec/wros-backup').write_bytes(b'altered'),
                                            'unsafe|md5sum|differs'),
            'launcher edited with md5 fixed up': (
                lambda r, i: replace_package_file(r, 'usr/libexec/wros-lock', b'#!/bin/sh\nexit 0\n'),
                'qualified source'),
            'setuid launcher': (lambda r, i: (r / 'usr/libexec/wros-update-stage').chmod(0o4755), 'unsafe'),
            'setgid launcher': (lambda r, i: (r / 'usr/libexec/wros-update-cancel').chmod(0o2755), 'unsafe'),
            'group-writable engine': (lambda r, i: (r / 'usr/lib/wros-update/engine.py').chmod(0o664),
                                      'unsafe package file permissions'),
            'launcher with the wrong mode': (lambda r, i: (r / 'usr/libexec/wros-update-repair').chmod(0o700),
                                             'mode'),
            'missing repair launcher': (lambda r, i: (r / 'usr/libexec/wros-update-repair').unlink(), 'missing'),
            'missing media launcher': (lambda r, i: (r / 'usr/libexec/wros-update-media').unlink(), 'missing'),
            'missing engine': (lambda r, i: (r / 'usr/lib/wros-update/engine.py').unlink(), 'missing'),
            'package file replaced by symlink': (symlink_over, 'regular file'),
            'release carries another version': (
                lambda r, i: replace_package_file(r, 'etc/writingdeck/release', b'WRITINGDECK_VERSION=9.9.9\n'),
                'package version'),
            'conffile not registered': (
                lambda r, i: (r / 'var/lib/dpkg/info/wros.conffiles').write_text(''), 'conffile'),
        }
        for name, (mutate, pattern) in cases.items():
            with self.subTest(name=name):
                self.refused(mutate, pattern)

    def test_an_unreadable_package_path_is_reported_as_unreadable_not_missing(self):
        # Debian's polkitd keeps /etc/polkit-1/rules.d root:polkitd 0750: an unprivileged audit of a
        # root-owned tree gets EACCES there, which must never be reported as a missing file.
        tmp, root, info = staged_image()
        with tmp:
            denied = root / 'etc/polkit-1/rules.d/50-writingdeck-keyboard.rules'
            self.assertTrue(denied.is_file())
            real_lstat = Path.lstat

            def lstat(path, *args, **kwargs):
                if path == denied:
                    raise PermissionError(13, 'Permission denied', str(path))
                return real_lstat(path, *args, **kwargs)

            with mock.patch.object(Path, 'lstat', lstat):
                with self.assertRaisesRegex(ValueError, 'cannot be read by this audit.*permission denied') as raised:
                    module.audit(root)
            self.assertNotIn('missing', str(raised.exception))

    def test_update_engine_may_not_consult_the_debian_archive_keyring(self):
        for relative in ('usr/libexec/wros-update', 'usr/libexec/wros-update-stage', 'usr/lib/wros-update/engine.py'):
            with self.subTest(relative=relative):
                self.refused(lambda r, i, relative=relative: replace_package_file(
                    r, relative, b'KEYRING = "/usr/share/keyrings/debian-archive-keyring.gpg"\n', qualify=True),
                    'Debian archive keyring')
        # APT still needs the base keyring: its absence is refused too.
        tmp, root, info = staged_image()
        with tmp:
            (root / 'usr/share/keyrings/debian-archive-keyring.gpg').unlink()
            with self.assertRaisesRegex(ValueError, 'missing or unsafe'):
                module.audit(root)
            # Unsafe symlink is refused
            (root / 'usr/share/keyrings/debian-archive-keyring.gpg').symlink_to('/etc/passwd')
            with self.assertRaisesRegex(ValueError, 'missing or unsafe'):
                module.audit(root)
            (root / 'usr/share/keyrings/debian-archive-keyring.gpg').unlink()
            # Valid Debian 13 symlink to debian-archive-keyring.pgp is accepted
            (root / 'usr/share/keyrings/debian-archive-keyring.pgp').write_bytes(b'pgp-keyring-data')
            (root / 'usr/share/keyrings/debian-archive-keyring.gpg').symlink_to('debian-archive-keyring.pgp')
            self.assertTrue(module.audit(root)['passed'])

    def test_update_trust_inputs(self):
        keyring = synthetic_keyring()
        other = synthetic_keyring(seed=b'other', uid='WrOS Backup Signer <b@wros.invalid>')

        def set_keyring(data):
            def mutate(root, info):
                (root / packaging.KEYRING_DEST).write_bytes(data)
            return mutate

        def set_pin(text):
            def mutate(root, info):
                (root / packaging.SIGNERS_DEST).write_text(text)
            return mutate

        def link_keyring(root, info):
            (root / packaging.KEYRING_DEST).unlink()
            (root / packaging.KEYRING_DEST).symlink_to('/etc/passwd')

        cases = {
            'no keyring': (lambda r, i: (r / packaging.KEYRING_DEST).unlink(), 'missing or unsafe'),
            'empty keyring': (set_keyring(b''), 'missing or unsafe'),
            'no pin': (lambda r, i: (r / packaging.SIGNERS_DEST).unlink(), 'missing or unsafe'),
            'empty pin': (set_pin('# only a comment\n'), 'missing or unsafe|pin is empty'),
            'malformed pin': (set_pin('not-a-fingerprint\n'), 'not a 40-hex'),
            'short pin': (set_pin('ABCD\n'), 'not a 40-hex'),
            'pin of another key': (set_pin(other[1] + '\n'), 'not a primary key'),
            'keyring has an unpinned key': (set_keyring(keyring[0] + RELEASE_KEYRING + other[0]), 'does not name'),
            'secret key in keyring': (set_keyring(keyring[0] + packet(7, b'secret subkey')), 'secret key material'),
            'secret primary in keyring': (set_keyring(packet(5, b'\x04secret') + packet(13, b'x')),
                                          'secret key material'),
            'armored keyring': (set_keyring(b'-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nxx\n'), 'binary'),
            'truncated keyring': (set_keyring(keyring[0][:-5]), 'truncated'),
            'keyring without public key': (set_keyring(packet(13, b'only a uid')), 'no public primary'),
            'keyring is a symlink': (link_keyring, 'missing or unsafe'),
            'group-writable pin': (lambda r, i: (r / packaging.SIGNERS_DEST).chmod(0o664), 'writable'),
            'world-writable pin': (lambda r, i: (r / packaging.SIGNERS_DEST).chmod(0o666), 'writable'),
            'group-writable keyring': (lambda r, i: (r / packaging.KEYRING_DEST).chmod(0o664), 'writable'),
            'world-writable trust directory': (lambda r, i: (r / packaging.SIGNERS_DEST).parent.chmod(0o777),
                                               'writable'),
        }
        for name, (mutate, pattern) in cases.items():
            with self.subTest(name=name):
                self.refused(mutate, pattern)

    def test_the_package_keyring_may_hold_only_primaries_the_image_pins(self):
        tmp, root, info = staged_image()
        with tmp:
            module.audit(root)
            synthetic, fingerprint = synthetic_keyring()
            install_trust(root, synthetic, [fingerprint])
            with self.assertRaisesRegex(ValueError, 'update-keyring.gpg holds a primary key the image pin does not name'):
                module.audit(root)

    def test_fixture_keys_are_refused_by_identity_and_by_fingerprint(self):
        for uid in ('WrOS Test Archive <archive@wros.invalid>', 'fixture key', 'Throwaway signer', 'disposable-VM'):
            with self.subTest(uid=uid), tempfile.TemporaryDirectory(prefix='wros-fixture-key-') as tmp:
                root = Path(tmp)
                fixture(root, signer_uid=uid)
                with self.assertRaisesRegex(ValueError, 'test/fixture'):
                    module.audit(root)
        tmp, root, info = staged_image()
        with tmp:
            module.audit(root)
            with self.assertRaisesRegex(ValueError, 'known test/fixture key'):
                module.audit(root, fixture_fingerprints={info['fingerprint'].lower()})
            module.audit(root, fixture_fingerprints={'0' * 40})

    def test_update_units_must_be_enabled_and_declare_their_target(self):
        offline = 'etc/systemd/system/wros-offline-update.service'
        reconcile = 'etc/systemd/system/wros-update-reconcile.service'
        offline_link = 'etc/systemd/system/system-update.target.wants/wros-offline-update.service'
        reconcile_link = 'etc/systemd/system/multi-user.target.wants/wros-update-reconcile.service'

        def relink(relative, target):
            def mutate(root, info):
                (root / relative).unlink()
                (root / relative).symlink_to(target)
            return mutate

        cases = {
            'offline unit not enabled': (lambda r, i: (r / offline_link).unlink(), 'not enabled'),
            'reconcile unit not enabled': (lambda r, i: (r / reconcile_link).unlink(), 'not enabled'),
            'keyboard unit not enabled': (lambda r, i: (
                r / 'etc/systemd/system/multi-user.target.wants/writingdeck-keyboard.service').unlink(), 'not enabled'),
            'offline unit without [Install]': (lambda r, i: replace_package_file(
                r, offline, b'[Unit]\nDescription=x\n[Service]\nExecStart=/bin/true\n', qualify=True), 'does not declare'),
            'offline unit wanted by the wrong target': (lambda r, i: replace_package_file(
                r, offline, b'[Install]\nWantedBy=multi-user.target\n', qualify=True), 'does not declare'),
            'reconcile unit for the wrong target': (lambda r, i: replace_package_file(
                r, reconcile, b'[Install]\nWantedBy=system-update.target\n', qualify=True), 'does not declare'),
            'link points elsewhere': (relink(offline_link, '../other.service'), 'not enabled'),
            'link is masked': (relink(reconcile_link, '/dev/null'), 'not enabled'),
        }
        for name, (mutate, pattern) in cases.items():
            with self.subTest(name=name):
                self.refused(mutate, pattern)

    def test_update_polkit_actions_are_exactly_the_four_and_default_to_no(self):
        policy = 'usr/share/polkit-1/actions/org.wros.update.policy'
        rules = 'etc/polkit-1/rules.d/50-wros-update.rules'
        text = UPDATE_POLICY.decode()

        def action(identifier, path='/usr/libexec/wros-update-stage', allow='no'):
            return ('  <action id="%s"><defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive>'
                    '<allow_active>%s</allow_active></defaults><annotate key="org.freedesktop.policykit.exec.path">%s'
                    '</annotate></action>\n' % (identifier, allow, path))

        def hidden(root, info):
            (root / 'usr/share/polkit-1/actions/org.example.policy').write_text(
                '<policyconfig>' + action('org.wros.update.sneaky') + '</policyconfig>')

        cases = {
            'extra action': (lambda r, i: replace_package_file(r, policy, text.replace(
                '</policyconfig>', action('org.wros.update.shell') + '</policyconfig>').encode(), qualify=True), 'not exactly'),
            'missing action': (lambda r, i: replace_package_file(r, policy, text.replace(
                'org.wros.update.media', 'org.wros.update.other').encode(), qualify=True), 'not exactly'),
            'wrong exec path': (lambda r, i: replace_package_file(r, policy, text.replace(
                '/usr/libexec/wros-update-cancel', '/usr/libexec/wros-update').encode(), qualify=True), 'exec path'),
            'implicit yes': (lambda r, i: replace_package_file(r, policy, text.replace(
                '<allow_active>no', '<allow_active>yes', 1).encode(), qualify=True), 'default to no'),
            'duplicate action': (lambda r, i: replace_package_file(r, policy, text.replace(
                '</policyconfig>', action('org.wros.update.stage') + '</policyconfig>').encode(), qualify=True), 'duplicate'),
            'update action in another policy file': (hidden, 'not exactly'),
            'rules not group-restricted': (lambda r, i: replace_package_file(
                r, rules, b'polkit.addRule(function(a, s) { if (a.id.indexOf("org.wros.update.") == 0) '
                          b'return polkit.Result.YES; });\n', qualify=True), 'restrict'),
            'policy missing': (lambda r, i: (r / policy).unlink(), 'missing'),
        }
        for name, (mutate, pattern) in cases.items():
            with self.subTest(name=name):
                self.refused(mutate, pattern)

    def test_private_key_material_anywhere_is_refused_but_tls_string_constants_are_not(self):
        body = ('-----BEGIN PGP PRIVATE KEY BLOCK-----\n\n' + 'lQdGBF' * 20 + '\n-----END PGP PRIVATE KEY BLOCK-----\n')
        pem = ('-----BEGIN PRIVATE KEY-----\n' + 'MIIEvQIBADANBgkqhkiG9w0BAQEFAASC' * 4
               + '\n-----END PRIVATE KEY-----\n')

        def write(relative, data):
            def mutate(root, info):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data if isinstance(data, bytes) else data.encode())
            return mutate

        cases = {
            'armored key in a home directory': write('root/key.asc', body),
            'armored key in a doc file': write('usr/share/doc/greetd/notes.txt', body),
            'PEM key': write('etc/ssl/server.pem', pem),
            'binary OpenPGP secret keyring': write('usr/share/wros/old.gpg',
                                                   synthetic_keyring()[0] + packet(7, b'\x04secret')),
            'binary secret key with an innocent name': write('usr/share/wros/note.txt',
                                                             packet(5, b'\x04secret material')),
            'gpg secret ring name': write('usr/share/wros/secring.gpg', b'x'),
            'ssh key name': write('etc/id_ed25519', b'x'),
            'gpg private key store': lambda r, i: (r / 'usr/share/wros/private-keys-v1.d').mkdir(),
        }
        for name, mutate in cases.items():
            with self.subTest(name=name):
                self.refused(mutate, 'private|secret')
        tmp, root, info = staged_image()
        with tmp:
            library = root / 'usr/lib/x86_64-linux-gnu/libfixturessl.so'
            library.parent.mkdir(parents=True, exist_ok=True)
            library.write_bytes(b'\0\1\2-----BEGIN PRIVATE KEY-----\n' + b'A' * 200)
            (root / 'usr/share/doc/greetd/format.txt').write_text('PEM files start with -----BEGIN PRIVATE KEY-----\n')
            self.assertTrue(module.audit(root)['private_key_material_absent'])

    def test_trust_anchors_are_image_inputs_not_package_files(self):
        tmp, root, info = staged_image()
        with tmp:
            listed = module.package_listing(root)
            self.assertNotIn('/' + packaging.KEYRING_DEST, listed)
            self.assertNotIn('/' + packaging.SIGNERS_DEST, listed)
            self.assertNotIn('/etc/pam.d/vlock', listed)
