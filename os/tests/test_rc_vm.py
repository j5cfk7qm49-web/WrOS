import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rc_vm as vm
import rc_vm_forms as forms
import rc_vm_phases as phases


class InstalledPowerPolicy(unittest.TestCase):
    def test_live_logind_checks_require_sleep_and_all_guarded_actions(self):
        properties = ('HandlePowerKey HandlePowerKeyLongPress HandleRebootKey HandleRebootKeyLongPress '
                      'HandleSuspendKey HandleSuspendKeyLongPress HandleHibernateKey HandleHibernateKeyLongPress '
                      'HandleLidSwitch HandleLidSwitchExternalPower HandleLidSwitchDocked IdleAction').split()
        capabilities = ('CanSuspend s "yes"\nCanHibernate s "na"\n'
                        'CanHybridSleep s "no"\nCanSuspendThenHibernate s "no"')

        def run(command, *_args, **_kwargs):
            if 'busctl call' in command:
                return 0, capabilities
            if 'busctl get-property' in command:
                for name in properties:
                    self.assertIn(name, command)
                return 0, '\n'.join(f'{name} s "ignore"' for name in properties)
            self.assertEqual(command, 'systemctl start systemd-suspend.service')
            return 1, 'systemd-suspend.service refused'

        checks = {}
        ctx = SimpleNamespace(result={}, check=lambda name, value, *_: checks.update({name: bool(value)}))
        guest = SimpleNamespace(run=run, m=SimpleNamespace(qmp=SimpleNamespace(count=lambda _: 0)))
        phases.feature_power_policy(ctx, guest)
        self.assertTrue(checks['CanSuspend is available'])
        self.assertTrue(all(checks.values()), checks)


class InstalledShutdown(unittest.TestCase):
    def test_update_shutdown_acknowledges_before_stopping_and_waits_for_qemu(self):
        steps = []

        def run(command, *_args, **_kwargs):
            self.assertRegex(command, r'\(sleep [1-9]; systemctl poweroff\).*& echo')
            steps.append('request')
            return 0, 'poweroff-requested'

        def wait_exit(timeout):
            self.assertGreater(timeout, 20)
            steps.append('exit')
            return 0

        guest = SimpleNamespace(run=run, m=SimpleNamespace(wait_exit=wait_exit))
        self.assertEqual(phases.poweroff(guest), 0)
        self.assertEqual(steps, ['request', 'exit'])


class RememberedMenu(unittest.TestCase):
    def test_a_row_above_the_remembered_selection_can_be_reached(self):
        guest = phases.Guest.__new__(phases.Guest)
        labels = ['Keyboard', 'Display', 'Battery and power', 'Storage']
        selected = [3]

        class Machine:
            def key(self, key):
                selected[0] = 0 if key == 'home' else min(selected[0] + 1, len(labels) - 1)

        guest.m = Machine()
        guest.vcs = lambda _: [forms.MARK + labels[selected[0]] + ' ›']
        self.assertEqual(guest.choose(r'^Display', limit=8), 'Display ›')


class DisplayVisibility(unittest.TestCase):
    def test_blank_frames_and_cursor_only_frames_are_rejected(self):
        from PIL import Image, ImageDraw
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp, 'frame.png')
            frame = Image.new('RGB', (240, 100), '#181411')
            frame.save(path)
            self.assertFalse(vm.frame_has_content(path))
            ImageDraw.Draw(frame).rectangle((0, 0, 7, 15), fill='#f9ab67')
            frame.save(path)
            self.assertFalse(vm.frame_has_content(path))
            ImageDraw.Draw(frame).text((0, 16), 'Press Enter to unlock.', fill='#f9ab67')
            frame.save(path)
            self.assertTrue(vm.frame_has_content(path))


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

    def test_real_string_form_with_footer_title(self):
        # captured from the real installer: empty field renders as a bare marker, footer title above F1 help
        text = screen('┌WrOS · Install────────', 'Full name for the new user:', 'A user account will be created.', '▸ ',
                      'Set up users and passwords · 1 of 1', '↑↓ choose  Enter continue  Esc back  F1 help', '└─────────')
        form = forms.parse_form(text)
        self.assertEqual(form.kind, 'string')
        self.assertEqual(forms.decide(form, forms.rules())[1], ('fill', 'RC Writer'))
        prefilled = forms.parse_form(screen('Hostname:', '▸ debian', 'Configure the network · 1 of 1', '↑↓ choose  Enter continue  Esc back  F1 help'))
        self.assertEqual(prefilled.kind, 'string')

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


class NativePixels(unittest.TestCase):
    """The unlock screen's native check: theme colours, not anti-aliasing."""

    def picture(self, colours):
        from PIL import Image
        image = Image.new('RGB', (len(colours), 1))
        image.putdata(colours)
        path = Path(tempfile.mkdtemp(prefix='native-pixels-')) / 'screen.png'
        image.save(path)
        return path

    def test_crisp_unifont_in_theme_colours_is_native(self):
        import rc_vm_phases
        path = self.picture([(0, 0, 0), (0, 255, 0), (242, 240, 255), (74, 69, 96), (143, 138, 168)])
        self.assertTrue(rc_vm_phases.native_pixels(path))

    def test_the_sixteen_colour_console_is_not(self):
        import rc_vm_phases
        path = self.picture(sorted(rc_vm_phases.CONSOLE_PALETTE))
        self.assertFalse(rc_vm_phases.native_pixels(path))


class InsideTheSettingsBox(unittest.TestCase):
    """rc13: Settings is a box over the page; helpers read only the box."""

    SCREEN = [
        "  WrOS  File  Edit  View  Book  Tools  Settings  Help        ✓ ",
        "┌ Outline ──────┐ ┌ WrOS · Settings › System ──┐ text beside",
        "│ Novel Format  │ │   Wi-Fi and networking ›    │ more prose",
        "│ Manuscript    │ │ ▸ Battery and power ›       │ │ prose here",
        "│ Chapter One   │ │                             │ even more",
        "└───────────────┘ └─────────────────────────────┘",
    ]

    def test_only_the_box_is_kept(self):
        box = forms.inside_box(self.SCREEN)
        self.assertTrue(box[0].startswith('┌ WrOS · Settings'), box)
        self.assertTrue(all(row.startswith(('│', '┌', '└')) for row in box), box)
        self.assertFalse(any('prose' in row or 'Outline' in row for row in box), box)

    def test_the_selected_row_ends_at_the_box_edge(self):
        row = next(l for l in forms.inside_box(self.SCREEN) if forms.MARK in l)
        self.assertEqual(row.split(forms.MARK, 1)[1].split('│', 1)[0].strip(), 'Battery and power ›')

    def test_an_outline_arrow_beside_the_box_is_not_its_highlight(self):
        # The outline draws collapsed folders with the same arrow, higher up.
        screen = [
            "  WrOS  File  Edit  View  Book  Tools  Settings  Help        ✓ ",
            "┌ Outline ─────────┐",
            "│  ▸ Manuscript Format┌ WrOS · Settings ──────┐",
            "│  ▸ Paperback        │▸ Writing ›            │ │",
            "│  ▸ Ebook            │  System ›             │ │",
            "└──────────────────── └───────────────────────┘",
        ]
        box = forms.inside_box(screen)
        self.assertTrue(box[0].startswith('┌ WrOS · Settings'), box)
        self.assertIn('▸ Writing ›', box[1])

    def test_a_full_screen_panel_is_left_alone(self):
        full = ["┌ Install WrOS ┐", "│ ▸ Yes        │", "└──────────────┘"]
        self.assertEqual(forms.inside_box(full), full)
