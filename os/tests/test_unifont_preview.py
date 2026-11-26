"""The owner preview uses the writing session's tile and bold pixels."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("unifont_preview", ROOT / "tools/unifont-preview.py")
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)

# rc31's W tile across its two cells: a 14x14 square (columns 1 and 14, rows 1 and 14) around
# Unifont's W, raised a row and centred on columns 4-11.
W_TILE = (
    0x0000, 0x7FFE, 0x4002, 0x4422, 0x4422, 0x4422, 0x4422, 0x45A2,
    0x45A2, 0x4662, 0x4662, 0x4422, 0x4422, 0x4002, 0x7FFE, 0x0000,
)


class UnifontPreview(unittest.TestCase):
    def test_tiles_and_bold_a_match_the_bitmaps(self):
        frame = {
            "theme": "Grimoire",
            "console": False,
            "w": 5,
            "h": 1,
            "rows": [[
                ["", "LightCyan", None, 0],
                ["", "LightCyan", None, 0],
                ["", "LightCyan", None, 0],
                ["", "LightCyan", None, 0],
                ["A", "#b4c5d6", 236, 1],
            ]],
        }
        image = preview.paint(frame, preview.load_glyphs())
        self.assertEqual(image.size, (40, 16))
        cyan, ground = (0, 255, 255), (13, 13, 16)
        for y, row in enumerate(W_TILE):
            for x in range(16):
                line = row & (0x8000 >> x)
                square = 1 <= x <= 14 and 1 <= y <= 14
                self.assertEqual(image.getpixel((x, y)), cyan if line else ground, ("plain", x, y))
                # Lit (the current view): the square filled with the ink, the lines black.
                lit = (0, 0, 0) if line else cyan if square else ground
                self.assertEqual(image.getpixel((16 + x, y)), lit, ("lit", x, y))

        # A with its native overstrike.
        bold_a = bytes.fromhex("000000001c363663637f636363630000")
        for y, row in enumerate(bold_a):
            for x in range(8):
                want = (180, 197, 214) if row & (0x80 >> x) else (48, 48, 48)
                self.assertEqual(image.getpixel((32 + x, y)), want, (x, y))


if __name__ == "__main__":
    unittest.main()
