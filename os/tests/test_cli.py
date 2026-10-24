"""Run the compiled app's headless CLI only; never spawn the interactive app as root."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get('GRIMOIRE_BINARY', ROOT/'target/debug/grimoire'))

@unittest.skipUnless(BINARY.is_file(), 'build the no-audio grimoire binary first')
class HeadlessCLI(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='writingdeck-cli-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = os.environ | {'HOME': str(self.root), 'XDG_DATA_HOME': str(self.root/'data'), 'XDG_CONFIG_HOME': str(self.root/'config')}

    def run_cli(self, *args, ok=True):
        result = subprocess.run([str(BINARY), 'writingdeck', *map(str,args)], env=self.env, capture_output=True, text=True, timeout=15)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def hashes(self, root):
        return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}

    def test_simulated_status_is_read_only_and_conspicuously_labeled(self):
        result = json.loads(self.run_cli('status', '--simulate').stdout)
        self.assertTrue(result['simulated'])
        self.assertEqual(list(self.root.iterdir()), [])
        self.run_cli('status', '--bogus', ok=False)
        self.run_cli('status', '--simulate', 'extra', ok=False)

    def test_help_version_and_invalid_commands_are_noninteractive(self):
        self.assertIn('F12', self.run_cli('help').stdout)
        self.assertIn('f3ca1e8', self.run_cli('version').stdout)
        self.run_cli('apply-install', ok=False)
        self.run_cli('tutorial-demo', ok=False)
        self.run_cli('check-config', self.root/'missing', ok=False)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_demo_refuses_existing_directory_and_config_validation_never_writes(self):
        demo = self.root/'new-demo'
        self.run_cli('tutorial-demo', demo)
        before = self.hashes(demo)
        self.assertTrue(any(p.startswith('manuscript/') for p in before))
        self.assertIn('No files changed', self.run_cli('check-config', demo).stdout)
        self.assertEqual(self.hashes(demo), before)
        self.run_cli('tutorial-demo', demo, ok=False)
        self.assertEqual(self.hashes(demo), before)
        empty = self.root/'existing-empty'
        empty.mkdir()
        self.run_cli('tutorial-demo', empty, ok=False)
        self.assertEqual(list(empty.iterdir()), [])

    def test_future_configuration_is_preserved_with_an_actionable_failure(self):
        demo = self.root/'demo'
        self.run_cli('tutorial-demo', demo)
        state = demo/'.grimoire/writingdeck/layout.json'
        state.parent.mkdir(parents=True, exist_ok=True)
        state.write_text('{"version":999,"selected":0,"workspaces":[],"dashboard":[],"launch_dashboard":false,"low_power":false}\n')
        before = self.hashes(demo)
        result = self.run_cli('check-config', demo, ok=False)
        self.assertIn('unsupported WritingDeck', result.stderr)
        self.assertEqual(self.hashes(demo), before)

if __name__ == '__main__':
    unittest.main()
