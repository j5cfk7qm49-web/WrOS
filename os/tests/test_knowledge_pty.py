"""Compiled character-knowledge workflow using fresh manuscript/HOME fixtures."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from test_journal_pty import fixture
from test_secondary_pty import BINARY, ROOT, TerminalProcess


@unittest.skipUnless(BINARY.is_file(), 'build the current binary first')
@unittest.skipIf(os.geteuid() == 0, 'Grimoire must run unprivileged')
class CharacterKnowledgePTY(unittest.TestCase):
    def test_character_knowledge_revision_history_filter_archive_source_and_restart_at_four_sizes(self):
        for width, height in [(80, 24), (100, 30), (120, 40), (160, 50)]:
            with self.subTest(width=width), tempfile.TemporaryDirectory(prefix='wros-knowledge-') as temporary:
                project, home, primary = fixture(Path(temporary))
                before = '---\ntitle: "The Brass Key"\ncustom: "λ retained"\n---\n\nλ Original sentence. %% PRIVATE_KNOWLEDGE_SECRET %%\n'
                primary.write_text(before, encoding='utf-8')
                metadata = project/'.grimoire/writingdeck/knowledge.json'
                registry = project/'.grimoire/writingdeck/review-notes.json'

                def entries():
                    return json.loads(metadata.read_bytes())['entries'] if metadata.is_file() else []

                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ Original')
                    terminal.send(b'\x0bContinuity tracker\r'); terminal.screen_shows('Continuity Tracker')
                    terminal.send(b'n'); terminal.screen_shows('New Fact'); terminal.send(b'\x1b[D'*2)
                    for text in ['Key', 'location', 'Archive', 'canon', 'FACT_META_ONLY']:
                        terminal.paste(text); terminal.send(b'\t')
                    terminal.send(b'\r'); terminal.screen_shows('Fact saved')
                    terminal.send(b'g'); terminal.screen_shows('Record character knowledge')
                    terminal.paste('Wren λ'); terminal.send(b'\t'*3); terminal.paste('KNOWLEDGE_META_ONLY'); terminal.send(b'\r')
                    terminal.screen_shows('Knowledge saved'); terminal.until(lambda:len(entries()) == 1, 'knowledge persisted')
                    self.assertEqual(entries()[0]['current']['state'], 'Knows')
                    self.assertEqual(entries()[0]['current']['moment'], 'After')
                    original_registry = registry.read_bytes()
                    terminal.send(b'e'); terminal.screen_shows('Record character knowledge'); terminal.send(b'\t'+b'\x1b[C'*2+b'\r')
                    terminal.screen_shows('Does not know it'); terminal.until(lambda:entries()[0]['current']['state'] == 'Unaware', 'knowledge revised')
                    self.assertEqual(len(entries()[0]['previous']), 1)
                    terminal.send(b'v'); terminal.screen_shows('Character knowledge · retained versions'); terminal.send(b'\x1b[F'); terminal.screen_shows('EARLIER VERSION 1')
                    terminal.screen_shows('Knows the stated fact')
                    if os.environ.get('WRITINGDECK_CAPTURE_PTY') == '1':
                        captures = ROOT/'build/verification/knowledge-screens'; captures.mkdir(parents=True, exist_ok=True)
                        (captures/f'knowledge-{width}x{height}.txt').write_text(terminal.screen.text+'\n', encoding='utf-8')
                    terminal.paste('blocked hidden source'); terminal.send(b'\x18\x1a')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1bOP'); terminal.screen_shows('Continuity & draft milestones')
                    for _ in range(8):
                        if 'Character knowledge' in terminal.screen.text:
                            break
                        old_screen = terminal.screen.text
                        terminal.send(b'\x1b[6~')
                        terminal.until(lambda:terminal.screen.text != old_screen, 'offline knowledge help scroll')
                    terminal.screen_shows('Character knowledge')
                    terminal.send(b'\x1bOP'); terminal.screen_shows('Character knowledge · retained versions')
                    terminal.send(b'\x1b'); terminal.screen_shows('5 Character Knowledge')
                    terminal.send(b'd'); terminal.until(lambda:entries()[0]['current']['archived'], 'knowledge archived')
                    terminal.send(b'h'); terminal.screen_shows('Archived'); terminal.send(b'd')
                    terminal.until(lambda:not entries()[0]['current']['archived'], 'knowledge unarchived')
                    self.assertEqual(len(entries()[0]['previous']), 3)
                    terminal.send(b'/'); terminal.screen_shows('Filter character knowledge'); terminal.paste('Wren5'); terminal.send(b'\r')
                    terminal.screen_shows('No matching knowledge')
                    terminal.send(b'/'); terminal.screen_shows('Filter character knowledge'); terminal.send(b'\x7f'*5); terminal.paste('wren'); terminal.send(b'\r')
                    terminal.screen_shows('Wren λ')
                    terminal.send(b'1'); terminal.screen_shows('1 Facts'); terminal.send(b'g'); terminal.screen_shows('Record character knowledge')
                    terminal.paste('Wren λ'); terminal.send(b'\t'*3); terminal.paste('SECOND_KNOWLEDGE_META_ONLY'); terminal.send(b'\r')
                    terminal.screen_shows('Conflicting assertion'); terminal.until(lambda:len(entries()) == 2, 'second explicit assertion retained')
                    terminal.send(b'g'); terminal.until(lambda:'Continuity Tracker' not in terminal.screen.text, 'knowledge source opened')
                    self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b[H'); terminal.paste('Added '); terminal.send(b'\x13')
                    changed = before.replace('λ Original', 'Added λ Original')
                    terminal.until(lambda:primary.read_bytes() == changed.encode(), 'ordinary edit saved')
                    terminal.send(b'\x0bContinuity tracker\r'); terminal.screen_shows('Continuity Tracker'); terminal.send(b'5'); terminal.screen_shows('Source revised')
                    terminal.send(b'\x1b'); terminal.until(lambda:'Continuity Tracker' not in terminal.screen.text, 'writer returned')
                    terminal.send(b'\x1a\x13'); terminal.until(lambda:primary.read_bytes() == before.encode(), 'normal source Undo preserved')
                    terminal.send(b'\x0bExport for readers\r'); terminal.screen_shows('EXPORT FOR READERS')
                    # Existing rows: Word, PDF, Paperback, EPUB, Markdown.
                    terminal.send(b'\x1b[H '+b'\x1b[B'*3+b' '+b'\x1b[B '+b'x')
                    terminal.screen_shows('.md'); exported = next((project/'exports').glob('*.md')).read_text(encoding='utf-8')
                    self.assertNotIn('META_ONLY', exported); self.assertNotIn('PRIVATE_KNOWLEDGE_SECRET', exported)
                    self.assertEqual(registry.read_bytes(), original_registry); self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b'); terminal.until(lambda:'EXPORT FOR READERS' not in terminal.screen.text, 'export dismissed')
                    terminal.send(b'\x11'); terminal.until(lambda:terminal.process.poll() is not None, 'normal quit'); self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()
                saved = metadata.read_bytes()
                terminal = TerminalProcess(project, home, width, height)
                try:
                    terminal.shows('The Brass Key'); terminal.send(b'\x0bThe Brass Key\r'); terminal.screen_shows('λ Original')
                    terminal.send(b'\x0bContinuity tracker\r'); terminal.screen_shows('Continuity Tracker'); terminal.send(b'5')
                    terminal.screen_shows('Conflicting assertion'); terminal.send(b'v'); terminal.screen_shows('Character knowledge · retained versions')
                    terminal.send(b'\x1b[F'); terminal.screen_shows('EARLIER VERSION 1')
                    self.assertEqual(metadata.read_bytes(), saved); self.assertEqual(primary.read_bytes(), before.encode())
                    terminal.send(b'\x1b'); terminal.until(lambda:'Character knowledge · retained versions' not in terminal.screen.text, 'restart list')
                    terminal.send(b'\x1b'); terminal.until(lambda:'Continuity Tracker' not in terminal.screen.text, 'restart writer')
                    terminal.send(b'\x11'); terminal.until(lambda:terminal.process.poll() is not None, 'restart quit'); self.assertEqual(terminal.process.returncode, 0)
                finally:
                    terminal.close()


if __name__ == "__main__":
    unittest.main()
