"""Unit tests for choosing an update bundle's Debian packages (tools/collect-update-runtime.py, #117)."""
import importlib.machinery
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
collect = importlib.machinery.SourceFileLoader(
    'collect_update_runtime', str(ROOT / 'tools/collect-update-runtime.py')).load_module()


class CollectRuntimeTests(unittest.TestCase):
    @unittest.skipUnless(Path('/usr/bin/dpkg-deb').exists(), 'needs dpkg-deb')
    def test_fields_are_read_from_a_real_package_one_or_several_at_a_time(self):
        import subprocess
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / 'tree'
            (tree / 'DEBIAN').mkdir(parents=True)
            (tree / 'DEBIAN/control').write_text(
                'Package: wros\nVersion: 0.1.0~rc12\nArchitecture: amd64\nMaintainer: WrOS <wros@invalid>\n'
                'Description: test\n multi-line: text that looks like a field\n')
            deb = Path(tmp) / 'wros.deb'
            subprocess.run(['/usr/bin/dpkg-deb', '--root-owner-group', '--build', str(tree), str(deb)],
                           check=True, capture_output=True)
            self.assertEqual(collect.deb_field(deb, 'Package'), ['wros'])
            self.assertEqual(collect.deb_field(deb, 'Package', 'Version'), ['wros', '0.1.0~rc12'])

    def test_sources_include_debians_updates_and_security_archives(self):
        lines = collect.SOURCES.splitlines()
        self.assertEqual([line.split()[2] for line in lines], ['trixie', 'trixie-updates', 'trixie-security'])
        self.assertTrue(all(line.startswith('deb https://') for line in lines))

    def test_the_plan_lists_every_installed_package_but_wros(self):
        simulation = ('NOTE: This is only a simulation!\n'
                      'Inst libfoo1 (1.2-3 Debian:13.2/stable [amd64])\n'
                      'Inst xserver-common [2:21.1.16-1.3+deb13u3] (2:21.1.16-1.3+deb13u4 '
                      'Debian-Security:13/stable-security [all])\n'
                      'Inst wros [0.1.0~rc11] (0.1.0~rc12 local-deb [amd64])\n'
                      'Conf libfoo1 (1.2-3 Debian:13.2/stable [amd64])\n')
        self.assertEqual(collect.planned(simulation),
                         {'libfoo1': '1.2-3', 'xserver-common': '2:21.1.16-1.3+deb13u4'})

    def test_an_archive_older_than_debians_newest_is_named(self):
        chosen = {'xserver-common': '2:21.1.16-1.3+deb13u3', 'cage': '0.2.0-2'}
        newest = {'xserver-common': '2:21.1.16-1.3+deb13u4', 'cage': '0.2.0-2'}
        self.assertEqual(collect.stale(newest, chosen),
                         [('xserver-common', '2:21.1.16-1.3+deb13u3', '2:21.1.16-1.3+deb13u4')])
        self.assertEqual(collect.stale(newest, {'cage': '0.2.0-2'}), [])
        self.assertEqual(collect.stale({}, {'gone': '1'}), [('gone', '1', None)])


if __name__ == '__main__':
    unittest.main()
