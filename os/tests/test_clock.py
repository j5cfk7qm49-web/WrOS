"""Unit tests for the narrow privileged local clock helper (os/scripts/wros-clock)."""
import importlib.machinery
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wros-clock'
clock = importlib.machinery.SourceFileLoader('wros_clock', str(SCRIPT)).load_module()


class LocalClockHelperTests(unittest.TestCase):
    def test_validates_calendar_dates_leap_years_and_refuses_metacharacters(self):
        self.assertEqual(clock.validate_timestamp('2026-10-31 19:45:00'), '2026-10-31 19:45:00')
        self.assertEqual(clock.validate_timestamp('2024-02-29 00:00:00'), '2024-02-29 00:00:00')
        for bad in (
            '',
            '2026-02-29 12:00:00',
            '2019-12-31 23:59:59',
            '2100-01-01 00:00:00',
            '2026-13-01 00:00:00',
            '2026-10-31 24:00:00',
            '2026-10-31 12:00',
            '2026-10-31 12:00:00;reboot',
            '--set-ntp=true',
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                clock.validate_timestamp(bad)

    def test_requires_root_and_non_root_pkexec_uid(self):
        with self.assertRaisesRegex(ValueError, 'must run as root'):
            clock.require_polkit_caller(euid=1000, environ={'PKEXEC_UID': '1000'})
        for bad_env in ({}, {'PKEXEC_UID': ''}, {'PKEXEC_UID': '0'}, {'PKEXEC_UID': '999'}, {'PKEXEC_UID': '65534'}, {'PKEXEC_UID': 'root'}):
            with self.subTest(env=bad_env), self.assertRaisesRegex(ValueError, 'polkit caller'):
                clock.require_polkit_caller(euid=0, environ=bad_env)
        self.assertEqual(clock.require_polkit_caller(euid=0, environ={'PKEXEC_UID': '1000'}), 1000)

    def test_apply_local_clock_disables_ntp_and_verifies_offline_state(self):
        calls = []
        replies = ['yes', '', '', 'no']

        def fake_runner(cmd):
            calls.append(cmd)
            return replies.pop(0)

        res = clock.apply_local_clock(
            '2026-11-01 09:30:00',
            runner=fake_runner,
            euid=0,
            environ={'PKEXEC_UID': '1000'},
        )
        self.assertEqual(res, {'status': 'applied', 'local_time': '2026-11-01 09:30:00', 'ntp': False})
        self.assertEqual(
            calls,
            [
                ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', '--value', '--property=NTP', 'show'],
                ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', 'set-ntp', 'false'],
                ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', 'set-time', '2026-11-01 09:30:00'],
                ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', '--value', '--property=NTP', 'show'],
            ],
        )

        # If NTP remains enabled after set-time, apply_local_clock fails closed.
        bad_replies = ['no', '', 'yes']
        with self.assertRaisesRegex(ValueError, 'NTP remained active'):
            clock.apply_local_clock(
                '2026-11-01 09:30:00',
                runner=lambda cmd: bad_replies.pop(0),
                euid=0,
                environ={'PKEXEC_UID': '1000'},
            )

    def test_network_time_can_be_switched_back_on_and_is_verified(self):
        calls = []
        replies = ['', 'yes']

        def fake_runner(cmd):
            calls.append(cmd)
            return replies.pop(0)

        res = clock.use_network_time(runner=fake_runner, euid=0, environ={'PKEXEC_UID': '1000'})
        self.assertEqual(res, {'status': 'applied', 'ntp': True})
        self.assertEqual(calls, [
            ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', 'set-ntp', 'true'],
            ['/usr/bin/timedatectl', '--no-ask-password', '--no-pager', '--value', '--property=NTP', 'show'],
        ])
        still_off = ['', 'no']
        with self.assertRaisesRegex(ValueError, 'Network time did not switch on'):
            clock.use_network_time(runner=lambda cmd: still_off.pop(0), euid=0, environ={'PKEXEC_UID': '1000'})
        with self.assertRaisesRegex(ValueError, 'polkit caller'):
            clock.use_network_time(runner=fake_runner, euid=0, environ={})
        # Only the two fixed commands are accepted.
        for argv in (['wros-clock', 'network-time', 'extra'], ['wros-clock', 'set-ntp', 'true'], ['wros-clock']):
            with self.subTest(argv=argv), self.assertRaises(ValueError):
                clock.main(argv)


if __name__ == '__main__':
    unittest.main()
