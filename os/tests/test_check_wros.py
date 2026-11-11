"""Exercise the documentation gate with read-only changes to its inputs."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('check_wros', ROOT / 'tools/check-wros.py')
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)
TUTORIAL = ROOT / 'crates/grimoire-core/assets/wros/tutorial.json'


class DocumentationGate(unittest.TestCase):
    def run_gate(self):
        checker.ERRORS.clear()
        with patch.object(sys, 'argv', ['check-wros.py']), \
             redirect_stdout(io.StringIO()) as stdout, redirect_stderr(io.StringIO()) as stderr:
            status = checker.main()
        return status, stdout.getvalue(), stderr.getvalue()

    def test_current_index_and_lesson_sequence_pass(self):
        status, stdout, stderr = self.run_gate()
        self.assertEqual(status, 0, stderr)
        count = len(json.loads(TUTORIAL.read_bytes()))
        self.assertIn(f'{count} lessons', stdout)

    def test_missing_current_document_is_rejected(self):
        is_file = Path.is_file
        removed = ROOT / 'docs/INSTALLATION.md'
        with patch.object(Path, 'is_file', lambda path: path != removed and is_file(path)):
            status, _, stderr = self.run_gate()
        self.assertEqual(status, 1)
        self.assertIn('missing INSTALLATION.md', stderr)

    def test_missing_duplicate_and_noninteger_lesson_ids_are_rejected(self):
        lessons = json.loads(TUTORIAL.read_bytes())
        read_text = Path.read_text
        for changed in (
            lessons[:9] + lessons[10:],
            lessons[:9] + [lessons[8]] + lessons[10:],
            [dict(lessons[0], id=True), *lessons[1:]],
            [dict(lessons[0], id=1.0), *lessons[1:]],
        ):
            def read(path, *args, **kwargs):
                return json.dumps(changed) if path == TUTORIAL else read_text(path, *args, **kwargs)

            with patch.object(Path, 'read_text', read):
                status, _, stderr = self.run_gate()
            self.assertEqual(status, 1)
            self.assertIn('tutorial must keep IDs 1–50', stderr)
