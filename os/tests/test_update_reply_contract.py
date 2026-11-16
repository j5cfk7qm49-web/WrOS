"""Every reply the NixOS updater can print is one the app's parsers accept.

rc14 and rc15 shipped an app whose strict parsers (serde deny_unknown_fields) refused fields the
NixOS updater adds (toplevel, closure_paths, version), so an update fetched online could not be
reviewed or staged from Settings. The VM tests drive the updater from a shell, so nothing caught
it. This reads both sides from source: the keys of each dictionary the updater returns, and the
fields of the Rust struct that parses it.
"""
import ast
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / 'nixos/scripts/wros-update-engine.py'
VIEWS = ROOT / 'crates/wros-sys/src/system_views.rs'

# updater function -> the Rust struct that parses what it returns
CONTRACT = {
    'review_bundle': 'RawReview',
    'stage_update': 'RawStaged',
    'stage_rollback_locked': 'RawGoneBack',
    'stage_generation_rollback': 'RawGoneBack',
    'apply_staged_transaction': 'RawApplied',
    'fetch_online': 'RawFetch',
    'inspect_recovery': 'RawRecovery',
}


def rust_fields(struct):
    source = VIEWS.read_text(encoding='utf-8')
    body = re.search(r'struct ' + struct + r' \{(.*?)\n    \}', source, re.S)
    assert body, struct
    fields = {}
    lines = body.group(1).splitlines()
    for index, line in enumerate(lines):
        match = re.match(r'\s+(\w+): ', line)
        if match:
            optional = any('serde(default' in lines[j] for j in range(max(0, index - 2), index))
            fields[match.group(1)] = optional or 'Option<' in line
    return fields


def returned_dicts(function):
    tree = ast.parse(ENGINE.read_text(encoding='utf-8'))
    node = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == function)
    dicts = []
    for sub in ast.walk(node):
        if isinstance(sub, ast.Return) and isinstance(sub.value, ast.Dict):
            keys = [k.value for k in sub.value.keys if isinstance(k, ast.Constant)]
            if 'status' in keys or 'root_filesystem' in keys:
                dicts.append(set(keys))
    return dicts


class UpdateReplyContract(unittest.TestCase):
    def test_every_updater_reply_parses_in_the_app(self):
        for function, struct in CONTRACT.items():
            fields = rust_fields(struct)
            required = {name for name, optional in fields.items() if not optional}
            replies = returned_dicts(function)
            with self.subTest(function=function):
                self.assertTrue(replies, f'{function} returns no reply dictionary')
                for keys in replies:
                    # Error and status-only replies of other shapes are not parsed by this struct.
                    if keys == {'status'} or (struct == 'RawGoneBack' and 'rollback' not in keys
                                              and 'transaction_id' not in keys):
                        continue
                    self.assertLessEqual(keys, set(fields), f'{function} sends fields {struct} refuses')
                    self.assertLessEqual(required, keys, f'{function} omits fields {struct} requires')

    def test_the_review_package_entries_parse(self):
        fields = set(rust_fields('RawPackage'))
        tree = ast.parse(ENGINE.read_text(encoding='utf-8'))
        review = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'review_bundle')
        for sub in ast.walk(review):
            if isinstance(sub, ast.Dict):
                keys = {k.value for k in sub.keys if isinstance(k, ast.Constant)}
                if 'to_version' in keys:
                    self.assertLessEqual(keys, fields)


if __name__ == '__main__':
    unittest.main()
