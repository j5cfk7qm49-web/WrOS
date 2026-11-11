"""Pure privilege/argv/session validation; never opens or changes a host VT."""
from contextlib import redirect_stderr
import importlib.machinery
from io import StringIO
import re
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
module = importlib.machinery.SourceFileLoader('wros_lock', str(ROOT / 'os/scripts/wros-lock')).load_module()


class ConsoleLockBoundary(unittest.TestCase):
    def test_wayland_still_needs_the_active_owned_tty1_and_a_verified_compositor(self):
        good = {'User': '1000', 'Active': 'yes', 'Remote': 'no', 'TTY': 'tty1',
                'VTNr': '1', 'Seat': 'seat0', 'Type': 'wayland', 'Class': 'user'}
        self.assertFalse(module.session_allowed(good, 1000))
        self.assertTrue(module.session_allowed(good, 1000, composited=True))
        for key, value in {'User': '1001', 'Active': 'no', 'Remote': 'yes', 'TTY': '',
                           'VTNr': '2', 'Seat': 'seat1', 'Type': 'x11', 'Class': 'greeter'}.items():
            self.assertFalse(module.session_allowed(good | {key: value}, 1000, composited=True))

    def test_compositor_proof_requires_the_packaged_ancestor_uid_and_same_session(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            cage, other = root / 'cage', root / 'other'
            cage.touch()
            other.touch()
            for pid, parent, exe in [(42, 43, other), (43, 1, cage)]:
                path = root / str(pid)
                path.mkdir()
                (path / 'status').write_text(f'PPid:\t{parent}\nUid:\t1000\t1000\t1000\t1000\n')
                (path / 'exe').symlink_to(exe)
            with mock.patch.object(module, 'trusted_file') as trusted:
                def check(session=lambda pid: '5'):
                    return module.trusted_compositor(42, 1000, proc_root=root, cage=cage, session_lookup=session)
                self.assertTrue(check())
                trusted.assert_called_with(cage, True)
                self.assertFalse(check(lambda pid: '5' if pid == 42 else '6'))
                (root / '43/status').write_text('PPid:\t1\nUid:\t1001\t1001\t1001\t1001\n')
                self.assertFalse(check())
                (root / '43/status').write_text('PPid:\t1\nUid:\t1000\t1000\t1000\t1000\n')
                (root / '43/exe').unlink()
                (root / '43/exe').symlink_to(other)
                self.assertFalse(check())

    def test_sleep_requires_protected_vt_with_keyboard_disabled_and_reader_retired(self):
        with mock.patch.object(module, 'console_state', return_value=(8, 0, 0)), \
             mock.patch.object(module, 'keyboard_mode', return_value=module.K_OFF), \
             mock.patch.object(module, 'locker_ready', return_value=True), \
             mock.patch.object(module, 'input_reader_retired', return_value=True):
            self.assertTrue(module.can_sleep(42, 99, 8))
            for guard in ('locker_ready', 'input_reader_retired'):
                with mock.patch.object(module, guard, return_value=False):
                    self.assertFalse(module.can_sleep(42, 99, 8))
            with mock.patch.object(module, 'keyboard_mode', return_value=module.K_UNICODE):
                self.assertFalse(module.can_sleep(42, 99, 8))

    def test_slow_suspend_keeps_its_proof_until_logind_finishes(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            states = iter(('b true', 'b true', 'b false'))

            def query(argv):
                self.assertEqual((run / 'suspend').read_text(), '42 99\n')
                if argv[0] == '/usr/bin/systemctl':
                    self.assertEqual(argv[1:], ['suspend'])
                    return ''
                self.assertEqual(argv[-1], 'PreparingForSleep')
                return next(states)

            with mock.patch.object(module, 'RUN', run), \
                 mock.patch.object(module, 'tty8', return_value=8), \
                 mock.patch.object(module, 'can_sleep', return_value=True), \
                 mock.patch.object(module, 'query', side_effect=query), \
                 mock.patch.object(module.time, 'monotonic', side_effect=(0, 21)), \
                 mock.patch.object(module.time, 'sleep') as wait, \
                 mock.patch.object(module.os, 'close'):
                self.assertIsNone(module.suspend(42, 99))
            self.assertEqual(wait.call_count, 2)
            self.assertFalse((run / 'suspend').exists())

    def test_a_refused_request_can_restore_input_only_after_a_confirmed_idle_state(self):
        refused = module.subprocess.CalledProcessError(1, ['/usr/bin/systemctl', 'suspend'])
        with tempfile.TemporaryDirectory() as temp, \
             mock.patch.object(module, 'RUN', Path(temp)), \
             mock.patch.object(module, 'tty8', return_value=8), \
             mock.patch.object(module, 'can_sleep', return_value=True), \
             mock.patch.object(module.os, 'close'):
            with mock.patch.object(module, 'query', side_effect=(refused, 'b false')):
                self.assertIs(module.suspend(42, 99), refused)
            with mock.patch.object(module, 'query', side_effect=(refused, 'unknown')):
                with self.assertRaisesRegex(ValueError, 'suspend completion'):
                    module.suspend(42, 99)
            self.assertFalse((Path(temp) / 'suspend').exists())

    def test_suspend_unit_requires_the_proof_without_holding_the_lock_unit_loaded(self):
        # A unit dependency on the lock service keeps its finished run loaded, and systemd-run then
        # refuses the next lock of the same boot (#93). assert-suspend checks the running service.
        unit = (ROOT / 'os/config/wros-suspend.conf').read_text()
        settings = [line for line in unit.splitlines() if line and not line.startswith('#')]
        self.assertEqual(settings, ['[Service]', 'ExecStartPre=/usr/libexec/wros-lock assert-suspend'])
        self.assertIn('--unit=wros-console-lock.service', module.systemd_command(1000, 42, '12345', 176))

    def test_sleep_needs_the_running_lock_service_to_own_the_locker(self):
        service = '0::/system.slice/wros-console-lock.service\n'
        with tempfile.TemporaryDirectory() as temp:
            run, proc = Path(temp, 'run'), Path(temp, 'proc')
            run.mkdir()
            (run / 'suspend').write_text('42 99\n')
            (proc / '99').mkdir(parents=True)
            cgroup, owns = proc / '99/cgroup', module.lock_service_owns

            def check(state, group):
                cgroup.write_text(group)
                with mock.patch.object(module, 'RUN', run), mock.patch.object(module, 'trusted_file'), \
                     mock.patch.object(module, 'query', return_value=state) as query, \
                     mock.patch.object(module, 'lock_service_owns', lambda pid: owns(pid, proc)), \
                     mock.patch.object(module, 'tty8', return_value=8), mock.patch.object(module.os, 'close'), \
                     mock.patch.object(module, 'can_sleep', return_value=True):
                    module.assert_suspend()
                    self.assertEqual(query.call_args.args[0][-1], 'wros-console-lock.service')

            check('active', service)
            for state, group in (('inactive', service),
                                 ('active', '0::/user.slice/user-1000.slice/session-3.scope\n')):
                with self.subTest(state=state, group=group), self.assertRaises(ValueError):
                    check(state, group)

    def test_the_lock_service_owns_only_lockers_in_its_own_control_group(self):
        with tempfile.TemporaryDirectory() as temp:
            proc = Path(temp)
            (proc / '99').mkdir()
            (proc / '99/cgroup').write_text('0::/system.slice/wros-console-lock.service\n')
            self.assertTrue(module.lock_service_owns(99, proc))
            (proc / '99/cgroup').write_text('0::/system.slice/wros-console-lock.service/x\n')
            self.assertFalse(module.lock_service_owns(99, proc))

    def test_lock_service_never_starts_while_the_editor_reader_is_alive(self):
        with mock.patch.object(module, 'validate_caller'), \
             mock.patch.object(module, 'input_reader_retired', return_value=False), \
             mock.patch.object(module, 'secure_process', side_effect=AssertionError('must refuse before setting up the service')):
            with self.assertRaises(ValueError):
                module.client()

    def test_session_must_match_the_callers_active_local_seat_and_console(self):
        good = {'User': '1000', 'Active': 'yes', 'Remote': 'no', 'TTY': 'tty1',
                'VTNr': '1', 'Seat': 'seat0', 'Type': 'tty', 'Class': 'user'}
        self.assertTrue(module.session_allowed(good, 1000))
        for key, value in {'User': '1001', 'Active': 'no', 'Remote': 'yes', 'TTY': 'tty2',
                           'VTNr': '2', 'Seat': 'seat1', 'Type': 'x11', 'Class': 'greeter'}.items():
            with self.subTest(key=key):
                self.assertFalse(module.session_allowed(good | {key: value}, 1000))
                missing = good.copy()
                del missing[key]
                self.assertFalse(module.session_allowed(missing, 1000))

    def test_fixed_command_cannot_source_a_user_vlockrc_or_accept_plugins_from_environment(self):
        arguments, environment = module.fixed_command('writer')
        self.assertEqual(arguments, ['/usr/bin/openvt', '-c', '8', '-s', '-w', '--',
                                     '/usr/sbin/vlock-main', 'all', 'nosysrq'])
        self.assertEqual(environment['USER'], 'writer')
        self.assertEqual(environment['HOME'], '/nonexistent')
        self.assertNotIn('VLOCK_PLUGINS', environment)
        self.assertNotIn('LD_PRELOAD', environment)
        self.assertNotIn('VLOCK_MESSAGE', environment)
        self.assertIn('VLOCK_ALL_MESSAGE', environment)
        self.assertNotIn('-f', arguments)

    def test_lock_clock_is_prepared_once_by_the_fixed_write_only_packaged_program(self):
        message = '\x1b[2J\x1b[3J\x1b[HWrOS - writing session locked\nLocked at 12:34\n███\nPress Enter to unlock.'
        reply = mock.Mock(stdout=message.encode(), stderr=b'')
        with mock.patch.object(module, 'trusted_file') as trusted, \
             mock.patch.object(module.subprocess, 'run', return_value=reply) as run:
            self.assertEqual(module.lock_message(), message)
            trusted.assert_called_once_with('/usr/lib/wros/wros-greeter', True)
            run.assert_called_once_with(['/usr/lib/wros/wros-greeter', '--lock-message'],
                stdin=module.subprocess.DEVNULL, capture_output=True, env=module.ENV,
                timeout=1, check=True)
            _, environment = module.fixed_command('writer', message)
            self.assertEqual(environment['VLOCK_ALL_MESSAGE'], message)

    def test_unavailable_or_invalid_clock_cannot_prevent_the_maintained_locker_starting(self):
        for failure in (OSError(), module.subprocess.TimeoutExpired('clock', 1),
                        module.subprocess.CalledProcessError(1, 'clock')):
            with mock.patch.object(module, 'trusted_file'), \
                 mock.patch.object(module.subprocess, 'run', side_effect=failure):
                self.assertEqual(module.lock_message(), module.LOCK_MESSAGE)
        with mock.patch.object(module, 'trusted_file', side_effect=ValueError), \
             mock.patch.object(module.subprocess, 'run') as run:
            self.assertEqual(module.lock_message(), module.LOCK_MESSAGE)
            run.assert_not_called()
        for output in (b'', b'x' * 8193, b'\xff', module.LOCK_MESSAGE.encode() + b'\x1b[6n',
                       module.LOCK_MESSAGE.encode() + b'\x00',
                       module.LOCK_MESSAGE.replace('Press Enter', '▀Press Enter').encode()):
            with self.subTest(output=output[:30]), mock.patch.object(module, 'trusted_file'), \
                 mock.patch.object(module.subprocess, 'run', return_value=mock.Mock(stdout=output)):
                self.assertEqual(module.lock_message(), module.LOCK_MESSAGE)

    def test_account_identity_cannot_be_an_option_ambiguous_or_nonlogin(self):
        self.assertEqual(module.account_name(1000, 'writer:x:1000:1000::/home/writer:/bin/bash\n'), 'writer')
        for text in ('-root:x:1000:1000::/home/writer:/bin/bash\n',
                     'writer:x:1000:1000::/home/writer:/usr/sbin/nologin\n',
                     'writer:x:1001:1001::/home/other:/bin/bash\n',
                     'a:x:1000:1000::/home/a:/bin/bash\nb:x:1000:1000::/home/b:/bin/bash\n'):
            with self.assertRaises(ValueError):
                module.account_name(1000, text)

    def test_unprivileged_direct_invocation_is_rejected_before_any_console_or_account_read(self):
        with mock.patch.object(module.os, 'geteuid', return_value=1000), \
             mock.patch.object(module.os, 'isatty', side_effect=AssertionError('must not inspect a host terminal')):
            with self.assertRaises(ValueError):
                module.validate_caller()

    def test_packaged_pam_policy_is_exact_and_does_not_replace_login_or_account_setup(self):
        self.assertEqual((ROOT / 'os/config/vlock.pam').read_text(), module.PAM)
        self.assertNotIn('pam_permit', module.PAM)

    def test_systemd_owns_the_service_and_only_fixed_worker_and_cleanup_commands(self):
        args = module.systemd_command(1000, 42, '12345', 176)
        self.assertEqual(args[0], '/usr/bin/systemd-run')
        for flag in ('--wait', '--collect', '--service-type=exec', '--property=KillMode=control-group'):
            self.assertIn(flag, args)
        self.assertIn('--property=ExecStopPost=/usr/libexec/wros-lock cleanup 176', args)
        self.assertEqual(args[-5:], ['/usr/libexec/wros-lock', 'worker', '1000', '42', '12345'])
        for values in ((0, 42, '12345', 176), (1000, 1, '12345', 176),
                       (1000, 42, '1\n/bin/sh', 176), (1000, 42, '12345', '1; sh'),
                       (True, 42, '12345', 176), (1000, 42, '12345', -1)):
            with self.subTest(values=values), self.assertRaises(ValueError):
                module.systemd_command(*values)

    def test_service_roles_cannot_be_called_from_a_pkexec_or_ordinary_parent(self):
        for parent, environment in ((42, {}), (1, {'PKEXEC_UID': '1000'})):
            with self.subTest(parent=parent), \
                 mock.patch.object(module.sys, 'argv', ['wros-lock', 'worker', '1000', '42', '12345']), \
                 mock.patch.object(module.os, 'geteuid', return_value=0), \
                 mock.patch.object(module.os, 'getppid', return_value=parent), \
                 mock.patch.dict(module.os.environ, environment, clear=True), \
                 mock.patch.object(module, 'worker', side_effect=AssertionError('must not run a service role')):
                with self.assertRaises(ValueError):
                    module.main()

    def test_reader_detection_matches_the_actual_worker_argument(self):
        with tempfile.TemporaryDirectory(prefix="wros-lock-sleep-") as tmp:
            proc = Path(tmp) / "proc"
            task_dir = proc / "42" / "task" / "42"
            task_dir.mkdir(parents=True)
            # A process-wide child can be missed by a task's children listing.
            (task_dir / "children").write_text("")
            worker_dir = proc / "99"
            worker_dir.mkdir(parents=True)
            process_stat = worker_dir / "stat"
            process_stat.write_text("99 (wros)input) S 42\n")
            # The argument the editor really starts its input reader with (crates/wros-input).
            rust = (ROOT / 'crates/wros-input/src/linux.rs').read_text()
            argument = re.search(r'pub const WORKER_ARGUMENT: &str = "([^"]+)";', rust).group(1)
            (worker_dir / "cmdline").write_bytes(b"/usr/bin/grimoire\0" + argument.encode() + b"\x0042\0")
            self.assertFalse(module.input_reader_retired(42, proc_root=proc))
            process_stat.write_text("99 (wros)input) S 7\n")
            self.assertTrue(module.input_reader_retired(42, proc_root=proc))
            process_stat.unlink()
            self.assertTrue(module.input_reader_retired(42, proc_root=proc))

    def test_worker_logs_the_failed_step_without_command_output(self):
        error = module.subprocess.CalledProcessError(2, ['loginctl'], stderr=b'private output')
        with mock.patch.object(module, 'secure_process'), \
             mock.patch.object(module.os, 'pidfd_open', return_value=9), \
             mock.patch.object(module.os, 'close'), \
             mock.patch.object(module, 'birth', return_value='1234'), \
             mock.patch.object(module.os.path, 'samefile', return_value=True), \
             mock.patch.object(module, 'input_reader_retired', return_value=True), \
             mock.patch.object(Path, 'read_text', return_value='Uid:\t1000\t1000\t1000\t1000\n'), \
             mock.patch.object(module, 'caller_session', return_value='3'), \
             mock.patch.object(module, 'query', side_effect=error), \
             redirect_stderr(StringIO()) as journal:
            self.assertEqual(module.worker('1000', '42', '1234', sleep=True), 1)
        self.assertEqual(journal.getvalue(),
                         'WrOS console lock: checking the logind session: command exited with status 2\n')
