"""The recovery menu with a scripted console, a fake engine and a recording runner.

Nothing here authenticates for real, touches the update state, or runs a command.
"""
import importlib.machinery
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
module = importlib.machinery.SourceFileLoader('wros_recovery', str(ROOT / 'os/scripts/wros-recovery')).load_module()

WRITER = module.Account('ada', 1000, Path('/home/ada'))
SNAPSHOT = 'a' * 64


class Console:
    """Answers come from a script; running out of answers ends the session."""

    def __init__(self, answers):
        self.answers, self.out, self.secrets_asked = list(answers), [], []

    def say(self, text=''):
        self.out.append(text)

    def ask(self, prompt):
        self.out.append(prompt)
        if not self.answers:
            raise EOFError
        return self.answers.pop(0)

    def secret(self, prompt):
        self.secrets_asked.append(prompt)
        self.out.append(prompt)  # the prompt shows; what is typed never does
        if not self.answers:
            raise EOFError
        return self.answers.pop(0)

    def text(self):
        return '\n'.join(self.out)


class Engine:
    def __init__(self, previous='0.1.0~rc4'):
        self.calls = []
        self.status = {'previous_version': previous, 'update_state': 'idle', 'went_back': False,
                       'went_back_automatically': False, 'interrupted_dpkg': False,
                       'status': 'dpkg: clean · no offline update staged.'}
        self.rollback_error = None

    def inspect_recovery(self):
        self.calls.append('inspect')
        return dict(self.status)

    def stage_rollback(self):
        self.calls.append('rollback')
        if self.rollback_error:
            raise ValueError(self.rollback_error)
        return {'status': 'staged', 'rollback': True, 'restart_required': True}

    def repair_updates(self):
        self.calls.append('repair')
        return {'status': 'repaired', 'message': 'Package state is consistent.'}


class Runner:
    def __init__(self, replies=None):
        self.calls, self.replies = [], dict(replies or {})

    def __call__(self, argv, *, stdin=None):
        self.calls.append((list(argv), stdin))
        key = argv[-1] if argv[0] != '/usr/sbin/runuser' else json.loads(stdin)['operation']
        return self.replies.get(key, (0, ''))


def helper_reply(data):
    return 0, json.dumps({'ok': True, 'data': data})


class RecoveryMenu(unittest.TestCase):
    def make(self, answers, *, password='right', engine=None, runner=None, accounts=(WRITER,)):
        self.console = Console(answers)
        self.engine = engine or Engine()
        self.runner = runner or Runner()
        self.attempts, self.sleeps = [], []

        def authenticate(user, secret):
            self.attempts.append((user, secret))
            return secret == password

        menu = module.Recovery(self.console, authenticate=authenticate, engine=self.engine, run=self.runner,
                               accounts=lambda: list(accounts), sleep=self.sleeps.append,
                               release=lambda: '0.1.0~rc5')
        try:
            menu.session()
        except EOFError:
            pass
        return menu

    def commands(self):
        return [argv for argv, _ in self.runner.calls]

    def test_nothing_is_offered_before_the_writers_password(self):
        self.make(['wrong', 'wrong', 'wrong', 'right', '7'])
        self.assertEqual([user for user, _ in self.attempts], ['ada'] * 4)
        self.assertEqual(self.sleeps, [module.LOCKOUT_SECONDS])  # three misses, then a pause
        text = self.console.text()
        self.assertLess(text.rindex('Password for ada'), text.index('Go back to 0.1.0~rc4'))
        self.assertEqual(set(self.engine.calls), {'inspect'})  # the status is only read

    def test_several_accounts_name_the_one_to_unlock(self):
        other = module.Account('bo', 1001, Path('/home/bo'))
        self.make(['bo', 'right'], accounts=(WRITER, other))
        self.assertEqual(self.attempts, [('bo', 'right')])
        self.make(['nobody', 'right'], accounts=(WRITER, other))
        self.assertEqual(self.attempts, [])  # an unknown name is refused before PAM

    def test_the_menu_offers_exactly_the_recovery_choices_and_never_a_shell(self):
        menu = self.make(['right'])
        labels = [label for _, label, _ in menu.choices()]
        self.assertEqual(labels, ['Go back to 0.1.0~rc4 · installs when you restart',
                                  'Repair system packages',
                                  'Restore a book from a backup',
                                  'Show what happened',
                                  'Continue to the normal login',
                                  'Restart',
                                  'Power off'])
        self.assertNotIn('shell', ' '.join(labels).lower())

    def test_going_back_is_offered_only_when_a_previous_version_is_kept(self):
        menu = self.make(['right'], engine=Engine(previous=None))
        labels = [label for _, label, _ in menu.choices()]
        self.assertFalse(any(label.startswith('Go back') for label in labels))

    def test_going_back_stages_the_kept_version_and_restarts_only_when_asked(self):
        self.make(['right', '1', 'n'])
        self.assertIn('rollback', self.engine.calls)
        self.assertNotIn(['/usr/bin/systemctl', 'reboot'], self.commands())
        self.assertIn('installs when you restart', self.console.text())
        self.make(['right', '1', 'y'])
        self.assertIn(['/usr/bin/systemctl', 'reboot'], self.commands())

    def test_a_refused_go_back_is_said_and_the_menu_stays(self):
        engine = Engine()
        engine.rollback_error = 'another offline update is already pending; original preserved'
        self.make(['right', '1', '7'], engine=engine)
        self.assertIn('another offline update is already pending', self.console.text())
        self.assertIn(['/usr/bin/systemctl', 'poweroff'], self.commands())

    def test_repair_runs_the_engines_bounded_repair(self):
        self.make(['right', '2'])
        self.assertIn('repair', self.engine.calls)
        self.assertIn('Package state is consistent.', self.console.text())

    def test_restore_runs_the_backup_helper_as_the_writer_into_a_new_folder(self):
        snapshots = {'repository': '/home/ada/Backups/wros', 'snapshots': [
            {'id': SNAPSHOT, 'time': '2026-11-04T21:10:00-08:00', 'path': '/home/ada/.local/share/grimoire/wros/backup/wros-backup-x/book', 'project': 'Clip',
             'restore_checked_at': '2026-11-04T21:11:00-08:00'}]}
        runner = Runner({'list': helper_reply(snapshots), 'restore': helper_reply({'notice': 'Restore verified'})})
        self.make(['right', '3', '1'], runner=runner)
        helper = [(argv, json.loads(stdin)) for argv, stdin in self.runner.calls if argv[0] == '/usr/sbin/runuser']
        self.assertEqual([request['operation'] for _, request in helper], ['list', 'restore'])
        argv, request = helper[1]
        self.assertEqual(argv[:4], ['/usr/sbin/runuser', '-u', 'ada', '--'])
        self.assertIn('HOME=/home/ada', argv)
        self.assertEqual(request['snapshot'], SNAPSHOT)
        destination = Path(request['destination'])
        self.assertEqual(destination.parent, Path('/home/ada/Restored books'))
        self.assertTrue(destination.name.startswith('Clip 2026-11-04'))
        self.assertIn('Restore verified', self.console.text())

    def test_restore_offers_the_settings_saved_with_the_backup(self):
        snapshots = {'repository': '/home/ada/Backups/wros', 'snapshots': [
            {'id': SNAPSHOT, 'time': '2026-11-04T21:10:00-08:00', 'path': '/home/ada/.local/share/grimoire/wros/backup/wros-backup-x/book', 'project': 'Clip',
             'restore_checked_at': '', 'has_profile': True}]}
        for answer, wanted in (('y', True), ('n', False)):
            runner = Runner({'list': helper_reply(snapshots), 'restore': helper_reply({'notice': 'Restored'})})
            self.make(['right', '3', '1', answer], runner=runner)
            request = json.loads(self.runner.calls[-1][1])
            self.assertEqual(request['operation'], 'restore')
            self.assertEqual(request.get('settings', False), wanted)
        self.assertIn('Restore your settings too', self.console.text())

    def test_a_drive_backup_asks_before_starting_the_network(self):
        snapshots = {'repository': 'drive:WrOS-Backups', 'snapshots': []}
        runner = Runner({'list': helper_reply(snapshots)})
        self.make(['right', '3', 'n'], runner=runner)
        self.assertNotIn(['/usr/bin/systemctl', 'start', 'NetworkManager.service'], self.commands())
        self.assertIn('Google Drive', self.console.text())
        self.make(['right', '3', 'y'], runner=Runner({'list': helper_reply(snapshots)}))
        self.assertIn(['/usr/bin/systemctl', 'start', 'NetworkManager.service'], self.commands())

    def test_show_what_happened_reads_the_update_status_and_the_failed_boots_errors(self):
        engine = Engine()
        engine.status['went_back_automatically'] = True
        runner = Runner({'--no-pager': (0, 'Nov 05 greetd[612]: greeter exited with status 101')})
        self.make(['right', '4'], engine=engine, runner=runner)
        text = self.console.text()
        self.assertIn('WrOS 0.1.0~rc5', text)
        self.assertIn('dpkg: clean', text)
        self.assertIn('went back to the previous version by itself', text)
        self.assertIn('greeter exited with status 101', text)
        journal = next(argv for argv in self.commands() if argv[0] == '/usr/bin/journalctl')
        self.assertIn('-b', journal)
        self.assertIn('-1', journal)

    def test_continue_restart_and_power_off_hand_over_to_systemd(self):
        for answer, command in (('5', ['/usr/bin/systemctl', 'isolate', 'default.target']),
                                ('6', ['/usr/bin/systemctl', 'reboot']),
                                ('7', ['/usr/bin/systemctl', 'poweroff'])):
            self.make(['right', answer])
            self.assertEqual(self.commands()[-1], command)

    def test_the_version_comes_from_the_release_files_version_field(self):
        # Seen in the VM: the whole file was printed after "This computer runs WrOS".
        text = ('WROS_VERSION=0.1.0~rc5\nWROS_COMMIT=2a2bb2bf\nBUILD_DATE=2026-11-06\n'
                'GRIMOIRE_BASE=0.7.5\nBASE_COMMIT=f3ca1e8\n')
        self.assertEqual(module.release_version(text), '0.1.0~rc5')
        with self.assertRaises(ValueError):
            module.release_version('WROS_COMMIT=2a2bb2bf\n')

    def test_an_unknown_choice_is_refused_and_asked_again(self):
        self.make(['right', 'sh', '0', '9', '7'])
        self.assertEqual(self.commands(), [['/usr/bin/systemctl', 'poweroff']])
        self.assertIn('Choose a number from the list.', self.console.text())


if __name__ == '__main__':
    unittest.main()
