"""Target-policy strings and synthetic sysfs; no system accounts or disks read."""
from pathlib import Path
import tempfile
import unittest

import native_target_audit as audit

PASSWD = ('root:x:0:0::/root:/bin/bash\n'
          '_greetd:x:995:995::/nonexistent:/usr/sbin/nologin\n'
          'fixture:x:1000:1000:Disposable target:/home/fixture:/bin/bash\n')
SHADOW = 'root:!:1::::::\n_greetd:!:1::::::\nfixture:$6$fixture$hash:1::::::\n'
GROUPS = 'writingdeck:x:997:fixture\nsudo:x:27:fixture\n'


class NativeTargetPolicy(unittest.TestCase):
    def test_actual_root_requires_the_single_correct_luks2_ancestor(self):
        entry={'mapping':'vda3_crypt','uuid':'12345678-abcd-1234-abcd-123456789abc'}
        with tempfile.TemporaryDirectory(prefix='wros-encrypted-root-policy-') as directory:
            root=Path(directory)
            for name in ('dm-1','dm-0'):
                (root/name/'dm').mkdir(parents=True)
                (root/name/'slaves').mkdir()
            (root/'vda3').mkdir();(root/'vda2').mkdir()
            (root/'dm-1/dm/uuid').write_text('LVM-fixture-root\n')
            expected='CRYPT-LUKS2-'+entry['uuid'].replace('-','')+'-'+entry['mapping']
            (root/'dm-0/dm/uuid').write_text(expected+'\n')
            (root/'dm-1/slaves/dm-0').symlink_to(root/'dm-0',target_is_directory=True)
            (root/'dm-0/slaves/vda3').symlink_to(root/'vda3',target_is_directory=True)
            self.assertEqual(audit.encrypted_root_chain('dm-1',entry,'vda3',root)['format'],'LUKS2')
            with self.assertRaises(ValueError):audit.encrypted_root_chain('vda2',entry,'vda3',root)
            for changed in (expected.replace('LUKS2','LUKS1'),expected.replace('12345678','87654321')):
                (root/'dm-0/dm/uuid').write_text(changed+'\n')
                with self.assertRaises(ValueError):audit.encrypted_root_chain('dm-1',entry,'vda3',root)
            (root/'dm-0/dm/uuid').write_text(expected+'\n')
            (root/'dm-1/slaves/vda2').symlink_to(root/'vda2',target_is_directory=True)
            with self.assertRaises(ValueError):audit.encrypted_root_chain('dm-1',entry,'vda3',root)
            (root/'dm-1/slaves/vda2').unlink()
            (root/'dm-0/slaves/vda3').unlink()
            (root/'dm-0/slaves/vda2').symlink_to(root/'vda2',target_is_directory=True)
            with self.assertRaises(ValueError):audit.encrypted_root_chain('dm-1',entry,'vda3',root)

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
