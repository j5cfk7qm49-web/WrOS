"""Pure target-policy strings only; no system accounts, mounts or disks read."""
import unittest

import native_target_audit as audit

PASSWD = ('root:x:0:0::/root:/bin/bash\n'
          '_greetd:x:995:995::/nonexistent:/usr/sbin/nologin\n'
          'fixture:x:1000:1000:Disposable target:/home/fixture:/bin/bash\n')
SHADOW = 'root:!:1::::::\n_greetd:!:1::::::\nfixture:$6$fixture$hash:1::::::\n'
GROUPS = 'writingdeck:x:997:fixture\nsudo:x:27:fixture\n'


class NativeTargetPolicy(unittest.TestCase):
    def test_luks_configuration_requires_one_uuid_and_interactive_unlock(self):
        value = 'vda3_crypt UUID=12345678-abcd-1234-abcd-123456789abc none luks,discard\n'
        self.assertEqual(audit.crypttab_entry(value)['mapping'], 'vda3_crypt')
        for changed in (value+value, value.replace('none', '/root/key'),
                        value.replace('luks,discard', 'plain'), value.replace('UUID=', '/dev/'),
                        value.replace('vda3_crypt', '../different')):
            with self.subTest(value=changed), self.assertRaises(ValueError):
                audit.crypttab_entry(changed)

    def test_account_proof_refuses_live_users_duplicate_credentials_or_privileged_greeter(self):
        audit.account_state(PASSWD, SHADOW, GROUPS)
        cases = [(PASSWD.replace('fixture:', 'wros-live:'), SHADOW, GROUPS),
                 (PASSWD, SHADOW+'root::1::::::\n', GROUPS),
                 (PASSWD, 'root::1::::::\n'+SHADOW, GROUPS),
                 (PASSWD.replace('_greetd:x:995:', '_greetd:x:0:'), SHADOW, GROUPS),
                 (PASSWD, SHADOW.replace('root:!', 'root:'), GROUPS),
                 (PASSWD, SHADOW, GROUPS.replace('sudo:x:27:fixture', 'sudo:x:27:'))]
        for passwd, shadow, groups in cases:
            with self.subTest(passwd=passwd), self.assertRaises(ValueError):
                audit.account_state(passwd, shadow, groups)
