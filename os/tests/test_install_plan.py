import importlib.machinery
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1] / 'scripts/install-plan.py'
module = importlib.machinery.SourceFileLoader('writingdeck_install_plan', str(SCRIPT)).load_module()


class InstallPlan(unittest.TestCase):
    def inventory(self, **changes):
        disk = dict(path='/dev/fixture-disk', type='disk', size=64 * 1024**3,
                    mountpoints=[None], ro=False, model='Disposable fixture', children=[])
        disk.update(changes)
        return {'blockdevices': [disk]}

    def test_clean_candidate_is_only_a_review_not_permission_to_apply(self):
        result = module.plan(self.inventory(), '/dev/fixture-disk', True, 'author', 'America/Los_Angeles', 'colemak-dh-ansi')
        self.assertTrue(result['selection_eligible_for_review'])
        self.assertFalse(result['can_apply'])
        self.assertFalse(result['destructive_operations_executed'])
        self.assertEqual(result['requested']['keyboard'], 'colemak-dh-ansi')
        self.assertIn('LUKS', result['requested']['encryption'])

    def test_mounted_children_readonly_and_small_targets_are_refused(self):
        for changes in [dict(children=[dict(mountpoints=['/'])]), dict(ro=True), dict(size=1024), dict(type='part')]:
            result = module.plan(self.inventory(**changes), '/dev/fixture-disk')
            self.assertFalse(result['selection_eligible_for_review'])
            self.assertTrue(result['errors'])

    def test_unknown_disk_username_timezone_and_layout_are_not_invented(self):
        self.assertFalse(module.plan(self.inventory(), '/dev/not-in-inventory')['selection_eligible_for_review'])
        for kwargs in [dict(username='root'), dict(username='x;reboot'), dict(timezone='../../etc/shadow'), dict(keyboard='colemak;reboot')]:
            with self.assertRaises(ValueError):
                module.plan(self.inventory(), **kwargs)

    def test_cli_has_no_apply_mode_and_fixture_never_needs_host_disk(self):
        with tempfile.TemporaryDirectory() as temp:
            fixture = Path(temp) / 'lsblk.json'
            fixture.write_text(json.dumps(self.inventory()))
            args = [sys.executable, str(SCRIPT), '--inventory-fixture', str(fixture), '--disk', '/dev/fixture-disk']
            result = subprocess.run(args, capture_output=True, text=True, check=True)
            self.assertFalse(json.loads(result.stdout)['destructive_operations_executed'])
            invalid = subprocess.run(args + ['--apply'], capture_output=True, text=True)
            self.assertNotEqual(invalid.returncode, 0)
            self.assertIn('unrecognized arguments', invalid.stderr)


if __name__ == '__main__':
    unittest.main()
