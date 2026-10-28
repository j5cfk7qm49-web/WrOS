"""Observed serial line boundaries, without a guest, credential or host device."""
import unittest
from installed_boot import has_line, marker_pattern, suspend_capability


class ConsoleProtocol(unittest.TestCase):
    def test_real_output_accepts_mixed_lf_crlf_and_repeated_cr(self):
        for newline in (b'\n', b'\r\n', b'\r\r\n', b'\r'):
            with self.subTest(newline=newline):
                self.assertTrue(has_line(b'prior' + newline + b'WROS_INPUT_UID_PASS' + newline,
                                         b'WROS_INPUT_UID_PASS'))

    def test_echoed_commands_and_partial_lines_are_not_success(self):
        for content in (b'echo WROS_INPUT_UID_PASS; fi\r\n', b'\nWROS_INPUT_UID_PASS',
                        b'\nNOT_WROS_INPUT_UID_PASS\n', b'\nWROS_INPUT_UID_PASS_extra\n'):
            self.assertFalse(has_line(content, b'WROS_INPUT_UID_PASS'))

    def test_search_respects_cursor_and_literal_service_output(self):
        content = b'\nWROS_DONE\r\ncommand echo\r\ns "na"\n'
        cursor = marker_pattern(b'WROS_DONE').search(content).end()
        self.assertIsNone(marker_pattern(b'WROS_DONE').search(content, cursor))
        self.assertIsNotNone(marker_pattern(b's "na"').search(content, cursor))

    def test_marker_metacharacters_are_literal(self):
        self.assertTrue(has_line(b'\nservice.a[1]\n', b'service.a[1]'))
        self.assertFalse(has_line(b'\nserviceXa1\n', b'service.a[1]'))

    def test_logind_denials_are_distinct_from_permission_or_a_challenge(self):
        for state in ('no', 'na', 'yes', 'challenge'):
            with self.subTest(state=state):
                self.assertEqual(suspend_capability(('\ns "' + state + '"\r\n').encode()), state)
        self.assertIsNone(suspend_capability(b'command echo s "no"; more\n'))
        self.assertIsNone(suspend_capability(b'\ns "unknown"\n'))
