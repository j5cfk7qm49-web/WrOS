"""The NixOS installer's session, screens backend and install core, as found installing rc14 on
the HP Stream (#132-#139). Pure tests: no disk, terminal or namespace is touched."""
import importlib.machinery
import importlib.util
import io
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    loader = importlib.machinery.SourceFileLoader(name.replace('-', '_'), str(ROOT / 'nixos/scripts' / name))
    module = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(module)
    return module


session = load('wros-install-session')
backend = load('wros-installer-backend')
core = load('wros-install-core')
IDENTITY = {'dev': '179:0', 'size': '61071360', 'diskseq': '3'}


class Session(unittest.TestCase):
    def test_the_worker_has_no_private_ipc_namespace(self):
        # cryptsetup waits for udev on a System V semaphore; in a private IPC namespace udev
        # can never release it, and opening the new disk hung for ten minutes (#136).
        command = session.worker_command('unit', Path('/x/wros-install-session'), 1000, 42, '7')
        self.assertIn('unshare', command)
        self.assertNotIn('--ipc', command)
        self.assertIn('--mount', command)
        self.assertNotIn('--ipc', session.UNSHARE)
        unshare = command.index('unshare')
        self.assertEqual(command[unshare + len(session.UNSHARE):],
                         ['/x/wros-install-session', 'worker', '1000', '42', '7'])

    def test_the_run_folder_lets_nixos_install_through(self):
        # nixos-install refuses a target under a folder others cannot enter (#137).
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp) / 'wros-installer'
            run.mkdir(mode=0o700)
            (run / 'baseline.json').write_text('{}')
            (run / 'baseline.json').chmod(0o600)
            session.prepare_run_dir(run)
            self.assertEqual(stat.S_IMODE(run.stat().st_mode), 0o755)
            self.assertEqual(stat.S_IMODE((run / 'baseline.json').stat().st_mode), 0o600)
            fresh = Path(tmp) / 'fresh'
            session.prepare_run_dir(fresh)
            self.assertEqual(stat.S_IMODE(fresh.stat().st_mode), 0o755)

    def test_a_new_attempt_forgets_the_last_result(self):
        # A leftover failure marker stopped every later attempt at once (#138).
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            for name in session.RUN_MARKERS + ('baseline.json', 'owner'):
                (run / name).write_text('x')
            session.clear_run_markers(run)
            self.assertEqual(sorted(p.name for p in run.iterdir()), ['baseline.json', 'owner'])
            self.assertIn('wros-target-failed', session.RUN_MARKERS)

    def test_the_backend_reason_reaches_the_outcome(self):
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / 'wros-target-failed'
            marker.write_text('cryptsetup failed (1): No key available\n')
            self.assertEqual(session.failure_reason(marker),
                             'installation failed: cryptsetup failed (1): No key available')
            marker.write_text('x' * 5000)
            self.assertLessEqual(len(session.failure_reason(marker)), 450)
            marker.write_text('')
            self.assertEqual(session.failure_reason(marker), 'installer finish step failed')

            class Proc:
                def poll(self):
                    return None
            marker.write_text('nixos-install failed (1): path should have permissions 755\n')
            outcome, reason, proof = session.supervise_installer(
                Proc(), Proc(), lambda: False, {}, di_root=Path(tmp), verify=lambda *a: None)
            self.assertEqual(outcome, 'failed')
            self.assertIn('permissions 755', reason)

    def test_the_backend_log_goes_to_the_journal(self):
        source = (ROOT / 'nixos/scripts/wros-install-session').read_text()
        block = source.split('backend = subprocess.Popen(', 1)[1].split('start_new_session', 1)[0]
        self.assertIn('stderr=None', block)
        self.assertNotIn('stderr=subprocess.DEVNULL', block)


class Channel:
    """The installer screens, answering from a script of answers per question."""

    def __init__(self, answers):
        self.answers = {tag: list(values) for tag, values in answers.items()}
        self.asked = []
        self.sent = []
        self.defined = {}

    def send(self, command):
        self.sent.append(command)
        return 0, ''

    def define(self, tag, kind, description, detail, **kwargs):
        self.defined[tag] = (description, detail)

    def ask(self, tag, kind, description, detail, choices=None, values=None, default=None):
        self.asked.append((tag, detail))
        queue = self.answers.get(tag)
        if queue:
            return queue.pop(0)
        return default or ''


GOOD = {'wros/keyboard_layout': ['colemak'], 'wros/user_password': ['writer-secret'],
        'wros/luks_passphrase': ['disk-secret-1'], 'wros/confirm_erase': ['true']}


class Dialog(unittest.TestCase):
    def run_dialog(self, answers, core_runner=None):
        channel = Channel({**GOOD, **answers})
        layouts = []
        payloads = []
        runner = core_runner or (lambda payload: payloads.append(payload) or {'passed': True})
        backend.run_dialog(channel, 'mmcblk0', '0' * 32, core_runner=runner,
                           target_identity=dict(IDENTITY), layout_runner=layouts.append)
        return channel, layouts, payloads

    def test_a_bad_hostname_or_username_is_asked_again_before_anything_is_erased(self):
        channel, _, payloads = self.run_dialog({
            'wros/hostname': ['My Laptop', 'hp-stream'],
            'wros/username': ['Alan', 'root', 'alan'],
        })
        hosts = [detail for tag, detail in channel.asked if tag == 'wros/hostname']
        users = [detail for tag, detail in channel.asked if tag == 'wros/username']
        self.assertEqual(len(hosts), 2)
        self.assertIn('lowercase letters, numbers and hyphens only', hosts[1])
        self.assertEqual(len(users), 3)
        self.assertIn('reserved', users[2])
        self.assertEqual((payloads[0]['hostname'], payloads[0]['username']), ('hp-stream', 'alan'))
        # Every answer the dialog accepts, the core accepts (#139).
        self.assertIsNotNone(core.HOSTNAME_RE.fullmatch(payloads[0]['hostname']))
        self.assertIsNotNone(core.USERNAME_RE.fullmatch(payloads[0]['username']))
        self.assertEqual(backend.HOSTNAME_RE.pattern, core.HOSTNAME_RE.pattern)
        self.assertEqual(backend.USERNAME_RE.pattern, core.USERNAME_RE.pattern)

    def test_a_short_password_is_asked_again(self):
        channel, _, payloads = self.run_dialog({'wros/user_password': ['short', 'long-enough']})
        asked = [d for t, d in channel.asked if t == 'wros/user_password']
        self.assertEqual(len(asked), 2)
        self.assertEqual(payloads[0]['user_password'], 'long-enough')

    def test_the_chosen_layout_applies_before_the_passwords_are_typed(self):
        channel, layouts, _ = self.run_dialog({})
        self.assertEqual(layouts, ['colemak'])
        order = [tag for tag, _ in channel.asked]
        self.assertLess(order.index('wros/keyboard_layout'), order.index('wros/user_password'))

    def test_the_install_shows_its_steps(self):
        # The bar sat at 20% for the whole install (#134).
        channel = Channel(GOOD)

        def runner(payload):
            backend.show_step(channel, 35, 'Copying WrOS onto the disk')
            return {'passed': True}
        backend.run_dialog(channel, 'mmcblk0', '0' * 32, core_runner=runner,
                           target_identity=dict(IDENTITY), layout_runner=lambda layout: None)
        self.assertIn('PROGRESS INFO wros/step', channel.sent)
        self.assertIn('PROGRESS SET 35', channel.sent)
        self.assertEqual(channel.defined['wros/step'][0], 'Copying WrOS onto the disk')

    def test_a_failure_is_shown_then_handed_to_the_session_without_passwords(self):
        def fail(payload):
            raise RuntimeError('chpasswd failed (1): writer-secret was rejected')
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / 'wros-target-failed'
            with mock.patch.object(backend, 'FAILED_OUT', marker):
                channel = Channel(GOOD)
                with self.assertRaises(RuntimeError) as raised:
                    backend.run_dialog(channel, 'mmcblk0', '0' * 32, core_runner=fail,
                                       target_identity=dict(IDENTITY), layout_runner=lambda layout: None)
                self.assertNotIn('writer-secret', str(raised.exception))
                text = marker.read_text()
            self.assertIn('[hidden] was rejected', text)
            shown = [d for t, d in channel.asked if t == 'wros/failed']
            self.assertEqual(len(shown), 1)
            self.assertIn('chpasswd failed', shown[0])
            self.assertNotIn('writer-secret', shown[0])

    def test_a_failure_still_reaches_the_session_when_the_screens_are_gone(self):
        class Gone(Channel):
            def ask(self, tag, *args, **kwargs):
                if tag == 'wros/failed':
                    raise RuntimeError('frontend closed passthrough channel')
                return super().ask(tag, *args, **kwargs)

        def fail(payload):
            raise RuntimeError('nixos-install failed (1)')
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / 'wros-target-failed'
            with mock.patch.object(backend, 'FAILED_OUT', marker):
                with self.assertRaises(RuntimeError):
                    backend.run_dialog(Gone(GOOD), 'mmcblk0', '0' * 32, core_runner=fail,
                                       target_identity=dict(IDENTITY), layout_runner=lambda layout: None)
                self.assertIn('nixos-install failed', marker.read_text())


class Backend(unittest.TestCase):
    def test_core_steps_go_to_the_screen_and_the_rest_to_the_log(self):
        steps = []
        err = io.StringIO()
        lines = [b'wros-step 22 Partitioning the disk\n', b'something odd: disk-secret-1\n',
                 b'wros-step 35 Copying WrOS onto the disk\n', b'wros-install-core: boom\n']
        with mock.patch.object(sys, 'stderr', err):
            kept = backend.follow_core(lines, lambda p, t: steps.append((p, t)), secrets=('disk-secret-1',))
        self.assertEqual(steps, [(22, 'Partitioning the disk'), (35, 'Copying WrOS onto the disk')])
        self.assertEqual(kept, ['something odd: [hidden]', 'wros-install-core: boom'])
        self.assertNotIn('disk-secret-1', err.getvalue())

    def test_the_layout_loads_on_the_installer_console(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'colemak.map').write_text('keymaps 0-127\n')
            calls = []

            def run(argv, **kwargs):
                calls.append(argv)
                return subprocess.CompletedProcess(argv, 0)
            with mock.patch.object(backend, 'KEYMAPS_DIR', Path(tmp)):
                self.assertTrue(backend.apply_live_layout('colemak', run=run))
                self.assertFalse(backend.apply_live_layout('dvorak', run=run))
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], backend.LOADKEYS)
        self.assertIn('--console=/dev/tty7', calls[0])
        self.assertEqual(calls[0][-1], str(Path(tmp) / 'colemak.map'))


class Core(unittest.TestCase):
    def test_a_failed_command_keeps_what_it_printed_on_stdout(self):
        # nixos-install reports on stdout; only stderr was kept, so the reason was lost (#137).
        res = subprocess.CompletedProcess([], 1, stdout=b'path /run/wros-installer should have permissions 755\n',
                                          stderr=b'')
        self.assertIn('permissions 755', core.command_output(res))
        long = subprocess.CompletedProcess([], 1, stdout=b'x' * 5000 + b' the reason', stderr=b'warning')
        self.assertTrue(core.command_output(long).endswith('the reason'))
        self.assertLessEqual(len(core.command_output(long)), 601)

    def test_the_install_announces_each_step_in_order_and_allows_an_hour_to_copy(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / 'target'
            closure = Path(tmp) / 'closure'
            closure.write_text('/nix/store/0000000000000000000000000000000-wros-system\n')
            timeouts = {}

            def runner(argv, *, input_bytes=None, timeout=600):
                timeouts[argv[0]] = timeout
                if 'switch-to-configuration' in ' '.join(argv):
                    (target / 'boot/EFI/BOOT').mkdir(parents=True, exist_ok=True)
                    (target / 'boot/EFI/BOOT/BOOTX64.EFI').write_bytes(b'grub')
                return 'uuid'
            params = {'target_disk': 'mmcblk0', 'target_identity': IDENTITY, 'username': 'writer',
                      'full_name': 'W', 'hostname': 'wros', 'keyboard_layout': 'colemak',
                      'timezone': 'UTC', 'luks_passphrase': 'p' * 8, 'user_password': 'q' * 8,
                      'session_nonce': '0' * 32}
            err = io.StringIO()
            with mock.patch.object(sys, 'stderr', err):
                core.install_target(params, target_root=target, closure_file=closure, runner=runner,
                                    check_disk=lambda disk, identity: None)
        steps = [line.split(' ', 2) for line in err.getvalue().splitlines() if line.startswith('wros-step ')]
        percents = [int(p) for _, p, _ in steps]
        self.assertEqual(percents, sorted(percents))
        self.assertGreaterEqual(len(steps), 5)
        self.assertTrue(any('Copying WrOS' in text for *_, text in steps))
        self.assertGreaterEqual(timeouts['nixos-install'], 3600)
        self.assertEqual(core.COPY_TIMEOUT, timeouts['nixos-install'])


class LiveMedium(unittest.TestCase):
    def test_a_stick_written_by_copying_files_still_mounts(self):
        # Rufus's default ISO mode copies the files onto FAT32 under the same label (#132).
        live = (ROOT / 'nixos/modules/live.nix').read_text()
        self.assertIn('fileSystems."/iso".fsType = lib.mkForce "auto";', live)
        hardware = (ROOT / 'nixos/modules/hardware.nix').read_text()
        for module in ('"vfat"', '"nls_cp437"'):
            self.assertIn(module, hardware)


if __name__ == '__main__':
    unittest.main()
