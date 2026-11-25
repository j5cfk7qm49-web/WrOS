"""The owner preview uses the writing session's tile and bold pixels."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("unifont_preview", ROOT / "tools/unifont-preview.py")
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class UnifontPreview(unittest.TestCase):
    def test_tile_and_bold_a_match_the_bitmaps(self):
        frame = {
            "theme": "Grimoire",
            "console": False,
            "w": 2,
            "h": 1,
            "rows": [[["\ue110", "LightCyan", None, 0], ["A", "#b4c5d6", 236, 1]]],
        }
        image = preview.paint(frame, preview.load_glyphs())
        self.assertEqual(image.size, (16, 16))

        # Unifont's W with rows 1 and 14 set, and A with its native overstrike.
        tile = bytes.fromhex("00ff0000424242425a5a66664242ff00")
        bold_a = bytes.fromhex("000000001c363663637f636363630000")
        for column, bitmap, fg, bg in (
            (0, tile, (0, 255, 255), (13, 13, 16)),
            (8, bold_a, (180, 197, 214), (48, 48, 48)),
        ):
            for y, row in enumerate(bitmap):
                for x in range(8):
                    pixel = column + x, y
                    self.assertEqual(image.getpixel(pixel), fg if row & (0x80 >> x) else bg, pixel)


if __name__ == "__main__":
    unittest.main()
