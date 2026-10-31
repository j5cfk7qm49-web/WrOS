"""Actual missing-registry refusal and explicit original-registry recovery."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run unprivileged')
class StoryIdentityPTY(unittest.TestCase):
    def test_lost_registry_cannot_reuse_an_id_and_original_registry_recovers_at_four_sizes(self):
        for width, height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-story-identity-') as temporary:
                project,home,primary=fixture(Path(temporary))
                for path in (project/'manuscript').rglob('*.md'):
                    path.write_text('Other independent source.\n',encoding='utf-8')
                a='---\ntitle: "The Brass Key"\ncustom: "λ A retained"\n---\n\nSAVED_A_ONLY.\n'
                b='---\ntitle: "Target B"\ncustom: "λ B retained"\n---\n\nTARGET_B_ONLY. %% private B %%\n'
                primary.write_text(a,encoding='utf-8')
                target=primary.parent/'99-Target-B.md';target.write_text(b,encoding='utf-8')
                registry=project/'.grimoire/writingdeck/review-notes.json'
                milestone=project/'.grimoire/writingdeck/milestones.json'
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bThe Brass Key\r');terminal.screen_shows('SAVED_A_ONLY')
                    terminal.send(b'\x0bDraft milestones\r');terminal.screen_shows('Named Milestones')
                    terminal.send(b'n');terminal.screen_shows('Create Named Milestone');terminal.paste('Original book');terminal.send(b'\r');terminal.screen_shows('Saved named milestone #1')
                    terminal.send(b'\x1b');terminal.until(lambda:'Named Milestones' not in terminal.screen.text,'writer')
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'normal quit');self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
                original_registry=registry.read_bytes();original_milestone=milestone.read_bytes()
                ids=json.loads(original_registry)['documents'];target_id=next(d['id'] for d in ids if project/d['path']==target)
                self.assertNotEqual(target_id,1)
                backup=registry.with_suffix('.original.json');registry.rename(backup)
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bTarget B\r');terminal.screen_shows('TARGET_B_ONLY')
                    terminal.send(b'\x0bDraft milestones\r');terminal.screen_shows('Named Milestones');terminal.send(b'\r')
                    terminal.screen_shows('identity registry')
                    self.assertNotIn('Scene Comparison',terminal.screen.text)
                    terminal.send(b'r');terminal.paste('blocked');terminal.send(b'\x18\x1a')
                    self.assertEqual(target.read_bytes(),b.encode());self.assertEqual(primary.read_bytes(),a.encode())
                    self.assertFalse(registry.exists());self.assertEqual(milestone.read_bytes(),original_milestone)
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                        captures=ROOT/'build/verification/story-identity-screens';captures.mkdir(parents=True,exist_ok=True)
                        (captures/f'story-identity-{width}x{height}.txt').write_text(terminal.screen.text+'\n',encoding='utf-8')
                    terminal.send(b'\x1bOP');terminal.screen_shows('Continuity & draft milestones')
                    terminal.send(b'\x1b');terminal.until(lambda:'Continuity & draft milestones' not in terminal.screen.text,'help returned')
                    terminal.send(b'\x1b');terminal.until(lambda:'Named Milestones' not in terminal.screen.text,'writer after refusal')
                    terminal.send(b'\x1b[H');terminal.paste('Added ');terminal.send(b'\x13')
                    changed=b.replace('TARGET_B_ONLY','Added TARGET_B_ONLY')
                    terminal.until(lambda:target.read_bytes()==changed.encode(),'ordinary writing remains available')
                    terminal.send(b'\x1a\x13');terminal.until(lambda:target.read_bytes()==b.encode(),'normal Undo remains available')
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'refusal normal quit');self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
                self.assertEqual(backup.read_bytes(),original_registry);registry.write_bytes(original_registry)
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bTarget B\r');terminal.screen_shows('TARGET_B_ONLY')
                    terminal.send(b'\x1b[H');terminal.paste('Changed ');terminal.send(b'\x13')
                    changed=b.replace('TARGET_B_ONLY','Changed TARGET_B_ONLY')
                    terminal.until(lambda:target.read_bytes()==changed.encode(),'B edited after original registry restored')
                    terminal.send(b'\x0bDraft milestones\r');terminal.screen_shows('Named Milestones');terminal.send(b'\r');terminal.screen_shows('Scene Comparison')
                    terminal.send(b'r');terminal.until(lambda:'Scene Comparison' not in terminal.screen.text,'reviewed B restored')
                    terminal.send(b'\x13');terminal.until(lambda:target.read_bytes()==b.encode(),'B recovered, not A')
                    terminal.send(b'\x1a\x13');terminal.until(lambda:target.read_bytes()==changed.encode(),'restored B Undo')
                    self.assertEqual(primary.read_bytes(),a.encode());self.assertEqual(milestone.read_bytes(),original_milestone)
                    self.assertEqual(json.loads(registry.read_bytes())['documents'],ids)
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'recovery normal quit');self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
