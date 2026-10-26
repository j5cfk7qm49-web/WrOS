#!/usr/bin/python3
"""VM-only installed prose/WordNet probe, run as the unprivileged writer."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

BINARY = os.environ.get('GRIMOIRE_BINARY', '/usr/bin/grimoire')
RUNTIME = Path(os.environ.get('WRITINGDECK_TOOLS_PYTHON', '/opt/writingdeck/python/bin/python3')).parent.parent
TEXT = '---\ntitle: Unicode fixture\n---\n\nCafé 🦊 e\u0301. We waited in order to see. He could of gone. The gate was opened by Mara.\n'


def invoke(args, home, timeout=110):
    return subprocess.run([BINARY, 'writingdeck', *args], capture_output=True,
                          text=True, timeout=timeout, env=os.environ | {'HOME': str(home)})


def main():
    assert os.geteuid() != 0, 'installed-tool probes must never run Grimoire as root'
    record = json.loads((RUNTIME/'.writingdeck-runtime.json').read_text())
    assert record['schema_version'] == 2 and len(record['packages']) == 49
    results = []
    with tempfile.TemporaryDirectory(prefix='wros-vm-prose-') as tmp:
        home = Path(tmp)
        source = home/'synthetic.md'
        source.write_text(TEXT)
        original = source.read_bytes()
        denied = invoke(['prose', str(source), '--engine', 'nabokov'], home)
        assert denied.returncode != 0 and '--allow-model' in denied.stderr
        for engine in ('native', 'proselint', 'nabokov'):
            args = ['prose', str(source), '--engine', engine]
            if engine == 'nabokov':
                args.append('--allow-model')
            started = time.monotonic()
            result = invoke(args, home)
            assert result.returncode == 0, result.stderr + result.stdout
            report = json.loads(result.stdout)
            assert report['status'] == 'complete'
            if engine == 'nabokov':
                finding = next(f for f in report['findings'] if f['rule'] == 'NB302')
                assert finding['expected'] == 'was opened by Mara' and finding['alternatives'] == []
            if engine == 'proselint':
                assert any(f['expected'] == 'could of' for f in report['findings'])
            assert source.read_bytes() == original, 'analysis changed original source'
            results.append({'engine': engine, 'wall_seconds': round(time.monotonic()-started, 3),
                            'worker_peak_rss_kib': report['child_peak_rss_kib'],
                            'findings': len(report['findings'])})
        result = invoke(['lexicon', 'hot'], home)
        assert result.returncode == 0, result.stderr
        lookup = json.loads(result.stdout)
        assert len(lookup['senses']) == 21
        assert any(a['word'] == 'cold' for sense in lookup['senses'] for a in sense['antonyms'])
        assert sorted(p.name for p in home.iterdir()) == ['synthetic.md']
    print(json.dumps({'uid': os.geteuid(), 'runtime_packages': len(record['packages']),
                      'runtime_marker_sha256': hashlib.sha256((RUNTIME/'.writingdeck-runtime.json').read_bytes()).hexdigest(),
                      'engines': results, 'wordnet_senses': 21,
                      'checks': ['explicit Nabokov model permission', 'actual installed native/proselint/Nabokov',
                                 'Unicode original unchanged', 'no advisory automatic rewriting',
                                 'real WordNet senses and antonyms', 'no runtime downloads or state files'],
                      'limits': 'Small synthetic fixture in networkless TCG VM; not physical, production, or whole-book qualification.'}))


if __name__ == '__main__':
    main()
