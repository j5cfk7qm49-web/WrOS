"""Tests for the per-generation firmware tool, using a fake system and store."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


TOOL = Path(__file__).resolve().parents[2] / 'nixos/scripts/wros-firmware'
SETS = Path('/nix/var/wros/firmware')
ROOTS = Path('/nix/var/nix/gcroots/wros-firmware')
PROFILE = Path('/nix/var/nix/profiles/system')
CURRENT = Path('/run/current-system')
BOOTED = Path('/run/booted-system')
OUT = Path('/boot/initrd-secrets')
ALIASES = ('pci:v00001234d00000001', 'usb:v1234p0002', 'pci:v00001234d00000003')


def store(name):
    return Path('/nix/store') / ('a' * 32 + '-' + name)


def read_newc(archive):
    entries = {}
    offset = 0
    while True:
        if archive[offset:offset + 6] != b'070701':
            raise ValueError('invalid newc header')
        size = int(archive[offset + 54:offset + 62], 16)
        namesize = int(archive[offset + 94:offset + 102], 16)
        name = archive[offset + 110:offset + 109 + namesize].decode()
        if name == 'TRAILER!!!':
            return entries
        mode = int(archive[offset + 14:offset + 22], 16)
        offset = (offset + 110 + namesize + 3) & ~3
        entries[name] = (mode, archive[offset:offset + size])
        offset = (offset + size + 3) & ~3


class FirmwareTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        loader = importlib.machinery.SourceFileLoader('wros_firmware', str(TOOL))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        cls.tool = importlib.util.module_from_spec(spec)
        loader.exec_module(cls.tool)
        cls.tool.TRUSTED_UID = os.geteuid()
        cls.tool.MODPROBE = shutil.which('modprobe')
        if not cls.tool.MODPROBE:
            raise RuntimeError('modprobe is required for the firmware tests')

        fixture = tempfile.TemporaryDirectory()
        cls.addClassCleanup(fixture.cleanup)
        cls.modules = Path(fixture.name)
        version = cls.modules / 'lib/modules/6.0.0-wros-test'
        version.mkdir(parents=True)
        # kmod reads depmod's binary indices. Generate those and the text fixtures
        # from tiny objects with just a module alias, without a running kernel build.
        for module, alias in zip(('i915', 'btusb'), ALIASES):
            subprocess.run(
                ['cc', '-x', 'c', '-c', '-o', version / f'{module}.ko', '-'],
                input=f'const char alias[] __attribute__((section(".modinfo"))) = "alias={alias}*";',
                text=True, check=True, capture_output=True,
            )
        for name in ('modules.order', 'modules.builtin', 'modules.builtin.modinfo'):
            (version / name).touch()
        subprocess.run(['depmod', '-b', cls.modules, version.name], check=True)
        for path in version.iterdir():
            if path.name not in ('modules.alias', 'modules.alias.bin', 'modules.dep', 'modules.dep.bin'):
                path.unlink()

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.old, self.new, self.stale = (store(f'nixos-system-wros-{name}')
                                          for name in ('old', 'new', 'stale'))
        self.packages = {}
        self.package('i915', ['i915'], 'intel/full.bin', size=30, early=True)
        self.package('i915-glk', ['i915'], 'intel/glk.bin', early=True)
        self.package('bt-intel', ['btusb'], 'intel/bt.bin')
        self.package('rtw88', ['rtw88_8822be'], 'rtw88/wifi.bin')
        self.package('sof', ['snd_sof'], 'intel/sof.bin')
        for system in (self.old, self.new, self.stale):
            disk = self.disk(system)
            disk.mkdir()
            (disk / 'kernel-modules').symlink_to(self.modules, target_is_directory=True)
            (disk / 'wros-firmware.json').write_text(json.dumps({'format': 1, 'packages': self.packages}))
        for number, alias in enumerate(ALIASES):
            path = self.disk(f'/sys/bus/test/devices/{number}/modalias')
            path.parent.mkdir(parents=True)
            path.write_text(alias + '\n')
        self.disk('/proc').mkdir()
        self.disk('/proc/modules').write_text('rtw88_8822be 1 0 - Live 0x0\n')
        self.disk('/boot').mkdir()
        self.disk(ROOTS.parent).mkdir(parents=True)
        self.disk(CURRENT).parent.mkdir()
        self.disk(CURRENT).symlink_to(self.old)

    def disk(self, path):
        return self.root / str(path).lstrip('/')

    def package(self, name, modules, file, *, size=10, early=False):
        path = store('wros-firmware-' + name)
        firmware = self.disk(path / 'lib/firmware' / file)
        firmware.parent.mkdir(parents=True)
        firmware.write_bytes(name.encode())
        self.packages[name] = {
            'store_path': str(path), 'nar_size': size, 'nar_sha256': 'b' * 64,
            'modules': modules, 'initrd': early,
        }

    def run_tool(self, *args, error=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = self.tool.main(['--root', str(self.root), *map(str, args)])
        if error:
            self.assertNotEqual(result, 0)
            self.assertEqual(stdout.getvalue(), '')
            self.assertIn(error, stderr.getvalue())
            return
        self.assertEqual(result, 0, stderr.getvalue())
        self.assertEqual(len(stdout.getvalue().splitlines()), 1)
        return json.loads(stdout.getvalue())

    def test_detect_maps_aliases_and_loaded_modules_to_packages(self):
        result = self.run_tool('detect', '--system', CURRENT, '--sys-root', self.root,
                               '--modules', self.modules)
        self.assertEqual(result['packages'], ['bt-intel', 'i915', 'rtw88'])
        self.assertFalse(self.disk(SETS).exists())

    def test_an_unknown_alias_is_reported_not_fatal(self):
        result = self.run_tool('detect', '--system', self.new)
        self.assertEqual(result['unknown'], [ALIASES[2]])

    def test_a_module_two_packages_share_picks_one(self):
        self.assertEqual(self.run_tool('detect', '--system', self.new)['packages'],
                         ['bt-intel', 'i915', 'rtw88'])
        full = self.disk(self.packages['i915']['store_path'])
        full.rename(full.with_name('full-i915-offline'))
        self.assertEqual(self.run_tool('detect', '--system', self.new)['packages'],
                         ['bt-intel', 'i915-glk', 'rtw88'])
        shutil.rmtree(self.disk(self.packages['i915-glk']['store_path']))
        self.assertEqual(self.run_tool('detect', '--system', self.new)['packages'],
                         ['bt-intel', 'i915', 'rtw88'])
        self.package('i915-glk', ['i915'], 'intel/glk.bin', early=True)
        self.run_tool('ensure', self.old)
        full.with_name('full-i915-offline').rename(full)
        self.assertEqual(self.run_tool('detect', '--system', self.new)['packages'],
                         ['bt-intel', 'i915-glk', 'rtw88'])

    def test_ensure_keeps_the_running_sets_packages(self):
        self.run_tool('ensure', self.old, '--add', 'sof')
        self.disk('/proc/modules').unlink()
        self.run_tool('ensure', self.new)
        target = self.disk(SETS / self.new.name)
        expected = ['bt-intel', 'i915', 'rtw88', 'sof']
        self.assertEqual(json.loads((target / 'packages.json').read_text())['packages'], expected)
        path = target / 'lib/firmware/rtw88/wifi.bin'
        self.assertEqual(os.readlink(path), str(store('wros-firmware-rtw88') / 'lib/firmware/rtw88/wifi.bin'))
        self.assertEqual(os.readlink(self.disk(ROOTS / f'{self.new.name}--sof')),
                         str(store('wros-firmware-sof')))
        with mock.patch.object(self.tool, 'exchange', side_effect=OSError('simulated rename failure')):
            self.run_tool('ensure', self.new, '--add', 'i915-glk', error='simulated rename failure')
        self.assertEqual(json.loads((target / 'packages.json').read_text())['packages'], expected)
        self.assertFalse(self.disk(ROOTS / f'{self.new.name}--i915-glk').is_symlink())
        self.assertFalse(any(path.name.startswith('.new-') for path in target.parent.iterdir()))
        self.run_tool('ensure', self.new, '--add', 'i915-glk')
        with mock.patch.object(self.tool, 'MODPROBE', shutil.which('false')):
            result = self.run_tool('ensure', self.new)
        self.assertEqual(result['packages'], sorted(expected + ['i915-glk']))
        self.assertEqual(result['unknown'], sorted(ALIASES))

    def test_ensure_reports_a_package_missing_from_the_store(self):
        package = self.disk(self.packages['rtw88']['store_path'])
        shutil.rmtree(package)
        result = self.run_tool('ensure', self.new, '--add', 'not-in-table')
        self.assertEqual(result['missing'], ['not-in-table', 'rtw88'])
        self.assertEqual(result['packages'], ['bt-intel', 'i915'])
        self.assertFalse(self.disk(ROOTS / f'{self.new.name}--rtw88').is_symlink())
        self.package('rtw88', ['rtw88_8822be'], 'rtw88/wifi.bin')
        self.run_tool('ensure', self.new)
        shutil.rmtree(package)
        result = self.run_tool('ensure', self.new)
        self.assertEqual(result['missing'], ['rtw88'])
        self.assertTrue(self.disk(SETS / self.new.name / 'lib/firmware/rtw88/wifi.bin').is_symlink())

    def test_ensure_refuses_a_planted_symlink(self):
        elsewhere = self.disk('/elsewhere')
        elsewhere.mkdir()
        for logical in (SETS, ROOTS, SETS / self.new.name, SETS / ('.new-' + self.new.name),
                        ROOTS / f'{self.new.name}--i915'):
            with self.subTest(path=logical):
                link = self.disk(logical)
                link.parent.mkdir(parents=True, exist_ok=True)
                link.symlink_to(elsewhere)
                self.run_tool('ensure', self.new, error=str(logical.name))
                self.assertEqual(list(elsewhere.iterdir()), [])
                self.assertEqual(os.readlink(link), str(elsewhere))
                link.unlink()
        self.disk(SETS).chmod(0o775)
        self.run_tool('ensure', self.new, error='trusted directory')

    def test_initrd_packs_only_early_boot_firmware(self):
        firmware = self.disk(store('wros-firmware-i915') / 'lib/firmware/intel')
        (firmware / 'copy.bin').symlink_to('full.bin')
        late = self.disk(store('wros-firmware-bt-intel') / 'lib/firmware/intel/full.bin')
        late.write_bytes(b'late version of the same file')
        old_mask = os.umask(0o137)  # GRUB's append-initrd-secrets caller
        try:
            output = self.run_tool('initrd', self.new, OUT)
        finally:
            os.umask(old_mask)
        self.assertEqual(output['bytes'], self.disk(OUT).stat().st_size)
        self.assertEqual(self.disk(SETS).stat().st_mode & 0o777, 0o755)
        entries = read_newc(self.disk(OUT).read_bytes())
        self.assertEqual(entries, {
            'wros-firmware': (0o40755, b''),
            'wros-firmware/intel': (0o40755, b''),
            'wros-firmware/intel/copy.bin': (0o100644, b'i915'),
            'wros-firmware/intel/full.bin': (0o100644, b'i915'),
        })

    def test_initrd_refuses_when_boot_is_almost_full(self):
        output = self.disk(OUT)
        output.write_bytes(b'old archive')
        free = mock.Mock(f_bavail=64 * 1024 * 1024 // 4096, f_frsize=4096)
        with mock.patch.object(self.tool.os, 'statvfs', return_value=free):
            self.run_tool('initrd', self.new, OUT, error='64 MiB')
        self.assertEqual(output.read_bytes(), b'old archive')
        self.assertEqual(list(output.parent.iterdir()), [output])
        output.unlink()
        output.symlink_to(self.disk(CURRENT))
        self.run_tool('initrd', self.new, OUT, error='trusted initrd output file')

    def test_link_replaces_its_own_symlink_and_refuses_a_directory(self):
        path = self.disk('/lib/firmware')
        self.run_tool('link', self.old)
        self.assertEqual(os.readlink(path), str(SETS / self.old.name / 'lib/firmware'))
        self.run_tool('link', self.new)
        self.assertEqual(os.readlink(path), str(SETS / self.new.name / 'lib/firmware'))
        path.unlink()
        path.symlink_to('/elsewhere')
        self.run_tool('link', self.new, error='firmware')
        self.assertEqual(os.readlink(path), '/elsewhere')
        path.unlink()
        path.mkdir()
        self.run_tool('link', self.new, error='firmware')
        self.assertTrue(path.is_dir())
        path.rmdir()
        planted = self.disk(SETS / self.new.name / 'lib/firmware/intel/planted.bin')
        planted.symlink_to('/elsewhere')
        self.run_tool('link', self.new, error='Unexpected firmware symlink')

    def test_prune_keeps_folders_of_existing_generations(self):
        for system in (self.old, self.new, self.stale):
            self.run_tool('ensure', system)
        self.disk(PROFILE).parent.mkdir(parents=True)
        self.disk(PROFILE.with_name('system-1-link')).symlink_to(self.old)
        self.disk(PROFILE.with_name('system-2-link')).symlink_to(self.new)
        self.disk(PROFILE).symlink_to('system-2-link')
        self.run_tool('prune', '--profile', PROFILE)
        self.assertEqual(sorted(path.name for path in self.disk(SETS).iterdir()),
                         [self.new.name, self.old.name])
        self.assertEqual({path.name.split('--')[0] for path in self.disk(ROOTS).iterdir()},
                         {self.new.name, self.old.name})

    def test_prune_keeps_the_running_and_booted_systems_sets(self):
        for system in (self.old, self.new, self.stale):
            self.run_tool('ensure', system, '--add', 'sof')
        self.disk(CURRENT).unlink()
        self.disk(CURRENT).symlink_to(self.stale)
        self.disk(BOOTED).symlink_to(self.old)
        self.disk(PROFILE).parent.mkdir(parents=True)
        self.disk(PROFILE.with_name('system-1-link')).symlink_to(self.new)
        self.disk(PROFILE).symlink_to('system-1-link')
        roots = {path.name for path in self.disk(ROOTS).iterdir()}

        self.assertEqual(self.run_tool('prune', '--profile', PROFILE), {'removed': []})
        self.assertEqual({path.name for path in self.disk(SETS).iterdir()},
                         {self.old.name, self.new.name, self.stale.name})
        self.assertEqual({path.name for path in self.disk(ROOTS).iterdir()}, roots)

    def test_prune_keeps_the_linked_set_before_current_system_changes(self):
        self.disk(PROFILE).parent.mkdir(parents=True)
        self.disk(PROFILE.with_name('system-1-link')).symlink_to(self.old)
        self.disk(BOOTED).symlink_to(self.old)
        self.run_tool('link', self.new)

        self.assertEqual(self.run_tool('prune', '--profile', PROFILE), {'removed': []})
        self.assertTrue(self.disk(SETS / self.new.name / 'lib/firmware').is_dir())
        self.assertTrue(self.disk(ROOTS / f'{self.new.name}--i915').is_symlink())


if __name__ == '__main__':
    unittest.main()
