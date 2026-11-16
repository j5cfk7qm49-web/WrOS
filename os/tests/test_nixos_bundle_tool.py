"""tools/build-nixos-update-bundle.py: cutting a full bundle down to a delta.

rc15 first went out with an archive missing: sudo.pam (new in rc15) and passwd.pam (already in
rc14) have identical contents, so they share one .nar.zst, which the manifest credited to
passwd.pam; the delta dropped it, and the laptop's update failed at the restart that installs it.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('bundle_tool', ROOT / 'tools/build-nixos-update-bundle.py')
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

TOP = '/nix/store/' + 'a' * 32 + '-nixos-system-unnamed-0.1.0-rc15'
OLD = '/nix/store/' + 'b' * 32 + '-passwd.pam'
NEW = '/nix/store/' + 'c' * 32 + '-sudo.pam'
GONE = '/nix/store/' + 'd' * 32 + '-in-the-baseline'


def bundle(root):
    """A full bundle: four store paths, two of them (OLD, NEW) sharing one archive."""
    cache = Path(root) / 'cache'
    (cache / 'nar').mkdir(parents=True)
    files = [{'path': 'cache/nix-cache-info', 'store_path': None}]
    (cache / 'nix-cache-info').write_text('StoreDir: /nix/store\n')
    for path, nar in ((TOP, 'top'), (OLD, 'shared'), (NEW, 'shared'), (GONE, 'gone')):
        name = Path(path).name.split('-', 1)[0] + '.narinfo'
        (cache / name).write_text(f'StorePath: {path}\nURL: nar/{nar}.nar.zst\n')
        files.append({'path': f'cache/{name}', 'store_path': path})
        if not (cache / 'nar' / f'{nar}.nar.zst').exists():
            (cache / 'nar' / f'{nar}.nar.zst').write_bytes(nar.encode())
            # The manifest credits a shared archive to the first path that names it.
            files.append({'path': f'cache/nar/{nar}.nar.zst', 'store_path': path})
    return files


class Delta(unittest.TestCase):
    def test_a_shared_archive_stays_while_a_kept_package_needs_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            kept = tool.prune_to_delta(tmp, files, {OLD, GONE}, TOP)
            paths = {item['path']: item['store_path'] for item in kept}
            self.assertIn('cache/nar/shared.nar.zst', paths)
            # Credited to the package the machine lacks, so it is downloaded when needed.
            self.assertEqual(paths['cache/nar/shared.nar.zst'], NEW)
            self.assertNotIn('cache/nar/gone.nar.zst', paths)
            self.assertFalse((Path(tmp) / 'cache/nar/gone.nar.zst').exists())
            self.assertNotIn(f'cache/{"b" * 32}.narinfo', paths)
            tool.check_archives(tmp, kept)

    def test_a_bundle_missing_an_archive_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            (Path(tmp) / 'cache/nar/shared.nar.zst').unlink()
            broken = [item for item in files if item['path'] != 'cache/nar/shared.nar.zst']
            with self.assertRaisesRegex(tool.BuildError, 'lacks'):
                tool.check_archives(tmp, broken)

    def test_the_old_rule_would_have_dropped_it(self):
        # What rc15's first bundle did: prune by each file's own credit.
        with tempfile.TemporaryDirectory() as tmp:
            files = bundle(tmp)
            old = [item for item in files
                   if not (item['store_path'] in {OLD, GONE} and item['store_path'] != TOP)]
            with self.assertRaises(tool.BuildError):
                tool.check_archives(tmp, old)


if __name__ == '__main__':
    unittest.main()
