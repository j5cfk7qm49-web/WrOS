"""Unit tests for the password change helper (os/scripts/wros-passwd, #121), against fake passwd programs."""
import importlib.machinery
import io
import json
import contextlib
import os
from pathlib import Path
import stat
import tempfile
import textwrap
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wros-passwd'
helper = importlib.machinery.SourceFileLoader('wros_passwd', str(SCRIPT)).load_module()

# A stand-in for Debian's passwd: the same prompts and messages, checking against fixed values.
FAKE = textwrap.dedent('''\
    #!/usr/bin/python3 -I
    import sys
    MODE = {mode!r}
    def ask(prompt):
        sys.stdout.write(prompt); sys.stdout.flush()
        return sys.stdin.readline().rstrip('\\n')
    print('Changing password for writer.')
    if ask('Current password: ') != 'old-secret':
        print('passwd: Authentication token manipulation error')
        print('passwd: password unchanged')
        sys.exit(10)
    for attempt in range(3):
        new = ask('New password: ')
        if MODE == 'short' and len(new) < 8:
            print('You must choose a longer password.')
            continue
        if ask('Retype new password: ') != new:
            print('Sorry, passwords do not match.')
            continue
        print('passwd: password updated successfully')
        sys.exit(0)
    print('passwd: Have exhausted maximum number of retries for service')
    sys.exit(10)
''')


class PasswdHelperTests(unittest.TestCase):
    def fake(self, mode='normal'):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = Path(tmp.name, 'passwd')
        path.write_text(FAKE.format(mode=mode))
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return (str(path),)

    def run_helper(self, data, argv):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = helper.main(io.BytesIO(data), argv)
        return code, json.loads(out.getvalue())

    def test_a_correct_current_password_changes_it(self):
        code, reply = self.run_helper(b'old-secret\nnew-secret-1\n', self.fake())
        self.assertEqual((code, reply['changed']), (0, True))
        self.assertIn('Password changed', reply['message'])

    def test_a_wrong_current_password_says_so(self):
        code, reply = self.run_helper(b'not-it\nnew-secret-1\n', self.fake())
        self.assertEqual((code, reply["changed"]), (0, False))
        self.assertEqual(reply['message'], 'The current password is wrong.')

    def test_a_refused_new_password_reports_pams_reason(self):
        code, reply = self.run_helper(b'old-secret\nshort\n', self.fake('short'))
        self.assertEqual((code, reply["changed"]), (0, False))
        self.assertIn('You must choose a longer password', reply['message'])

    def test_secrets_never_appear_in_the_reply(self):
        for data in (b'old-secret\nnew-secret-1\n', b'not-it\nnew-secret-1\n', b'old-secret\nshort\n'):
            _, reply = self.run_helper(data, self.fake('short'))
            for secret in data.decode().split():
                self.assertNotIn(secret, reply['message'])

    def test_bad_input_is_refused_before_passwd_runs(self):
        never = ('/nonexistent/passwd',)
        for data in (b'', b'only-one-line', b'same\nsame\n', b'old\n\n', b'a\x07b\nnew\n',
                     b'old\n' + b'x' * 600 + b'\n', b'\xff\xfe\nnew\n'):
            with self.subTest(data=data[:20]):
                code, reply = self.run_helper(data, never)
                self.assertEqual(code, 0)
                self.assertFalse(reply['changed'])

    def test_a_silent_program_times_out(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = Path(tmp.name, 'passwd')
        path.write_text('#!/bin/sh\nsleep 60\n')
        path.chmod(0o755)
        original = helper.TIMEOUT
        helper.TIMEOUT = 1
        self.addCleanup(setattr, helper, 'TIMEOUT', original)
        code, reply = self.run_helper(b'old-secret\nnew-secret-1\n', (str(path),))
        self.assertEqual((code, reply["changed"]), (0, False))
        self.assertIn('did not answer', reply['message'])


if __name__ == '__main__':
    unittest.main()
