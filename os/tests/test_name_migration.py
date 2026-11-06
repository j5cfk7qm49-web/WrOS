from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
MIGRATE = ROOT / 'os/scripts/wros-name-migrate.py'


class NameMigration(unittest.TestCase):
    def test_unrelated_groups_with_both_names_are_not_combined(self):
        with tempfile.TemporaryDirectory(prefix='wros-name-') as tmp:
            root = Path(tmp)
            (root / 'etc/wros').mkdir(parents=True)
            (root / 'etc/group').write_text('writingdeck:x:200:writer\nwros:x:201:someone\n')
            attempt = subprocess.run(['python3', '-I', str(MIGRATE), str(root)], capture_output=True, text=True)
            self.assertNotEqual(attempt.returncode, 0)
            self.assertIn('Both writer group names already exist', attempt.stderr)
            self.assertFalse((root / 'etc/wros/writer-group').exists())

    def test_upgrade_preserves_the_committed_layout_and_builds_legacy_aliases(self):
        with tempfile.TemporaryDirectory(prefix='wros-name-') as tmp:
            root = Path(tmp)
            files = {
                'etc/writingdeck/keyboard.json': b'{"layout":"colemak"}\n',
                'etc/writingdeck/installer-keyboard-pending': b'',
                'etc/writingdeck/image-profile': b'IMAGE_PROFILE=installer\n',
                'etc/wros/keyboard.json': b'{"layout":"qwerty"}\n',
                'etc/wros/update-signers': b'fixture pin\n',
                'etc/wros/release': b'WROS_VERSION=0.1.0~rc7\n',
                'usr/share/wros/keymaps/colemak.map': b'fixture map\n',
                'usr/libexec/wros-keyboard': b'fixture keyboard helper\n',
                'etc/systemd/system/wros-keyboard.service': b'fixture service\n',
            }
            for relative, data in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            for _ in range(2):
                subprocess.run(['python3', '-I', str(MIGRATE), str(root)], check=True, capture_output=True)
            for relative in ('keyboard.json', 'installer-keyboard-pending', 'image-profile'):
                old = root / 'etc/writingdeck' / relative
                self.assertTrue(old.is_symlink())
                self.assertEqual((root / 'etc/wros' / relative).read_bytes(), files['etc/writingdeck/' + relative])
                self.assertEqual(old.read_bytes(), files['etc/writingdeck/' + relative])
            self.assertEqual((root / 'etc/wros/update-signers').read_bytes(), b'fixture pin\n')
            self.assertIn('WRITINGDECK_VERSION=0.1.0~rc7', (root / 'etc/writingdeck/release').read_text())
            for relative in ('usr/libexec/writingdeck-keyboard', 'usr/share/writingdeck/keymaps',
                             'etc/systemd/system/writingdeck-keyboard.service'):
                self.assertTrue((root / relative).is_symlink())
