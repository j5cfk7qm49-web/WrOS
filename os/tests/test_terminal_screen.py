"""The small tracker must reconstruct differential text, not invent it."""
import unittest
from terminal_screen import Screen


class TerminalScreen(unittest.TestCase):
    def test_cursor_erase_and_style_sequences_reconstruct_differential_runs(self):
        screen = Screen(40, 4)
        screen.feed(b'old line\x1b[2;3H\x1b[38;2;214;173;96mWorld\x1b[0m')
        self.assertIn('  World', screen.text)
        screen.feed(b'\x1b[1;1HQ again to quit anyway\x1b[1;3Hag\x1b[1;6Hin to\x1b[1;12Hquit anyway')
        self.assertIn('Q again to quit anyway', screen.text)
        screen.feed(b'\x1b[1;1H\x1b[2K')
        self.assertEqual(screen.cells[0], [' '] * 40)
        self.assertIn('World', screen.text)
        screen.feed(b'\x1b[2J')
        self.assertFalse(screen.text.strip())

    def test_utf8_csi_and_osc_can_be_split_at_every_byte_boundary(self):
        payload = ('\x1b[?1049h\x1b]0;ignored title\x07\x1b[2;3H界é\x1b[3;1Hready\x1b[?25l').encode()
        whole, split = Screen(40, 4), Screen(40, 4)
        whole.feed(payload)
        for b in payload:
            split.feed(bytes([b]))
        self.assertEqual(whole.text, split.text)
        self.assertEqual(split.cells[1][2], '界')
        self.assertEqual(split.cells[1][3], '')
        self.assertEqual(split.cells[1][4], 'é')
        self.assertIn('ready', split.text)


if __name__ == '__main__':
    unittest.main()
