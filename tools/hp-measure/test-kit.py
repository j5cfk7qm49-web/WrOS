#!/usr/bin/env python3
"""Check the built kit without an HP Stream or a network connection."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class Kit(unittest.TestCase):
    def invoke(self, *args, **options):
        return subprocess.run([self.wrapper, *args], capture_output=True, text=True,
                              timeout=60, **options)

    def test_manifest_hashes_match(self):
        manifest = json.loads((self.corpus / "MANIFEST.json").read_text(encoding="utf-8"))
        hashes = manifest["sha256"]
        self.assertEqual(hashes.keys(), {path.name for path in self.corpus.iterdir()} - {"MANIFEST.json"})
        for name, digest in hashes.items():
            with self.subTest(name=name), (self.corpus / name).open("rb") as stream:
                self.assertEqual(hashlib.file_digest(stream, "sha256").hexdigest(), digest)
        for document in ("paper", "report", "book"):
            self.assertIn(f"{document}.pdf", hashes)
            self.assertIn(f"{document}-scan-gray.pdf", hashes)
            self.assertIn(f"{document}-scan-g4.pdf", hashes)
            if manifest["jbig2"] == "generated":
                self.assertIn(f"{document}-scan-jbig2.pdf", hashes)
        self.assertIn("chinese.pdf", hashes)
        self.assertIn("vertical.pdf", hashes)
        result = self.invoke("--verify-corpus", check=True)
        self.assertIn(f"Verified {len(hashes)} corpus hashes", result.stdout)

    def test_wrapper_argument_parsing(self):
        self.assertIn("--pdf-dir", self.invoke("--help", check=True).stdout)
        self.assertEqual(self.invoke("--quick", "--full").returncode, 2)
        self.assertEqual(self.invoke("--measure", "unknown").returncode, 2)
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            (home / "private draft.PDF").symlink_to(self.corpus / "paper.pdf")
            self.invoke("--quick", "--measure", "hardware", "--pdf-dir", home,
                        check=True, env={**os.environ, "HOME": directory})
            report, = home.glob("hp-measure-????-??-??.json")
            data = json.loads(report.read_text(encoding="utf-8"))
            with (self.corpus / "paper.pdf").open("rb") as source:
                digest = hashlib.file_digest(source, "sha256").hexdigest()
            self.assertEqual(data["profile"], "quick")
            self.assertEqual(data["selected"], ["hardware"])
            self.assertEqual(data["samples"], [{"source": "owner", "page": 1, "sha256": digest}])
            self.assertNotIn("private draft", report.read_text(encoding="utf-8"))

    def test_packaged_render_extract_smoke(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "smoke.json"
            self.invoke("--measure", "smoke", "--output", output, check=True)
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertTrue(report["smoke"])
            sample, = report["samples"]
            self.assertEqual(sample["source"], "paper.pdf")
            self.assertEqual(sample["render"][0]["cold"]["width"], 1366)
            self.assertGreater(sample["extract"][0]["characters"], 0)


if __name__ == "__main__":
    kit = Path(sys.argv.pop(1))
    Kit.wrapper = kit / "bin/wros-hp-measure"
    Kit.corpus = kit / "share/hp-measure/corpus"
    unittest.main()
