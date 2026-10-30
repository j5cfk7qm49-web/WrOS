"""Integrated continuity and milestone workflow with disposable source/HOME."""
import os
from pathlib import Path
import tempfile
import unittest
import subprocess
import json
from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess

@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run unprivileged')
class ArgonWritingPTY(unittest.TestCase):
    def test_cli_aliases_and_local_byte_comparison_are_read_only(self):
        for command in ['help','version']:
            outputs=[subprocess.run([str(BINARY),alias,command],capture_output=True,check=True).stdout for alias in ['wros','writingdeck']]
            self.assertEqual(outputs[0],outputs[1])
        with tempfile.TemporaryDirectory(prefix='wros-local-audit-') as temporary:
            a=Path(temporary)/'original.md';b=Path(temporary)/'returned.md'
            original='---\ncustom: λ\n---\n\nOriginal *prose*. %% private %%\n'
            a.write_text(original,encoding='utf-8');b.write_text(original,encoding='utf-8')
            same=subprocess.run([str(BINARY),'wros','docs-audit',str(a),str(b)],capture_output=True,check=True)
            self.assertTrue(json.loads(same.stdout)['lossless'])
            b.write_text(original.replace('prose','changed'),encoding='utf-8')
            changed=subprocess.run([str(BINARY),'wros','docs-audit',str(a),str(b)],capture_output=True)
            self.assertNotEqual(changed.returncode,0)
            self.assertFalse(json.loads(changed.stdout)['lossless'])
            self.assertEqual(a.read_text(encoding='utf-8'),original)
    def test_milestone_restore_undo_continuity_help_and_restart_at_four_sizes(self):
        for width,height in [(80,24),(100,30),(120,40),(160,50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-argon-') as temporary:
                project,home,primary=fixture(Path(temporary))
                before='---\ntitle: "The Brass Key"\ncustom: "λ original"\n---\n\nλ Original sentence. %% private note %%\n'
                primary.write_text(before,encoding='utf-8')
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r');terminal.screen_shows('λ Original')
                    terminal.send(b'\x0bDraft milestones\r');terminal.screen_shows('Named Milestones')
                    terminal.send(b'n');terminal.screen_shows('Create Named Milestone')
                    terminal.paste('Draft λ');terminal.send(b'\r');terminal.screen_shows('Saved named milestone #1')
                    terminal.send(b'\x1b');terminal.until(lambda:'Named Milestones' not in terminal.screen.text,'returned to writer')
                    terminal.send(b'\x1b[H');terminal.paste('Added ');terminal.send(b'\x13')
                    changed=before.replace('λ Original','Added λ Original')
                    terminal.until(lambda:primary.read_bytes()==changed.encode(),'source edited normally')
                    terminal.send(b'\x0bDraft milestones\r');terminal.screen_shows('Named Milestones')
                    terminal.send(b'\r');terminal.screen_shows('Scene Comparison')
                    terminal.send(b'\r');terminal.paste('blocked');terminal.send(b'\x18\x1a')
                    self.assertEqual(primary.read_bytes(),changed.encode())
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY')=='1':
                        captures=ROOT/'build/verification/argon-writing-screens';captures.mkdir(parents=True,exist_ok=True)
                        (captures/f'argon-writing-{width}x{height}.txt').write_text(terminal.screen.text+'\n',encoding='utf-8')
                    terminal.send(b'r');terminal.until(lambda:'Scene Comparison' not in terminal.screen.text,'restored to writer')
                    terminal.send(b'\x13');terminal.until(lambda:primary.read_bytes()==before.encode(),'restored original bytes')
                    terminal.send(b'\x1a\x13');terminal.until(lambda:primary.read_bytes()==changed.encode(),'Undo restore retains original YAML')
                    terminal.send(b'\x0bContinuity tracker\r');terminal.screen_shows('Continuity Tracker')
                    terminal.send(b'n');terminal.screen_shows('New Fact')
                    for text in ['Wren','eyes','blue','canon']:
                        terminal.paste(text);terminal.send(b'\t')
                    terminal.send(b'\r');terminal.screen_shows('Fact saved')
                    terminal.send(b'4');terminal.screen_shows('Contradictions')
                    terminal.send(b'\x1bOP');terminal.screen_shows('Continuity & draft milestones')
                    terminal.send(b'\x1b');terminal.until(lambda:'Continuity & draft milestones' not in terminal.screen.text,'help returned')
                    terminal.send(b'\x1b');terminal.until(lambda:'Continuity Tracker' not in terminal.screen.text,'returned to writer')
                    self.assertEqual(primary.read_bytes(),changed.encode())
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'normal quit')
                    self.assertEqual(terminal.process.returncode,0)
                finally: terminal.close()
                terminal=TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key');terminal.send(b'\x0bContinuity tracker\r')
                    terminal.screen_shows('Wren')
                    self.assertEqual(primary.read_bytes(),changed.encode())
                    terminal.send(b'\x1b');terminal.until(lambda:'Continuity Tracker' not in terminal.screen.text,'restart returned')
                    terminal.send(b'\x11');terminal.until(lambda:terminal.process.poll() is not None,'restart normal quit')
                    self.assertEqual(terminal.process.returncode,0)
                finally:terminal.close()
