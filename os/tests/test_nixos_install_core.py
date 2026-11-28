"""nixos/scripts/wros-install-core: the wrong-disk guard and the interruption-safe order.

Issue #3 asked for the wrong-target and interrupted-install cases. The install core now refuses
any disk but the one the session chose, or one in use, before its first write, and makes the
disk bootable only after the writer account exists.
"""
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'nixos/scripts/wros-install-core'


def load():
    loader = importlib.machinery.SourceFileLoader('wros_install_core', str(SCRIPT))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


core = load()
IDENTITY = {'dev': '253:16', 'size': '20971520', 'diskseq': '7'}
SYSTEM = '/nix/store/00000000000000000000000000000000-wros-system'
FIRMWARE = '/run/current-system/sw/bin/wros-firmware'


def firmware_reply(argv, *, packages=(), missing=()):
    if argv[:2] == [FIRMWARE, 'detect']:
        return json.dumps({'packages': list(packages), 'unknown': ['unknown-device']})
    if argv[0] == FIRMWARE and 'ensure' in argv:
        return json.dumps({'packages': list(packages), 'missing': list(missing), 'unknown': []})
    return ''


class FakeSys:
    """A /sys/class/block with one disk, vdb, and its partitions."""

    def __init__(self, tmp, partitions=('vdb1', 'vdb2')):
        self.root = Path(tmp) / 'block'
        disk = self.root / 'vdb'
        (disk / 'holders').mkdir(parents=True)
        for key, value in IDENTITY.items():
            (disk / key).write_text(value + '\n')
        (disk / 'ro').write_text('0\n')
        for index, name in enumerate(partitions, 1):
            part = disk / name
            (part / 'holders').mkdir(parents=True)
            (part / 'partition').write_text(f'{index}\n')
            (part / 'dev').write_text(f'253:{16 + index}\n')
        self.mountinfo = Path(tmp) / 'mountinfo'
        self.mountinfo.write_text('22 1 0:21 / / rw - tmpfs tmpfs rw\n')
        self.swaps = Path(tmp) / 'swaps'
        self.swaps.write_text('Filename\tType\tSize\tUsed\tPriority\n')

    def check(self, identity=IDENTITY):
        core.check_target_disk('vdb', identity, sysroot=self.root,
                               mountinfo_path=self.mountinfo, swaps_path=self.swaps)


class TargetDiskGuard(unittest.TestCase):
    def test_the_chosen_idle_disk_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            FakeSys(tmp).check()

    def test_another_disk_under_the_same_name_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeSys(tmp)
            for key, value in (('diskseq', '8'), ('size', '41943040'), ('dev', '8:0')):
                with self.subTest(key=key):
                    with self.assertRaisesRegex(ValueError, f'not the disk chosen.*{key} changed'):
                        fake.check({**IDENTITY, key: value})

    def test_a_mounted_partition_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeSys(tmp)
            fake.mountinfo.write_text(fake.mountinfo.read_text()
                                      + '40 22 253:17 / /media/old rw - vfat /dev/vdb1 rw\n')
            with self.assertRaisesRegex(ValueError, 'mounted at /media/old'):
                fake.check()

    def test_an_open_container_or_swap_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeSys(tmp)
            (fake.root / 'vdb/vdb2/holders/dm-0').mkdir()
            with self.assertRaisesRegex(ValueError, 'vdb2 is in use by another device'):
                fake.check()
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeSys(tmp)
            fake.swaps.write_text(fake.swaps.read_text() + '/dev/vdb1 partition 1048572 0 -2\n')
            with self.assertRaisesRegex(ValueError, 'holds active swap'):
                fake.check()

    def test_a_partition_or_read_only_disk_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeSys(tmp)
            with self.assertRaisesRegex(ValueError, 'not a whole disk'):
                core.check_target_disk('vdb1', IDENTITY, sysroot=fake.root / 'vdb',
                                       mountinfo_path=fake.mountinfo, swaps_path=fake.swaps)
            (fake.root / 'vdb/ro').write_text('1\n')
            with self.assertRaisesRegex(ValueError, 'read-only'):
                fake.check()


class InstallOrder(unittest.TestCase):
    def params(self):
        return {
            'target_disk': 'vdb', 'target_identity': dict(IDENTITY), 'username': 'writer',
            'full_name': 'Ada Writer', 'hostname': 'wros-book', 'keyboard_layout': 'colemak',
            'timezone': 'UTC', 'luks_passphrase': 'colemak-passphrase',
            'user_password': 'writer-secret-password', 'session_nonce': '0' * 32,
        }

    def install(self, tmp, check_disk, *, detected=(), firmware_paths=None, missing=(), calls=None):
        target = Path(tmp) / 'target'
        closure = self.closure(tmp)
        calls = [] if calls is None else calls

        def runner(argv, *, input_bytes=None, timeout=600):
            calls.append(list(argv))
            if 'switch-to-configuration' in ' '.join(argv):
                (target / 'boot/EFI/BOOT').mkdir(parents=True, exist_ok=True)
                (target / 'boot/EFI/BOOT/BOOTX64.EFI').write_bytes(b'grub')
            if argv == ['cat', f'{SYSTEM}/wros-firmware.json']:
                return json.dumps({'format': 1, 'packages': {
                    name: {'store_path': path} for name, path in (firmware_paths or {}).items()
                }})
            if argv[:2] == ['cryptsetup', 'luksUUID']:
                return 'luks-uuid'
            return firmware_reply(argv, packages=detected, missing=missing)

        proof = core.install_target(self.params(), target_root=target, closure_file=closure,
                                    runner=runner, check_disk=check_disk)
        return proof, calls

    def test_nothing_is_written_when_the_disk_check_fails(self):
        def refuse(disk, identity):
            raise ValueError('not the disk chosen')

        with tempfile.TemporaryDirectory() as tmp:
            calls = []
            with self.assertRaisesRegex(ValueError, 'not the disk chosen'):
                core.install_target(self.params(), target_root=Path(tmp) / 't',
                                    closure_file=self.closure(tmp),
                                    runner=lambda argv, **kw: calls.append(argv),
                                    check_disk=refuse)
            self.assertEqual(calls, [])

    def closure(self, tmp):
        path = Path(tmp) / 'closure'
        path.write_text(SYSTEM + '\n')
        return path

    def test_the_disk_becomes_bootable_only_after_the_account_exists(self):
        checked = []
        with tempfile.TemporaryDirectory() as tmp:
            proof, calls = self.install(tmp, lambda disk, identity: checked.append((disk, identity)))
        self.assertEqual(checked, [('vdb', IDENTITY)])
        self.assertTrue(proof['passed'])
        flat = [' '.join(c) for c in calls]
        self.assertEqual(calls[0][:2], ['sgdisk', '--zap-all'])
        install = next(i for i, c in enumerate(flat) if c.startswith('nixos-install '))
        self.assertIn('--no-bootloader', calls[install])
        useradd = next(i for i, c in enumerate(flat) if ' useradd ' in c)
        lock = next(i for i, c in enumerate(flat) if c.endswith('passwd -l root'))
        boot = next(i for i, c in enumerate(flat) if 'switch-to-configuration boot' in c)
        self.assertIn('NIXOS_INSTALL_BOOTLOADER=1', flat[boot])
        self.assertLess(install, useradd)
        self.assertLess(useradd, boot)
        self.assertLess(lock, boot)
        self.assertEqual(sum('switch-to-configuration' in c for c in flat), 1)

    def test_install_copies_detected_firmware_and_writes_the_set(self):
        paths = {
            'radeon': '/nix/store/11111111111111111111111111111111-radeon',
            'rtw88': '/nix/store/22222222222222222222222222222222-rtw88',
            'amdgpu': '/nix/store/33333333333333333333333333333333-amdgpu',
        }
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / 'target'
            proof, calls = self.install(tmp, lambda disk, identity: None,
                                        detected=('radeon', 'rtw88'), firmware_paths=paths)
        self.assertTrue(proof['passed'])
        install = next(i for i, argv in enumerate(calls) if argv[0] == 'nixos-install')
        detect = calls.index([FIRMWARE, 'detect', '--system', SYSTEM])
        copy = calls.index(['nix', '--extra-experimental-features', 'nix-command', 'copy', '--offline',
                            '--from', 'local', '--to', f'local?root={target}', '--no-check-sigs',
                            paths['radeon'], paths['rtw88']])
        ensure = calls.index([FIRMWARE, '--root', str(target), 'ensure', SYSTEM, '--add', 'radeon', 'rtw88'])
        boot = next(i for i, argv in enumerate(calls) if 'switch-to-configuration boot' in ' '.join(argv))
        self.assertEqual([install, detect, copy, ensure, boot], sorted([install, detect, copy, ensure, boot]))

    def test_install_stops_before_grub_when_ensuring_the_set_reports_missing_firmware(self):
        calls = []
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, 'Firmware missing from the target store: radeon'):
                self.install(tmp, lambda disk, identity: None, detected=('radeon',), missing=('radeon',),
                             firmware_paths={'radeon': '/nix/store/11111111111111111111111111111111-radeon'},
                             calls=calls)
        self.assertFalse(any('switch-to-configuration boot' in ' '.join(argv) for argv in calls))

    def test_a_missing_boot_loader_fails_the_install(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, 'boot loader was not installed'):
                core.install_target(self.params(), target_root=Path(tmp) / 't',
                                    closure_file=self.closure(tmp),
                                    runner=lambda argv, **kw: firmware_reply(argv),
                                    check_disk=lambda disk, identity: None)


class Backend(unittest.TestCase):
    """The installer screens' backend hands the session's disk identity to the core."""

    def test_the_chosen_disk_identity_reaches_the_install_core(self):
        loader = importlib.machinery.SourceFileLoader(
            'wros_installer_backend', str(ROOT / 'nixos/scripts/wros-installer-backend'))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        backend = importlib.util.module_from_spec(spec)
        loader.exec_module(backend)

        class Channel:
            def send(self, command):
                return 0, ''

            def define(self, *args, **kwargs):
                pass

            def ask(self, tag, kind, *args, **kwargs):
                return {'wros/confirm_erase': 'true', 'wros/user_password': 'p' * 8,
                        'wros/luks_passphrase': 'q' * 8}.get(tag, kwargs.get('default') or '')

        seen = []
        backend.run_dialog(Channel(), 'vdb', '0' * 32, core_runner=seen.append,
                           target_identity=dict(IDENTITY))
        self.assertEqual(len(seen), 1)
        self.assertEqual(seen[0]['target_disk'], 'vdb')
        self.assertEqual(seen[0]['target_identity'], IDENTITY)


class Payload(unittest.TestCase):
    def test_the_disk_identity_is_required(self):
        import io
        import json
        import sys
        base = {'target_disk': 'vdb', 'username': 'writer', 'luks_passphrase': 'x' * 8,
                'user_password': 'y' * 8, 'session_nonce': '0' * 32}
        argv = sys.argv
        sys.argv = ['wros-install-core']
        try:
            with self.assertRaisesRegex(ValueError, 'target_identity'):
                core.read_credentials_from_stream(io.BytesIO(json.dumps(base).encode()))
            got = core.read_credentials_from_stream(
                io.BytesIO(json.dumps({**base, 'target_identity': IDENTITY}).encode()))
            self.assertEqual(got['target_identity'], IDENTITY)
        finally:
            sys.argv = argv


if __name__ == '__main__':
    unittest.main()
