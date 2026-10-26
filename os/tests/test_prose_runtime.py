"""Real pinned artifact repro, kernel denials, CLI and parent-death contracts.

No network requests, root app, credentials, devices or user manuscripts. Socket
creation probes do not transmit packets. A syscall boundary is not a complete
filesystem/malicious-code sandbox or independent security qualification.
"""
import ctypes
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

from test_secondary_pty import BINARY, ROOT
from test_prose_pty import RUNTIME

HELPER = ROOT/'crates/grimoire-core/assets/writingdeck/prose_tools.py'


def provisioner():
    spec = importlib.util.spec_from_file_location('writingdeck_prose_provisioner', ROOT/'tools/prepare-prose-tools.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def process_state(pid):
    try:
        return (Path('/proc')/str(pid)/'stat').read_text().rsplit(')', 1)[1].split()[0]
    except FileNotFoundError:
        return None


@unittest.skipIf(os.geteuid() == 0, 'real runtime/app probes must be non-root')
class ProseRuntime(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BINARY.is_file() or not RUNTIME.is_file():
            raise AssertionError('build binary and exact runtime; missing genuine coverage must not silently pass')

    def test_exact_two_git_source_repacks_reproduce_locked_wheels_and_runtime_payload(self):
        p = provisioner()
        manifest = p.locked_manifest()
        self.assertEqual(len(manifest['packages']), 49)
        record = p.verify_runtime(RUNTIME.parent.parent)
        self.assertEqual(len(record['packages']), 49)
        with tempfile.TemporaryDirectory(prefix='wd-source-repack-') as tmp:
            for package in manifest['packages']:
                if source := package['source_repack']:
                    archive = ROOT/'build/prose-runtime/sources'/source['archive_filename']
                    self.assertTrue(archive.is_file(), 'retain the exact Git archives with the provisioned runtime')
                    self.assertEqual(p.digest(archive), source['sha256'])
                    output = Path(tmp)/package['filename']
                    p.source_wheel(package, archive, output)
                    self.assertEqual(p.digest(output), package['sha256'])
            unknown = Path(tmp)/'unknown.whl'
            unknown.write_bytes(b'opaque original must survive')
            with self.assertRaises(ValueError):
                p.artifact({'sha256': '0'*64}, unknown, [], True)
            self.assertEqual(unknown.read_bytes(), b'opaque original must survive')
        self.assertFalse((RUNTIME.parent/'pip').exists(), 'no downloader CLI in the runtime')

    def test_fresh_materialization_is_reproducible_and_forged_or_failed_outputs_are_not_trusted(self):
        p = provisioner()
        manifest = p.locked_manifest()
        with tempfile.TemporaryDirectory(prefix='wd-real-materialization-') as tmp:
            outputs = [Path(tmp)/name for name in ('first', 'second', 'failed')]
            for output in outputs:
                output.mkdir()
                (output/'wheels').symlink_to(ROOT/'build/prose-runtime/wheels')
            for output in outputs[:2]:
                p.install_runtime(output, manifest, Path('/usr/bin/python3'))
            first, second = [p.verify_runtime(out/'runtime') for out in outputs[:2]]
            self.assertEqual(first, second, 'temporary build paths must not affect payloads')
            # Reuse validates, rather than overwriting or trusting the marker.
            p.install_runtime(outputs[0], manifest, Path('/usr/bin/python3'))
            root = outputs[0]/'runtime'
            victim = root/'lib/python3.13/site-packages/proselint/__init__.py'
            victim.write_bytes(victim.read_bytes()+b'\n# altered fixture\n')
            forged = first | {'files': p.payload_files(root)}
            marker = root/p.MARKER
            marker.write_text(json.dumps(forged))
            before = marker.read_bytes()
            with self.assertRaisesRegex(ValueError, 'exact locked wheel payload'):
                p.install_runtime(outputs[0], manifest, Path('/usr/bin/python3'))
            self.assertEqual(marker.read_bytes(), before)
            with patch.object(p, 'verify_runtime', side_effect=ValueError('injected final audit failure')):
                with self.assertRaisesRegex(ValueError, 'injected final audit failure'):
                    p.install_runtime(outputs[2], manifest, Path('/usr/bin/python3'))
            self.assertFalse((outputs[2]/'runtime').exists(), 'unverified output must not be published')
            self.assertEqual([f.name for f in outputs[2].iterdir()], ['wheels'])

    def test_kernel_blocks_libc_socket_exec_clone_io_uring_and_x32_not_merely_python_monkeypatches(self):
        # Establish that socket creation itself is permitted in the outer
        # environment. Creating/closing an unconnected socket sends no packet.
        libc = ctypes.CDLL(None, use_errno=True)
        fd = libc.socket(2, 1, 0)
        self.assertGreaterEqual(fd, 0, 'outer environment must permit the baseline socket probe')
        libc.close(fd)
        code = r'''
import ctypes,json,os
scope={'__name__':'writingdeck_kernel_probe'}
exec(compile(open(HELPER).read(), HELPER, 'exec'),scope)
scope['configure_process']()
libc=ctypes.CDLL(None,use_errno=True)
checks={}
for key,n,args in [
    ('socket',41,(2,1,0)),
    ('exec_missing',59,(ctypes.c_char_p(b'/no-such-offline-worker-command'),ctypes.c_void_p(0),ctypes.c_void_p(0))),
    ('clone3_null',435,(ctypes.c_void_p(0),0)),
    ('io_uring_null',425,(1,ctypes.c_void_p(0))),
    ('x32_socket',0x40000000|41,(2,1,0)),
]:
    ctypes.set_errno(0)
    value=libc.syscall(n,*args)
    error=ctypes.get_errno()
    if key=='socket' and value>=0:libc.close(value)
    checks[key]={'value':value,'errno':error}
checks['no_new_privs']=libc.prctl(39,0,0,0,0)
print(json.dumps(checks))
'''
        code = 'HELPER=' + repr(str(HELPER)) + '\n' + code
        result = subprocess.run([str(RUNTIME), '-I', '-B', '-c', code], capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        checks = json.loads(result.stdout)
        self.assertEqual(checks.pop('no_new_privs'), 1)
        for name, value in checks.items():
            self.assertEqual(value, {'value': -1, 'errno': errno.EPERM}, name)

    def test_real_rust_python_cli_permission_no_literal_rewrite_and_missing_engine_failure_are_read_only(self):
        with tempfile.TemporaryDirectory(prefix='wd-prose-cli-real-') as tmp:
            root = Path(tmp)
            path = root/'synthetic.md'
            path.write_text('---\ntitle: untouched\n---\n\nCafé 🦊 e\u0301. The gate was opened by Mara. He could of gone.\n')
            before = path.read_bytes()
            env = os.environ | {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME), 'HOME': str(root)}
            denied = subprocess.run([str(BINARY), 'writingdeck', 'prose', str(path), '--engine', 'nabokov'],
                                    env=env, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(denied.returncode, 0)
            self.assertIn('--allow-model', denied.stderr)
            allowed = subprocess.run([str(BINARY), 'writingdeck', 'prose', str(path), '--engine', 'nabokov', '--allow-model'],
                                     env=env, capture_output=True, text=True, timeout=60)
            self.assertEqual(allowed.returncode, 0, allowed.stderr)
            report = json.loads(allowed.stdout)
            self.assertEqual(report['status'], 'complete')
            finding = next(d for d in report['findings'] if d['rule'] == 'NB302')
            self.assertEqual(finding['expected'], 'was opened by Mara')
            self.assertEqual(finding['alternatives'], [])
            self.assertFalse(any(d['rule'].startswith(('NB5', 'NB7')) for d in report['findings']))
            env['WRITINGDECK_TOOLS_PYTHON'] = '/no-such-pinned-runtime'
            missing = subprocess.run([str(BINARY), 'writingdeck', 'prose', str(path), '--engine', 'proselint'],
                                     env=env, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(missing.returncode, 0)
            self.assertEqual(json.loads(missing.stdout)['status'], 'unavailable')
            self.assertEqual(path.read_bytes(), before)
            self.assertEqual(sorted(p.name for p in root.iterdir()), ['synthetic.md'])

    def test_actual_rust_owner_sigkill_does_not_leave_its_bound_python_model_process_running(self):
        with tempfile.TemporaryDirectory(prefix='wd-prose-parent-death-') as tmp:
            root = Path(tmp)
            source = root/'synthetic.md'
            source.write_text('The gate was opened by Mara. '*1800)
            before = hashlib.sha256(source.read_bytes()).hexdigest()
            env = os.environ | {'WRITINGDECK_TOOLS_PYTHON': str(RUNTIME), 'HOME': str(root)}
            owner = subprocess.Popen([str(BINARY), 'writingdeck', 'prose', str(source), '--engine', 'nabokov', '--allow-model'],
                                     env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            child = None
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    candidates = (Path('/proc')/str(owner.pid)/'task'/str(owner.pid)/'children').read_text().split()
                    for candidate in candidates:
                        status = (Path('/proc')/candidate/'status').read_text()
                        if 'NoNewPrivs:\t1' in status:
                            rss = next((int(line.split()[1]) for line in status.splitlines() if line.startswith('VmRSS:')), 0)
                            if rss >= 40_000:
                                child = int(candidate)
                                break
                    if child is not None:
                        break
                    self.assertIsNone(owner.poll(), 'owner finished before the genuine bound subprocess could be inspected')
                    time.sleep(0.005)
                self.assertIsNotNone(child, 'observe actual kernel-bound Python, not a mock or unrelated child')
                self.assertNotIn(process_state(child), (None, 'Z', 'X'))
                owner.kill()  # ONLY the Rust process, not its process group.
                owner.wait(timeout=5)
                deadline = time.monotonic() + 3
                while process_state(child) not in (None, 'Z', 'X') and time.monotonic() < deadline:
                    time.sleep(0.005)
                self.assertIn(process_state(child), (None, 'Z', 'X'), 'Python must not continue after its bound owner is SIGKILLed')
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), before)
            finally:
                if owner.poll() is None:
                    owner.kill(); owner.wait(timeout=5)
                if child is not None and process_state(child) not in (None, 'Z', 'X'):
                    os.kill(child, signal.SIGKILL)
