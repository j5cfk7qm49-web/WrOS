"""Real WordNet data and binary, not generated definition/antonym fixtures.

Only disposable demo/HOME; compiled tests require documented wordnet-base.
No root app, telemetry, runtime download or user manuscript.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from test_checks_pty import quit_normally
from test_journal_pty import fixture
from test_productivity_pty import records
from test_secondary_pty import BINARY, ROOT, TerminalProcess

TEXT = 'Café 🦊 e\u0301. hot bank.\n'


def lexical_fixture(base):
    project, home, primary = fixture(base)
    front, delimiter, _ = primary.read_text().partition('\n---\n')
    if not delimiter:
        raise AssertionError('keep actual original frontmatter')
    primary.write_text(front+delimiter+'\n'+TEXT)
    return project, home, primary


def open_lexicon(terminal):
    terminal.send(b'\x0bDictionary and thesaurus\r')
    terminal.screen_shows('Search word or phrase:')


def ready(terminal, count):
    terminal.screen_shows(f'{count} indexed senses')


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'never launch the application as root')
class LexiconPTY(unittest.TestCase):
    def test_real_senses_specific_antonyms_copy_native_unicode_paste_undo_and_restart_at_four_sizes(self):
        # Missing dependency is a failed prerequisite, not skipped fake coverage.
        self.assertTrue(Path('/usr/share/wordnet/data.adj').is_file(), 'install wordnet-base')
        for width,height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width,height=height), tempfile.TemporaryDirectory(prefix='wd-lexicon-pty-') as tmp:
                project,home,primary=lexical_fixture(Path(tmp)); before=primary.read_bytes()
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r')
                    # Actual Unicode/scalar cursor + Shift-right selection of hot.
                    terminal.send(b'\x1b[C'*11+b'\x1b[1;2C'*3)
                    open_lexicon(terminal); terminal.screen_shows('Search word or phrase: hot')
                    terminal.send(b'\r'); ready(terminal,21)
                    terminal.paste('ecec Pasted commands must not run.')
                    terminal.screen_shows('pasted commands ignored')
                    terminal.send(b'\r'); terminal.screen_shows('SYNSET WORDS / SYNONYMS')
                    terminal.send(b'\t'); terminal.screen_shows('EXPLICIT ANTONYM POINTERS')
                    terminal.screen_shows('cold'); terminal.send(b'c'); terminal.screen_shows('Copied')
                    terminal.send(b'\x18\x1a')  # cut/undo cannot reach the hidden editor
                    self.assertEqual(primary.read_bytes(),before)
                    self.assertEqual(records(project),[], 'dictionary/copy is not authorship')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                        (ROOT/'docs/evidence'/f'lexicon-pty-{width}x{height}.txt').write_text(terminal.screen.text+'\n')
                    terminal.send(b'\x1b'); ready(terminal,21)
                    terminal.send(b'\x1b'); terminal.screen_shows('Search word or phrase: hot')
                    terminal.send(b'\x1b')
                    terminal.until(lambda:'Dictionary and thesaurus' not in terminal.screen.text, 'returned to the one original editor')
                    terminal.send(b'\x16\x13')
                    changed=before.replace(b'hot bank',b'cold bank',1)
                    terminal.until(lambda:primary.read_bytes()==changed and any(e['origin']=='paste' for e in records(project)), 'explicit native paste replaces the retained original selection and checkpoints Paste origin')
                    self.assertFalse(any(e['origin']=='typing' for e in records(project)))
                    terminal.send(b'\x1a\x13')
                    terminal.until(lambda:primary.read_bytes()==before and any(e['origin']=='undo' for e in records(project)), 'exact native Unicode undo and checkpoint')
                    quit_normally(terminal)
                finally:
                    terminal.close()
                restarted=TerminalProcess(project,home,width,height)
                try:
                    restarted.shows('The Brass Key'); self.assertEqual(primary.read_bytes(),before)
                    self.assertFalse((project/'.grimoire/writingdeck/checks.json').exists())
                    self.assertFalse((home/'.grimoire/writingdeck/checks-global.json').exists())
                    quit_normally(restarted)
                finally:
                    restarted.close()

    def test_cli_real_polysemy_morphology_sense_specific_antonyms_and_no_input_file_write(self):
        with tempfile.TemporaryDirectory(prefix='wd-lexicon-cli-') as tmp:
            home=Path(tmp); env=os.environ|{'HOME':str(home)}
            def lookup(query):
                result=subprocess.run([str(BINARY),'writingdeck','lexicon',query],env=env,capture_output=True,text=True,check=True,timeout=15)
                return json.loads(result.stdout)
            bank=lookup('bank'); self.assertEqual(len(bank['senses']),18)
            self.assertTrue(any('financial institution' in s['gloss'] for s in bank['senses']))
            self.assertTrue(any('slope beside a body of water' in s['gloss'] for s in bank['senses']))
            hot=lookup('hot'); self.assertEqual(hot['senses'][0]['antonyms'][0]['word'],'cold')
            self.assertEqual(hot['senses'][1]['antonyms'],[], 'not all senses inherit a physical-temperature opposite')
            went=lookup('went'); self.assertTrue(any(s['lemma']=='go' and s['morphological'] for s in went['senses']))
            self.assertEqual(list(home.iterdir()),[], 'lookup does not create a project, profile or prose copy')

    def test_lesson25_requires_real_review_and_missing_data_never_falls_back_to_invented_results(self):
        with tempfile.TemporaryDirectory(prefix='wd-lexicon-lesson-') as tmp:
            project,home,primary=lexical_fixture(Path(tmp)); before=primary.read_bytes()
            progress=project/'.grimoire/writingdeck/tutorial.json'; progress.parent.mkdir(parents=True,exist_ok=True)
            progress.write_text(json.dumps({'version':1,'active':None,'completed':[26],'skipped':[]}))
            terminal=TerminalProcess(project,home,100,30)
            try:
                terminal.shows('The Brass Key'); terminal.send(b'\x0bInteractive tutorial\r')
                terminal.screen_shows('Interactive tutorial'); terminal.send(b'\x1b[H'+b'\x1b[B'*24+b'\r')
                terminal.screen_shows('Lesson 25'); terminal.send(b'\r')
                terminal.screen_shows('Search word or phrase:')
                terminal.send(b'\x1b[3~bank\r'); ready(terminal,18)  # Delete clears query, not original selection
                self.assertEqual(json.loads(progress.read_text())['completed'],[26])
                terminal.send(b'\r'); terminal.screen_shows('SYNSET WORDS / SYNONYMS')
                terminal.until(lambda:json.loads(progress.read_text())['completed']==[25,26], 'genuine definition review persisted; historical spelling preserved')
                self.assertEqual(primary.read_bytes(),before); self.assertEqual(records(project),[])
                quit_normally(terminal)
            finally:
                terminal.close()
            missing=Path(tmp)/'missing-dataset'
            with patch.dict(os.environ, {'WRITINGDECK_WORDNET_DIR':str(missing)}):
                unavailable=TerminalProcess(project,home,80,24)
            try:
                unavailable.shows('The Brass Key'); open_lexicon(unavailable)
                unavailable.send(b'\x1b[3~hot\r'); unavailable.screen_shows('unavailable/invalid')
                self.assertNotIn('21 indexed senses',unavailable.screen.text)
                self.assertFalse(missing.exists()); self.assertEqual(primary.read_bytes(),before)
                quit_normally(unavailable)
            finally:
                unavailable.close()
