"""The import helper validates its own devices and never mounts writable media."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wros-import-media'


def load_helper():
    loader = importlib.machinery.SourceFileLoader('wros_import_media', str(SCRIPT))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def partition(path, fstype='vfat', label='BOOKS', **extra):
    return dict(path=path, type='part', fstype=fstype, label=label, **extra)


def disk(path, children, **extra):
    return dict(path=path, type='disk', tran='usb', rm=True, children=children, **extra)


def flatten(devices, parent=None):
    rows = []
    for device in devices:
        row = {key: value for key, value in device.items() if key != 'children'}
        row['name'] = Path(device['path']).name
        row['pkname'] = parent
        rows.append(row)
        rows += flatten(device.get('children', []), row['name'])
    return rows


class ImportMedia(unittest.TestCase):
    def setUp(self):
        original_umask = os.umask(0o077)
        self.addCleanup(os.umask, original_umask)
        self.helper = load_helper()
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'media'
        self.calls = []
        self.mounted = False
        self.failing_tool = None
        self.devices = [disk('/dev/sdb', [partition('/dev/sdb1')])]

    def runner(self, argv):
        self.calls.append(argv)
        tool = Path(argv[0]).name
        if tool == self.failing_tool:
            return subprocess.CompletedProcess(argv, 1, '', 'device unavailable')
        if tool == 'findmnt':
            output = '/dev/mapper/root\n'
        elif tool == 'lsblk' and '-J' in argv:
            # lsblk -J --list prints every device as its own row, naming its parent.
            self.assertIn('--list', argv)
            output = json.dumps({'blockdevices': flatten(self.devices)})
        elif tool == 'lsblk':
            output = '/dev/mapper/root\n/dev/sda2\n/dev/sda\n'
        elif tool == 'mount':
            self.mounted = True
            output = ''
        elif tool == 'umount':
            self.assertTrue(self.mounted)
            self.mounted = False
            output = ''
        else:
            self.fail(f'unexpected command: {argv}')
        return subprocess.CompletedProcess(argv, 0, output, '')

    def mountinfo(self):
        if not self.mounted:
            return ''
        return f'25 1 8:17 / {self.root}/1 ro - vfat /dev/sdb1 ro\n'

    def run_helper(self, argv, **kwargs):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = self.helper.main(
                argv, environ={'PKEXEC_UID': '1000'}, euid=0, runner=self.runner,
                media_root=self.root, mountinfo_text=self.mountinfo(), **kwargs)
        self.assertEqual(result, 0)
        return json.loads(output.getvalue())

    def test_list_hides_the_root_disk_luks_and_non_usb_devices(self):
        self.devices += [
            disk('/dev/sda', [partition('/dev/sda1'),
                              partition('/dev/sda2', 'crypto_LUKS')]),
            disk('/dev/sdc', [partition('/dev/sdc1', 'crypto_LUKS', children=[
                dict(path='/dev/mapper/secret', type='crypt', fstype='ext4')])]),
            dict(path='/dev/nvme0n1', type='disk', tran='nvme', rm=False,
                 children=[partition('/dev/nvme0n1p1', 'ext4')]),
            dict(path='/dev/sdd', type='disk', tran='usb', rm=True, fstype='vfat'),
            disk('/dev/sde', [partition('/dev/sde1', 'swap')]),
        ]
        self.assertEqual(self.run_helper(['list']), [
            dict(device='/dev/sdb1', label='BOOKS', mount=None)])

    def test_mount_refuses_any_device_not_in_a_fresh_listing(self):
        for device in ['/dev/sda1', '/dev/sdb', '--bind', '/dev/../etc', '/tmp/stick']:
            with self.subTest(device=device), self.assertRaises(ValueError):
                self.run_helper(['mount', device])
        self.assertFalse(any(Path(c[0]).name == 'mount' for c in self.calls))

    def test_mount_is_read_only_and_replaces_the_previous_mount(self):
        reply = self.run_helper(['mount', '/dev/sdb1'])
        self.assertEqual(reply, [dict(device='/dev/sdb1', label='BOOKS',
                                     mount=str(self.root / '1'))])
        mount = next(c for c in self.calls if Path(c[0]).name == 'mount')
        self.assertEqual(mount[mount.index('-o') + 1], 'ro,nodev,nosuid,noexec')
        self.assertEqual(mount[-3:], ['--', '/dev/sdb1', str(self.root / '1')])
        self.run_helper(['mount', '/dev/sdb1'])
        verbs = [Path(c[0]).name for c in self.calls if Path(c[0]).name in ('mount', 'umount')]
        self.assertEqual(verbs, ['mount', 'umount', 'mount'])

    def test_unmount_twice_succeeds(self):
        self.run_helper(['mount', '/dev/sdb1'])
        self.assertEqual(self.run_helper(['unmount']), [])
        self.assertEqual(self.run_helper(['unmount']), [])
        self.assertEqual(sum(Path(c[0]).name == 'umount' for c in self.calls), 1)

    def test_ext_journals_are_not_replayed_onto_the_source(self):
        self.devices[0]['children'][0]['fstype'] = 'ext4'
        self.run_helper(['mount', '/dev/sdb1'])
        mount = next(c for c in self.calls if Path(c[0]).name == 'mount')
        self.assertEqual(mount[mount.index('-o') + 1], 'ro,nodev,nosuid,noexec,noload')

    def test_requires_polkit_root_and_exact_arguments(self):
        for argv in [[], ['list', '/dev/sdb1'], ['mount'], ['unmount', '/tmp'], ['other']]:
            with self.subTest(argv=argv), self.assertRaises(ValueError):
                self.run_helper(argv)
        for environ, euid in [({}, 0), ({'PKEXEC_UID': 'bad'}, 0),
                              ({'PKEXEC_UID': '1000'}, 1000)]:
            with self.assertRaises(ValueError):
                self.helper.main(['list'], environ=environ, euid=euid, runner=self.runner)
        self.assertEqual(self.calls, [])

    def test_tool_failures_are_not_reported_as_success(self):
        for tool in ['findmnt', 'lsblk', 'mount']:
            self.failing_tool = tool
            with self.subTest(tool=tool), self.assertRaises(ValueError):
                self.run_helper(['mount', '/dev/sdb1'])
        self.failing_tool = None
        self.run_helper(['mount', '/dev/sdb1'])
        self.failing_tool = 'umount'
        with self.assertRaises(ValueError):
            self.run_helper(['unmount'])
        self.assertTrue(self.mounted)

    def test_refuses_symlink_mount_directories(self):
        elsewhere = Path(self.temp.name) / 'elsewhere'
        elsewhere.mkdir()
        self.root.symlink_to(elsewhere, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.run_helper(['mount', '/dev/sdb1'])
        self.assertFalse(any(Path(c[0]).name == 'mount' for c in self.calls))


if __name__ == '__main__':
    unittest.main()
