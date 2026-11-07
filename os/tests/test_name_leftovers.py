"""#14: "writingdeck" survives only where renaming it would break something.

Each pattern below is a name that is stored in writers' books or settings, is on
installed systems, or keeps an older release working. Everything else says WrOS.
"""
from pathlib import Path
import re
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]

# History and pinned upstream or hashed content, read as they were written.
SKIPPED = (
    'docs/archive/', 'docs/evidence/', 'crates/crossterm-upstream/', 'os/licenses/',
    'os/python-tools/wheel-metadata/', 'CHANGELOG.md',
    'crates/grimoire-core/assets/wros/spark-corpus.json',
    'os/tests/test_name_leftovers.py', '.gitleaksignore', 'docs/NAMING_MIGRATION.md',
    # The migration itself and its test name every old path on purpose.
    'os/scripts/wros-name-migrate.py', 'os/tests/test_name_migration.py',
)

KEPT = [
    # Stored in every book: the hidden state folder and the files inside it.
    (r'\.grimoire/writingdeck\b', 'the hidden state folder inside each book'),
    (r'\.writingdeck-demo\b|WritingDeck safe tutorial project v1', "the tutorial book's marker"),
    (r'writingdeck-native(-1)?\b', 'engine names saved with review findings and voice profiles'),
    (r'writingdeck-original-[a-z0-9-]+', 'content IDs saved with Story Spark and challenge history'),
    (r'writingdeck-documents-', 'temporary folder prefix older versions clean up'),
    (r'writingdeck/shortcuts\.json', "each computer's saved shortcuts"),
    (r'writingdeck-(source-repack|pinned-runtime)-1', 'written into the pinned prose-tools wheels'),
    # On installed systems, kept by the rc7 migration so older pieces still work.
    (r'opt/writingdeck\b|\.writingdeck-runtime\.json', 'the installed prose-tools runtime'),
    (r'/etc/writingdeck\b|/usr/share/writingdeck\b|/usr/share/doc/writingdeck\b',
     'compatibility links to /etc/wros and /usr/share/wros'),
    (r'writingdeck-(keyboard|session)(-revert)?(\.service|\.timer)?\b|writingdeck\.sh\b',
     'compatibility links for the keyboard units and session helper'),
    (r'org\.writingdeck\.keyboard', "the older greeter's keyboard permission"),
    (r'isInGroup\("writingdeck"\)|ingroup writingdeck|\bwros writingdeck\b|wros,writingdeck'
     r"|'writingdeck'|\"writingdeck\"|-G', 'writingdeck'|'--system', 'writingdeck'"
     r'|writingdeck:x:|grep -qw writingdeck|`writingdeck` compatibility group',
     'the compatibility group that mirrors the wros group'),
    (r'WRITINGDECK_[A-Z_]*', 'older environment and release-file names, still accepted'),
    (r"grimoire writingdeck\b|BINARY, 'writingdeck'|alias: writingdeck",
     'the older command-line alias'),
    # History.
    (r'WritingDeck( OS)?|WRITINGDECK OS', 'the earlier working title, named as history'),
    (r'writingdeck/(foundation|m7b-wip|m7b|m7a)\b|codex/m7b', 'historical branch names'),
]
KEPT_RE = re.compile('|'.join(f'(?:{p})' for p, _ in KEPT))
WORD = re.compile('writingdeck', re.I)


def tracked():
    out = subprocess.run(['git', 'ls-files', '-z'], cwd=ROOT, capture_output=True, check=True)
    return [p for p in out.stdout.decode().split('\0') if p and not p.startswith(SKIPPED)]


def leftovers(files):
    found = []
    for name in files:
        path = ROOT / name
        try:
            text = path.read_text(encoding='utf-8')
        except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if WORD.search(KEPT_RE.sub('', line)):
                found.append(f'{name}:{number}: {line.strip()[:120]}')
    return found


class NameLeftovers(unittest.TestCase):
    @unittest.skipUnless((ROOT / '.git').exists(), 'needs the git checkout')
    def test_writingdeck_is_only_where_renaming_would_break_something(self):
        found = leftovers(tracked())
        self.assertEqual(found, [], f'{len(found)} lines:\n' + '\n'.join(found[:80]))

    def test_the_kept_list_catches_a_new_stray_name(self):
        self.assertTrue(WORD.search(KEPT_RE.sub('', 'see tools/writingdeck_helper.py')))
        self.assertFalse(WORD.search(KEPT_RE.sub('', 'reads .grimoire/writingdeck/pins.json')))


if __name__ == '__main__':
    unittest.main()
