"""The login screen's power menu in a disposable installed VM.

Boots a qcow2 overlay of an installed disk (never the disk itself), records
whether logind authorizes the login screen's own process to power off and
restart, then, with --fixture, installs the greeter under test from a read-only
WROS-TRIAL image and drives the menu: Back to login and Escape change nothing,
Escape on the form clears it in place, Restart boots the guest again, and
Shut down ends QEMU.
"""
import argparse
import json
from pathlib import Path
import shlex
import time
import traceback
import rc_vm_phases as phases


ACTIONS = ('org.freedesktop.login1.power-off', 'org.freedesktop.login1.reboot',
           'org.freedesktop.login1.power-off-multiple-sessions',
           'org.freedesktop.login1.reboot-multiple-sessions')
LOGIN_READY = phases.GREETER_READY
POWER_MENU = r'(?=.*WrOS · Power)(?=.*Back to login)(?=.*Shut down)(?=.*Restart)'


def authorization(ctx, g, label):
    """The greeter's logind session and polkit's answer for its process."""
    g.wait_vcs(1, LOGIN_READY, 240, what=label + '-login')
    pid = g.run('pgrep -xo wros-greeter')[1].strip()
    session = g.run('ps -o lsession= -p ' + pid, check=False)[1].strip()
    _, detail = g.run('loginctl show-session ' + shlex.quote(session) +
                      ' -p Class -p Type -p Active -p Remote -p Seat -p VTNr -p Name', check=False)
    answers = {}
    for action in ACTIONS:
        rc, _ = g.run('pkcheck --action-id %s --process %s >/dev/null 2>&1' % (action, pid), check=False)
        answers[action] = rc
    sessions = g.run('loginctl list-sessions --no-legend', check=False)[1]
    ctx.result[label + '_authorization'] = {'session': session, 'session_properties': detail.strip(),
                                            'pkcheck_exit': answers, 'sessions': sessions.strip()}
    ctx.log('%s: session %s %s; pkcheck %s' % (label, session, detail.strip().replace('\n', ' '), answers))
    return answers


def cursor(g):
    """The journal position now; the next boot reads only what came after it."""
    _, out = g.run('journalctl -n0 --show-cursor --no-pager | sed -n "s/^-- cursor: //p"')
    return out.strip()


def login_journal_after(g, position):
    """The login screen's syslog and greetd's output after `position`, in the previous boot."""
    rc, out = g.run('journalctl -b -1 --no-pager -o cat --after-cursor=%s '
                    'SYSLOG_IDENTIFIER=wros-greeter + _SYSTEMD_UNIT=greetd.service | tail -40'
                    % shlex.quote(position), check=False)
    if rc:
        raise phases.Failure('the previous boot journal is unavailable')
    return out.strip()


def open_menu(g, m):
    g.wait_vcs(1, LOGIN_READY, 120, what='login-before-menu')
    m.key('f1')
    return g.wait_vcs(1, POWER_MENU, 30, what='power-menu')


def trial(ctx):
    a = ctx.args
    state = phases.load_state(a.install_run)
    credentials = json.loads(Path(state['secrets']).read_text())
    disk = ctx.dir / 'disk.qcow2'
    phases.vm.create_overlay(a.base_disk, disk, base_format='qcow2')

    def attach(machine):
        if a.fixture:
            machine.usb_stick(a.fixture, readonly=True, ident='trial')

    m = phases.boot_installed(ctx, state, 'greeter-power', disk=disk, fmt='qcow2', extra=attach, reboot=True)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False)
    ctx.guest = g
    authorization(ctx, g, 'installed')
    if not a.fixture:
        ctx.result['passed'] = True
        return
    g.run('mkdir -p /mnt/trial && mount -o ro,nodev,nosuid LABEL=WROS-TRIAL /mnt/trial')
    g.run('install -m755 /mnt/trial/wros-greeter /usr/lib/wros/wros-greeter && '
          'install -m755 /mnt/trial/wros-greeter /usr/bin/wros-greeter && umount /mnt/trial && systemctl restart greetd')
    ctx.result['greeter_sha256'] = g.run('sha256sum /usr/bin/wros-greeter')[1].split()[0]
    answers = authorization(ctx, g, 'fixture')
    ctx.check('logind authorizes the login screen to shut down and restart',
              all(code == 0 for code in answers.values()), answers)
    login = g.wait_vcs(1, LOGIN_READY, 60, what='login-hint')
    ctx.check('the login hint offers the power menu', 'F1: power' in login, login)

    baseline = m.qmp.count('RESET')
    # Typed account text must not survive into the menu or back out of it.
    m.type_text('powercheck', state['layout'])
    g.wait_vcs(1, 'powercheck', 15, what='typed-account')
    menu = open_menu(g, m)
    m.shot('power-menu')
    ctx.check('the menu opens with Back to login selected', '▸ Back to login' in menu, menu)
    m.key('ret')
    login = g.wait_vcs(1, LOGIN_READY, 30, what='back-to-login')
    ctx.check('Back to login returns to an empty form', 'powercheck' not in login, login)
    open_menu(g, m)
    m.key('down')
    g.wait_vcs(1, '▸ Shut down', 15, what='shutdown-selected')
    m.key('esc')
    g.wait_vcs(1, LOGIN_READY, 30, what='escape-to-login')
    time.sleep(3)
    ctx.check('Back to login and Escape request nothing', m.is_running() and m.qmp.count('RESET') == baseline)

    # Escape on the form clears it in place; the login screen must not exit and restart.
    greeter = g.run('pgrep -xo wros-greeter')[1].strip()
    m.type_text('esccheck', state['layout'])
    g.wait_vcs(1, 'esccheck', 15, what='typed-before-escape')
    m.key('esc')
    time.sleep(3)
    login = g.wait_vcs(1, LOGIN_READY, 30, what='escape-on-form')
    after = g.run('pgrep -xo wros-greeter', check=False)[1].strip()
    ctx.check('Escape on the form clears it without restarting the login screen',
              'esccheck' not in login and after == greeter, {'before': greeter, 'after': after})

    position = cursor(g)
    resets = m.qmp.count('RESET')
    open_menu(g, m)
    m.key('down')
    m.key('down')
    g.wait_vcs(1, '▸ Restart', 15, what='restart-selected')
    m.key('ret')
    phases.wait_reset(ctx, m, resets, 180, 'greeter-restart')
    ctx.check('Restart from the login screen reboots the guest', True)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False, after_reset=resets + 1)
    ctx.guest = g
    journal = login_journal_after(g, position)
    ctx.result['restart_login_journal'] = journal[-3000:]
    _, context = g.run('journalctl -b -1 --no-pager -o short-monotonic --after-cursor=%s | '
                       'grep -i -E "greet|request|reboot|session" | tail -60' % shlex.quote(position), check=False)
    (ctx.dir / 'restart-journal.txt').write_text(context)
    ctx.check('the login screen records the accepted restart',
              'restart requested from the login screen' in journal, journal)
    ctx.check('the accepted restart is not reported as a failed login',
              'login interrupted' not in journal and 'login unavailable' not in journal, journal)

    open_menu(g, m)
    m.key('down')
    g.wait_vcs(1, '▸ Shut down', 15, what='shutdown-selected-2')
    m.key('ret')
    code = m.wait_exit(180)
    ctx.check('Shut down from the login screen powers the guest off', code == 0, code)
    ctx.result['passed'] = True


def main():
    import os
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', type=Path, required=True)
    parser.add_argument('--base-disk', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, help='read-only WROS-TRIAL image with the greeter under test')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    ctx = phases.Ctx('greeter-power', args)
    error = None
    try:
        trial(ctx)
    except Exception:
        error = traceback.format_exc()
        ctx.log(error)
        if hasattr(ctx, 'guest') and ctx.guest.m.is_running():
            try:
                ctx.guest.m.shot('failure-final')
                _, diagnostic = ctx.guest.run('fgconsole; ps -eo pid,user,stat,comm; loginctl list-sessions --no-legend; '
                                              'journalctl -b --no-pager -t wros-greeter -u greetd -n60', check=False)
                (ctx.dir / 'failure-state.log').write_text(diagnostic)
            except Exception as failure:
                ctx.log('Failure-state capture unavailable: ' + type(failure).__name__)
    finally:
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
