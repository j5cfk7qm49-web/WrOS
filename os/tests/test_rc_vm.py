import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rc_vm as vm
import rc_vm_forms as forms


def screen(*lines):
    return ['  ' + line for line in lines]


class KeyboardLayouts(unittest.TestCase):
    def test_us_is_identity(self):
        self.assertEqual(vm.keystrokes('ab1 -', 'us'), [['a'], ['b'], ['1'], ['spc'], ['minus']])

    def test_colemak_moves_the_keys_hex_secrets_use(self):
        # To print d, e, f on Colemak the physical keys are QWERTY g, k, e.
        self.assertEqual(vm.keystrokes('def', 'colemak'), [['g'], ['k'], ['e']])
        self.assertEqual(vm.keystrokes('abc012', 'colemak')[:3], [['a'], ['b'], ['c']])

    def test_colemak_shift_and_semicolon(self):
        self.assertEqual(vm.keystrokes('D', 'colemak'), [['shift', 'g']])
        self.assertEqual(vm.keystrokes(';', 'colemak'), [['p']])
        self.assertEqual(vm.keystrokes(':', 'colemak'), [['shift', 'p']])
        self.assertEqual(vm.keystrokes('o', 'colemak'), [['semicolon']])

    def test_every_lowercase_letter_is_typeable_and_bijective(self):
        for layout in ('colemak', 'colemak_dh'):
            chords = [tuple(c) for c in vm.keystrokes('abcdefghijklmnopqrstuvwxyz', layout)]
            self.assertEqual(len(set(chords)), 26, layout)

    def test_dh_differs_from_plain_colemak(self):
        self.assertNotEqual(vm.keystrokes('dhmbv', 'colemak'), vm.keystrokes('dhmbv', 'colemak_dh'))

    def test_generated_secret_is_layout_sensitive_and_hex(self):
        for _ in range(20):
            secret = vm.generated_secret('colemak')
            self.assertRegex(secret, '^[0-9a-f]{24}$')
            self.assertTrue(vm.wrong_layout_differs(secret, 'colemak'))

    def test_dvorak_positions(self):
        self.assertEqual(vm.keystrokes('def', 'dvorak'), [['h'], ['d'], ['y']])
        self.assertEqual(vm.keystrokes('-', 'dvorak'), [['apostrophe']])
        self.assertEqual(vm.keystrokes(':', 'dvorak'), [['shift', 'z']])
        chords = [tuple(c) for c in vm.keystrokes('abcdefghijklmnopqrstuvwxyz', 'dvorak')]
        self.assertEqual(len(set(chords)), 26)

    def test_untypeable_character_raises(self):
        with self.assertRaises(ValueError):
            vm.keystrokes('é', 'colemak')


class Forms(unittest.TestCase):
    def test_select_form(self):
        form = forms.parse_form(screen(
            'WrOS · Install', 'Install WrOS · 3 of 9', 'Select a language', '',
            '▸ English', '  German', '  French'))
        self.assertEqual(form.kind, 'select')
        self.assertEqual(form.selected, 'English')
        rule, action = forms.decide(form, forms.rules())
        self.assertEqual((rule.name, action[0]), ('language', 'enter'))

    def test_boolean_write_confirmation_requires_target_identity(self):
        text = screen('Install WrOS · 1 of 1', 'Write the changes to disks and configure LVM?', '',
                      'The partition tables of vda will be changed', '▸ No', '  Yes')
        form = forms.parse_form(text)
        self.assertEqual(form.kind, 'boolean')
        self.assertEqual(forms.decide(form, forms.rules())[1], ('yes',))
        refused = forms.parse_form(screen('Write the changes to disks?', '▸ No', '  Yes'))
        self.assertIsNone(forms.decide(refused, forms.rules()))   # names no vda: never answered

    def test_password_and_string(self):
        pw = forms.parse_form(screen('Install WrOS · 1 of 2', 'Encryption passphrase:', '', '▸ [hidden]'))
        self.assertEqual(pw.kind, 'password')
        self.assertEqual(forms.decide(pw, forms.rules())[1], ('secret', 'luks'))
        st = forms.parse_form(screen('Install WrOS · 1 of 2', 'Hostname:', '', '▸ debian'))
        self.assertEqual(st.kind, 'string')
        self.assertEqual(forms.decide(st, forms.rules())[1], ('fill', 'wros-rc'))

    def test_unknown_form_is_not_answered(self):
        form = forms.parse_form(screen('Install WrOS · 1 of 1', 'Choose software to install:', '',
                                       '▸ [ ] Games', '  [x] Fonts'))
        self.assertIsNone(forms.decide(form, forms.rules()))

    def test_progress_waits(self):
        form = forms.parse_form(screen('Install WrOS', 'Installing the base system', 'Extracting libc6'))
        self.assertEqual(form.kind, 'progress')
        self.assertEqual(forms.decide(form, forms.rules())[1], ('wait',))

    def test_keyboard_rule_prefers_plain_colemak(self):
        rule = next(r for r in forms.rules() if r.name == 'keyboard-layout')
        self.assertEqual(rule.action[0], 'select')
        want, avoid = rule.action[1], rule.action[2]

        def pick(text):
            return bool(re.search(want, text, re.I)) and not re.search(avoid, text, re.I)
        self.assertTrue(pick('English (US) - English (Colemak)'))
        self.assertFalse(pick('English (US) - English (Colemak-DH)'))
        self.assertFalse(pick('English (US) - English (Dvorak)'))


@unittest.skipUnless(sys.platform == 'linux' and hasattr(os, 'SEEK_DATA'), 'needs sparse-file support')
class SparseDigest(unittest.TestCase):
    def test_hole_equals_zero_data_and_data_changes_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp, 'a'), Path(tmp, 'b')
            for p in (a, b):
                with open(p, 'wb') as s:
                    s.truncate(64 << 20)
            with open(b, 'r+b') as s:
                s.seek(1 << 20)
                s.write(bytes(8192))                  # explicit zeroes
            self.assertEqual(vm.sparse_digest(a), vm.sparse_digest(b))
            with open(b, 'r+b') as s:
                s.seek(5 << 20)
                s.write(b'x')
            self.assertNotEqual(vm.sparse_digest(a), vm.sparse_digest(b))


@unittest.skipUnless(shutil.which('mke2fs') and shutil.which('debugfs'), 'needs e2fsprogs')
class UpdateMedia(unittest.TestCase):
    def test_label_contents_and_tamper(self):
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp, 'bundle')
            (bundle / 'pool').mkdir(parents=True)
            (bundle / 'InRelease').write_text('x')
            (bundle / 'Packages').write_text('y')
            (bundle / 'pool' / 'wros.deb').write_bytes(bytes(range(256)) * 16)
            image = vm.make_update_media(bundle, Path(tmp, 'media.img'), 'ext4', 8)
            header = subprocess.check_output(['dumpe2fs', '-h', str(image)], stderr=subprocess.DEVNULL)
            self.assertIn(b'WROS-UPDATE', header)
            vm.flip_byte_in_ext4(image, '/pool/wros.deb', 100)
            out = Path(tmp, 'out.deb')
            subprocess.run(['debugfs', '-R', 'dump /pool/wros.deb %s' % out, str(image)], check=True, capture_output=True)
            original = (bundle / 'pool' / 'wros.deb').read_bytes()
            changed = out.read_bytes()
            self.assertEqual(len(original), len(changed))
            self.assertEqual([i for i in range(len(original)) if original[i] != changed[i]], [100])

    def test_refuses_non_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                vm.make_update_media(tmp, Path(tmp, 'x.img'))


class Ocr(unittest.TestCase):
    def test_round_trip(self):
        try:
            import PIL  # noqa: F401
            font = vm.Font()
        except (ImportError, FileNotFoundError):
            self.skipTest('needs Pillow and a console font')
        text = 'Please unlock disk sda5_crypt:\nNo key available with this passphrase.\nWrOS login: 0123456789 [hidden]'
        image = font.render(text, 100, 37)
        got = vm.ocr(image, font)
        self.assertEqual(got[0], 'Please unlock disk sda5_crypt:')
        self.assertEqual(got[1], 'No key available with this passphrase.')
        self.assertEqual(got[2], 'WrOS login: 0123456789 [hidden]')



if __name__ == "__main__":
    unittest.main()
