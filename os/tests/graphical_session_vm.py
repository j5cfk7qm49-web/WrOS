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
import shutil
import time
import traceback
import rc_vm_phases as phases


def wait_for_startup_picture(ctx, machine):
    from PIL import Image

    # Use raw captures to sample the brief first frame without PNG encoding.
    path = ctx.dir / 'startup-frame.ppm'
    deadline = time.monotonic() + 15
    try:
        while time.monotonic() < deadline:
            machine.qmp.command('screendump', filename=str(path), format='ppm')
            with Image.open(path) as picture:
                w, h = picture.size
                centre = picture.crop((w // 4, h // 4, w * 3 // 4, h * 3 // 4))
                colours = centre.getcolors(65536) or []
                # The logo has a range of gold shades with a fixed red/blue
                # relationship. One text colour on a login or writing frame
                # does not satisfy this check.
                gold = [rgb for _, rgb in colours if 217 <= rgb[0] <= 237
                        and 149 <= rgb[1] <= 221 and abs(rgb[2] - (91 + 3 * (rgb[0] - 219))) <= 3]
                if len(gold) >= 16:
                    picture.save(ctx.dir / 'graphical-startup.png')
                    return True
            time.sleep(.025)
        return False
    finally:
        path.unlink(missing_ok=True)


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
    ctx.writer = state['username']
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
    if ctx.args.test_clock:
        g.run('install -m755 /mnt/trial/wros-greeter /usr/lib/wros/wros-greeter && '
              'install -m755 /mnt/trial/wros-greeter /usr/bin/wros-greeter && systemctl restart greetd')
        ctx.result['greeter_sha256'] = g.run('sha256sum /usr/lib/wros/wros-greeter')[1].split()[0]
        login = g.wait_vcs(1, r'(?=.*\d\d:\d\d)(?=.*Local account)(?=.*Password)(?=.*Console session)(?=.*maintenance)',
                           45, what='large-login-clock')
        (ctx.dir / 'clock-login.txt').write_text(login)
        ctx.check('the graphical login clock keeps the whole form visible',
                  'Local account' in login and 'Password' in login and 'Console session' in login)
        ctx.check('the clock login uses the native graphical display',
                  bool(g.probe('vcs', '1').get('graphics')))
        # Retain the exact image whose complete form was decoded, rather than
        # making an unrelated screenshot after the screen assertion.
        shutil.copyfile(ctx.dir / 'native-tty1.png', ctx.dir / 'large-login-clock.png')
        from PIL import Image, ImageChops
        with Image.open(ctx.dir / 'large-login-clock.png') as picture:
            footer_rect = (0, picture.height * 3 // 4, picture.width, picture.height)
            footer = picture.convert('RGB').crop(footer_rect)
        for index, delay in enumerate((.013, .031, .047, .019, .061, .023, .041, .017, .053, .029, .037, .011)):
            time.sleep(delay)
            with Image.open(m.shot(f'idle-login-{index}')) as picture:
                unchanged = ImageChops.difference(footer, picture.convert('RGB').crop(footer_rect)).getbbox() is None
            ctx.check(f'idle native redraw {index + 1} keeps the complete form footer visible', unchanged)
    display_helper = ctx.args.source_tree / 'os/scripts/wros-display'
    g.shell.put('/usr/libexec/wros-display', display_helper.read_bytes(), '0755')
    ctx.result['display_helper_sha256'] = phases.vm.sha256_file(display_helper)
    if ctx.args.backend == 'drm':
        ctx.result['native_warmth_before_login'] = json.loads(g.run('/usr/libexec/wros-display status')[1])
    if ctx.args.delay_health:
        # Only this disposable VM's read-only health query is delayed. The real
        # updater still produces the reply; authentication helpers are unchanged.
        g.run('cp -p /usr/libexec/wros-update /usr/libexec/wros-update-trial-real')
        g.shell.put('/usr/libexec/wros-update',
                    '#!/usr/bin/python3 -I\nimport os, sys, time\nfrom pathlib import Path\n'
                    'if sys.argv[1:] == ["recovery-status"]:\n'
                    '    Path("/run/user/%d/health-delay-seen" % os.getuid()).touch()\n'
                    f'    time.sleep({ctx.args.delay_health})\n'
                    'os.execv("/usr/libexec/wros-update-trial-real", ["wros-update", *sys.argv[1:]])\n', '0755')
        ctx.result['controlled_health_delay_seconds'] = ctx.args.delay_health
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
    m.type_text(credentials['login'], state['layout'])
    m.key('ret', gap=0)
    if ctx.args.test_splash:
        ctx.check('the picture reaches the graphical display before writing', wait_for_startup_picture(ctx, m))
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        rc, _ = g.run('test -S /run/user/1000/wros-vm-test.sock', check=False)
        if rc == 0:
            break
        time.sleep(1)
    ctx.check('the graphical writing app opened its private test socket', rc == 0)
    driver_command = 'runuser -u ' + user + ' -- python3 /tmp/trial-client.py'

    def drive(request=None, send_only=False):
        command = driver_command
        if request is not None:
            command += ' ' + shlex.quote(json.dumps(request))
        if send_only:
            command += ' send-only'
        _, reply = g.run(command, 45)
        if not send_only:
            (ctx.dir / 'last-tree.json').write_text(reply)
        return reply

    def wait_for_writer():
        # Unlock closes the old connection. Retry snapshots only, so no action
        # from the locked connection can be replayed into the resumed writer.
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            rc, reply = g.run('timeout 4s ' + driver_command, 6, check=False)
            if rc == 0 and 'Writing session unlocked.' in reply and 'Battery and power' in reply:
                (ctx.dir / 'last-tree.json').write_text(reply)
                return
            time.sleep(.1)
        raise phases.Failure('the test driver did not publish the writing settings after unlock')

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
    def system_busy(tree):
        nodes = [json.loads(tree)]
        while nodes:
            node = nodes.pop()
            if isinstance(node, dict):
                if node.get('id') == 'system-busy':
                    return node.get('value') == 'true'
                nodes.extend(node.values())
            elif isinstance(node, list):
                nodes.extend(node)
        raise phases.Failure('the test tree has no system-busy state')
    def warmth_page():
        drive({'kind':'act', 'node':'open-settings', 'action':'activate'})
        for row in ('System', 'Display', 'Warm screen · blue light filter'):
            drive({'choose': row})
        deadline = time.monotonic() + 25
        while system_busy(drive()) and time.monotonic() < deadline:
            time.sleep(.1)
        drive({'choose': 'Check display support'})
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            tree = drive({'kind': 'key', 'key': 'f24'})
            if not system_busy(tree) and ('Display support: unavailable' in tree or 'Display support: available' in tree):
                return tree
            time.sleep(.1)
        raise phases.Failure('the warm-screen support query did not finish')
    if backend != 'drm':
        colour_pid = g.run('pgrep -x wros-colours')[1].strip()
        ctx.check('compositor colours have a separate process', bool(colour_pid) and colour_pid != editor_pid)
        _, descriptors = g.run('for fd in /proc/' + colour_pid + '/fd/*; do readlink "$fd"; done')
        ctx.check('the colour process has no keyboard devices or password terminal',
                  '/dev/input/' not in descriptors and '/dev/tty' not in descriptors)
        tree = warmth_page()
        supported = 'Display support: available' in tree
        ctx.result['compositor_warmth_supported'] = supported
        # The installed QEMU display currently refuses gamma controls. Record
        # that driver result explicitly; a future supported driver needs its own
        # positive acceptance phase rather than silently changing this assertion.
        ctx.check('the emulated display honestly reports unsupported compositor warmth', not supported
                  and 'this display cannot accept screen warmth' in tree)
        before = g.run('if test -f /etc/wros/display.json; then sha256sum /etc/wros/display.json; else echo missing; fi')[1]
        drive({'choose': 'Filter: off'})
        drive({'choose': 'Preview choice'})
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            tree = drive({'kind': 'key', 'key': 'f24'})
            if 'this display cannot accept screen warmth' in tree and not system_busy(tree):
                break
            time.sleep(.1)
        after = g.run('if test -f /etc/wros/display.json; then sha256sum /etc/wros/display.json; else echo missing; fi')[1]
        ctx.check('an unsupported compositor preview preserves the saved preference', before == after)
        ctx.check('the compositor never starts a native display preview',
                  g.run('test ! -e /run/wros-display/preview.json', check=False)[0] == 0)
        m.shot('compositor-warmth-unavailable')
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
    m.wait_visible('visible-lock')
    ctx.check('the lock prompt reaches the display before any rescue VT switch', True)
    if ctx.args.test_clock:
        g.wait_vcs(8, r'Locked at \d\d:\d\d', 10, what='large-lock-clock')
        ctx.check('the maintained locker shows the labelled lock-start time', True)
    m.key('alt', 'f1')
    ctx.check('the locker prevents returning to the writing screen', g.run('fgconsole')[1].strip() == '8')
    ctx.check('the writing input reader is retired', not any(p['comm']=='wros-input' for p in g.snap()['procs']))
    if ctx.args.test_driver_lock:
        lock_view = json.loads(drive())['root']['children']
        ctx.check('the private tree exposes only a read-only lock handoff status',
                  [item['id'] for item in lock_view] == ['lock-handoff'] and not lock_view[0].get('actions'))
    m.shot('compositor-locked')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password')
    m.wait_visible('visible-lock-password')
    ctx.check('the password prompt reaches the display', True)
    wrong = 'trialwrongpassword'
    m.type_text(wrong+'\n', state['layout'])
    g.wait_vcs(8, r'\A(?!.*Password:).*Press Enter to unlock', 30, what='compositor-wrong-rejected')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password-retry')
    m.type_text(credentials['login']+'\n', state['layout'])
    wait_for_writer()
    ctx.check('authentication returns to the writing settings', g.run('fgconsole')[1].strip() == '1')
    if backend == 'drm':
        m.wait_visible('visible-native-unlock')
        ctx.check('native writing is visible after unlocking without a rescue VT switch', True)
    m.shot('compositor-unlocked')
    (ctx.dir / 'before-lock-2.json').write_text(drive())
    if ctx.args.delay_health:
        pending = json.loads(drive())
        nodes = [pending]
        busy = None
        while nodes:
            value = nodes.pop()
            if isinstance(value, dict):
                if value.get('id') == 'system-busy':
                    busy = value.get('value')
                    break
                nodes.extend(value.values())
            elif isinstance(value, list):
                nodes.extend(value)
        ctx.check('the controlled health query is pending before the second lock', busy == 'true')
    drive({'choose':'Lock writing session'}, True)
    g.wait_vcs(8, 'Press Enter to unlock', 45, what='compositor-lock-again')
    m.key('ret')
    g.wait_vcs(8, 'Password:', 20, what='compositor-password-again')
    m.type_text(credentials['login']+'\n', state['layout'])
    wait_for_writer()
    ctx.check('a second lock returns to the same writing settings', True)
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
        wait_for_writer()
        ctx.check(f'lock cycle {cycle} returns to the same editor', g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
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
        m.wait_visible('visible-wake-lock')
        ctx.check('the lock prompt reaches the display after wake', True)
        m.key('ret')
        g.wait_vcs(8, 'Password:', 20, what='compositor-wake-password')
        m.type_text(credentials['login']+'\n', state['layout'])
        wait_for_writer()
        ctx.check('wake and authentication return to the same editor', g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
    if backend == 'drm':
        ctx.check('native input reacquires devices after wake', '/dev/input/event' in native_devices())
        # Exercise the first lock before a maintenance VT switch can refresh scanout.
        m.key('ctrl', 'alt', 'f2')
        time.sleep(2)
        ctx.check('native input and display release for another VT', g.run('fgconsole')[1].strip() == '2')
        ctx.check('the inactive reader has released its input devices', '/dev/input/event' not in native_devices())
        m.key('ctrl', 'alt', 'f1')
        time.sleep(2)
        ctx.check('native devices return with tty1', g.run('fgconsole')[1].strip() == '1'
                  and '/dev/input/event' in native_devices())
        g.run('pkill -KILL -x wros-input')
        g.wait_vcs(1, 'Local account', 45, what='native-reader-crash-greeter')
        ctx.check('reader death ends the writer and restores the graphical login',
                  not any(p['comm'] in ('grimoire', 'wros-input', 'vlock-main') for p in g.snap()['procs']))
        ctx.check('reader death preserves the saved scene', bool(g.probe('find-text', 'colemak é qwerty xxxxx', home)))
    else:
        g.run('pkill -KILL -x wros-colours')
        tree = warmth_page()
        ctx.check('colour-worker death leaves the writer usable and reports warmth unavailable',
                  'Display support: unavailable' in tree and g.run('pgrep -xo grimoire')[1].strip() == editor_pid)
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
    parser.add_argument('--test-clock', action='store_true', help='fixture also contains the greeter under test')
    parser.add_argument('--test-splash', action='store_true', help='capture the picture as the graphical session opens')
    parser.add_argument('--test-driver-lock', action='store_true', help='check the private view while authentication owns the console')
    parser.add_argument('--lock-cycles', type=int, default=2, choices=range(2, 13))
    parser.add_argument('--delay-health', type=int, default=0, choices=range(7))
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
