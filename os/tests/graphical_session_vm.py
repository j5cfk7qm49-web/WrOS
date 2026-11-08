"""Compositor lock/suspend in a no-NIC disposable installed VM.

The read-only fixture image (WROS-TRIAL) contains a debug gui-drm/gui-wgpu/
test-driver grimoire, Debian's cached debs/ and apt lists/. --source-tree supplies
the exact session and lock helpers under test. The private driver is injected
only into this disposable VM; neither a release binary nor an owner book is used.
"""
import argparse
import json
from pathlib import Path
import shlex
import time
import traceback
import rc_vm_phases as phases

def trial(ctx):
    state = phases.load_state(ctx.args.install_run)
    credentials = json.loads(Path(state['secrets']).read_text())
    disk = ctx.dir / 'disk.qcow2'
    phases.vm.create_overlay(ctx.args.base_disk, disk, base_format='qcow2')
    def attach(m):
        m.usb_stick(ctx.args.fixture, readonly=True, ident='trial')
        if ctx.args.test_suspend:
            m.args.extend(['-global', 'ICH9-LPC.disable_s3=0'])
    m = phases.boot_installed(ctx, state, 'graphical-trial', disk=disk, fmt='qcow2', extra=attach)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False)
    g.run('mkdir -p /mnt/trial && mount -o ro,nodev,nosuid LABEL=WROS-TRIAL /mnt/trial')
    _, installed = g.run("dpkg-query -W -f='${binary:Package} ${Version}\\n'")
    (ctx.dir / 'installed-packages.txt').write_text(installed)
    g.shell.put('/tmp/trial-sources.list', 'deb https://deb.debian.org/debian trixie main\n')
    g.run('mkdir -p /var/cache/wros-trial/archives/partial && cp /mnt/trial/debs/*.deb /var/cache/wros-trial/archives/', 90)
    rc, _ = g.run('apt-get -y --no-download --no-install-recommends -o Debug::NoLocking=1 '
          '-o Dir::Etc::sourcelist=/tmp/trial-sources.list -o Dir::Etc::sourceparts=- '
          '-o Dir::Cache::archives=/var/cache/wros-trial/archives -o Dir::State::lists=/mnt/trial/lists '
          'install cage foot libegl1 libgl1-mesa-dri libwayland-egl1 libgles2 fonts-dejavu-mono > /tmp/trial-apt.log 2>&1', 240, check=False)
    if rc:
        raise phases.Failure(g.run('tail -35 /tmp/trial-apt.log')[1])
    g.run('install -m755 /mnt/trial/grimoire /usr/bin/grimoire')
    launcher = (ctx.args.source_tree / 'os/scripts/wros-session').read_text()
    launcher = launcher.replace("if __name__ == '__main__':", "if __name__ == '__main__':\n    os.environ['WROS_TEST_DRIVER_SOCKET'] = '/run/user/%d/wros-vm-test.sock' % os.getuid()")
    launcher = launcher.replace('self.run(command, env=env, stdin=subprocess.DEVNULL, check=False)',
                                "self.run(command, env=env, stdin=subprocess.DEVNULL, stderr=open(self.runtime_dir / 'trial-window.log', 'w'), check=False)")
    g.shell.put('/usr/libexec/wros-session', launcher, '0755')
    helper = ctx.args.source_tree / 'os/scripts/wros-lock'
    g.shell.put('/usr/libexec/wros-lock', helper.read_bytes(), '0755')
    ctx.result['lock_helper_sha256'] = phases.vm.sha256_file(helper)
    ctx.result['fixture_sha256'] = phases.vm.sha256_file(ctx.args.fixture)
    ctx.result['editor_sha256'] = g.run('sha256sum /usr/bin/grimoire')[1].split()[0]
    g.shell.put('/tmp/trial-client.py', Path(__file__).with_name('graphical_driver.py').read_bytes(), '0644')
    user, backend = state['username'], ctx.args.backend
    ctx.result['backend'] = backend
    ctx.result['debug_fixture'] = True
    home = '/home/' + user
    g.run('rm -f ' + home + '/.local/state/wros/session.json')
    g.run('install -d -m700 -o ' + user + ' -g ' + user + ' ' + home + '/.config/wros')
    g.shell.put(home + '/.config/wros/session.json', json.dumps({'session': 'graphical', 'backend': backend}))
    g.run('chown ' + user + ':' + user + ' ' + home + '/.config/wros/session.json')
    m.type_text(user + '\n', state['layout'])
    g.wait_vcs(1, r'\[hidden\]', 30, what='trial-greeter-password')
    m.type_text(credentials['login'] + '\n', state['layout'])
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        rc, _ = g.run('test -S /run/user/1000/wros-vm-test.sock', check=False)
        if rc == 0:
            break
        time.sleep(1)
    ctx.check('the graphical writing app opened its private test socket', rc == 0)
    def drive(request=None, send_only=False):
        command = 'runuser -u ' + user + ' -- python3 /tmp/trial-client.py'
        if request is not None:
            command += ' ' + shlex.quote(json.dumps(request))
        if send_only:
            command += ' send-only'
        _, reply = g.run(command, 45)
        return reply
    drive()
    rc, _ = g.run('pgrep -x cage', check=False)
    if rc:
        _, detail = g.run('tail -80 /run/user/1000/trial-window.log; journalctl -b --no-pager -t wros-session -n30; cat /proc/cmdline; '
                         'ls -l /run/wros-session /run/user/1000; '
                         'pid=$(pgrep -xo grimoire); tr "\\0" "\\n" < /proc/$pid/environ | '
                         'grep -E "^(HOME|WROS_SESSION|XDG_RUNTIME_DIR|WROS_LIVE_SESSION)="', check=False)
        (ctx.dir / 'compositor-startup.log').write_text(detail)
        raise phases.Failure(detail[-6000:])
    ctx.check('the compositor backend is running', True)
    editor_pid = g.run('pgrep -xo grimoire')[1].strip()
    _, properties = g.run("for sid in $(loginctl list-sessions --no-legend | awk '$2 == \"1000\" && $4 == \"seat0\" {print $1}'); do loginctl show-session $sid -p User -p Active -p Remote -p TTY -p VTNr -p Seat -p Type -p Class; done")
    (ctx.dir / 'session-properties.txt').write_text(properties)
    m.shot('graphical-writing')
    drive({'kind':'act', 'node':'open-settings', 'action':'activate'})
    drive({'choose':'System'})
    drive({'choose':'Battery and power'})
    drive({'choose':'Lock writing session'}, True)
    try:
        g.wait_vcs(8, 'Press Enter to unlock', 45, what='compositor-lock')
    except phases.Failure:
        _, detail = g.run('cat /run/user/1000/trial-window.log; journalctl -b --no-pager -n90; '
                         'fgconsole; ps -eo pid,ppid,comm; loginctl list-sessions --no-legend', check=False)
        (ctx.dir / 'lock-failure.log').write_text(detail)
        if ctx.args.hold:
            phases.hold_for_operator(ctx, g)
        raise phases.Failure(detail[-9000:])
    ctx.check('the maintained locker owns tty8', g.run('fgconsole')[1].strip() == '8')
    m.key('alt', 'f1')
    ctx.check('the locker prevents returning to the compositor', g.run('fgconsole')[1].strip() == '8')
    ctx.check('the terminal input reader is retired', not any(p['comm']=='wros-input' for p in g.snap()['procs']))
    m.shot('compositor-locked')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password')
    wrong = 'trialwrongpassword'
    m.type_text(wrong+'\n', state['layout'])
    g.wait_vcs(8, r'\A(?!.*Password:).*Press Enter to unlock', 30, what='compositor-wrong-rejected')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password-retry')
    m.type_text(credentials['login']+'\n', state['layout'])
    deadline = time.monotonic() + 45
    while g.run('fgconsole')[1].strip() != '1' and time.monotonic() < deadline:
        time.sleep(1)
    tree = drive()
    ctx.check('authentication returns to the writing settings', 'Battery and power' in tree)
    m.shot('compositor-unlocked')
    drive({'choose':'Lock writing session'}, True)
    g.wait_vcs(8, 'Press Enter to unlock', 45, what='compositor-lock-again')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password-again')
    m.type_text(credentials['login']+'\n', state['layout'])
    time.sleep(3)
    ctx.check('a second lock returns to the same writing settings', 'Battery and power' in drive())
    ctx.check('the same editor process survived both locks', g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    if ctx.args.test_suspend:
        drive({'choose':'Lock and suspend'})
        drive({'kind':'key','key':'right'})
        drive({'kind':'key','key':'enter'}, True)
        deadline = time.monotonic() + 45
        while m.qmp.count('SUSPEND') == 0 and time.monotonic() < deadline:
            time.sleep(.2)
        ctx.check('the compositor requested actual QEMU suspend', m.qmp.count('SUSPEND') == 1)
        m.qmp.command('system_wakeup')
        time.sleep(4)
        g.wait_vcs(8, 'Press Enter to unlock', 30, what='compositor-wake-lock')
        m.key('ret')
        g.wait_vcs(8, 'Password:', 20, what='compositor-wake-password')
        m.type_text(credentials['login']+'\n', state['layout'])
        time.sleep(3)
        ctx.check('wake and authentication return to the same editor', 'Battery and power' in drive()
                  and g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    g.shell.put('/dev/shm/trial-pattern', wrong+'\n'+credentials['login']+'\n')
    rc, _ = g.run('grep -rlaF -f /dev/shm/trial-pattern /home >/dev/null; found=$?; '
                  'rm -f /dev/shm/trial-pattern; test "$found" = 1', check=False)
    ctx.check('neither lock password reached the saved book or recovery files', rc == 0)
    phases.secret_scan(ctx, g, credentials)
    ctx.result['passed'] = True

def main():
    import os
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('install-run', 'base-disk', 'fixture', 'source-tree'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--backend', choices=['foot', 'wgpu'], required=True)
    parser.add_argument('--test-suspend', action='store_true')
    parser.add_argument('--hold', action='store_true')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    ctx = phases.Ctx('graphical-' + args.backend, args)
    error = None
    try:
        trial(ctx)
    except Exception:
        error = traceback.format_exc()
        ctx.log(error)
    finally:
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
