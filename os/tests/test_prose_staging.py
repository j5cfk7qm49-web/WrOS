"""Real pinned payload staging, without root, devices or host installation."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_prose_pty import ROOT, RUNTIME

spec = importlib.util.spec_from_file_location('wros_stage_runtime', ROOT/'tools/stage-prose-runtime.py')
stager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stager)


def debian_root(base):
    root = base/'guest'
    (root/'etc').mkdir(parents=True)
    (root/'etc/debian_version').write_text('13\n')
    return root


@unittest.skipIf(os.geteuid() == 0, 'staging contract fixtures must be non-root')
class ProseStaging(unittest.TestCase):
    def test_live_root_and_destination_symlinks_are_refused_without_touching_targets(self):
        with self.assertRaisesRegex(ValueError, 'never the live root'):
            stager.destination(Path('/'))
        with tempfile.TemporaryDirectory(prefix='wros-stage-refusal-') as tmp:
            base = Path(tmp)
            root = debian_root(base)
            foreign = base/'foreign'
            foreign.mkdir()
            (foreign/'keep').write_bytes(b'original')
            (root/'opt').symlink_to(foreign)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                stager.stage(root, RUNTIME.parent.parent)
            self.assertEqual((foreign/'keep').read_bytes(), b'original')
            self.assertEqual(sorted(p.name for p in foreign.iterdir()), ['keep'])

    def test_exact_runtime_is_copied_and_reused_but_forged_installed_payload_is_preserved(self):
        self.assertTrue(RUNTIME.is_file(), 'provision actual runtime before staging tests')
        with tempfile.TemporaryDirectory(prefix='wros-stage-real-') as tmp:
            root = debian_root(Path(tmp))
            source = RUNTIME.parent.parent
            target = stager.stage(root, source)
            marker = target/stager.provisioner.MARKER
            before = marker.read_bytes()
            self.assertEqual(stager.stage(root, source), target)
            self.assertEqual(marker.read_bytes(), before)
            self.assertTrue((target/'bin/python3').is_symlink())
            victim = target/'lib/python3.13/site-packages/proselint/__init__.py'
            victim.write_bytes(victim.read_bytes()+b'\n# tampered fixture\n')
            record = json.loads(before)
            record['files'] = stager.provisioner.payload_files(target)
            marker.write_text(json.dumps(record))
            forged = marker.read_bytes()
            with self.assertRaisesRegex(ValueError, 'exact locked wheel payload'):
                stager.stage(root, source)
            self.assertEqual(marker.read_bytes(), forged)
            self.assertTrue(victim.read_bytes().endswith(b'# tampered fixture\n'))

    def test_failed_copy_verification_does_not_publish_an_installed_runtime(self):
        with tempfile.TemporaryDirectory(prefix='wros-stage-failure-') as tmp:
            root = debian_root(Path(tmp))
            real_verify = stager.provisioner.verify_runtime
            calls = 0
            def fail_copy(*args, **kwargs):
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise ValueError('injected staged verification failure')
                return real_verify(*args, **kwargs)
            with patch.object(stager.provisioner, 'verify_runtime', side_effect=fail_copy):
                with self.assertRaisesRegex(ValueError, 'injected staged verification failure'):
                    stager.stage(root, RUNTIME.parent.parent)
            self.assertFalse((root/'opt/writingdeck/python').exists())
            self.assertEqual(list((root/'opt/writingdeck').iterdir()), [])
