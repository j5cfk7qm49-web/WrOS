"""Exercise real encrypted Brain backups without any network or local library."""
import hashlib
import importlib.machinery
import json
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "scripts/wros-backup"
backend = importlib.machinery.SourceFileLoader("wros_brain_backup", str(SCRIPT)).load_module()


def checked(path, payload):
    path.write_bytes(hashlib.sha256(payload).digest() + payload)


class BrainBackup(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="wros-brain-backup-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.library = self.root / "Brain"
        self.journal = self.library / "store/journal"
        self.journal.mkdir(parents=True)
        checked(self.journal / "00000000000000000001.update", b"first durable edit")
        (self.library / "device-id").write_text("10\n")
        (self.library / "readable").mkdir()
        (self.library / "readable/draft.md").write_text("Derived copy")
        (self.library / "files").mkdir()
        media = b"Immutable media fixture"
        self.media = hashlib.sha256(media).hexdigest()
        (self.library / "files" / self.media).write_bytes(media)
        self.service = backend.Service(self.root / "backup-state")

    def test_brain_backup_tracks_brain_only_saves_and_restores_with_new_identity(self):
        self.service.configure(str(self.root / "encrypted"))
        result = self.service.backup_brain(str(self.library), revision="a" * 64)
        self.assertEqual(result["snapshots"], [])
        self.assertEqual(result["brain_revisions"][str(self.library)], "a" * 64)
        older = result["brain_snapshots"][0]["id"]
        checked(self.journal / "00000000000000000002.update", b"second durable edit")
        result = self.service.backup_brain(str(self.library), automatic=True, revision="b" * 64)
        self.assertEqual(result["brain_revisions"][str(self.library)], "b" * 64)
        target = self.root / "Brain restored"
        self.service.restore(older, str(target))
        self.assertNotEqual((target / "device-id").read_text(), "10\n")
        self.assertEqual((target / "files" / self.media).read_bytes(), b"Immutable media fixture")
        self.assertFalse((target / "readable").exists())
        self.assertFalse((target / "store/journal/00000000000000000002.update").exists())
        self.assertTrue((self.journal / "00000000000000000002.update").exists())

    def test_pinning_hardlinks_committed_records_and_refuses_corruption(self):
        stage = self.root / "pinned"
        stage.mkdir()
        backend.pin_brain(self.library, stage)
        self.assertEqual((stage / "files" / self.media).stat().st_ino,
                         (self.library / "files" / self.media).stat().st_ino)
        update = self.journal / "00000000000000000001.update"
        update.rename(self.journal / "quarantined")
        self.assertEqual(backend.checked_brain_payload(stage / "store/journal" / update.name, 64),
                         b"first durable edit")
        checked(update, b"broken")
        update.write_bytes(b"truncated")
        self.service.configure(str(self.root / "encrypted"))
        with self.assertRaisesRegex(ValueError, "invalid checksum"):
            self.service.backup_brain(str(self.library))
        self.assertEqual(self.service.snapshots(kind=backend.BRAIN_TAG), [])


if __name__ == "__main__":
    unittest.main()
