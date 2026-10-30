"""Real manuscript DOCX/PDF page-footnotes, isolated HOME and original bytes."""
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

from test_secondary_pty import BINARY, ROOT


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'publishing must run as an ordinary writer')
class ManuscriptPageFootnotes(unittest.TestCase):
    def test_word_relationships_and_real_pdf_notes_follow_their_reference_pages(self):
        for format_,paper,spacing in [('modern','letter','double'),('classic','a4','1.5')]:
            with self.subTest(format=format_), tempfile.TemporaryDirectory(prefix='wros-page-notes-') as temporary:
                for tool in ['libreoffice','pdftotext','pdftoppm']:
                    self.assertIsNotNone(shutil.which(tool),f'Debian publishing qualification requires {tool}')
                base=Path(temporary);project=base/'book';home=base/'home'
                home.mkdir();project.mkdir()
                env=os.environ | {'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),'XDG_CACHE_HOME':str(home/'cache'),'XDG_DATA_HOME':str(home/'data')}
                meta=(f'title = "Page Notes Fixture"\nauthor = "WrOS Test Writer"\n[manuscript]\nformat = "{format_}"\npaper = "{paper}"\nspacing = "{spacing}"\n')
                (project/'novel.toml').write_text(meta,encoding='utf-8')
                source=project/'manuscript/01-Chapter-One/01-Scene.md';source.parent.mkdir(parents=True)
                paragraphs=[]
                prose='The archivist followed the corridor and checked the original records before returning to the quiet room. '
                for index in range(25):
                    anchor={0:'BODYANCHORALPHA[^first]. ',12:'BODYANCHORBETA[^second]. ',24:'BODYANCHORREPEAT[^first]. '}.get(index,'')
                    paragraphs.append(anchor+prose*3)
                text=('---\ntitle: "A λ manuscript"\ncustom: "original preserved"\n---\n\n'
                      +'\n\n'.join(paragraphs)
                      +'\n\n[^first]: NOTEANCHORALPHA café λ *public context*. %% PRIVATE_PAGE_SECRET %%\n\n    SECONDNOTEPARAGRAPH with **emphasis**.\n\n[^second]: NOTEANCHORBETA another public source.\n')
                source.write_text(text,encoding='utf-8');before=source.read_bytes()
                result=subprocess.run([str(BINARY),'export','--docx','--pdf',str(project)],env=env,capture_output=True,text=True,encoding='utf-8',timeout=90)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                docx=next((project/'exports').glob('*.docx'));pdf=docx.with_suffix('.pdf')
                self.assertTrue(pdf.is_file(),result.stdout+result.stderr)
                with zipfile.ZipFile(docx) as archive:
                    notes=archive.read('word/footnotes.xml');document=archive.read('word/document.xml')
                    ns={'w':'http://schemas.openxmlformats.org/wordprocessingml/2006/main'}
                    refs=ET.fromstring(document).findall('.//w:footnoteReference',ns)
                    self.assertEqual([r.get('{'+ns['w']+'}id') for r in refs],['1','2','3'])
                    self.assertNotIn(b'PRIVATE_PAGE_SECRET',notes+document)
                    self.assertIn(b'footnotes.xml',archive.read('word/_rels/document.xml.rels'))
                    self.assertIn(b'wordprocessingml.footnotes+xml',archive.read('[Content_Types].xml'))
                    styles=archive.read('word/styles.xml')
                    self.assertIn(b'FootnoteText',styles)
                    self.assertIn(b'pageBottom',archive.read('word/settings.xml'))
                bbox=base/'pages.html'
                subprocess.run(['pdftotext','-bbox',str(pdf),str(bbox)],env=env,capture_output=True,check=True,timeout=30)
                pages=ET.parse(bbox).getroot().findall('.//{*}page')
                self.assertGreaterEqual(len(pages),4,'fixture must cross real page boundaries')
                words=[(page_index,float(page.get('height')),word) for page_index,page in enumerate(pages)
                       for word in page.findall('.//{*}word')]
                text_pdf=' '.join(word.text or '' for _,_,word in words)
                self.assertNotIn('PRIVATE_PAGE_SECRET',text_pdf)
                self.assertNotIn('[^first]',text_pdf)
                self.assertIn('café',text_pdf)
                self.assertIn('λ',text_pdf)
                positions=[]
                for anchor,note in [('BODYANCHORALPHA','NOTEANCHORALPHA'),('BODYANCHORBETA','NOTEANCHORBETA'),('BODYANCHORREPEAT','NOTEANCHORALPHA')]:
                    references=[(p,h,w) for p,h,w in words if (w.text or '').startswith(anchor)]
                    self.assertEqual(len(references),1,(anchor,text_pdf))
                    page,height,reference=references[0]
                    footnotes=[w for p,_,w in words if p==page and (w.text or '').startswith(note)]
                    self.assertEqual(len(footnotes),1,f'{note} must be on the page of {anchor}')
                    foot=footnotes[0]
                    self.assertGreater(float(foot.get('yMin')),height*.70,'note must be at the foot of its reference page')
                    self.assertLess(float(reference.get('yMax')),float(foot.get('yMin')))
                    positions.append({'body':anchor,'note':note,'page':page+1,'page_height':height,
                                      'body_y_max':float(reference.get('yMax')),'note_y_min':float(foot.get('yMin'))})
                self.assertNotEqual(positions[0]['page'],positions[-1]['page'],'repeated note must follow a later-page reference')
                self.assertEqual(source.read_bytes(),before)
                self.assertEqual((project/'novel.toml').read_text(encoding='utf-8'),meta)
                if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                    captures=ROOT/'build/verification/manuscript-footnotes';captures.mkdir(parents=True,exist_ok=True)
                    for path in [docx,pdf,bbox]:shutil.copyfile(path,captures/f'{format_}-{path.name}')
                    prefix=captures/f'{format_}-reference-page'
                    subprocess.run(['pdftoppm','-png','-f',str(positions[0]['page']),'-singlefile','-scale-to','1400',str(pdf),str(prefix)],env=env,capture_output=True,check=True,timeout=30)
                    receipt={'passed':True,'format':format_,'paper':paper,'spacing':spacing,'pages':len(pages),'positions':positions,
                             'docx_sha256':hashlib.sha256(docx.read_bytes()).hexdigest(),'pdf_sha256':hashlib.sha256(pdf.read_bytes()).hexdigest(),
                             'source_sha256':hashlib.sha256(before).hexdigest(),'original_source_preserved':True,
                             'libreoffice':subprocess.run(['libreoffice','--version'],capture_output=True,text=True,check=True,env=env).stdout.strip()}
                    (captures/f'{format_}-render.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')

    def test_long_note_continues_across_real_pages_without_becoming_an_endnote(self):
        with tempfile.TemporaryDirectory(prefix='wros-long-page-note-') as temporary:
            base=Path(temporary);project=base/'book';home=base/'home';home.mkdir();project.mkdir()
            (project/'novel.toml').write_text('title = "Long Note Fixture"\nauthor = "WrOS Test Writer"\n',encoding='utf-8')
            source=project/'manuscript/01-Chapter-One/01-Scene.md';source.parent.mkdir(parents=True)
            wording='The retained source explains the archival context and preserves its original account for later review. '
            paragraphs=['LONGNOTE_START '+wording*3]+[wording*3 for _ in range(22)]+['LONGNOTE_FINISH '+wording]
            text='Original body anchor LONGNOTE_BODY[^long].\n\n[^long]: '+'\n\n    '.join(paragraphs)+'\n'
            source.write_text(text,encoding='utf-8')
            env=os.environ | {'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),'XDG_CACHE_HOME':str(home/'cache'),'XDG_DATA_HOME':str(home/'data')}
            result=subprocess.run([str(BINARY),'export','--docx','--pdf',str(project)],env=env,capture_output=True,text=True,timeout=90)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            pdf=next((project/'exports').glob('*.pdf'));bbox=base/'long.html'
            subprocess.run(['pdftotext','-bbox',str(pdf),str(bbox)],env=env,check=True,capture_output=True,timeout=30)
            pages=ET.parse(bbox).getroot().findall('.//{*}page')
            words=[(i,w) for i,p in enumerate(pages) for w in p.findall('.//{*}word')]
            def point(prefix):
                found=[(i,w) for i,w in words if (w.text or '').startswith(prefix)]
                self.assertEqual(len(found),1,prefix)
                return found[0]
            body_page,body=point('LONGNOTE_BODY');start_page,start=point('LONGNOTE_START');end_page,_=point('LONGNOTE_FINISH')
            self.assertEqual(body_page,start_page,'the note must begin on the reference page')
            self.assertGreater(end_page,start_page,'the note must continue onto another page')
            self.assertGreater(float(start.get('yMin')),float(body.get('yMax')))
            self.assertEqual(source.read_text(encoding='utf-8'),text)
            if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                captures=ROOT/'build/verification/manuscript-footnotes';captures.mkdir(parents=True,exist_ok=True)
                shutil.copyfile(pdf,captures/'long-note.pdf')
                for label,page in [('start',start_page),('finish',end_page)]:
                    subprocess.run(['pdftoppm','-png','-f',str(page+1),'-singlefile','-scale-to','1400',str(pdf),str(captures/f'long-note-{label}')],env=env,capture_output=True,check=True,timeout=30)
                (captures/'long-render.json').write_text(json.dumps({'passed':True,'pages':len(pages),'reference_page':body_page+1,'note_start_page':start_page+1,'note_end_page':end_page+1,'source_preserved':True,'pdf_sha256':hashlib.sha256(pdf.read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8')


if __name__=='__main__':unittest.main()
