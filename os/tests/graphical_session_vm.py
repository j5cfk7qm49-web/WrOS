"""Graphical input, lock and suspend in a no-NIC disposable installed VM.

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
        if ctx.args.backend == 'drm':
            m.args.extend(['-device', 'usb-tablet,bus=xhci.0,id=trial-pointer'])
        if ctx.args.test_suspend:
            m.args.extend(['-global', 'ICH9-LPC.disable_s3=0'])
    m = phases.boot_installed(ctx, state, 'graphical-trial', disk=disk, fmt='qcow2', extra=attach)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False)
    ctx.guest = g
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
    ctx.writer = user
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
        if not send_only:
            (ctx.dir / 'last-tree.json').write_text(reply)
        return reply
    tree = drive()
    if backend == 'drm':
        ctx.check('the native session uses the device reader', 'evdev' in tree)
    rc, _ = g.run('pgrep -x cage', check=False)
    if rc and backend != 'drm':
        _, detail = g.run('tail -80 /run/user/1000/trial-window.log; journalctl -b --no-pager -t wros-session -n30; cat /proc/cmdline; '
                         'ls -l /run/wros-session /run/user/1000; '
                         'pid=$(pgrep -xo grimoire); tr "\\0" "\\n" < /proc/$pid/environ | '
                         'grep -E "^(HOME|WROS_SESSION|XDG_RUNTIME_DIR|WROS_LIVE_SESSION)="', check=False)
        (ctx.dir / 'compositor-startup.log').write_text(detail)
        raise phases.Failure(detail[-6000:])
    ctx.check('the graphical backend is running', rc == 0 or backend == 'drm')
    editor_pid = g.run('pgrep -xo grimoire')[1].strip()
    def native_devices(pid='$(pgrep -xo wros-input)'):
        return g.run('for fd in /proc/' + pid + '/fd/*; do readlink "$fd"; done')[1]
    if backend == 'drm':
        ctx.check('only the disposable reader owns native input devices', '/dev/input/event' in native_devices()
                  and '/dev/input/event' not in native_devices(editor_pid))
        def layout(variant):
            g.shell.put('/etc/default/keyboard', 'XKBMODEL="pc105"\nXKBLAYOUT="us"\n'
                        f'XKBVARIANT="{variant}"\nXKBOPTIONS="compose:ralt"\n', '0644')
            time.sleep(1)
        layout('colemak')
        drive({'kind':'act', 'node':'open-settings', 'action':'activate'})
        for _ in range(2):
            drive({'kind':'key', 'key':'escape'})
        drive({'kind':'key', 'key':'ctrl+k'})
        drive({'kind':'text', 'text':'new scene'})
        drive({'kind':'key', 'key':'enter'})
        drive({'kind':'text', 'text':'native-input'})
        drive({'kind':'key', 'key':'enter'})
        m.type_text('colemak ', 'colemak')
        m.key('alt_r')
        m.key('apostrophe')
        m.key('k')
        m.key('spc')
        layout('')
        m.type_text('qwerty ', 'us')
        m.key('x', hold=650, gap=.85)
        m.ctrl('s', 'us')
        time.sleep(1)
        ctx.check('physical Colemak, compose and live layout changes reach the saved scene',
                  bool(g.probe('find-text', 'colemak é qwerty xxxxx', home)))
        ctx.check('the private tree contains no typed prose', 'colemak é qwerty' not in drive())
        layout('colemak')
        drive({'kind':'key', 'key':'ctrl+d'})  # Focus mode removes animated scenery.
        time.sleep(.5)
        from PIL import Image, ImageChops
        def editor_pixels(name):
            with Image.open(m.shot(name)) as image:
                return image.convert('RGB').crop((0, 0, image.width, image.height - 24))
        before = editor_pixels('native-before-pointer')
        time.sleep(.5)
        ctx.check('the pointer baseline is a static writing page',
                  ImageChops.difference(before, editor_pixels('native-pointer-baseline')).getbbox() is None)
        m.qmp.command('input-send-event', events=[
            {'type':'abs', 'data':{'axis':'x','value':8192}},
            {'type':'abs', 'data':{'axis':'y','value':16384}}])
        time.sleep(1)
        changed = ImageChops.difference(before, editor_pixels('native-pointer')).getbbox()
        ctx.check('native pointer motion draws the bounded cursor overlay', changed is not None
                  and changed[2] - changed[0] <= 10 and changed[3] - changed[1] <= 16)
        m.key('ctrl', 'alt', 'f2')
        time.sleep(2)
        ctx.check('native input and display release for another VT', g.run('fgconsole')[1].strip() == '2')
        ctx.check('the inactive reader has released its input devices', '/dev/input/event' not in native_devices())
        m.key('ctrl', 'alt', 'f1')
        time.sleep(2)
        ctx.check('native devices return with tty1', g.run('fgconsole')[1].strip() == '1'
                  and '/dev/input/event' in native_devices())
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
    ctx.check('the locker prevents returning to the writing screen', g.run('fgconsole')[1].strip() == '8')
    ctx.check('the writing input reader is retired', not any(p['comm']=='wros-input' for p in g.snap()['procs']))
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
    (ctx.dir / 'before-lock-2.json').write_text(drive())
    drive({'choose':'Lock writing session'}, True)
    g.wait_vcs(8, 'Press Enter to unlock', 45, what='compositor-lock-again')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password-again')
    m.type_text(credentials['login']+'\n', state['layout'])
    time.sleep(3)
    ctx.check('a second lock returns to the same writing settings', 'Battery and power' in drive())
    ctx.check('the same editor process survived both locks', g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    if backend == 'drm':
        ctx.check('a fresh native reader owns devices after authentication', '/dev/input/event' in native_devices())
    for cycle in range(3, ctx.args.lock_cycles + 1):
        (ctx.dir / f'before-lock-{cycle}.json').write_text(drive())
        drive({'choose':'Lock writing session'}, True)
        g.wait_vcs(8, 'Press Enter to unlock', 45, what=f'graphical-lock-{cycle}')
        m.key('ret')
        g.wait_vcs(8, 'Password:', 20, what=f'graphical-password-{cycle}')
        m.type_text(credentials['login']+'\n', state['layout'])
        time.sleep(3)
        ctx.check(f'lock cycle {cycle} returns to the same editor', 'Battery and power' in drive()
                  and g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    if ctx.args.test_suspend:
        drive({'choose':'Lock and suspend'})
        drive({'kind':'key','key':'right'})
        drive({'kind':'key','key':'enter'}, True)
        deadline = time.monotonic() + 45
        while m.qmp.count('SUSPEND') == 0 and time.monotonic() < deadline:
            time.sleep(.2)
        ctx.check('the writing session requested actual QEMU suspend', m.qmp.count('SUSPEND') == 1)
        m.qmp.command('system_wakeup')
        time.sleep(4)
        g.wait_vcs(8, 'Press Enter to unlock', 30, what='compositor-wake-lock')
        m.key('ret')
        g.wait_vcs(8, 'Password:', 20, what='compositor-wake-password')
        m.type_text(credentials['login']+'\n', state['layout'])
        time.sleep(3)
        ctx.check('wake and authentication return to the same editor', 'Battery and power' in drive()
                  and g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    if backend == 'drm':
        ctx.check('native input reacquires devices after wake', '/dev/input/event' in native_devices())
        g.run('pkill -KILL -x wros-input')
        g.wait_vcs(1, 'Local account', 45, what='native-reader-crash-greeter')
        ctx.check('reader death ends the writer and restores the graphical login',
                  not any(p['comm'] in ('grimoire', 'wros-input', 'vlock-main') for p in g.snap()['procs']))
        ctx.check('reader death preserves the saved scene', bool(g.probe('find-text', 'colemak é qwerty xxxxx', home)))
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
    parser.add_argument('--backend', choices=['drm', 'foot', 'wgpu'], required=True)
    parser.add_argument('--test-suspend', action='store_true')
    parser.add_argument('--lock-cycles', type=int, default=2, choices=range(2, 13))
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
        if hasattr(ctx, 'guest'):
            try:
                _, diagnostic = ctx.guest.run(
                    'fgconsole; ps -eo pid,ppid,stat,comm; '
                    'systemctl show wros-console-lock.service -p ActiveState -p SubState -p Result; '
                    'journalctl -b --no-pager -u wros-console-lock.service -n50; '
                    'journalctl -b --no-pager -t wros-session -n50; '
                    'tail -40 /run/user/1000/trial-window.log', check=False)
                (ctx.dir / 'failure-state.log').write_text(diagnostic)
                _, tree = ctx.guest.run('runuser -u ' + shlex.quote(ctx.writer) + ' -- python3 /tmp/trial-client.py', 30, check=False)
                (ctx.dir / 'failure-tree.json').write_text(tree)
            except Exception as failure:
                ctx.log('Failure-state capture unavailable: ' + type(failure).__name__)
    finally:
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
