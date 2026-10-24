#!/usr/bin/env python3
"""Offline, read-only structural documentation/content/notice checks. Not legal approval."""
import argparse
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT/'docs'
REQUIRED = 'ROADMAP FEATURE_MATRIX ARCHITECTURE WRITING_DECK_OS INSTALLATION LINUX_SYSTEM_INTEGRATION COLEMAK WORKSPACES KEYBOARD_SHORTCUTS WRITING_TOOLS CHALLENGE_LIBRARY CONTENT_LICENSES STORY_SPARK DAILY_CHALLENGES SYNC_AND_BACKUP TUTORIAL TESTING'.split()
ERRORS = []

def check(ok, message):
    if not ok:
        ERRORS.append(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def load(name):
    try:
        return json.loads((DOCS/name).read_text())
    except (OSError, ValueError) as exc:
        ERRORS.append(f'{name}: {exc}')
        return {}

class OfflineHTML(HTMLParser):
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        for field in ('src', 'href'):
            value = attrs.get(field, '')
            check(not value or value.startswith(('data:', '#')), f'HTML uses external resource {value!r}')
        check(tag not in ('script', 'iframe'), f'unnecessary active HTML: {tag}')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--require-current-evidence', action='store_true', help='also reject stale-source or failed software/VM evidence')
    args=parser.parse_args()
    for name in REQUIRED + ['STATUS']:
        check((DOCS/(name+'.md')).is_file(), f'missing {name}.md')
    for path in DOCS.glob('*.md'):
        text = path.read_text()
        for target in re.findall(r'!?\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)', text):
            target = unquote(target).split('#')[0].split('?')[0]
            if target and not re.match(r'[a-zA-Z][a-zA-Z0-9+.-]*:', target):
                check((path.parent/target).exists(), f'{path.relative_to(ROOT)}: broken link {target}')
    content = load('content-licenses.json')
    for entry in content.get('bundled', []):
        file = ROOT/entry['file']
        check(file.is_file(), f'content file missing: {entry["id"]}')
        if file.is_file():
            check(digest(file) == entry.get('sha256'), f'content hash changed: {entry["id"]}; review/regenerate manifest')
        check((ROOT/entry.get('license_notice', '')).is_file(), f'license notice missing: {entry["id"]}')
    for entry in content.get('evaluated_sources', []):
        check(entry.get('bundled') is False, f'unreviewed candidate included: {entry["id"]}')
    software = load('software-licenses.json')
    check(software.get('rust_packages'), 'Rust license inventory empty')
    bundle = ROOT/software.get('rust_notice_bundle', 'MISSING')
    check(bundle.is_file(), 'retained Rust third-party notices missing')
    dictionaries = software.get('existing_dictionaries', {})
    for entry in dictionaries.get('files', []):
        path = ROOT/entry['file']
        check(path.is_file() and digest(path) == entry['sha256'], f'dictionary hash changed: {entry["file"]}')
    features = load('feature-status.json').get('features', [])
    allowed = {'Existing', 'Planned', 'In progress', 'Implemented', 'Tested', 'Hardware verified', 'Blocked'}
    check(len(features) >= 60, 'feature inventory unexpectedly truncated')
    for feature in features:
        check(feature.get('status') in allowed, f'invalid feature status: {feature}')
        check(not feature.get('hardware_verified'), 'hardware claims require new identified hardware evidence/review')
        expected='| '+' | '.join(str(feature[k]).replace('|','\\|') for k in ['area','capability','status','verification','detail'])+' |'
        check(expected in (DOCS/'FEATURE_MATRIX.md').read_text(), f'matrix differs from machine inventory: {feature["capability"]}')
    lessons = json.loads((ROOT/'crates/grimoire-core/assets/writingdeck/tutorial.json').read_text())
    check([l['id'] for l in lessons] == list(range(1,40)), 'tutorial must catalog lessons 1–39 once each')
    reference = DOCS/'keyboard-reference.html'
    check(reference.is_file(), 'printable reference missing')
    if reference.is_file():
        html = reference.read_text()
        OfflineHTML().feed(html)
        check(digest(ROOT/'assets/help/24-every-key.md') in html, 'printable keys stale; run tools/generate-keyboard-reference.py')
    if args.require_current_evidence:
        from writingdeck_source import fingerprint
        current=fingerprint()
        results=load('evidence/test-results.json')
        vm=load('evidence/vm-boot.json')
        for label,record in [('software',results),('VM',vm)]:
            check(record.get('source_fingerprint') == current, f'{label} evidence is historical, not current source')
        check(bool(results.get('rust_suites')), 'missing Rust suite evidence')
        for suite in results.get('rust_suites',[]):
            check(suite.get('exit_code') == 0 and suite.get('failed') == 0 and suite.get('passed',0)>0, 'Rust evidence does not pass')
        python=results.get('python',{})
        check(python.get('exit_code') == 0 and python.get('failed') == 0 and python.get('skipped') == 0 and python.get('passed',0)>0, 'Python evidence missing, skipped or failing')
        check(vm.get('passed') is True, 'VM evidence did not pass')
    if ERRORS:
        print('\n'.join('ERROR: '+e for e in ERRORS), file=sys.stderr)
        return 1
    print(f'PASS: {len(REQUIRED)} required docs + STATUS, local links, {len(features)} capability rows, 39 lessons, content/dictionary hashes, notices, offline printable reference')
    if args.require_current_evidence:
        print('PASS: software and VM evidence fingerprints match the current source; this is NOT physical hardware verification')
    print(f'RELEASE GATE: {len(software.get("missing_root_notice_files_requiring_review", []))} all-platform Rust packages lack root-level notice files; {len(software.get("selected_build", {}).get("missing_root_notice_files", []))} used by the Linux no-audio build. No legal/hardware/production clearance implied.')
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
