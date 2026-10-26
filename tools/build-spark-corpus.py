#!/usr/bin/env python3
"""Compile original, explicitly identified Spark records; never generate plots.

Offline, standard-library-only. --check verifies checked-in artifacts. Source
IDs are literal and immutable. No downloads, manuscripts or user state are read.
Semantic acceptance is an editorial decision, not a row-count/Jaccard proof.
"""
import argparse
import collections
import hashlib
import heapq
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'crates/grimoire-core/assets/writingdeck'
SOURCE = BASE / 'corpus'
VERSION = 'writingdeck-original-1'
FEATURES = {'single-scene', 'multi-scene', 'continuous-time', 'time-jumps',
            'dialogue-only', 'narration-only', 'no-spoken-dialogue', 'visual-detail',
            'nonvisual-only', 'first-person', 'third-person', 'nonlinear-order', 'linear-order'}
GENRES = {'realist', 'speculative', 'mystery', 'adventure', 'romance', 'family',
          'satire', 'political', 'historical'}
ATTRIBUTION = ('WritingDeck contributors. Original AI-assisted drafting and agent editorial review; '
               'MIT to the extent rights subsist. No exclusive-copyright or independent-human-review claim.')

def sha(data):
    return hashlib.sha256(data).hexdigest()

def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n').encode()

def csv(text):
    return sorted(set(x.strip() for x in text.split(',') if x.strip()))

def title(text):
    words = text.split()
    return ' '.join(words[:9]).rstrip('.,;:') + ('…' if len(words) > 9 else '')

def build():
    decisions = json.loads((SOURCE / 'editorial-decisions.json').read_text())
    records, files, ids, drafts = [], [], set(), []
    for path in sorted(SOURCE.glob('*.tsv')):
        raw = path.read_bytes()
        relative = str(path.relative_to(ROOT))
        file_hash = sha(raw)
        count = 0
        for line_number, line in enumerate(raw.decode().splitlines(), 1):
            if not line or line.startswith('#'):
                continue
            count += 1
            fields = line.split('|')
            is_plot = path.name.startswith('original-plots-')
            assert len(fields) == (4 if is_plot else 9), (path, line_number)
            item_id = fields[0]
            assert re.fullmatch(r'wd-[px]-[0-9]{4}', item_id) and item_id not in ids, item_id
            ids.add(item_id)
            if is_plot:
                _, category, genres, text = fields
                part = int(path.stem.rsplit('-', 1)[1])
                difficulty = 1 if part in (6, 10, 12) else (3 if part == 11 or (part == 4 and 'speculative' in genres) else 2)
                minutes, mode = 20, None
                required = decisions['conditions'].get(item_id, [])
                excluded = []
                if 'single-scene' in required:
                    excluded += ['multi-scene', 'time-jumps']
                if 'no-spoken-dialogue' in required:
                    excluded += ['dialogue-only']
                item_title = title(text)
            else:
                _, category, difficulty, minutes, mode, requires, excludes, item_title, text = fields
                difficulty, minutes = int(difficulty), int(minutes)
                genres = ''
                required, excluded = csv(requires), csv(excludes)
            assert 1 <= difficulty <= 3 and 1 <= minutes <= 120
            assert 20 <= len(text.split()) <= 150 and len(text) <= 2000, item_id
            assert text == text.strip() and not any(ord(c) < 32 for c in text)
            assert set(csv(genres)) <= GENRES
            assert set(required) <= FEATURES and set(excluded) <= FEATURES
            assert not set(required) & set(excluded)
            record = dict(id=item_id, kind='plot' if is_plot else 'exercise', title=item_title,
                          text=text, category=category, genres=csv(genres), difficulty=difficulty,
                          minutes=minutes, work_mode=mode, requires=sorted(required),
                          excludes=sorted(excluded), sha256=sha(text.encode()),
                          source=dict(id='writingdeck-original-m6', edition=VERSION, section=item_id,
                                      file=relative, line=line_number, file_sha256=file_hash,
                                      license='MIT', attribution=ATTRIBUTION,
                                      adaptation='Original wording; no third-party prompt text'))
            if is_plot:
                drafts.append(record)
            if item_id not in decisions['excluded']:
                records.append(record)
        files.append(dict(file=relative, sha256=file_hash, records=count))
    assert set(decisions['excluded']) <= ids and set(decisions['conditions']) <= ids
    assert len({r['text'] for r in records}) == len(records), 'exact duplicate wording'
    normalized = [re.sub(r'[^a-z0-9]+', ' ', r['text'].lower()).strip() for r in records]
    assert len(set(normalized)) == len(records), 'punctuation/case-only duplicate'
    plots = [r for r in records if r['kind'] == 'plot']
    exercises = [r for r in records if r['kind'] == 'exercise']
    assert len(plots) >= 1000 and len(exercises) >= 70
    categories = sorted(set(r['category'] for r in exercises))
    assert len(categories) >= 14
    bank = dict(schema_version=1, version=VERSION, license='MIT', attribution=ATTRIBUTION,
                plot_count=len(plots), exercise_count=len(exercises), records=records)
    payload = encoded(bank)
    stop = set('a an the is are was were be been being of to in on at by for from with as and or that this his her he she him it its have has had not one who whom when which into through some certain another their they them every only would could should while after before without until all both each more most than other own can must first two new same also just rather now no anyone everyone someone nothing person people family town city partner couple discovers finds realizes'.split())
    words, closest = [], []
    for i, record in enumerate(drafts):
        text = re.sub(r'\b(?:partner|partners|couple|family|town|city|resident|residents|employee|worker|person|people)\b', 'person', record['text'].lower())
        current = set(re.findall(r'[a-z]+', text)) - stop
        for j, previous in enumerate(words):
            similarity = len(current & previous) / max(1, len(current | previous))
            candidate = (similarity, j, i)
            if len(closest) < 32:
                heapq.heappush(closest, candidate)
            elif candidate > closest[0]:
                heapq.heapreplace(closest, candidate)
        words.append(current)
    pairs = [dict(left=drafts[j]['id'], right=drafts[i]['id'], jaccard=round(score, 6),
                  left_sha256=drafts[j]['sha256'], right_sha256=drafts[i]['sha256'],
                  decision='held-out' if any(drafts[k]['id'] in decisions['excluded'] for k in (j, i)) else 'distinct concrete mechanism/obstacle/stakes on agent review')
             for score, j, i in sorted(closest, reverse=True)]
    sample = sorted(set(range(0, len(plots), 17)) | {0, len(plots)-1})
    report = dict(schema_version=1, version=VERSION, corpus_sha256=sha(payload),
                  review_scope=decisions['review_type'], draft_plots=len(drafts),
                  accepted_plots=len(plots), accepted_exercises=len(exercises),
                  excluded=decisions['excluded'], genre_counts=dict(sorted(collections.Counter(g for r in plots for g in r['genres']).items())),
                  exercise_categories=categories, closest_pairs=pairs,
                  editorial_resample=[dict(id=plots[i]['id'], sha256=plots[i]['sha256'], text=plots[i]['text'],
                                          result='concrete actionable conflict, mystery or consequential choice; no mere trait/name variant') for i in sample],
                  limits=['Lexical similarity is a review aid, not semantic uniqueness proof.',
                          'No independent human editorial or legal attestation is claimed.',
                          'Historical research drafts and M5 exercises are not counted.',
                          'Difficulty is an editorial starting-burden estimate, not a quality ranking.',
                          'All runtime selection is offline and non-AI.'])
    manifest = dict(schema_version=1, version=VERSION, corpus_file=str((BASE/'spark-corpus.json').relative_to(ROOT)),
                    corpus_sha256=sha(payload), license='MIT', attribution=ATTRIBUTION,
                    accepted_plots=len(plots), accepted_exercises=len(exercises), files=files,
                    editorial_decisions_sha256=sha((SOURCE/'editorial-decisions.json').read_bytes()),
                    excluded_source_ids=['plotto-1928', 'garykac-modern-cleanup', 'modern-third-party-prompt-lists'],
                    notice='LICENSE; original wording, MIT to extent rights subsist; no exclusive copyright claim')
    return {BASE/'spark-corpus.json': payload, ROOT/'docs/evidence/spark-corpus.json': encoded(report),
            ROOT/'docs/spark-corpus-manifest.json': encoded(manifest)}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    artifacts = build()
    for path, data in artifacts.items():
        if args.check:
            assert path.read_bytes() == data, f'stale corpus artifact: {path.relative_to(ROOT)}'
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    corpus = json.loads(artifacts[BASE/'spark-corpus.json'])
    counts = collections.Counter(r['kind'] for r in corpus['records'])
    print(f'Spark original corpus: {counts["plot"]} accepted plots, {counts["exercise"]} exercises; ' + ('pinned source/artifacts verified' if args.check else 'artifacts built'))

if __name__ == '__main__':
    main()
