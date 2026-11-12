"""Unit tests for the sound volume helper (os/scripts/wros-sound)."""
import contextlib
import importlib.machinery
import io
import json
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wros-sound'
sound = importlib.machinery.SourceFileLoader('wros_sound', str(SCRIPT)).load_module()


class FakeMixer:
    """A Master control with the HDA Intel range, 0..87, and a mute switch."""

    def __init__(self, raw=60, on=True, switch=True, fail=False):
        self.raw, self.on, self.switch, self.fail = raw, on, switch, fail
        self.closed = False

    def volume_range(self):
        return 0, 87

    def volume(self):
        return self.raw

    def set_volume(self, raw):
        if self.fail:
            raise OSError('Could not change the volume.')
        self.raw = raw

    def has_switch(self):
        return self.switch

    def switch_on(self):
        return self.on

    def set_switch(self, on):
        self.on = on

    def close(self):
        self.closed = True


def call(argv, mixer):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = sound.main(argv, open_mixer=lambda: mixer)
    return code, json.loads(out.getvalue())


class SoundHelperTests(unittest.TestCase):
    def test_reads_volume_as_a_percentage_of_the_card_range(self):
        mixer = FakeMixer(raw=60)
        code, reply = call(['status'], mixer)
        self.assertEqual(code, 0)
        self.assertEqual(reply, {'available': True, 'volume_percent': 69, 'muted': False,
                                 'message': 'Volume 69%'})
        self.assertTrue(mixer.closed)

    def test_setting_a_volume_unmutes_and_lands_on_the_nearest_step_the_card_has(self):
        mixer = FakeMixer(raw=0, on=False)
        _, reply = call(['set', '50'], mixer)
        self.assertEqual(mixer.raw, 44)
        # 87 steps cannot show exactly 50%; the nearest one is reported, not the request.
        self.assertEqual((reply['volume_percent'], reply['muted']), (51, False))
        for asked in range(0, 101, 10):
            _, reply = call(['set', str(asked)], mixer)
            self.assertLessEqual(abs(reply['volume_percent'] - asked), 1)
        _, reply = call(['set', '100'], mixer)
        self.assertEqual((mixer.raw, reply['volume_percent']), (87, 100))
        _, reply = call(['set', '0'], mixer)
        self.assertEqual((mixer.raw, reply['volume_percent']), (0, 0))

    def test_mute_uses_the_switch_and_keeps_the_volume(self):
        mixer = FakeMixer(raw=60)
        _, reply = call(['mute'], mixer)
        self.assertEqual((reply['volume_percent'], reply['muted']), (69, True))
        self.assertEqual(reply['message'], 'Volume 69% · muted')
        _, reply = call(['unmute'], mixer)
        self.assertFalse(reply['muted'])

    def test_up_and_down_move_one_step_and_up_unmutes(self):
        mixer = FakeMixer(raw=60, on=False)
        _, reply = call(['up'], mixer)
        self.assertEqual((reply['volume_percent'], reply['muted']), (74, False))
        _, reply = call(['down'], mixer)
        self.assertEqual(reply['volume_percent'], 69)
        mixer.on = False
        _, reply = call(['down'], mixer)
        self.assertEqual((reply['volume_percent'], reply['muted']), (64, True))

    def test_up_and_down_stop_at_the_ends(self):
        mixer = FakeMixer(raw=87)
        _, reply = call(['up'], mixer)
        self.assertEqual((mixer.raw, reply['volume_percent']), (87, 100))
        mixer.raw = 1
        _, reply = call(['down'], mixer)
        self.assertEqual((mixer.raw, reply['volume_percent']), (0, 0))
        _, reply = call(['down'], mixer)
        self.assertEqual(mixer.raw, 0)

    def test_toggle_flips_the_mute_switch(self):
        mixer = FakeMixer(raw=60)
        _, reply = call(['toggle'], mixer)
        self.assertEqual((reply['volume_percent'], reply['muted']), (69, True))
        _, reply = call(['toggle'], mixer)
        self.assertFalse(reply['muted'])
        no_switch = FakeMixer(raw=60, switch=False)
        _, reply = call(['toggle'], no_switch)
        self.assertEqual((reply['volume_percent'], reply['muted']), (0, True))

    def test_a_card_without_a_mute_switch_mutes_by_volume_zero(self):
        mixer = FakeMixer(raw=60, switch=False)
        _, reply = call(['mute'], mixer)
        self.assertEqual((reply['volume_percent'], reply['muted']), (0, True))

    def test_refuses_anything_but_the_four_commands(self):
        for bad in ([], ['set'], ['set', '101'], ['set', '-1'], ['set', '5%'], ['set', '0050'],
                    ['sset', 'Master', '50%'], ['status', 'extra'], ['mute', 'now']):
            err = io.StringIO()
            with self.subTest(bad=bad), contextlib.redirect_stderr(err):
                self.assertEqual(sound.main(bad, open_mixer=self.fail_if_opened), 2)
                self.assertIn('usage', err.getvalue())

    @staticmethod
    def fail_if_opened():
        raise AssertionError('the mixer was opened for a refused command')

    def test_a_missing_card_or_failed_change_is_reported_not_raised(self):
        def no_card():
            raise OSError('Could not find a sound card.')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(sound.main(['status'], open_mixer=no_card), 0)
        reply = json.loads(out.getvalue())
        self.assertEqual((reply['available'], reply['volume_percent']), (False, None))
        self.assertIn('sound card', reply['message'])
        mixer = FakeMixer(fail=True)
        _, reply = call(['set', '40'], mixer)
        self.assertFalse(reply['available'])
        self.assertTrue(mixer.closed)

    def test_real_libasound_without_a_sound_card_answers_unavailable(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(sound.main(['status']), 0)
        reply = json.loads(out.getvalue())
        self.assertEqual(set(reply), {'available', 'volume_percent', 'muted', 'message'})
        if not reply['available']:
            self.assertIsNone(reply['volume_percent'])


if __name__ == '__main__':
    unittest.main()
