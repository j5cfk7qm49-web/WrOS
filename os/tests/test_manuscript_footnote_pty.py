"""Actual TUI Word/PDF export, retained editor Undo and fresh process restart."""
import os
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess

@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid()==0, 'Grimoire must run as an ordinary writer')
class ManuscriptFootnotePTY(unittest.TestCase):
    def test_real_word_pdf_export_preserves_original_edit_undo_and_restart_at_four_sizes(self):
        for width,height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-manuscript-notes-') as temporary:
                project,home,primary=fixture(Path(temporary))
                for path in (project/'manuscript').rglob('*.md'):
                    if path!=primary:path.write_text('Another independent scene.\n',encoding='utf-8')
                before='---\ntitle: "The Brass Key"\ncustom: "λ retained"\n---\n\nλ Intro[^a].\n\n[^a]: A *café* note. %% PRIVATE_MANUSCRIPT %%\n'
                primary.write_text(before,encoding='utf-8')
                progress=project/'.grimoire/writingdeck/tutorial.json'
                progress.parent.mkdir(parents=True,exist_ok=True)
                progress.write_text(json.dumps({'version':1,'active':34,'completed':[],'skipped':[]})+'\n',encoding='utf-8')
                def completed():return 34 in json.loads(progress.read_text())['completed']
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bThe Brass Key\r');terminal.screen_shows('λ Intro')
                    terminal.send(b'\x1b[H');terminal.paste('Added ');terminal.send(b'\x13')
                    changed=before.replace('λ Intro','Added λ Intro')
                    terminal.until(lambda:primary.read_bytes()==changed.encode(),'original edit saved')
                    terminal.send(b'\x0bExport for readers\r');terminal.screen_shows('EXPORT FOR READERS')
                    terminal.send(b'x');terminal.screen_shows("couldn't export")
                    self.assertFalse((project/'exports').exists());self.assertFalse(completed())
                    terminal.send(b'\x1b');terminal.until(lambda:'EXPORT FOR READERS' not in terminal.screen.text,'failed export dismissed')
                    terminal.send(b'\x0bExport for readers\r');terminal.screen_shows('EXPORT FOR READERS')
                    # Existing export rows: Word, PDF, Paperback, EPUB, Markdown.
                    terminal.send(b'\x1b[B'*3+b' '+b'x');terminal.screen_shows('.docx')
                    self.assertFalse(completed());self.assertFalse(list((project/'exports').glob('*.pdf')))
                    terminal.send(b'\x1b');terminal.until(lambda:'EXPORT FOR READERS' not in terminal.screen.text,'Word result dismissed')
                    terminal.send(b'\x0bExport for readers\r');terminal.screen_shows('EXPORT FOR READERS')
                    terminal.send(b'\x1b[B'*3+b' ' + b'\x1b[A'*2+b' ')
                    terminal.send(b'p');terminal.screen_shows('EXPORT PREFLIGHT')
                    self.assertNotIn('Rich export is blocked',terminal.screen.text)
                    terminal.send(b'x');self.assertFalse(list((project/'exports').glob('*.pdf')));self.assertFalse(completed())
                    terminal.send(b'\x1b');terminal.until(lambda:'EXPORT PREFLIGHT' not in terminal.screen.text and 'EXPORT FOR READERS' in terminal.screen.text,'returned to selected export')
                    terminal.send(b'x');terminal.until(lambda:'PDF ready:' in terminal.screen.text,'actual asynchronous PDF completion',timeout=45)
                    terminal.until(completed,'genuine footnote tutorial progress saved')
                    docxs=list((project/'exports').glob('*.docx'));pdfs=list((project/'exports').glob('*.pdf'))
                    self.assertEqual(len(docxs),1);self.assertEqual(len(pdfs),1)
                    self.assertTrue(pdfs[0].read_bytes().startswith(b'%PDF-'))
                    with zipfile.ZipFile(docxs[0]) as archive:
                        self.assertIn('word/footnotes.xml',archive.namelist())
                        self.assertNotIn(b'PRIVATE_MANUSCRIPT',archive.read('word/footnotes.xml'))
                    self.assertEqual(primary.read_bytes(),changed.encode())
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                        captures=ROOT/'build/verification/manuscript-footnote-screens';captures.mkdir(parents=True,exist_ok=True)
                        (captures/f'manuscript-footnotes-{width}x{height}.txt').write_text(terminal.screen.text+'\n',encoding='utf-8')
                    terminal.send(b'\x1b');terminal.until(lambda:'EXPORT FOR READERS' not in terminal.screen.text,'returned to original editor')
                    terminal.send(b'\x1a\x13');terminal.until(lambda:primary.read_bytes()==before.encode(),'original Undo survives real publishing')
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'normal quit');self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bThe Brass Key\r');terminal.screen_shows('λ Intro')
                    self.assertEqual(primary.read_bytes(),before.encode())
                    self.assertTrue(completed())
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'restart normal quit');self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
