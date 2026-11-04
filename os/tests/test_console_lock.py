"""Pure privilege/argv/session validation; never opens or changes a host VT."""
import importlib.machinery
import re
from pathlib import Path
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
module = importlib.machinery.SourceFileLoader('wros_lock', str(ROOT / 'os/scripts/wros-lock')).load_module()


class ConsoleLockBoundary(unittest.TestCase):
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

    def test_lock_before_sleep_requires_input_reader_retired_and_tty8_vlock_ready(self):
        import tempfile
        with tempfile.TemporaryDirectory(prefix="wros-lock-sleep-") as tmp:
            proc = Path(tmp) / "proc"
            task_dir = proc / "42" / "task" / "42"
            task_dir.mkdir(parents=True)
            (task_dir / "children").write_text("99\n")
            worker_dir = proc / "99"
            worker_dir.mkdir(parents=True)
            # The argument the editor really starts its input reader with (crates/wros-input).
            rust = (ROOT / 'crates/wros-input/src/linux.rs').read_text()
            argument = re.search(r'pub const WORKER_ARGUMENT: &str = "([^"]+)";', rust).group(1)
            (worker_dir / "cmdline").write_bytes(b"/usr/bin/grimoire\0" + argument.encode() + b"\x0042\0")
            sleep_conf = Path(tmp) / "60-wros-sleep-guard.conf"
            sleep_conf.write_text("[Sleep]\nAllowSuspend=no\n")

            self.assertFalse(module.input_reader_retired(42, proc_root=proc))
            with self.assertRaisesRegex(ValueError, "input reader is still active"):
                module.evaluate_pre_sleep_lock(
                    1000, 42, "12345", 0, sleep_enabled=False,
                    proc_root=proc, sleep_conf=sleep_conf, locker_ready=True
                )

            # Once input worker is retired, tty8 locker readiness is still required.
            (task_dir / "children").write_text("")
            self.assertTrue(module.input_reader_retired(42, proc_root=proc))
            with self.assertRaisesRegex(ValueError, "tty8 vlock-main"):
                module.evaluate_pre_sleep_lock(
                    1000, 42, "12345", 0, sleep_enabled=False,
                    proc_root=proc, sleep_conf=sleep_conf, locker_ready=False
                )

            # With input retired and tty8 locker ready, software lock-before-sleep verifies cleanly while keeping sleep disabled (#32).
            res = module.evaluate_pre_sleep_lock(
                1000, 42, "12345", 0, sleep_enabled=False,
                proc_root=proc, sleep_conf=sleep_conf, locker_ready=True, pam_unlocked=True
            )
            self.assertEqual(res["status"], "lock-verified-sleep-disabled")
            self.assertTrue(res["input_retired"])
            self.assertTrue(res["locker_ready"])
            self.assertFalse(res["sleep_permitted"])

            # Requesting actual sleep_enabled=True or weakening AllowSuspend=no is refused.
            with self.assertRaisesRegex(ValueError, "Suspend unavailable"):
                module.evaluate_pre_sleep_lock(
                    1000, 42, "12345", 0, sleep_enabled=True,
                    proc_root=proc, sleep_conf=sleep_conf, locker_ready=True
                )
            sleep_conf.write_text("[Sleep]\nAllowSuspend=yes\n")
            with self.assertRaises(ValueError):
                module.evaluate_pre_sleep_lock(
                    1000, 42, "12345", 0, sleep_enabled=False,
                    proc_root=proc, sleep_conf=sleep_conf, locker_ready=True
                )
