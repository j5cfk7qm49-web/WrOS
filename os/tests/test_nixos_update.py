"""Unit tests for the NixOS generation-based update engine (nixos/scripts/wros-update-engine.py)."""
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
ENGINE_PATH = REPO / 'nixos/scripts/wros-update-engine.py'


def load_nixos_engine():
    loader = importlib.machinery.SourceFileLoader('wros_nixos_update_engine', str(ENGINE_PATH))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class NixosUpdateEngineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.engine = load_nixos_engine()
        cls.engine.TRUSTED_UID = os.geteuid()
        cls.engine._base.TRUSTED_UID = os.geteuid()

    def test_constants_match_debian_engine(self):
        base = self.engine._base
        for name in (
            'MAX_VALIDITY',
            'MAX_STAGED_AGE',
            'CLOCK_SKEW',
            'BOOT_CLOCK_SKEW',
            'PROBATION_FAILED_LOGINS',
            'LAUNCHER_VERBS',
        ):
            self.assertEqual(getattr(self.engine, name), getattr(base, name), name)
        self.assertEqual(base.ONLINE_URL, 'https://j5cfk7qm49-web.github.io/WrOS/updates/')
        self.assertEqual(self.engine.ONLINE_URL, 'https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/')
        self.assertEqual(self.engine.PINNED_SUITES, ('wros-nixos-updates',))

    def test_tilde_version_comparison_orders_rc_before_release(self):
        cmp_v = self.engine.compare_versions
        self.assertTrue(cmp_v('0.1.0~rc13', 'lt', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'gt', '0.1.0~rc13'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'lt', '0.1.0'))
        self.assertTrue(cmp_v('0.1.0', 'gt', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0~rc14', 'eq', '0.1.0~rc14'))
        self.assertTrue(cmp_v('0.1.0', 'lt', '0.1.0+hotfix1'))

    def _make_signed_bundle(self, root, *, version='0.1.0~rc14',
                            toplevel='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14',
                            moment=None, tamper_nar=False, baseline=None, orphan_narinfo=False):
        moment = moment or datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc)
        until = moment + timedelta(days=14)
        gnupg = root / 'gnupg'
        gnupg.mkdir(mode=0o700)
        batch = (
            '%no-protection\n'
            'Key-Type: RSA\n'
            'Key-Length: 2048\n'
            'Name-Real: WrOS Test Signer\n'
            'Name-Email: test@wros.invalid\n'
            'Expire-Date: 0\n'
            '%commit\n'
        )
        # The caller's PATH first: the Nix check sandbox has gpg, but no /usr/bin.
        env = {'PATH': os.environ.get('PATH', '') + ':/usr/sbin:/usr/bin:/sbin:/bin',
               'LC_ALL': 'C', 'GNUPGHOME': str(gnupg)}
        subprocess.run(['gpg', '--batch', '--generate-key'], input=batch.encode(), env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        colons = subprocess.check_output(
            ['gpg', '--batch', '--with-colons', '--fingerprint', '--list-secret-keys'],
            env=env, text=True)
        fpr = next(line.split(':')[9] for line in colons.splitlines() if line.startswith('fpr:'))
        keyring = root / 'keyring.gpg'
        signers = root / 'update-signers'
        subprocess.run(['gpg', '--batch', '--output', str(keyring), '--export', fpr], env=env, check=True)
        signers.write_text(fpr + '\n', encoding='utf-8')
        os.chmod(keyring, 0o644)
        os.chmod(signers, 0o644)

        bundle = root / 'bundle'
        (bundle / 'cache/nar').mkdir(parents=True)
        cache_info = b'StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30\n'
        (bundle / 'cache/nix-cache-info').write_bytes(cache_info)
        top_hash = Path(toplevel).name.split('-', 1)[0]
        nar_rel = f'cache/nar/{top_hash}.nar.zst'
        nar_bytes = b'fake-compressed-nar-payload-' + version.encode()
        (bundle / nar_rel).write_bytes(nar_bytes)
        narinfo_rel = f'cache/{top_hash}.narinfo'
        narinfo_bytes = (
            f'StorePath: {toplevel}\n'
            f'URL: nar/{top_hash}.nar.zst\n'
            f'Compression: zstd\n'
            f'FileHash: sha256:{hashlib.sha256(nar_bytes).hexdigest()}\n'
            f'FileSize: {len(nar_bytes)}\n'
        ).encode()
        (bundle / narinfo_rel).write_bytes(narinfo_bytes)

        closure = {
            'schema': 2,
            'format': 'wros-nixos-cache-v2',
            'version': version,
            'system': 'x86_64-linux',
            'toplevel': toplevel,
            'baseline': baseline,
            'app_only': False,
            'cache_files': [
                {
                    'path': 'cache/nix-cache-info',
                    'sha256': hashlib.sha256(cache_info).hexdigest(),
                    'size': len(cache_info),
                    'store_path': None,
                },
                {
                    'path': narinfo_rel,
                    'sha256': hashlib.sha256(narinfo_bytes).hexdigest(),
                    'size': len(narinfo_bytes),
                    'store_path': toplevel,
                },
                {
                    'path': nar_rel,
                    'sha256': hashlib.sha256(nar_bytes).hexdigest(),
                    'size': len(nar_bytes),
                    'store_path': toplevel,
                },
            ],
        }
        if orphan_narinfo:
            # A package whose archive the bundle lacks (rc15's first sudo.pam).
            orphan = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-sudo.pam'
            orphan_rel = 'cache/abcdfghijklmnpqrsvwxyz0123456789.narinfo'
            orphan_bytes = f'StorePath: {orphan}\nURL: nar/missing.nar.zst\n'.encode()
            (bundle / orphan_rel).write_bytes(orphan_bytes)
            closure['cache_files'].append({
                'path': orphan_rel, 'sha256': hashlib.sha256(orphan_bytes).hexdigest(),
                'size': len(orphan_bytes), 'store_path': orphan})
        closure_raw = (json.dumps(closure, indent=2, sort_keys=True) + '\n').encode()
        (bundle / 'closure-manifest.json').write_bytes(closure_raw)
        if tamper_nar:
            (bundle / nar_rel).write_bytes(nar_bytes + b'-corrupted')

        rfc_now = format_datetime(moment, usegmt=True).replace('GMT', 'UTC')
        rfc_until = format_datetime(until, usegmt=True).replace('GMT', 'UTC')
        release = (
            f'Origin: WrOS\nLabel: WrOS NixOS updates\nSuite: wros-nixos-updates\n'
            f'Date: {rfc_now}\nValid-Until: {rfc_until}\nArchitectures: amd64\nSHA256:\n'
            f' {hashlib.sha256(closure_raw).hexdigest()} {len(closure_raw)} closure-manifest.json\n'
        )
        (bundle / 'Release').write_text(release, encoding='utf-8')
        subprocess.run(['gpg', '--batch', '--yes', '--local-user', fpr, '--clearsign',
                        '--output', str(bundle / 'InRelease'), str(bundle / 'Release')],
                       env=env, check=True)
        return bundle, keyring, signers, moment

    def test_ci_accepts_a_signed_delta_without_a_nix_store(self):
        """The Pages workflow checks a release's delta bundle on a runner with no Nix store;
        the release it updates from only has to exist on the machines being updated."""
        import tarfile
        spec = importlib.util.spec_from_file_location(
            'publish_nixos_updates', ENGINE_PATH.parents[2] / 'tools/publish-nixos-updates.py')
        publish = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(publish)
        with tempfile.TemporaryDirectory(prefix='wros-nixos-publish-') as tmp:
            root = Path(tmp)
            baseline = '/nix/store/abcdfghijklmnpqrsvwxyz0123456789-nixos-system-unnamed-0.1.0-rc14'
            now = datetime.now(timezone.utc) - timedelta(minutes=2)
            for name, tamper, orphan in (('good', False, False), ('bad', True, False),
                                         ('orphan', False, True)):
                work = root / name
                work.mkdir()
                bundle, keyring, signers, _ = self._make_signed_bundle(
                    work, version='0.1.0~rc15', moment=now, baseline=baseline, tamper_nar=tamper,
                    orphan_narinfo=orphan)
                tar = work / 'wros-nixos-updates.tar'
                with tarfile.open(tar, 'w') as archive:
                    for item in sorted(bundle.rglob('*')):
                        archive.add(item, arcname=item.relative_to(bundle).as_posix(), recursive=False)
                check = lambda: publish.check_bundle(tar, work / 'site/nixos-updates', '0.1.0~rc15',
                                                     keyring=keyring, pin=signers)
                if tamper:
                    with self.assertRaisesRegex(ValueError, 'SHA-256'):
                        check()
                elif orphan:
                    with self.assertRaisesRegex(ValueError, 'archive the bundle lacks'):
                        check()
                else:
                    self.assertEqual(check()['version'], '0.1.0~rc15')
                    self.assertTrue((work / 'site/nixos-updates/InRelease').is_file())

    def test_downgrade_and_equal_version_refused_on_fresh_install(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-ver-') as tmp:
            root = Path(tmp)
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            cur_sys = root / 'current-system'
            store_dir = root / 'nix/store'
            store_dir.mkdir(parents=True)

            # Equal version refused
            eq_root = root / 'eq'
            eq_root.mkdir()
            b_eq, kr, sig, moment = self._make_signed_bundle(eq_root, version='0.1.0~rc14')
            with self.assertRaisesRegex(ValueError, 'already installed'):
                self.engine.verify_bundle(
                    b_eq, keyring=kr, signers=sig, now=moment + timedelta(minutes=1),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir)

            # Older version (downgrade) refused on fresh install without replay.json
            old_root = root / 'old'
            old_root.mkdir()
            b_old, kr2, sig2, moment2 = self._make_signed_bundle(old_root, version='0.1.0~rc13')
            with self.assertRaisesRegex(ValueError, 'downgrade from 0.1.0~rc14 to 0.1.0~rc13 refused'):
                self.engine.verify_bundle(
                    b_old, keyring=kr2, signers=sig2, now=moment2 + timedelta(minutes=1),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir)

    def test_nar_hash_checked_before_import_and_stage_apply_rollback_gc(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-upd-') as tmp:
            root = Path(tmp)
            bad_root = root / 'bad'
            bad_root.mkdir()
            bad_bundle, bad_kr, bad_sig, moment = self._make_signed_bundle(bad_root, tamper_nar=True)
            with self.assertRaisesRegex(ValueError, 'failed SHA-256 or size check'):
                self.engine.verify_bundle(
                    bad_bundle, keyring=bad_kr, signers=bad_sig, now=moment + timedelta(minutes=1),
                    version_file=root / 'nonexistent')

            # Valid bundle v1 -> stage -> apply -> v2 -> stage -> apply -> healthy GC -> rollback to v1
            v1_root = root / 'v1'
            v1_root.mkdir()
            top1 = '/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13'
            b1, kr, sig, t1 = self._make_signed_bundle(
                v1_root, version='0.1.0~rc13', toplevel=top1,
                moment=datetime(2026, 11, 14, 10, 0, tzinfo=timezone.utc))

            state_dir = root / 'state'
            trigger = root / 'system-update'
            store_dir = root / 'nix/store'
            store_dir.mkdir(parents=True)
            gcroots = root / 'gcroots'
            profile = root / 'profile'
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc12\n', encoding='utf-8')
            cur_sys = root / 'current-system'

            rev1 = self.engine.review_bundle(
                b1, keyring=kr, signers=sig, state_dir=state_dir,
                now=t1 + timedelta(minutes=1), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir)
            self.assertEqual(rev1['status'], 'reviewed')
            self.assertEqual(rev1['packages'][0]['to_version'], '0.1.0~rc13')

            staged1 = self.engine.stage_update(
                b1, rev1['review_id'], keyring=kr, signers=sig, state_dir=state_dir,
                trigger_path=trigger, now=t1 + timedelta(minutes=2),
                allowed_prefixes=(str(root),), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots)
            self.assertEqual(staged1['status'], 'staged')
            self.assertEqual(os.readlink(gcroots / 'staged'), top1)

            commands = []
            def fake_runner(cmd):
                commands.append(cmd)
                if len(cmd) >= 4 and cmd[0] == 'nix' and 'copy' in cmd:
                    # The engine copies the staged paths, not a closure: --no-recursive.
                    self.assertIn('--no-recursive', cmd)
                    for arg in cmd:
                        if arg.startswith('/nix/store/'):
                            (store_dir / Path(arg).name).mkdir(parents=True, exist_ok=True)
                return subprocess.CompletedProcess(cmd, 0, b'', b'')

            applied1 = self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, keyring=kr, signers=sig,
                store_dir=store_dir, profile=profile, current_system=cur_sys,
                gcroots_dir=gcroots, runner=fake_runner, now=t1 + timedelta(minutes=5))
            self.assertEqual(applied1['status'], 'completed')
            # Verify staged/cache was removed before keeping installed/ so no NAR duplicates remain (U4)
            self.assertFalse((state_dir / 'installed/cache').exists())
            ver_file.write_text('0.1.0~rc13\n', encoding='utf-8')

            # Stage and apply v2 (0.1.0~rc14)
            v2_root = root / 'v2'
            v2_root.mkdir()
            top2 = '/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14'
            b2, kr2, sig2, t2 = self._make_signed_bundle(
                v2_root, version='0.1.0~rc14', toplevel=top2,
                moment=datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc))
            rev2 = self.engine.review_bundle(
                b2, keyring=kr2, signers=sig2, state_dir=state_dir,
                now=t2 + timedelta(minutes=1), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir)
            self.engine.stage_update(
                b2, rev2['review_id'], keyring=kr2, signers=sig2, state_dir=state_dir,
                trigger_path=trigger, now=t2 + timedelta(minutes=2),
                allowed_prefixes=(str(root),), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots)
            self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, keyring=kr2, signers=sig2,
                store_dir=store_dir, profile=profile, current_system=cur_sys,
                gcroots_dir=gcroots, runner=fake_runner, now=t2 + timedelta(minutes=5))
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            self.assertEqual(os.readlink(gcroots / 'previous'), top1)
            self.assertFalse((state_dir / 'previous/cache').exists())

            # Stage rollback to v1 (signed with kr/sig)
            rb = self.engine.stage_rollback(
                keyring=kr, signers=sig, state_dir=state_dir, trigger_path=trigger,
                now=t2 + timedelta(minutes=10), version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots, profile=profile)
            self.assertTrue(rb['rollback'])
            self.assertEqual(rb['toplevel'], top1)

    def test_going_back_uses_previous_system_generation(self):
        """NixOS keeps the previous generation, so even the first update can go back, by
        nix-env --switch-generation, with no kept update tree."""
        with tempfile.TemporaryDirectory(prefix='wros-nixos-gen-') as tmp:
            root = Path(tmp)
            store_dir = root / 'nix/store'
            top1 = '/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13'
            top2 = '/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14'
            for top, ver in ((top1, '0.1.0~rc13'), (top2, '0.1.0~rc14')):
                etc = store_dir / Path(top).name / 'etc/wros'
                etc.mkdir(parents=True)
                (etc / 'version').write_text(ver + '\n', encoding='utf-8')
            profiles = root / 'profiles'
            profiles.mkdir()
            os.symlink(top1, profiles / 'system-1-link')
            os.symlink(top2, profiles / 'system-2-link')
            os.symlink('system-2-link', profiles / 'system')
            profile = profiles / 'system'
            ver_file = root / 'version'
            ver_file.write_text('0.1.0~rc14\n', encoding='utf-8')
            state_dir, trigger = root / 'state', root / 'system-update'
            gcroots, cur_sys = root / 'gcroots', root / 'current-system'
            moment = datetime(2026, 11, 14, 12, 0, tzinfo=timezone.utc)

            rb = self.engine.stage_rollback(
                state_dir=state_dir, trigger_path=trigger, now=moment, version_file=ver_file,
                current_system=cur_sys, store_dir=store_dir, gcroots_dir=gcroots, profile=profile)
            self.assertEqual((rb['status'], rb['rollback'], rb['toplevel']), ('staged', True, top1))
            self.assertTrue(trigger.is_symlink())

            commands = []

            def fake_runner(cmd):
                commands.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, '', '')

            applied = self.engine.execute_offline_boot_transaction(
                state_dir=state_dir, trigger_path=trigger, store_dir=store_dir, profile=profile,
                current_system=cur_sys, gcroots_dir=gcroots, runner=fake_runner,
                now=moment + timedelta(minutes=5))
            self.assertEqual(applied['status'], 'completed')
            self.assertIn(['nix-env', '-p', str(profile), '--switch-generation', '1'], commands)
            self.assertFalse(any('--set' in cmd for cmd in commands))
            self.assertFalse(any(cmd[0] == 'nix' for cmd in commands))  # nothing to import

            # Back on generation 1 there is nothing older: going back again is refused.
            os.unlink(profile)
            os.symlink('system-1-link', profile)
            ver_file.write_text('0.1.0~rc13\n', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'no previous version'):
                self.engine.stage_rollback(
                    state_dir=state_dir, trigger_path=trigger, now=moment + timedelta(minutes=10),
                    version_file=ver_file, current_system=cur_sys, store_dir=store_dir,
                    gcroots_dir=gcroots, profile=profile)

    def test_same_version_generation_is_not_a_previous_version_and_repair_adds_none(self):
        with tempfile.TemporaryDirectory(prefix='wros-nixos-gen2-') as tmp:
            root = Path(tmp)
            store_dir = root / 'nix/store'
            tops = {
                1: ('/nix/store/0123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc13', '0.1.0~rc13'),
                2: ('/nix/store/1123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14', '0.1.0~rc14'),
                3: ('/nix/store/2123456789abcdfghijklmnpqrsvwxyz-nixos-system-wros-0.1.0-rc14', '0.1.0~rc14'),
            }
            profiles = root / 'profiles'
            profiles.mkdir()
            for number, (top, ver) in tops.items():
                etc = store_dir / Path(top).name / 'etc/wros'
                etc.mkdir(parents=True)
                (etc / 'version').write_text(ver + '\n', encoding='utf-8')
                os.symlink(top, profiles / f'system-{number}-link')
            os.symlink('system-3-link', profiles / 'system')
            profile = profiles / 'system'
            # Generation 2 carries the running version; going back skips it to generation 1.
            self.assertEqual(self.engine.previous_generation(profile, store_dir=store_dir),
                             (1, tops[1][0], '0.1.0~rc13'))

            commands = []

            def fake_runner(cmd):
                commands.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, '', '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=fake_runner)
            self.assertEqual(res['status'], 'recovered', res)
            self.assertIn([tops[3][0] + '/bin/switch-to-configuration', 'boot'], commands)
            self.assertFalse(any(cmd[:1] == ['nix-env'] for cmd in commands), commands)

            # The contents checked are the closure of what will boot, not the whole store: a path
            # only an older generation keeps must not fail the repair.
            commands.clear()
            closure = [tops[3][0], '/nix/store/3123456789abcdfghijklmnpqrsvwxyz-wros-helpers']

            def closure_runner(cmd):
                commands.append(cmd)
                out = '\n'.join(closure) + '\n' if cmd[:3] == ['nix-store', '--query', '--requisites'] else ''
                return subprocess.CompletedProcess(cmd, 0, out, '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=closure_runner)
            self.assertEqual(res['status'], 'recovered', res)
            self.assertIn(['nix-store', '--verify-path', *closure], commands)
            self.assertNotIn(['nix-store', '--verify', '--check-contents'], commands)

            def failing_runner(cmd):
                failed = cmd[:2] == ['nix-store', '--verify-path']
                return subprocess.CompletedProcess(cmd, 1 if failed else 0, '',
                                                   "path '/nix/store/x' was modified!" if failed else '')

            res = self.engine.repair_updates(
                state_dir=root / 'state', trigger_path=root / 'system-update', profile=profile,
                current_system=root / 'current-system', gcroots_dir=root / 'gcroots',
                runner=failing_runner)
            self.assertEqual(res['status'], 'failed', res)
            self.assertIn('was modified', res['message'])


if __name__ == '__main__':
    unittest.main()
