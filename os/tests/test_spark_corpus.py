"""Offline corpus/provenance build contracts, not semantic/legal attestation."""
import collections
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT/'crates/grimoire-core/assets/writingdeck'


class OriginalSparkCorpus(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.corpus = json.loads((BASE/'spark-corpus.json').read_text())
        cls.manifest = json.loads((ROOT/'docs/spark-corpus-manifest.json').read_text())
        cls.decisions = json.loads((BASE/'corpus/editorial-decisions.json').read_text())

    def test_compiler_has_no_artifact_drift(self):
        spec = importlib.util.spec_from_file_location('spark_corpus_builder', ROOT/'tools/build-spark-corpus.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for path, data in module.build().items():
            self.assertEqual(path.read_bytes(), data, str(path.relative_to(ROOT)))

    def test_literal_plot_ids_are_immutable_including_four_held_original_drafts(self):
        plots = []
        for file in sorted((BASE/'corpus').glob('original-plots-*.tsv')):
            for line in file.read_text().splitlines():
                if line and not line.startswith('#'):
                    self.assertEqual(len(line.split('|')), 4)
                    plots.append(line.split('|')[0])
        self.assertEqual(plots, [f'wd-p-{i:04}' for i in range(1, 1052)])
        held = set(self.decisions['excluded'])
        self.assertEqual(held, {'wd-p-0544', 'wd-p-0679', 'wd-p-0922', 'wd-p-0986'})
        accepted = {r['id'] for r in self.corpus['records'] if r['kind'] == 'plot'}
        self.assertEqual(set(plots)-held, accepted)
        self.assertEqual(len(accepted), 1047)

    def test_every_accepted_record_has_exact_source_line_hash_edition_and_original_license(self):
        hashes = set()
        for r in self.corpus['records']:
            source = r['source']
            path = ROOT/source['file']
            raw = path.read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), source['file_sha256'])
            self.assertTrue(raw.decode().splitlines()[source['line']-1].startswith(r['id']+'|'))
            self.assertEqual(source['id'], 'writingdeck-original-m6')
            self.assertEqual(source['section'], r['id'])
            self.assertEqual(source['edition'], self.corpus['version'])
            self.assertEqual(source['license'], 'MIT')
            self.assertIn('AI-assisted', source['attribution'])
            self.assertIn('No exclusive-copyright or independent-human-review claim', source['attribution'])
            self.assertEqual(hashlib.sha256(r['text'].encode()).hexdigest(), r['sha256'])
            self.assertNotIn(r['sha256'], hashes)
            hashes.add(r['sha256'])
        self.assertEqual(len(hashes), 1131)

    def test_real_exercises_cover_fourteen_categories_and_existing_text_work_modes(self):
        exercises = [r for r in self.corpus['records'] if r['kind'] == 'exercise']
        self.assertEqual(len(exercises), 84)
        self.assertEqual(len({r['category'] for r in exercises}), 14)
        self.assertEqual({r['work_mode'] for r in exercises}, {'drafting', 'revision', 'read-through'})
        self.assertTrue(all(len(r['text'].split()) >= 25 for r in exercises))
        self.assertEqual({r['id'] for r in exercises}, {f'wd-x-{i:04}' for i in range(1, 85)})
        self.assertEqual(collections.Counter(r['kind'] for r in self.corpus['records']), {'plot': 1047, 'exercise': 84})
        self.assertEqual(self.corpus['plot_count'], 1047)  # no product of plot/exercise combinations

    def test_manifest_exact_artifact_and_source_digests_exclude_historical_transcriptions(self):
        self.assertEqual(self.manifest['corpus_sha256'], hashlib.sha256((BASE/'spark-corpus.json').read_bytes()).hexdigest())
        self.assertEqual(self.manifest['editorial_decisions_sha256'], hashlib.sha256((BASE/'corpus/editorial-decisions.json').read_bytes()).hexdigest())
        self.assertEqual(set(self.manifest['excluded_source_ids']), {'plotto-1928', 'garykac-modern-cleanup', 'modern-third-party-prompt-lists'})
        for file in self.manifest['files']:
            self.assertEqual(file['sha256'], hashlib.sha256((ROOT/file['file']).read_bytes()).hexdigest())

    def test_creative_license_registry_pins_the_same_bundle_without_claiming_release_clearance(self):
        registry = json.loads((ROOT/'docs/content-licenses.json').read_text())
        original = next(r for r in registry['bundled'] if r['id'] == 'writingdeck-original-m6')
        self.assertEqual(original['sha256'], self.manifest['corpus_sha256'])
        self.assertEqual(original['item_count'], len(self.corpus['records']))
        self.assertEqual(original['license'], 'MIT')
        self.assertFalse(registry['legal_clearance'])
        self.assertEqual(registry['third_party_creative_items_bundled'], 0)
        self.assertTrue(all(s['bundled'] is False for s in registry['evaluated_sources']))
