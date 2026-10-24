#!/usr/bin/env python3
"""No root, loadkeys, systemd or real hardware operations in these tests."""
import importlib.machinery
import json
from pathlib import Path
import tempfile
import unittest

module = importlib.machinery.SourceFileLoader(
    "writingdeck_keyboard", str(Path(__file__).parents[1] / "scripts/writingdeck-keyboard")
).load_module()


class KeyboardSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.maps = self.root / "maps"
        self.maps.mkdir()
        for layout in module.LAYOUTS:
            (self.maps / (layout + ".map")).write_text("test keymap fixture")
        self.commands = []
        self.now = 100.0
        self.manager = module.KeyboardManager(self.root / "etc/keyboard.json",
                                              self.root / "run/preview.json", self.maps,
                                              self.commands.append, lambda: self.now)

    def tearDown(self):
        self.temp.cleanup()

    def preview(self, layout="colemak-dh-ansi"):
        return self.manager.command("preview", layout)["token"]

    def test_timer_is_armed_before_console_changes(self):
        self.preview()
        self.assertEqual(self.commands[0], ["/usr/bin/systemctl", "restart", "writingdeck-keyboard-revert.timer"])
        self.assertEqual(self.commands[1][0], "/usr/bin/loadkeys")
        self.assertEqual(self.commands[1][-1], str(self.maps / "colemak-dh-ansi.map"))

    def test_no_confirmation_reverts_without_client(self):
        self.preview()
        self.manager.command("rollback")
        self.assertEqual(self.commands[-1][-1], str(self.maps / "qwerty.map"))
        self.assertFalse(self.manager.pending.exists())

    def test_cancel_after_independent_timeout_is_idempotent(self):
        token = self.preview()
        self.now = 116.0
        self.manager.command("rollback")
        before = list(self.commands)
        result = self.manager.command("cancel", token)
        self.assertEqual(result["status"], "no-preview")
        self.assertEqual(result["layout"], "qwerty")
        self.assertEqual(self.commands, before)

    def test_expired_confirmation_reverts_and_fails(self):
        token = self.preview()
        self.now = 116.0
        with self.assertRaisesRegex(ValueError, "expired"):
            self.manager.command("confirm", token)
        self.assertEqual(self.commands[-1][-1], str(self.maps / "qwerty.map"))
        self.assertEqual(self.manager.saved()["layout"], "qwerty")

    def test_confirmation_is_durable_and_token_is_not_reusable(self):
        token = self.preview()
        self.manager.command("confirm", token)
        self.assertEqual(self.manager.saved()["layout"], "colemak-dh-ansi")
        with self.assertRaises(ValueError):
            self.manager.command("confirm", token)
        self.manager.command("boot")
        self.assertEqual(self.commands[-1][-1], str(self.maps / "colemak-dh-ansi.map"))

    def test_cancel_restores_previously_committed_layout(self):
        self.manager.command("confirm", self.preview("colemak"))
        token = self.preview("colemak-dh-iso")
        self.manager.command("cancel", token)
        self.assertEqual(self.commands[-1][-1], str(self.maps / "colemak.map"))

    def test_client_cannot_confirm_someone_elses_token(self):
        self.preview()
        with self.assertRaises(ValueError):
            self.manager.command("confirm", "0" * 32)
        self.assertTrue(self.manager.pending.exists())
        self.assertEqual(self.manager.saved()["layout"], "qwerty")

    def test_concurrent_previews_are_rejected(self):
        self.preview()
        before = list(self.commands)
        with self.assertRaises(ValueError):
            self.preview("colemak")
        self.assertEqual(self.commands, before)

    def test_arbitrary_paths_and_shell_text_are_rejected_before_changes(self):
        for layout in ("../../etc/shadow", "colemak;reboot", "--help", ""):
            with self.assertRaises(ValueError):
                self.preview(layout)
        self.assertEqual(self.commands, [])

    def test_timer_failure_never_leaves_a_new_layout_loaded(self):
        def failing(args):
            self.commands.append(args)
            if args[0] == "/usr/bin/systemctl":
                raise RuntimeError("systemd unavailable")
        self.manager.run = failing
        with self.assertRaises(RuntimeError):
            self.preview()
        self.assertEqual(self.commands[-1][-1], str(self.maps / "qwerty.map"))
        self.assertFalse(self.manager.pending.exists())

    def test_missing_and_symlinked_maps_are_refused(self):
        path = self.maps / "colemak.map"
        path.unlink()
        with self.assertRaises(ValueError):
            self.manager.load("colemak")
        path.symlink_to(self.maps / "qwerty.map")
        with self.assertRaises(ValueError):
            self.manager.load("colemak")

    def test_committed_before_crash_cannot_be_rolled_back(self):
        token = self.preview()
        # Simulate a crash after durable config rename but before clearing pending.
        module.atomic_json(self.manager.config, {"layout": "colemak-dh-ansi", "generation": token, "version": 1})
        before = list(self.commands)
        self.manager.command("rollback")
        self.assertEqual(self.commands, before)
        self.assertFalse(self.manager.pending.exists())

    def test_corrupt_config_is_preserved_and_no_keyboard_is_changed(self):
        self.manager.config.parent.mkdir()
        self.manager.config.write_text('{"layout":"../../bad"}')
        with self.assertRaises(ValueError):
            self.preview()
        self.assertEqual(self.commands, [])
        self.assertEqual(json.loads(self.manager.config.read_text())["layout"], "../../bad")


if __name__ == "__main__":
    unittest.main()
