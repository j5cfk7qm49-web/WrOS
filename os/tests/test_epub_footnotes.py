"""Actual EPUBCheck and compiled TUI publishing over generated book/HOME fixtures."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess

XHTML = '{http://www.w3.org/1999/xhtml}'
EPUB = '{http://www.idpf.org/2007/ops}'


def check_links(test, package):
    references = 0
    global_ids = set()
    with zipfile.ZipFile(package) as archive:
        test.assertEqual(archive.infolist()[0].filename, 'mimetype')
        test.assertEqual(archive.infolist()[0].compress_type, zipfile.ZIP_STORED)
        test.assertEqual(archive.read('mimetype'), b'application/epub+zip')
        for name in archive.namelist():
            if not name.endswith('.xhtml'):
                continue
            data = archive.read(name)
            test.assertNotIn(b'PRIVATE_EPUB', data)
            doc = ET.fromstring(data)
            nodes = list(doc.iter())
            ids = {node.get('id'): node for node in nodes if node.get('id')}
            test.assertEqual(len(ids), sum(bool(node.get('id')) for node in nodes), name)
            for node in nodes:
                role = node.get('role')
                if role not in ('doc-noteref', 'doc-backlink'):
                    continue
                target = node.get('href')
                test.assertTrue(target.startswith('#'), target)
                test.assertIn(target[1:], ids)
                other = ids[target[1:]]
                test.assertTrue(node.get('aria-label'))
                if role == 'doc-noteref':
                    references += 1
                    test.assertEqual(node.get(EPUB+'type'), 'noteref')
                    test.assertEqual(other.tag, XHTML+'aside')
                    test.assertEqual(other.get('role'), 'doc-footnote')
                    backs = [n for n in other.iter() if n.get('role') == 'doc-backlink']
                    test.assertEqual(len(backs), 1)
                    test.assertEqual(backs[0].get('href'), '#'+node.get('id'))
                    test.assertGreaterEqual(len(other.findall(XHTML+'p')), 2)
                else:
                    test.assertEqual(other.get('role'), 'doc-noteref')
                if node.get('id'):
                    test.assertNotIn(node.get('id'), global_ids)
                    global_ids.add(node.get('id'))
    return references


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'publishing must run as an ordinary writer')
class EpubFootnotes(unittest.TestCase):
    def test_real_epubcheck_validates_public_scene_local_notes_and_original_source(self):
        self.assertIsNotNone(shutil.which('java'), 'Debian publishing qualification requires Java')
        jar = Path('/usr/share/java/epubcheck.jar')
        self.assertTrue(jar.is_file(), 'Debian publishing qualification requires epubcheck')
        with tempfile.TemporaryDirectory(prefix='wros-epub-notes-') as temporary:
            base = Path(temporary); project = base/'book'; home = base/'home'
            project.mkdir(); home.mkdir()
            env = os.environ | {'HOME': str(home), 'XDG_CONFIG_HOME': str(home/'config'),
                                'XDG_DATA_HOME': str(home/'data'), 'XDG_CACHE_HOME': str(home/'cache')}
            (project/'novel.toml').write_text('title = "EPUB Notes Fixture"\nauthor = "WrOS Test Writer"\n[ebook]\nlanguage = "en"\n', encoding='utf-8')
            source = ('---\ntitle: "A λ manuscript"\ncustom: "original kept"\n---\n\n'
                      '"A *café* claim"[^a--b] and again[^a--b].\n'
                      '**A line\ncontinued** with `"literal -- code"` and [source](https://example.invalid/source--two).\n\n'
                      '#\n\n[^a--b]: FIRST_NOTEPARAGRAPH café λ *context*. %% PRIVATE_EPUB_SECRET %%\n\n'
                      '    SECOND_NOTEPARAGRAPH with **emphasis** and `"code -- note"`.\n')
            originals = {}
            for relative, text in [
                ('manuscript/01-Chapter-One/01-Scene.md', source),
                ('manuscript/01-Chapter-One/02-Scene.md', 'Another[^a--b].\n\n[^a--b]: SCENE_LOCAL '+('A retained archival context. '*150)+'\n'),
                ('front-matter/01-Ebook/01-Title.md', '# Title\n\nFront[^a--b].\n\n[^a--b]: FRONT_CONTEXT.\n'),
                ('back-matter/01-Ebook/01-Afterword.md', '# Afterword\n\nBack[^a--b].\n\n[^a--b]: BACK_CONTEXT.\n'),
                ('manuscript/01-Chapter-One/03-Cut.md', '---\ncompile: false\n---\n\nExcluded[^missing].\n'),
            ]:
                path = project/relative; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding='utf-8'); originals[path] = path.read_bytes()
            result = subprocess.run([str(BINARY), 'export', '--epub', '--md', str(project)], env=env,
                                    capture_output=True, text=True, encoding='utf-8', timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
            package = next((project/'exports').glob('*.epub'))
            self.assertEqual(check_links(self, package), 5)
            with zipfile.ZipFile(package) as archive:
                chapter = archive.read('OEBPS/chapter-01.xhtml').decode('utf-8')
                self.assertIn('“A <em>café</em> claim”', chapter)
                self.assertIn('<code>"literal -- code"</code>', chapter)
                self.assertIn('https://example.invalid/source--two', chapter)
                self.assertNotIn('Excluded', chapter)
                doc = ET.fromstring(chapter)
                notes = [n for n in doc.iter() if n.get('role') == 'doc-footnote']
                self.assertEqual(len(notes), 3)
                paragraphs = notes[0].findall(XHTML+'p')
                self.assertEqual(len(paragraphs), 3)
                self.assertIn('SECOND_NOTEPARAGRAPH', ''.join(paragraphs[1].itertext()))
                self.assertIn('SCENE_LOCAL', ''.join(notes[2].itertext()))
            markdown = next((project/'exports').glob('*.md')).read_text(encoding='utf-8')
            self.assertIn('`"literal -- code"`', markdown)
            self.assertIn('https://example.invalid/source--two', markdown)
            report = base/'epubcheck.json'
            checked = subprocess.run(['java', '-Xmx256m', '-jar', str(jar), str(package), '--json', str(report)],
                                     env=env, capture_output=True, text=True, encoding='utf-8', timeout=90)
            self.assertEqual(checked.returncode, 0, checked.stdout+checked.stderr)
            receipt = json.loads(report.read_text(encoding='utf-8'))
            self.assertFalse(receipt['messages'], receipt)
            self.assertIn('No errors or warnings detected', checked.stdout+checked.stderr)
            for path, before in originals.items():
                self.assertEqual(path.read_bytes(), before)
            if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                captures = ROOT/'build/verification/epub-footnotes'; captures.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(package, captures/'public-notes.epub')
                shutil.copyfile(report, captures/'epubcheck.json')
                (captures/'render.json').write_text(json.dumps({
                    'passed': True, 'epub_sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
                    'source_sha256': hashlib.sha256(source.encode()).hexdigest(), 'references': 5,
                    'source_preserved': True, 'validator_stdout': checked.stdout, 'validator_stderr': checked.stderr,
                }, indent=2)+'\n', encoding='utf-8')

    def test_epub_only_workflow_preserves_edit_undo_help_and_restart_at_four_sizes(self):
        for width, height in [(80,24), (100,30), (120,40), (160,50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-epub-ui-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = '---\ntitle: "The Brass Key"\ncustom: "λ kept"\n---\n\nλ Intro[^a].\n\n[^a]: A café note. %% PRIVATE_EPUB_SECRET %%\n'
                primary.write_text(before, encoding='utf-8')
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ Intro')
                    terminal.send(b'\x1b[H'); terminal.paste('Added '); terminal.send(b'\x13')
                    changed = before.replace('λ Intro', 'Added λ Intro')
                    terminal.until(lambda: primary.read_bytes() == changed.encode(), 'original edit saved')
                    terminal.send(b'\x0bExport for readers\r'); terminal.screen_shows('EXPORT FOR READERS')
                    terminal.send(b' '); terminal.send(b'p'); terminal.screen_shows('EXPORT PREFLIGHT')
                    self.assertNotIn('Rich export is blocked', terminal.screen.text)
                    terminal.send(b'\x1bOP'); terminal.screen_shows('Compile: the manuscript')
                    terminal.send(b'\x1b'); terminal.until(lambda: 'Compile: the manuscript' not in terminal.screen.text and 'EXPORT PREFLIGHT' in terminal.screen.text, 'help returned')
                    terminal.send(b'\x1b'); terminal.until(lambda: 'EXPORT PREFLIGHT' not in terminal.screen.text and 'EXPORT FOR READERS' in terminal.screen.text, 'review closed')
                    terminal.send(b'x'); terminal.screen_shows('.epub')
                    package = next((project/'exports').glob('*.epub'))
                    self.assertEqual(check_links(self, package), 1)
                    self.assertEqual(len(list((project/'exports').iterdir())), 1)
                    self.assertEqual(primary.read_bytes(), changed.encode())
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/epub-footnote-screens'; captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'epub-footnotes-{width}x{height}.txt').write_text(terminal.screen.text+'\n', encoding='utf-8')
                    terminal.send(b'\x1b'); terminal.until(lambda: 'EXPORT FOR READERS' not in terminal.screen.text, 'returned to editor')
                    terminal.send(b'\x1a\x13'); terminal.until(lambda: primary.read_bytes() == before.encode(), 'original Undo survives EPUB')
                    terminal.send(b'\x11'); terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ Intro')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x11'); terminal.until(lambda: terminal.process.poll() is not None, 'restart normal quit')
                    self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
