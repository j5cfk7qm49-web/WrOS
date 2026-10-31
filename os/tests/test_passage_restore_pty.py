"""Real reviewed-draft and selective restore through the retained editor."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run unprivileged')
class PassageRestorePTY(unittest.TestCase):
    def workflow(self, variant):
        for width, height in [(80,24), (100,30), (120,40), (160,50)]:
            with self.subTest(variant=variant, width=width), tempfile.TemporaryDirectory(prefix='wros-passage-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = ('---\ntitle: "The Brass Key"\ncustom: "λ original"\n---\n\n'
                          'λ First sentence.\nMiddle retained words.\nLast sentence. %% private original %%\n')
                primary.write_text(before, encoding='utf-8')
                second = before.replace('λ First', 'Second λ First')
                current = second.replace('Second λ First', 'Current Second λ First').replace('Last sentence', 'Added Last sentence')
                partial = current.replace('Added Last sentence', 'Last sentence')
                terminal = TerminalProcess(project, home, width, height)
                def open_list():
                    terminal.send(b'\x0bDraft milestones\r'); terminal.screen_shows('Named Milestones')
                    if variant:
                        terminal.send(b'2'); terminal.screen_shows('Active Scene Variants')
                def save_named(name, number):
                    open_list(); terminal.send(b'n')
                    terminal.screen_shows('Save Alternative Version' if variant else 'Create Named Milestone')
                    terminal.paste(name); terminal.send(b'\r')
                    terminal.screen_shows(f'Saved scene variant #{number}' if variant else f'Saved named milestone #{number}')
                    terminal.send(b'\x1b'); terminal.until(lambda: 'Scene Variants' not in terminal.screen.text and 'Named Milestones' not in terminal.screen.text, 'returned to writer')
                def second_comparison():
                    open_list(); terminal.send(b'\x1b[A'*8+b'\x1b[B\r')
                    terminal.screen_shows('Alternative Comparison' if variant else 'Scene Comparison')
                    terminal.screen_shows('Second saved')
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ First')
                    save_named('First saved',1)
                    terminal.send(b'\x1b[H'); terminal.paste('Second '); terminal.send(b'\x13')
                    terminal.until(lambda: primary.read_bytes() == second.encode(), 'second draft saved')
                    save_named('Second saved',2)
                    terminal.send(b'\x1b[H'); terminal.paste('Current ')
                    terminal.send(b'\x1b[B'*2+b'\x1b[H'); terminal.paste('Added '); terminal.send(b'\x13')
                    terminal.until(lambda: primary.read_bytes() == current.encode(), 'two independent changes saved')
                    second_comparison(); terminal.send(b'r')
                    terminal.until(lambda: 'Comparison' not in terminal.screen.text, 'whole reviewed draft restored')
                    terminal.send(b'\x13'); terminal.until(lambda: primary.read_bytes() == second.encode(), 'second saved draft, not first, restored')
                    terminal.send(b'\x1a\x13'); terminal.until(lambda: primary.read_bytes() == current.encode(), 'whole restore Undo')
                    second_comparison(); terminal.send(b'p'); terminal.screen_shows('Restore one reviewed passage')
                    terminal.screen_shows('CURRENT WORDING'); terminal.screen_shows('SAVED WORDING')
                    terminal.paste('blocked hidden edit'); terminal.send(b'\x18\x1a')
                    self.assertEqual(primary.read_bytes(),current.encode())
                    terminal.send(b'\x1bOP'); terminal.screen_shows('Continuity & draft milestones')
                    terminal.send(b'\x1b'); terminal.until(lambda: 'Continuity & draft milestones' not in terminal.screen.text and 'Restore one reviewed passage' in terminal.screen.text, 'help returned to same review')
                    terminal.send(b'\r'); terminal.until(lambda: 'Restore one reviewed passage' not in terminal.screen.text, 'default Cancel returned')
                    self.assertEqual(primary.read_bytes(),current.encode())
                    terminal.send(b'\x1b[Cp'); terminal.screen_shows('Restore one reviewed passage')
                    terminal.screen_shows('Added')
                    terminal.send(b'\x1b[B')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/passage-restore-screens'; captures.mkdir(parents=True,exist_ok=True)
                        (captures/f'{"variant" if variant else "milestone"}-{width}x{height}.txt').write_text(terminal.screen.text+'\n',encoding='utf-8')
                    terminal.send(b'\r'); terminal.until(lambda: 'Restore one reviewed passage' not in terminal.screen.text, 'explicit passage restore')
                    terminal.send(b'\x13'); terminal.until(lambda: primary.read_bytes() == partial.encode(), 'only second changed passage restored')
                    variants = json.loads((project/'.grimoire/writingdeck/variants.json').read_text(encoding='utf-8'))
                    self.assertEqual(variants['variants'][-1]['content'], current.split('---\n',2)[-1].lstrip('\n'))
                    history = list((project/'.grimoire/history').rglob('*.md'))
                    self.assertTrue(any(path.read_bytes() == current.encode() for path in history))
                    terminal.send(b'\x1a\x13'); terminal.until(lambda: primary.read_bytes() == current.encode(), 'selective restore Undo retains original YAML')
                    terminal.send(b'\x11'); terminal.until(lambda: terminal.process.poll() is not None, 'normal quit')
                    self.assertEqual(terminal.process.returncode,0)
                finally:
                    terminal.close()
                terminal = TerminalProcess(project,home,width,height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('Current Second')
                    self.assertEqual(primary.read_bytes(),current.encode())
                    terminal.send(b'\x11'); terminal.until(lambda: terminal.process.poll() is not None, 'restart normal quit')
                    self.assertEqual(terminal.process.returncode,0)
                finally:
                    terminal.close()

    def test_milestone_selected_draft_passage_cancel_help_undo_and_restart_at_four_sizes(self):
        self.workflow(False)

    def test_variant_selected_draft_passage_cancel_help_undo_and_restart_at_four_sizes(self):
        self.workflow(True)
