"""QMP event handling and the serial-shell marker protocol, against local fakes."""
import json
import errno
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rc_vm as vm
import rc_vm_phases as phases


class InstallerReceipt(unittest.TestCase):
    def test_vm_receipt_keeps_the_release_version_and_the_boot_binary_location(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            iso = base / 'candidate.iso'
            iso.write_bytes(b'ISO fixture')
            boot_hash = 'b' * 64
            digests = {name: {'squashfs_sha256': 'a' * 64}
                       for name in ('grimoire', 'wros-greeter', 'wros-installer')}
            digests['wros-boot'] = {'squashfs_sha256': boot_hash}
            iso.with_suffix('.receipt.json').write_text(json.dumps({
                'version': '0.1.0~rc11', 'iso': {'sha256': vm.sha256_file(iso)}, 'binaries': digests,
            }))
            with mock.patch.object(phases, 'BASE', base / 'runs'):
                ctx = phases.Ctx('install', SimpleNamespace(iso=str(iso)))
                try:
                    self.assertEqual(ctx.result['release_version'], '0.1.0~rc11')
                    self.assertEqual(ctx.result['receipt_iso_sha256'], vm.sha256_file(iso))
                    hashes = phases.receipt_binary_hashes(ctx.result['receipt_binaries_sha256'])
                    self.assertEqual(hashes['/usr/lib/wros/wros-boot'], boot_hash)
                    self.assertNotIn('/usr/bin/wros-boot', hashes)
                    self.assertEqual(len(hashes), 4)
                    self.assertEqual(phases.receipt_binary_hashes(digests | {'wros-boot': {}}), {})
                finally:
                    ctx.log_file.close()


class FirmwareShutdown(unittest.TestCase):
    def test_poweroff_may_lose_its_serial_marker_only_after_a_clean_vm_exit(self):
        for error in (RuntimeError('guest stopped'), TimeoutError('missing serial marker')):
            for running, exit_code in ((False, 0), (False, 1), (True, None)):
                with self.subTest(error=type(error).__name__, running=running, exit_code=exit_code):
                    guest = phases.Guest.__new__(phases.Guest)
                    guest.shell = mock.Mock()
                    guest.shell.run.side_effect = error
                    guest.m = mock.Mock()
                    guest.m.is_running.return_value = running
                    guest.m.wait_exit.return_value = exit_code
                    if not running and exit_code == 0:
                        self.assertEqual(guest.run('sudo -n /sbin/poweroff'), (0, ''))
                        with self.assertRaises(type(error)):
                            guest.run('echo still running')
                    else:
                        with self.assertRaises(type(error)):
                            guest.run('sudo -n /sbin/poweroff')

    def test_nonzero_exit_or_timeout_cannot_pass_firmware_boot(self):
        with tempfile.TemporaryDirectory() as directory:
            with (mock.patch.object(phases, 'BASE', Path(directory)),
                  mock.patch.object(phases, 'boot_live') as boot,
                  mock.patch.object(phases, 'live_login'),
                  mock.patch.object(phases, 'assert_live_state')):
                for name, outcome, error in (('ok', 0, None), ('failed', 1, phases.Failure),
                                              ('timeout', TimeoutError('QEMU did not exit'), TimeoutError)):
                    with self.subTest(name=name):
                        ctx = phases.Ctx(name, SimpleNamespace(secboot=False))
                        boot.return_value.wait_exit.side_effect = (
                            outcome if isinstance(outcome, Exception) else None)
                        boot.return_value.wait_exit.return_value = outcome
                        try:
                            if error is None:
                                phases.phase_firmware(ctx)
                                self.assertTrue(ctx.result['passed'])
                            else:
                                with self.assertRaises(error):
                                    phases.phase_firmware(ctx)
                                self.assertFalse(ctx.result['passed'])
                        finally:
                            ctx.log_file.close()


class InstallerCompletion(unittest.TestCase):
    def test_waits_for_cleanup_to_finish_and_preserves_a_failed_final_verdict(self):
        for clean in (True, False):
            with self.subTest(clean=clean):
                ctx = SimpleNamespace(args=SimpleNamespace(layout='colemak'), result={}, guard=lambda: None)
                guest = mock.Mock()
                snapshots = [
                    {'active_vt': 'tty7', 'installer_unit': state,
                     'run_files': {'outcome.json': {'outcome': 'completed', 'host_cleanup_verified': False}}}
                    for state in ('active', 'deactivating')
                ]
                final = {'active_vt': 'tty1', 'installer_unit': 'inactive' if clean else 'failed',
                         'run_files': {'outcome.json': {'outcome': 'completed', 'host_cleanup_verified': clean}}}
                guest.snap.side_effect = [*snapshots, final]
                with mock.patch.object(phases.time, 'sleep'):
                    self.assertIs(phases.run_installer(ctx, guest, {}), final)
                self.assertEqual(guest.snap.call_count, 3)
                self.assertEqual(ctx.result['installer_steps'], [])


class FirmwareMenu(unittest.TestCase):
    def test_background_or_partial_menu_is_not_ready_for_enter(self):
        from PIL import Image, ImageDraw
        for rows, footer in ((False, False), (True, False), (False, True), (True, True)):
            with self.subTest(rows=rows, footer=footer):
                picture = Image.new('RGB', (800, 600))
                draw = ImageDraw.Draw(picture)
                draw.rectangle((40, 40, 200, 200), fill='white')
                if rows:
                    draw.rectangle((80, 320, 500, 330), fill='white')
                if footer:
                    draw.rectangle((240, 556, 600, 566), fill='white')
                self.assertEqual(phases.firmware_menu_ready(picture), rows and footer)


class OfflineInstallNetwork(unittest.TestCase):
    def test_optional_network_attempts_are_allowed_but_isolated_install_must_complete(self):
        for isolated, outcome in ((True, 'completed'), (False, 'completed'), (True, 'failed')):
            with self.subTest(isolated=isolated, outcome=outcome):
                ctx = mock.Mock()
                ctx.dir = Path('/synthetic-capture')
                ctx.result = {'network_namespace_isolated': isolated, 'outcome': {'outcome': outcome}}
                def check(name, condition, detail=None):
                    if not condition:
                        raise phases.Failure(name)
                ctx.check.side_effect = check
                attempts = 'DNS attempt to a time server\nDNS attempt to the update repository\n'
                with mock.patch.object(phases.subprocess, 'check_output', side_effect=['DHCP request\n', attempts]):
                    if isolated and outcome == 'completed':
                        phases.check_install_network_capture(ctx)
                        self.assertEqual(ctx.result['installer_ipv4_attempts'], attempts.splitlines())
                    else:
                        with self.assertRaises(phases.Failure):
                            phases.check_install_network_capture(ctx)


class GreeterRetry(unittest.TestCase):
    def test_credentials_wait_for_the_reset_form_or_a_requested_restart(self):
        ready = 'WrOS · Sign in\nLocal account\nPassword\nEnter: continue  Up/Down: accounts'
        password = 'WrOS · Sign in\nLocal account\nwriter\nPassword\n[hidden]\nEnter: sign in'
        rejected = ('WrOS · Sign in\nLocal account\nwriter\nPassword\n'
                    'Login was not accepted. Try again\nEnter: sign in')
        restart = 'WrOS · Sign in\nEnter to restart login\nLogin was not accepted. Try again'
        for needs_restart in (False, True):
            with self.subTest(needs_restart=needs_restart):
                # Use the actual screen polling so a label on the pending
                # rejection frame cannot masquerade as a ready account form.
                frames = iter([ready, password, rejected, rejected,
                               *([restart, ready] if needs_restart else [ready]),
                               password, 'WrOS\nResume writing'])
                current = ['']
                guest = phases.Guest.__new__(phases.Guest)
                guest.ctx = mock.Mock()
                def screen(n):
                    current[0] = next(frames)
                    return current[0]
                guest.screen = screen
                machine = mock.Mock()
                guest.m = machine
                def type_text(text, layout):
                    expected = ready if text == 'writer\n' else password
                    self.assertEqual(current[0], expected, 'credentials sent before the form was ready')
                machine.type_text.side_effect = type_text
                with mock.patch.object(phases.time, 'sleep'):
                    phases.greeter_login(guest.ctx, guest, machine, 'writer', 'synthetic-password', 'colemak')
                self.assertEqual(machine.type_text.call_args_list, [
                    mock.call('writer\n', 'colemak'), mock.call('synthetic-password\n', 'us'),
                    mock.call('writer\n', 'colemak'), mock.call('synthetic-password\n', 'colemak')])
                self.assertEqual(machine.key.call_args_list, [mock.call('ret')] if needs_restart else [])
                guest.ctx.check.assert_any_call('correct greeter password accepted', True)


class SerialLoginDeadline(unittest.TestCase):
    def test_delayed_ready_marker_uses_the_requested_budget_without_waiting_forever(self):
        for ready_delay, succeeds in ((80, True), (130, False)):
            with self.subTest(ready_delay=ready_delay):
                clock = [1000.0]
                delays = iter((10, 2, ready_delay))
                serial = mock.Mock()
                def expect(pattern, timeout):
                    delay = next(delays)
                    clock[0] += min(delay, timeout)
                    if delay > timeout:
                        raise TimeoutError('synthetic delayed login marker')
                serial.expect.side_effect = expect
                def sleep(seconds):
                    clock[0] += seconds
                with (mock.patch.object(vm.time, 'monotonic', side_effect=lambda: clock[0]),
                      mock.patch.object(vm.time, 'sleep', side_effect=sleep)):
                    shell = vm.Shell(serial)
                    if succeeds:
                        shell.login('writer', 'synthetic-password', timeout=120)
                        self.assertLess(clock[0], 1120)
                    else:
                        with self.assertRaises(TimeoutError):
                            shell.login('writer', 'synthetic-password', timeout=120)
                        self.assertEqual(clock[0], 1120)
                self.assertEqual(serial.send.call_count, 4)


class SparseDiskDigest(unittest.TestCase):
    def test_unavailable_extent_queries_cannot_report_an_unchanged_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'target'
            path.write_bytes(b'actual target data')
            for code in (errno.EINVAL, errno.EIO):
                with self.subTest(errno=code), mock.patch.object(vm.os, 'lseek', side_effect=OSError(code, 'extent query failed')):
                    with self.assertRaises(OSError) as caught:
                        vm.sparse_digest(path)
                    self.assertEqual(caught.exception.errno, code)
            path.write_bytes(b'\0' * 4096)
            expected = vm.sparse_digest(path)
            with mock.patch.object(vm.os, 'lseek', side_effect=OSError(errno.ENXIO, 'no more data')):
                self.assertEqual(vm.sparse_digest(path), expected)


class MachineStartup(unittest.TestCase):
    def test_slow_socket_creation_is_bounded_and_child_failure_is_immediate(self):
        for ready_delay, exit_code in ((30, None), (None, None), (None, 1)):
            with self.subTest(ready_delay=ready_delay, exit_code=exit_code), tempfile.TemporaryDirectory() as directory:
                machine = vm.Machine(directory, 'startup')
                clock = [1000.0]
                child = mock.Mock()
                child.poll.return_value = exit_code
                exists = Path.exists
                def ready(path):
                    if path in (machine.qmp_sock, machine.serial_sock):
                        return ready_delay is not None and clock[0] >= 1000 + ready_delay
                    return exists(path)
                def sleep(seconds):
                    clock[0] += seconds
                try:
                    with (mock.patch.object(vm.os, 'geteuid', return_value=1000),
                          mock.patch.object(vm.subprocess, 'Popen', return_value=child),
                          mock.patch.object(Path, 'exists', autospec=True, side_effect=ready),
                          mock.patch.object(vm, 'Serial'),
                          mock.patch.object(vm.time, 'monotonic', side_effect=lambda: clock[0]),
                          mock.patch.object(vm.time, 'sleep', side_effect=sleep)):
                        if ready_delay is not None:
                            self.assertIs(machine.start(timeout=60), machine)
                            self.assertGreaterEqual(clock[0], 1030)
                            machine.serial.connect.assert_called_once()
                        else:
                            with self.assertRaisesRegex(RuntimeError, 'exit=%r' % exit_code):
                                machine.start(timeout=60)
                            if exit_code is not None:
                                self.assertEqual(clock[0], 1000)
                            else:
                                self.assertLess(clock[0], 1060.2)
                finally:
                    if machine.log:
                        machine.log.close()


class NativeScreenshot(unittest.TestCase):
    def test_antialias_edges_are_normalized_to_the_foreground_contrast(self):
        from PIL import Image
        atlas = Image.new('RGB', (176, 360))
        index = ord('r') - 32
        x, y = index % 16 * 11, index // 16 * 20
        points = [(i % 11, i // 11, 255 if i < 4 else 50) for i in range(14)]
        for dx, dy, value in points:
            atlas.putpixel((x + dx, y + dy), (value,) * 3)
        font = vm.NativeFont(atlas, 11, 20)
        screen = Image.new('RGB', (11, 20))
        for dx, dy, value in points:
            screen.putpixel((dx, dy), (value * 3 // 4,) * 3)
        self.assertEqual(vm.ocr(screen, font), ['r'])

    def test_the_native_font_reads_the_same_ascii_and_selection_marker(self):
        from PIL import Image
        atlas = Image.new('RGB', (64, 108))
        for index, points in ((ord('A') - 32, [(0, 0), (1, 1), (2, 2), (3, 3)]),
                              (95, [(1, 0), (2, 1), (3, 2), (2, 3), (1, 4)])):
            for style in range(3):
                x, y = index % 16 * 4, (index // 16 + style * 6) * 6
                for dx, dy in points:
                    atlas.putpixel((x + dx, y + dy), (255, 255, 255))
        font = vm.NativeFont(atlas, 4, 6)
        screen = font.render('A ▸', 3, 1)
        self.assertEqual(vm.ocr(screen, font), ['A ▸'])
        selected = font.render('A ▸', 3, 1, bg=(30, 50, 130))
        canvas = Image.new('RGB', (16, 6))
        canvas.paste(selected, (0, 0))
        self.assertEqual(vm.ocr(canvas, font), ['A ▸'])


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


class ConsoleMap(unittest.TestCase):
    def test_mismatch_report(self):
        import rc_vm_phases as ph
        good = {str(code): ph.SYMBOL_NAMES.get(vm.LAYOUT_KEYS['colemak_dh'][q][0], vm.LAYOUT_KEYS['colemak_dh'][q][0])
                for code, q in ph.KEYCODE_QCODES.items()}
        self.assertEqual(ph.console_map_mismatches(good, 'colemak_dh'), [])
        bad = dict(good, **{'44': 'z'})            # qwerty z key printing z (no angle mod)
        report = ph.console_map_mismatches(bad, 'colemak_dh')
        self.assertEqual(len(report), 1)
        self.assertIn('keycode 44', report[0])
