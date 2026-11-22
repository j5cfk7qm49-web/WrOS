"""Exercise the Essay workflow through the compiled Grimoire CLI."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from xml.etree import ElementTree
from zipfile import ZipFile


ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("GRIMOIRE_BINARY", ROOT / "target/debug/grimoire"))
WORD = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}


@unittest.skipUnless(BINARY.is_file(), "build the current binary first")
class MlaEssay(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run(
            [str(BINARY.resolve()), *map(str, args)],
            capture_output=True,
            encoding="utf-8",
            timeout=60,
            check=False,
        )

    @unittest.skipUnless(shutil.which("pdftotext") and shutil.which("pdffonts"), "needs Poppler utilities")
    def test_essay_word_pdf_and_rejected_missing_source(self):
        with tempfile.TemporaryDirectory(prefix="wros-mla-") as temp:
            project = Path(temp) / "Reading and attention"
            created = self.run_cli("new", "--essay", project)
            self.assertEqual(created.returncode, 0, created.stderr)
            manuscript = project / "manuscript"
            self.assertEqual(sorted(p.stem for p in manuscript.iterdir()),
                             ["01-Introduction", "02-Body", "03-Conclusion"])
            with (project / "project.toml").open("a", encoding="utf-8") as settings:
                settings.write('\n[mla]\nname = "Casey Lane"\nsurname = "Lane"\n'
                               'instructor = "Professor Gray"\ncourse = "Writing 201"\n'
                               'date = "22 November 2026"\n')
            (project / "sources.json").write_text(json.dumps([{
                "id": "baron", "type": "article-journal",
                "title": "Redefining Reading: The Impact of Digital Communication Media",
                "author": [{"family": "Baron", "given": "Naomi S."}], "container-title": "PMLA",
                "volume": "128", "issue": "1", "page": "193-200", "issued": {"date-parts": [[2013, 1]]},
            }]), encoding="utf-8")
            source = manuscript / "01-Introduction.md"
            opening = ("Writing and reading support each other [@baron, p. 194].[^one]\n\n"
                       "[^one]: A public note [-@baron, p. 194].\n")
            source.write_text(opening, encoding="utf-8")
            (manuscript / "02-Body.md").write_text(
                ("A paragraph about reading and attention. " * 12 + "\n\n") * 14,
                encoding="utf-8",
            )
            result = self.run_cli("export", project, "--docx", "--pdf", "--md")
            self.assertEqual(result.returncode, 0, result.stderr)
            word, = (project / "exports").glob("*_MLA.docx")
            pdf = word.with_suffix(".pdf")
            self.assertTrue(pdf.is_file(), result.stderr)
            with ZipFile(word) as zipped:
                document = ElementTree.fromstring(zipped.read("word/document.xml"))
                field = ElementTree.fromstring(zipped.read("word/header1.xml")).find(".//w:instrText", WORD)
                note = ElementTree.fromstring(zipped.read("word/footnotes.xml"))
                self.assertIn("PAGE", field.text)
                self.assertIsNotNone(document.find(".//w:footnoteReference", WORD))
                self.assertIn("Works Cited", "".join(document.itertext()))
                self.assertIn("A public note (194).", "".join(note.itertext()))
            fonts = subprocess.check_output(["pdffonts", str(pdf)], encoding="utf-8")
            self.assertIn("LiberationSerif", fonts)
            self.assertRegex(fonts, r"LiberationSerif.*\byes\b")
            text = subprocess.check_output(["pdftotext", "-layout", str(pdf), "-"], encoding="utf-8")
            pages = [page for page in text.split("\f") if page.strip()]
            self.assertGreaterEqual(len(pages), 3)
            for i, page in enumerate(pages, 1):
                self.assertIn(f"Lane {i}", page)
            self.assertIn("Works Cited", pages[-1])
            self.assertIn("(Baron 194)", pages[0])
            self.assertIn("A public note (194).", text)
            self.assertEqual(source.read_text(encoding="utf-8"), opening)
            previous = {p: p.read_bytes() for p in (project / "exports").iterdir()}
            source.write_text(opening.replace("@baron", "@missing"), encoding="utf-8")
            refused = self.run_cli("export", project, "--docx", "--pdf", "--md")
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("missing", refused.stderr)
            self.assertEqual({p: p.read_bytes() for p in previous}, previous)


if __name__ == "__main__":
    unittest.main()
