import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import rc_vm_chain as chain


class ReleaseChain(unittest.TestCase):
    def test_failed_update_skips_its_features_and_still_runs_the_rc6_migration(self):
        with tempfile.TemporaryDirectory(prefix='wros-chain-') as tmp:
            calls = []

            def phase(label, *args):
                calls.append((label, args))
                path = Path(tmp) / label
                path.mkdir()
                return chain.Outcome(label, label == 'rc6-migration', path)

            with patch.object(chain, 'run_phase', side_effect=lambda _, label, *args: phase(label, *args)):
                outcomes = chain.qualify(Path(tmp), Path('/install'), Path('/bundle'),
                                         '0.1.0~rc7', Path('/rc6.qcow2'))
            self.assertEqual([outcome.passed for outcome in outcomes], [False, False, True])
            self.assertEqual([label for label, _ in calls], ['rc1-update', 'rc6-migration'])
            self.assertIn('--check-migration', calls[1][1])
            with contextlib.redirect_stdout(io.StringIO()) as report:
                self.assertEqual(chain.summarize(outcomes), 1)
            self.assertIn('SKIP features', report.getvalue())
            self.assertIn('PASS rc6-migration', report.getvalue())

    def test_a_saved_disk_at_the_laptops_version_is_the_start_instead_of_rc1(self):
        with tempfile.TemporaryDirectory(prefix='wros-chain-') as tmp:
            calls = []

            def phase(label, *args):
                calls.append((label, args))
                path = Path(tmp) / label
                path.mkdir()
                return chain.Outcome(label, True, path)

            with patch.object(chain, 'run_phase', side_effect=lambda _, label, *args: phase(label, *args)):
                outcomes = chain.qualify(Path(tmp), Path('/install'), Path('/bundle'), '0.1.0~rc9',
                                         start=(Path('/rc7.qcow2'), '0.1.0~rc7'))
            self.assertTrue(all(outcome.passed for outcome in outcomes))
            self.assertEqual([label for label, _ in calls], ['0.1.0~rc7-update', 'features'])
            update = calls[0][1]
            self.assertEqual(update[update.index('--base-disk') + 1], Path('/rc7.qcow2'))
            self.assertEqual(update[update.index('--from-version') + 1], '0.1.0~rc7')
            features = calls[1][1]
            self.assertEqual(features[features.index('--base-disk') + 1], Path(tmp) / '0.1.0~rc7-update' / 'disk.qcow2')

    def test_phase_result_has_to_pass_and_belong_to_this_run(self):
        with tempfile.TemporaryDirectory(prefix='wros-chain-') as tmp:
            def phase(*_, env):
                base = Path(env['RC_VM_BASE']) / 'update-ok-fixture'
                base.mkdir()
                (base / 'result.json').write_text(json.dumps({'passed': False}))
                return 0

            with patch.object(chain.subprocess, 'call', side_effect=phase):
                outcome = chain.run_phase(Path(tmp), 'rc1-update', 'update')
            self.assertFalse(outcome.passed)
            self.assertEqual(outcome.run.name, 'update-ok-fixture')
