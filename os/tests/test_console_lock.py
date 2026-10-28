"""Pure privilege/argv/session validation; never opens or changes a host VT."""
import importlib.machinery
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
