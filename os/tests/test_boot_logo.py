"""nixos/scripts/wros-boot-logo: the boot menu's start picture fitted to the screen (#165)."""
import importlib.machinery
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader('wros_boot_logo', str(REPO / 'nixos/scripts/wros-boot-logo'))
module = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
loader.exec_module(module)


class BootLogo(unittest.TestCase):
    def test_the_logo_goes_where_the_unlock_screen_draws_it(self):
        # The HP Stream: a 170x48 grid, the 23-row block centred, the logo at column 67, row 12.
        self.assertEqual(module.placement(1366, 768), (12 * 16, 67 * 8))
        # QEMU's 1280x800: 160x50 cells, so column 62 and row 13.
        self.assertEqual(module.placement(1280, 800), (13 * 16, 62 * 8))

    def test_a_screen_too_small_or_absurd_gets_no_picture(self):
        for size in ((279, 768), (1366, 367), (0, 0), (8000, 600), (800, 5000)):
            self.assertIsNone(module.placement(*size), size)
        self.assertIsNotNone(module.placement(280, 368))

    def test_the_size_is_read_from_the_boot_parameters_grub_passed(self):
        params = bytearray(4096)
        struct.pack_into('<HH', params, 0x12, 1366, 768)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'data'
            path.write_bytes(bytes(params))
            self.assertEqual(module.screen(path), (1366, 768))


if __name__ == '__main__':
    unittest.main()
