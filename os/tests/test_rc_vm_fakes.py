"""QMP event handling and the serial-shell marker protocol, against local fakes."""
import json
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rc_vm as vm


@unittest.skipUnless(sys.platform == 'linux', 'needs unix sockets and bash')
class FakeGuest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_qmp_persistent_connection_collects_events(self):
        path = self.dir / 'qmp.sock'
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(path))
        server.listen(1)

        def serve():
            conn, _ = server.accept()
            f = conn.makefile('rwb', buffering=0)
            f.write(b'{"QMP":{}}\n')
            while True:
                line = f.readline()
                if not line:
                    break
                cmd = json.loads(line)['execute']
                if cmd == 'system_reset':
                    f.write(b'{"event":"RESET","data":{}}\n')
                f.write(b'{"return":{}}\n')
        threading.Thread(target=serve, daemon=True).start()
        q = vm.Qmp(path)
        q.command('system_reset')
        q.send_key(['a'])
        self.assertEqual(q.count('RESET'), 1)

    def test_shell_run_and_put(self):
        path = self.dir / 'serial.sock'
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(path))
        server.listen(1)
        proc = subprocess.Popen(['bash', '--noediting', '--norc', '--noprofile'], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=self.dir)

        def pump(conn):
            for chunk in iter(lambda: proc.stdout.read1(4096), b''):
                conn.sendall(chunk)

        def serve():
            conn, _ = server.accept()
            threading.Thread(target=pump, args=(conn,), daemon=True).start()
            while True:
                data = conn.recv(4096)
                if not data:
                    break
                proc.stdin.write(data)
                proc.stdin.flush()
        threading.Thread(target=serve, daemon=True).start()
        serial = vm.Serial(path, self.dir / 'transcript', lambda: True)
        serial.connect()
        shell = vm.Shell(serial)
        rc, out = shell.run('echo hello; false', check=False)
        self.assertEqual((rc, out.strip()), (1, 'hello'))
        rc, out = shell.run('printf "a\\nb"')
        self.assertEqual(out.strip(), 'a\nb')
        blob = bytes(range(256)) * 20
        shell.put(str(self.dir / 'blob.bin'), blob, '0644')
        self.assertEqual((self.dir / 'blob.bin').read_bytes(), blob)
        with self.assertRaises(RuntimeError):
            shell.run('false')
        serial.close()
        proc.kill()


if __name__ == '__main__':
    unittest.main()


class SecretVariants(unittest.TestCase):
    def test_variants_cover_layout_misreads(self):
        import rc_vm_phases as ph
        variants = ph.secret_variants({'luks': 'abcdef0123'}, 'dvorak')['luks']
        self.assertIn('abcdef0123', variants)
        # the same physical keys read as US QWERTY give a different string that must also be scanned for
        self.assertTrue(any(v != 'abcdef0123' for v in variants))
        self.assertIn(''.join(vm.US_KEYS[c[0]][0] for c in vm.keystrokes('abcdef0123', 'dvorak')), variants)
