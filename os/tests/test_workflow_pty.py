"""Actual non-root scratchpad/promotion/handoff/preset input and loaded restart.

Only new demo/HOME fixtures; no host devices/services/credentials. The small
ANSI/CPR shim is not full terminal or physical-keyboard emulation.
"""
import json
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the no-audio binary first')
@unittest.skipIf(os.geteuid() == 0, 'interactive writing application must be non-root')
class WritingWorkflowPTY(unittest.TestCase):
    def test_original_scratch_promotion_handoff_and_real_loaded_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-workflow-pty-') as tmp:
                project, home, primary = fixture(Path(tmp))
                originals = {p: p.read_bytes() for p in (project/'manuscript').rglob('*.md')}
                meta = project/'.grimoire/writingdeck/workflow.json'
                source = project/'notes/scratchpad/000001.md'
                target = project/'manuscript/90-Scratchpad-000001.md'
                handoff = project/'notes/handoff.md'

                def state():
                    return json.loads(meta.read_text()) if meta.exists() else {}

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bThe Brass Key\r\x0bScratchpad\rn')
                    terminal.paste('PTY original scene-linked idea. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: source.is_file() and b'PTY original scene-linked idea.' in source.read_bytes(), 'actual original scratchpad save')
                    saved_source = source.read_bytes()
                    terminal.send(b'\x0bScratchpad\r')
                    terminal.paste('nNplgra These pasted reader commands must never execute.')
                    terminal.send(b'lThe Brass Key\rp')
                    terminal.until(lambda: target.is_file() and state().get('notes', [{}])[0].get('promotion', {}).get('phase') == 'ready', 'recorded ready independent scene snapshot')
                    saved_target = target.read_bytes()
                    self.assertIn(b'PTY original scene-linked idea.', saved_target)
                    self.assertEqual(source.read_bytes(), saved_source)
                    self.assertEqual(len(state()['notes']), 1)
                    self.assertNotIn('PTY original scene-linked idea', meta.read_text())
                    self.assertEqual(state()['notes'][0]['scene'], str(primary.relative_to(project)))
                    terminal.send(b'p\x0bSession handoff\r\r')
                    terminal.paste('Next open, follow the original river. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: handoff.is_file() and b'Next open, follow the original river.' in handoff.read_bytes(), 'original next-session intention saved')
                    saved_handoff = handoff.read_bytes()
                    terminal.send(b'\x0bSession handoff\r')
                    terminal.paste('oesg Reader paste is not a command or prose writer.')
                    terminal.send(b's')
                    terminal.until(lambda: state().get('handoff', {}).get('show_on_open') is True and state()['handoff']['intention'] is not None, 'explicit saved intention armed')
                    self.assertNotIn('Next open, follow the original river', meta.read_text())
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal workflow quit')
                    self.assertEqual(terminal.process.returncode, 0)
                    self.assertEqual(source.read_bytes(), saved_source)
                    self.assertEqual(target.read_bytes(), saved_target)
                    self.assertEqual(handoff.read_bytes(), saved_handoff)
                    for p, original in originals.items():
                        self.assertEqual(p.read_bytes(), original)
                    self.assertFalse(list((project/'.grimoire/writingdeck/journal').glob('*.json')))
                finally:
                    terminal.close()

                restarted = TerminalProcess(project, home, width, height)
                try:
                    restarted.screen_shows('Session handoff')
                    restarted.screen_shows('Next open, follow the original river.')
                    restarted.send(b'g')
                    restarted.paste('PTY resumed manuscript context. ')
                    restarted.send(b'\x13')
                    restarted.until(lambda: b'PTY resumed manuscript context.' in primary.read_bytes(), 'actual explicit handoff context resume and manuscript save')
                    self.assertEqual(source.read_bytes(), saved_source)
                    self.assertEqual(target.read_bytes(), saved_target)
                    self.assertEqual(handoff.read_bytes(), saved_handoff)
                    restarted.send(b'\x0bSession handoff\r\r')
                    restarted.paste('Loaded original intention edit. ')
                    restarted.send(b'\x13')
                    restarted.until(lambda: b'Loaded original intention edit.' in handoff.read_bytes(), 'restart opens an actual loaded original handoff editor')
                    restarted.send(b'\x1a\x13')
                    restarted.until(lambda: handoff.read_bytes() == saved_handoff, 'isolated loaded-source undo saves exact original intention')
                    restarted.send(b'\x11')
                    restarted.until(lambda: restarted.process.poll() is not None, 'normal restarted workflow quit')
                    self.assertEqual(restarted.process.returncode, 0)
                    self.assertEqual(source.read_bytes(), saved_source)
                    self.assertEqual(target.read_bytes(), saved_target)
                finally:
                    restarted.close()

    def test_future_workflow_preserves_metadata_and_allows_only_explicit_original_handoff_editing(self):
        with tempfile.TemporaryDirectory(prefix='wd-workflow-future-pty-') as tmp:
            project, home, primary = fixture(Path(tmp))
            original_manuscript = primary.read_bytes()
            meta = project/'.grimoire/writingdeck/workflow.json'
            meta.parent.mkdir(parents=True, exist_ok=True)
            original = b'{"version":999,"future":{"preserve":"all bytes"}}\n'
            meta.write_bytes(original)
            handoff = project/'notes/handoff.md'
            handoff.parent.mkdir(parents=True, exist_ok=True)
            handoff.write_bytes(b'---\ntitle: Original handoff\ncustom: keep\n---\n\nOriginal manual intention.\n')
            terminal = TerminalProcess(project, home, 80, 24)
            try:
                terminal.shows('The Brass Key')
                terminal.send(b'\x0bScratchpad\rn\x0bSession handoff\r\r')
                terminal.paste('Explicit original despite incompatible index. ')
                terminal.send(b'\x13')
                terminal.until(lambda: b'Explicit original despite incompatible index.' in handoff.read_bytes(), 'original independent note save despite unknown metadata')
                terminal.send(b'\x0bSession handoff\rs\x11')
                terminal.until(lambda: terminal.process.poll() is not None, 'normal future-workflow quit without invented manuscript writing')
                self.assertEqual(terminal.process.returncode, 0)
                self.assertEqual(meta.read_bytes(), original)
                self.assertFalse((project/'notes/scratchpad').exists())
                self.assertIn(b'custom: keep', handoff.read_bytes())
                self.assertEqual(primary.read_bytes(), original_manuscript)
            finally:
                terminal.close()

    def test_real_starter_secondary_pin_set_and_external_layout_preservation_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width, height=height), tempfile.TemporaryDirectory(prefix='wd-preset-pty-') as tmp:
                project, home, primary = fixture(Path(tmp))
                manuscript = primary.read_bytes()
                note = project/'research/workspace.md'
                layout = project/'.grimoire/writingdeck/layout.json'
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key')
                    terminal.send(b'\x0bWorkspaces\rbApply Research starter\r\t')
                    terminal.paste('Actual independent research preset note. ')
                    terminal.send(b'\x13')
                    terminal.until(lambda: note.is_file() and b'Actual independent research preset note.' in note.read_bytes(), 'starter opens a real editable original secondary source')
                    self.assertEqual(primary.read_bytes(), manuscript)
                    saved_note = note.read_bytes()
                    terminal.send(b'\x0bWorkspaces\rp')
                    terminal.paste('Az These pasted pin-set commands must be ignored.')
                    terminal.send(b'z\r')
                    terminal.until(lambda: layout.is_file() and json.loads(layout.read_text())['workspaces'][0]['pin_ids'] == [], 'explicit empty per-workspace pin set')
                    future = b'{"version":999,"keep":"external original layout"}\n'
                    layout.write_bytes(future)
                    terminal.send(b'bApply Planning starter\r')
                    terminal.screen_shows('Starter not applied:')
                    terminal.send(b'\x11')
                    terminal.until(lambda: terminal.process.poll() is not None, 'normal preset quit with external layout preserved')
                    self.assertEqual(terminal.process.returncode, 0)
                    self.assertEqual(layout.read_bytes(), future)
                    self.assertFalse((project/'notes/workspaces/planning.md').exists())
                    self.assertEqual(note.read_bytes(), saved_note)
                    self.assertEqual(primary.read_bytes(), manuscript)
                finally:
                    terminal.close()


if __name__ == '__main__':
    unittest.main()
