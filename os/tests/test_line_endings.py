"""The LF/BOM gate rejects committed CRLF and byte-order marks; it needs git only."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('check_writingdeck', ROOT / 'tools/check-writingdeck.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def git(repo, *args):
    subprocess.run(['git', '-c', 'core.autocrlf=false', '-c', 'user.name=t', '-c', 'user.email=t@example.invalid',
                    '-C', str(repo), *args], check=True, capture_output=True)


class LineEndings(unittest.TestCase):
    def repo(self, files, attributes=''):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        repo = Path(tmp.name)
        git(repo, 'init', '-q')
        if attributes:
            (repo/'.gitattributes').write_bytes(attributes.encode())
        for name, data in files.items():
            path = repo/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        git(repo, 'add', '-A')
        return repo

    def test_clean_lf_tree_passes(self):
        repo = self.repo({'os/scripts/a.sh': b'#!/bin/bash\nexit 0\n', 'docs/x.md': b'x\n', 'bin.dat': b'\0\1\2'})
        self.assertEqual(module.line_ending_errors(repo), [])

    def test_committed_crlf_script_is_rejected(self):
        repo = self.repo({'os/scripts/a.sh': b'#!/bin/bash\r\nexit 0\r\n'})
        errors = module.line_ending_errors(repo)
        self.assertEqual(len(errors), 1)
        self.assertIn('os/scripts/a.sh', errors[0])

    def test_mixed_endings_are_rejected(self):
        repo = self.repo({'tools/m.py': b'a = 1\nb = 2\r\n'})
        self.assertEqual(len(module.line_ending_errors(repo)), 1)

    def test_utf8_bom_in_an_infrastructure_tree_is_rejected(self):
        repo = self.repo({'os/config/installer.preseed': b'\xef\xbb\xbfd-i x boolean true\n'})
        self.assertEqual(len(module.line_ending_errors(repo)), 1)

    def test_bom_outside_the_infrastructure_trees_is_not_a_finding(self):
        repo = self.repo({'example/readme.txt': b'\xef\xbb\xbfhello\n'})
        self.assertEqual(module.line_ending_errors(repo), [])

    def test_declared_upstream_exemption_may_keep_crlf(self):
        repo = self.repo({'third_party/keep.c': b'int x;\r\n'}, attributes='third_party/** -text\n')
        self.assertEqual(module.line_ending_errors(repo), [])

    def test_missing_git_metadata_is_not_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(module.line_ending_errors(Path(tmp)), [])

    def test_this_checkout_follows_the_rule(self):
        self.assertEqual(module.line_ending_errors(ROOT), [])


if __name__ == '__main__':
    unittest.main()
