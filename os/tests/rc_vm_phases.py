#!/usr/bin/env python3
"""WrOS release-candidate VM qualification: phase drivers.

Runs as the unprivileged `builder` user inside the Debian builder chroot (see
.audit-tools/rc-vm-go.sh). Every phase gets a fresh run directory, a result.json with
the ISO sha256 and the exact QEMU commands, screendumps and serial transcripts.
Failed attempts are never overwritten.

    rc_vm_phases.py firmware   --iso ISO [--secboot]
    rc_vm_phases.py live       --iso ISO         (phases 1-3: live writing, refusal, cancel)
    rc_vm_phases.py install    --iso ISO         (phase 4)
    rc_vm_phases.py install-failure --iso ISO --kill backend|writer   (phases 5/6)
    rc_vm_phases.py installed-boot --install-run DIR                  (phase 6)
    rc_vm_phases.py update     --install-run DIR --bundle DIR [--variant ok|cancel|tamper|powercut|badstage]
    rc_vm_phases.py rescue     --iso ISO --install-run DIR

Hard rules: no NIC, only freshly created disposable images, generated secrets only,
no QA kernel arguments (the serial channel is a QEMU SMBIOS credential).
"""
import argparse
import base64
import gzip
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import time
import traceback
from pathlib import Path

import rc_vm as vm
import rc_vm_forms as forms
from fixture_storage import fixture_serial

BASE = Path(os.environ.get('RC_VM_BASE', '/home/builder/rc-vm-runs'))
GUEST = Path(__file__).with_name('rc_vm_guest.py')
LIVE_USER, LIVE_PASSWORD = 'wros-live', 'live'   # Debian's public live credential, not a secret
TARGET_BYTES = 16 * 1024 ** 3
# installer keymap list entry to choose, and entries to avoid, per requested layout
LAYOUT_CHOICES = {'dvorak': (r'^Dvorak$', ''), 'colemak': (r'^Colemak$', ''),
                  'colemak_dh': (r'^Colemak Mod-DH \(ANSI\)$', '')}
KEYMAP_ROWS_EXPECTED = [r'^Colemak$', r'^Colemak Mod-DH \(ANSI\)$', r'^Colemak Mod-DH \(ISO\)$']


def harness_commit():
    path = Path(__file__).resolve().parents[2] / 'HARNESS_COMMIT'
    return path.read_text().strip() if path.exists() else 'unknown'


class Failure(Exception):
    pass


class Ctx:
    def __init__(self, phase, args):
        self.phase, self.args = phase, args
        stamp = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())
        BASE.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.dir = BASE / ('%s-%s' % (phase, stamp))
        self.dir.mkdir(mode=0o700)
        self.log_file = open(self.dir / 'driver.log', 'a')
        self.iso = Path(args.iso).resolve() if getattr(args, 'iso', None) else None
        self.result = {'phase': phase, 'passed': False, 'steps': [], 'assertions': [], 'failed_assertion': None,
                       'harness_commit': harness_commit(),
                       'release_commit': None,
                       'machines': [], 'started': stamp}
        if self.iso:
            self.result['iso'] = str(self.iso)
            self.result['iso_sha256_before'] = vm.sha256_file(self.iso)
            receipt = Path(getattr(args, 'receipt', None) or str(self.iso).rsplit('.', 1)[0] + '.receipt.json')
            if receipt.is_file():
                data = json.loads(receipt.read_text())
                self.result['release_commit'] = data.get('git_commit')
                self.result['receipt'] = str(receipt)
                self.result['receipt_sha256'] = vm.sha256_file(receipt)
                self.result['receipt_binaries_sha256'] = data.get('binaries')
        self.machines = []
        self.started = time.monotonic()

    def log(self, message):
        line = '[%7.1fs] %s' % (time.monotonic() - self.started, message)
        print(line, flush=True)
        self.log_file.write(line + '\n')
        self.log_file.flush()

    def step(self, label, **data):
        self.result['steps'].append({'t': round(time.monotonic() - self.started, 1), 'step': label, **data})
        self.log('STEP ' + label + (' ' + json.dumps(data, default=str)[:300] if data else ''))

    def check(self, name, condition, detail=None):
        self.result['assertions'].append({'name': name, 'ok': bool(condition), 'detail': detail})
        self.log('%s %s%s' % ('PASS' if condition else 'FAIL', name, '' if condition or detail is None else ' :: ' + str(detail)[:400]))
        if not condition:
            self.result['failed_assertion'] = name
            raise Failure(name)

    def guard(self):
        if (BASE / 'STOP').exists():
            raise Failure('STOP file present (host disk floor); aborting')
        if vm.free_bytes(BASE) < (3 << 30):
            raise Failure('builder disk below 3 GiB')

    def finish(self, error=None):
        for machine in self.machines:
            self.result['machines'].append({'name': machine.name, 'command': machine.command})
            try:
                machine.stop()
            except Exception:
                pass
        if self.iso:
            self.result['iso_sha256_after'] = vm.sha256_file(self.iso)
            self.result['iso_unchanged'] = self.result['iso_sha256_after'] == self.result['iso_sha256_before']
            if not self.result['iso_unchanged']:
                self.result['passed'] = False
        if error:
            self.result['error'] = error
        self.result['seconds'] = round(time.monotonic() - self.started, 1)
        (self.dir / 'result.json').write_text(json.dumps(self.result, indent=2, default=str) + '\n')
        self.log('result: %s -> %s' % ('PASS' if self.result['passed'] else 'FAIL', self.dir / 'result.json'))


class Guest:
    """A running VM plus its serial shell and the read-only guest probe."""

    def __init__(self, ctx, machine, root_via_sudo=True):
        self.ctx, self.m = ctx, machine
        self.shell = vm.Shell(machine.serial)
        self.prefix = 'sudo -n ' if root_via_sudo else ''

    def login(self, user, password, timeout=420):
        self.shell.login(user, password, timeout)
        self.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')

    def become_root(self, password):
        """Replace the user shell with a root shell via `sudo -S` (password over serial, no echo)."""
        s = self.m.serial
        s.send("exec sudo -S -p '' env PS1= TERM=dumb bash --noediting --norc --noprofile\n")
        time.sleep(0.5)
        s.send(password + '\n')
        time.sleep(1.5)
        s.send('stty -echo -onlcr; echo RCSHELL_READY_root\n')
        s.expect(re.compile(r'^RCSHELL_READY_root\r?$', re.M), 45)
        self.prefix = ''

    def run(self, command, timeout=60, check=True):
        try:
            return self.shell.run(command, timeout, check)
        except RuntimeError as error:
            # a power-off command legitimately ends the guest before its end marker can be read
            if 'poweroff' in command and 'guest stopped' in str(error):
                return 0, ''
            raise

    def probe(self, *args, timeout=90):
        rc, out = self.shell.run('%spython3 -I /tmp/rcg.py %s' % (self.prefix, ' '.join(args)), timeout, check=True)
        for line in out.splitlines():
            if line.startswith('RCJSON'):
                return json.loads(line[6:])
        raise Failure('probe %s produced no RCJSON: %s' % (args, out[-300:]))

    def vcs(self, n):
        probe = self.probe('vcs', str(n))
        if probe.get('graphics') and probe.get('active'):
            return self.m.screen_text('native-tty%d' % n).splitlines()
        return probe.get('lines', [])

    def snap(self):
        return self.probe('snap', timeout=120)

    def screen(self, n=1):
        return '\n'.join(self.vcs(n))

    def wait_vcs(self, n, pattern, timeout=120, interval=1.0, what=None):
        regex = re.compile(pattern, re.S)
        deadline = time.monotonic() + timeout
        text = ''
        while time.monotonic() < deadline:
            self.ctx.guard()
            text = self.screen(n)
            if regex.search(text):
                return text
            time.sleep(interval)
        self.m.shot('timeout-' + re.sub(r'\W+', '_', what or pattern)[:30])
        raise Failure('tty%d never showed %r; last screen:\n%s' % (n, pattern, text))

    def layout_in_effect(self):
        keys = self.probe('keymap', timeout=60)['keycodes']
        pair = (keys.get('18', '').lower(), keys.get('20', '').lower())
        found = {('e', 't'): 'us', ('period', 'y'): 'dvorak', ('f', 'g'): 'colemak', ('f', 'b'): 'colemak_dh'}
        return found.get(pair, 'other:%s,%s' % pair)

    # Grimoire navigation (list highlight is the shared "▸ " marker) ------------
    def selected_row(self, n=1):
        for line in self.vcs(n):
            if forms.MARK in line:
                return line.split(forms.MARK, 1)[1].strip().strip('│| ').strip()
        return None

    def choose(self, pattern, n=1, limit=45):
        regex = re.compile(pattern, re.I)
        current = self.selected_row(n)
        if current and regex.search(current):
            return current
        # Settings remembers the last row. Search from the beginning so a target
        # above that row is reachable too; these lists do not wrap at the bottom.
        self.m.key('home')
        for _ in range(limit):
            current = self.selected_row(n)
            if current and regex.search(current):
                return current
            self.m.key('down')
        raise Failure('could not select %r; selected %r' % (pattern, self.selected_row(n)))


# ----------------------------------------------------------------------------- boot helpers


def wait_for_firmware_menu(ctx, m, name='grub-menu', timeout=120):
    """Wait until something stable and non-black is on screen, then return its path."""
    from PIL import Image
    deadline = time.monotonic() + timeout
    previous = None
    while time.monotonic() < deadline:
        path = m.shot('probe-' + name)
        image = Image.open(path).convert('L')
        bright = sum(1 for p in image.resize((100, 75)).getdata() if p > 40)
        signature = vm.image_signature(path)
        if bright > 40 and signature == previous:
            final = m.run / (name + '.png')
            shutil.copyfile(path, final)
            return final
        previous = signature if bright > 40 else None
        time.sleep(2)
    raise Failure('firmware menu never appeared')


def boot_live(ctx, name, media, *, target=None, serial_token=None, secboot=False, extra=None,
              memory=4096, press_enter=True):
    m = vm.Machine(ctx.dir, name, memory=memory)
    ctx.machines.append(m)
    variables = vm.fresh_vars(ctx.dir, name + '-OVMF_VARS.fd', vm.OVMF_VARS_MS if secboot else vm.OVMF_VARS)
    m.firmware(variables, secboot)
    if media == 'cdrom':
        m.cdrom(ctx.iso)
    elif media == 'usb':
        m.usb_stick(ctx.iso, readonly=True, bootindex=1)
    else:
        raise ValueError(media)
    if target:
        m.virtio_disk(target, serial=fixture_serial(serial_token) if serial_token else None, ident='target')
    if extra:
        extra(m)
    m.serial_login_credential()
    m.start()
    ctx.step('qemu-started', name=name, media=media, secboot=secboot, command=' '.join(m.command))
    if press_enter:
        wait_for_firmware_menu(ctx, m, name + '-grub')
        ctx.step('grub-menu-seen', screenshot=name + '-grub.png')
        m.key('ret')
    return m


def live_login(ctx, m, timeout=420):
    g = Guest(ctx, m)
    deadline = time.monotonic() + timeout
    shots = 0
    while True:
        try:
            m.serial.expect(re.compile(r'login: ?'), 20, consume=False)
            break
        except TimeoutError:
            shots += 1
            m.shot('boot-%02d' % shots)
            ctx.guard()
            if time.monotonic() > deadline or not m.is_running():
                raise Failure('live system never offered a serial login (credential getty)')
    g.login(LIVE_USER, LIVE_PASSWORD, 60)
    ctx.step('live-serial-login')
    return g


def assert_live_state(ctx, g, media):
    rc, out = g.run('cat /proc/cmdline; test -d /sys/firmware/efi && echo UEFI; findmnt -n -o SOURCE,FSTYPE,OPTIONS /run/live/medium; '
                    "loginctl list-sessions --no-legend | awk '{print $1}' | while read s; do loginctl show-session $s -p Id -p User -p Active -p Remote -p TTY -p VTNr -p Seat -p Type -p Class | tr '\\n' ' '; echo; done")
    ctx.result['live_state'] = out
    ctx.check('boot=live on the cmdline', 'boot=live' in out and 'wros.live=1' in out)
    ctx.check('no QA serial console argument', 'console=ttyS' not in out.split('UEFI')[0])
    ctx.check('booted via UEFI firmware', 'UEFI' in out)
    ctx.check('live medium mounted read-only', re.search(r'/dev/\S+\s+\S+\s+ro[,\s]', out) is not None, out[-300:])
    wanted = ('User=1000', 'Active=yes', 'Remote=no', 'TTY=tty1', 'VTNr=1', 'Seat=seat0', 'Type=tty', 'Class=user')
    ctx.check('autologin session is the tty1 user session',
              any(all(token in line.split() for token in wanted) for line in out.split(chr(10))), out[-600:])
    screen = g.wait_vcs(1, r'Live session', 240, what='live-home')
    ctx.check('Grimoire live home visible on tty1', 'Resume writing' in screen or 'Live session' in screen, screen[-400:])
    m = g.m
    m.shot('live-home-%s' % media)


def make_target(ctx, name='target.img', size=TARGET_BYTES):
    path = ctx.dir / name
    vm.create_sparse_target(path, size)
    token = secrets.token_hex(16)
    (ctx.dir / 'fixture-token').write_text(token + '\n')
    ctx.result['target'] = {'path': str(path), 'bytes': size, 'serial': fixture_serial(token),
                            'sparse_digest_before': vm.sparse_digest(path)}
    return path, token


def on_dashboard(g):
    lines = g.vcs(1)
    return bool(lines) and 'Resume writing' in '\n'.join(lines) and 'GRIMOIRE' not in lines[0]


def grim_dashboard(g):
    """Show the WrOS dashboard (F12 toggles it, so only press it from the editor)."""
    if not on_dashboard(g):
        g.m.key('f12')
    g.wait_vcs(1, r'Resume writing', 30, what='dashboard')


def grim_editor(g):
    """Return to the editor (Esc closes the dashboard; in the editor Esc would open the menu)."""
    if on_dashboard(g):
        g.m.key('esc')
        time.sleep(1)
    g.wait_vcs(1, r'GRIMOIRE', 20, what='editor')


def grim_open_settings(g, layout='us'):
    grim_dashboard(g)
    g.choose(r'^Settings$')
    g.m.key('ret')
    g.wait_vcs(1, r'Writing.*(?:Interface|Look and keyboard).*System', 30, what='settings-root')


def grim_to_panel(g, panel_regex):
    # From rc9, Updates and Recovery have their own group; before, they sat under System.
    if re.search(r'Updates|Recovery', panel_regex) and 'Updates and recovery' in g.screen(1):
        g.choose(r'^Updates and recovery(\s*›)?$')
        g.m.key('ret')
        g.wait_vcs(1, r'Recovery', 30, what='updates-group')
    else:
        g.choose(r'^System(\s*›)?$')  # rows that open a page end in › from rc2 on
        g.m.key('ret')
        g.wait_vcs(1, r'Wi-Fi and networking', 30, what='system-panels')
    g.choose(panel_regex)
    g.m.key('ret')
    time.sleep(1)


def grim_new_scene(g, name, text, layout='us'):
    """Dashboard (or editor) -> new scene named `name` -> type text -> save. Returns the screen."""
    m = g.m
    grim_editor(g)
    m.ctrl('k', layout)
    time.sleep(0.8)
    m.type_text('new scene', layout)
    g.wait_vcs(1, r'\u25b8 New scene|\u2192 New scene|New scene', 20, what='palette-new-scene')
    m.key('ret')
    g.wait_vcs(1, r'NEW SCENE', 20, what='new-scene-dialog')
    m.type_text(name, layout)
    m.key('ret')
    g.wait_vcs(1, r'Start writing', 20, what='new-scene-editor')
    m.type_text(text, layout)
    m.ctrl('s', layout)
    time.sleep(2)
    return g.screen(1)


def grim_logout_login(ctx, g, user, password, layout):
    # WrOS has no Quit (rc9): end the session as a shutdown would, which saves first.
    g.run('pkill -TERM -x grimoire', 20, check=False)
    g.wait_vcs(1, r'Local account', 60, what='after-logout')
    greeter_login(ctx, g, g.m, user, password, layout, wrong_first=False)
    grim_dashboard(g)
    g.m.key('ret')           # Resume writing
    time.sleep(3)
    return g.screen(1)


# ----------------------------------------------------------------------------- phase 1


def phase_firmware(ctx):
    """Phase 1: the real ISO through OVMF and its own GRUB, as cdrom and as USB stick."""
    for media in ('cdrom', 'usb'):
        m = boot_live(ctx, 'fw-' + media, media)
        g = live_login(ctx, m)
        assert_live_state(ctx, g, media)
        ctx.step('media-booted', media=media)
        g.run('sudo -n /sbin/poweroff', 20, check=False)
        try:
            m.wait_exit(90)
        except TimeoutError:
            m.kill()
        ctx.check('%s boot ended cleanly' % media, True)
    if ctx.args.secboot:
        try:
            m = boot_live(ctx, 'fw-secboot', 'cdrom', secboot=True)
            time.sleep(45)
            shot = m.shot('secboot-after-45s')
            ctx.step('secure-boot-observation', screenshot=shot.name,
                     serial_login_prompt=m.serial.contains(b'login:'))
        except Exception as error:
            ctx.step('secure-boot-observation', error=str(error))
    ctx.result['passed'] = True


# ----------------------------------------------------------------------------- phases 2-3


def phase_live(ctx):
    target, token = make_target(ctx)
    before = ctx.result['target']['sparse_digest_before']
    m = boot_live(ctx, 'live', 'cdrom', target=target, serial_token=token)
    g = live_login(ctx, m)
    assert_live_state(ctx, g, 'cdrom-with-target')
    # ---- phase 2: live writing, RAM only
    phrase = 'rclive' + secrets.token_hex(5)
    screen = grim_new_scene(g, 'rcscene', 'Chapter one ' + phrase, 'us')
    ctx.result['after_save_screen'] = screen
    m.shot('live-after-save')
    hits = g.probe('find-text', phrase, '/home')
    ctx.step('live-save-search', hits=hits)
    ctx.check('typed text reached a saved file', bool(hits), screen[-500:])
    ctx.check('saved file belongs to the live user in RAM-backed home',
              all(h[0].startswith('/home/wros-live/') and h[1] == 1000 for h in hits), hits)
    reopened = grim_logout_login(ctx, g, LIVE_USER, LIVE_PASSWORD, 'us')
    m.shot('live-reopened')
    ctx.check('saved text reopened after logout/login', phrase in reopened, reopened[-400:])
    ctx.check('target untouched by live writing', vm.sparse_digest(target) == before)
    # ---- phase 3: installer refusal + cancel
    grim_open_settings(g)
    grim_to_panel(g, r'Storage')
    row = g.choose(r'Install WrOS to disk')
    ctx.check('Install entry offered in live Storage panel', 'Install WrOS' in row, row)
    m.shot('storage-panel')
    snap0 = g.snap()
    input_pids0 = sorted(p['pid'] for p in snap0['procs'] if p['comm'] == 'wros-input')
    ctx.check('no installer run state before opening', not snap0['run_dir_exists'] and snap0['installer_unit'] != 'active')
    m.key('ret')
    screen = g.wait_vcs(1, r'Cancel and remain in the live writing session', 20, what='installer-confirm')
    ctx.check('confirmation defaults to Cancel', re.search(r'▸ .*Cancel and remain', screen) is not None, screen[-500:])
    m.shot('installer-confirm')
    m.key('ret')
    time.sleep(2)
    snap1 = g.snap()
    ctx.check('Enter on Cancel changes nothing',
              not snap1['run_dir_exists'] and snap1['installer_unit'] != 'active' and snap1['active_vt'] == 'tty1'
              and sorted(p['pid'] for p in snap1['procs'] if p['comm'] == 'wros-input') == input_pids0
              and vm.sparse_digest(target) == before)
    ctx.step('cancel-default-ok')
    # start, then cancel inside the installer before any write
    m.key('ret')
    g.wait_vcs(1, r'Start encrypted WrOS installer', 20, what='installer-confirm-2')
    m.key('down')
    ctx.check('start row highlighted', re.search(r'▸ .*Start encrypted WrOS installer', g.screen(1)) is not None)
    m.key('ret')
    screen7 = g.wait_vcs(7, r'WrOS . Install', 240, what='installer-first-form')
    m.shot('installer-vt7')
    snap2 = g.snap()
    ctx.result['installer_started_snapshot'] = {k: snap2[k] for k in ('active_vt', 'procs', 'run_files', 'installer_unit', 'hostname', 'mappers')}
    ctx.check('VT7 active while installer runs', snap2['active_vt'] == 'tty7', snap2['active_vt'])
    ctx.check('installer unit active', snap2['installer_unit'] == 'active', snap2['installer_unit'])
    frontend = [p for p in snap2['procs'] if p['comm'] == 'wros-installer']
    ctx.check('frontend runs as the locked installer account, not uid 1000',
              len(frontend) == 1 and frontend[0]['uid'][0] not in ('0', '1000') and 'wros-installer-session' in frontend[0]['cgroup'], frontend)
    ctx.check('Grimoire still alive but wros-input retired',
              any(p['comm'] == 'grimoire' for p in snap2['procs']) and not any(p['comm'] == 'wros-input' for p in snap2['procs']))
    baseline = snap2['run_files'].get('baseline.json', {})
    ctx.check('baseline names the one fixture disk', baseline.get('target_disk') == 'vda' or 'vda' in json.dumps(baseline), baseline)
    ctx.check('target unchanged at the first form', vm.sparse_digest(target) == before)
    m.key('ctrl', 'c')
    last = None
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        snap3 = g.snap()
        if 'outcome.json' in snap3['run_files']:
            last = snap3['run_files']['outcome.json']
        if snap3['active_vt'] == 'tty1' and any(p['comm'] == 'wros-input' for p in snap3['procs']):
            break
        time.sleep(2)
    else:
        raise Failure('writer input never came back after cancel; vt=%s' % snap3['active_vt'])
    ctx.result['cancel_outcome'] = last
    final_screen = g.screen(1)
    ctx.result['after_cancel_screen'] = final_screen
    m.shot('after-installer-cancel')
    ctx.check('outcome is cancelled', (last or {}).get('outcome') == 'cancelled', last)
    ctx.check('host cleanup verified', (last or {}).get('host_cleanup_verified') is True, last)
    ctx.check('Grimoire explains the installer closed', 'closed without completing' in final_screen.lower() or 'Installer closed' in final_screen, final_screen[-400:])
    ctx.check('fresh wros-input after cancel',
              sorted(p['pid'] for p in snap3['procs'] if p['comm'] == 'wros-input') != input_pids0)
    ctx.check('target hash unchanged after cancel', vm.sparse_digest(target) == before)
    ctx.check('no leftover mappers', snap3['mappers'] in ([], ['control']), snap3['mappers'])
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    ctx.result['passed'] = True


# ----------------------------------------------------------------------------- installer driver


def apply_action(ctx, g, m, form, rule, action, secrets_, layout_wanted):
    name = action[0]
    if name == 'enter':
        m.key('ret')
    elif name == 'wait':
        time.sleep(3)
    elif name == 'no':
        ctx.check('boolean starts at No', form.selected == 'No', form.raw[-300:])
        m.key('ret')
    elif name == 'yes':
        m.key('down')
        now = forms.parse_form(g.vcs(7))
        ctx.check('boolean now highlights Yes', now.selected == 'Yes', now.raw[-300:])
        m.key('ret')
    elif name == 'select':
        want, avoid = action[1], action[2]
        current = form
        seen_rows = []
        for _ in range(500):
            sel = current.selected
            for row in [sel] + list(current.rows):
                if row and row not in seen_rows:
                    seen_rows.append(row)
            if re.search(want, sel, re.I) and not (avoid and re.search(avoid, sel, re.I)):
                if rule.name == 'keyboard-layout':
                    ctx.result['keymap_rows_seen'] = seen_rows
                    if ctx.args.layout.startswith('colemak'):
                        for expected in KEYMAP_ROWS_EXPECTED:
                            ctx.check('installer keymap list offers %s' % expected.strip('^$').replace('\\', ''),
                                      any(re.search(expected, r) for r in seen_rows), seen_rows[-12:])
                        i = [k for k, r in enumerate(seen_rows) if r == 'Colemak']
                        ctx.check('Colemak entries follow Chinese in the list',
                                  bool(i) and i[0] > 0 and seen_rows[i[0] - 1].startswith('Chinese'), seen_rows[max(0, i[0] - 2):i[0] + 3] if i else seen_rows[-8:])
                m.key('ret')
                ctx.step('selected', rule=rule.name, choice=sel)
                return
            m.key('down')
            nxt = forms.parse_form(g.vcs(7))
            if nxt.selected == sel and nxt.raw == current.raw:
                raise Failure('end of list without %r in %s; rows %s' % (want, rule.name, nxt.rows))
            current = nxt
        raise Failure('selection loop exceeded for ' + rule.name)
    elif name == 'fill':
        for _ in range(len(form.selected) + 2):
            m.key('backspace', gap=0.05)
        verify_console_map(ctx, g)
        m.type_text(action[1], layout_in_effect_safe(g))
        m.key('ret')
    elif name == 'secret':
        ctx.check('password field is masked', form.selected == '[hidden]', form.raw[-200:])
        verify_console_map(ctx, g)
        effective = layout_in_effect_safe(g)
        ctx.result.setdefault('secret_entries', []).append({'rule': rule.name, 'layout_in_effect': effective})
        m.type_text(secrets_[action[1]], effective)
        shown = '\n'.join(g.vcs(7))
        ctx.check('typed secret never rendered', secrets_[action[1]] not in shown)
        m.key('ret')
    else:
        raise Failure('unknown action ' + str(action))


KEYCODE_QCODES = dict(zip(
    list(range(16, 28)) + list(range(30, 41)) + list(range(44, 54)),
    ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'bracket_left', 'bracket_right',
     'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'semicolon', 'apostrophe',
     'z', 'x', 'c', 'v', 'b', 'n', 'm', 'comma', 'dot', 'slash']))
SYMBOL_NAMES = {';': 'semicolon', ',': 'comma', '.': 'period', '/': 'slash', "'": 'apostrophe',
                '[': 'bracketleft', ']': 'bracketright', '-': 'minus', '=': 'equal'}


def console_map_mismatches(keycodes, layout):
    """Compare the kernel console's unshifted letter/punctuation keys with the layout table the harness types by."""
    table = vm.LAYOUT_KEYS[layout]
    bad = []
    for code, qcode in sorted(KEYCODE_QCODES.items()):
        plain = table[qcode][0]
        want = SYMBOL_NAMES.get(plain, plain)
        got = keycodes.get(str(code), '').lstrip('+').lower()
        if got != want:
            bad.append('keycode %d (US %s): console gives %r, harness expects %r' % (code, qcode, got, want))
    return bad


def verify_console_map(ctx, g):
    """Once per run, before anything is typed in a non-US layout: the console map must equal the table we type by."""
    if ctx.result.get('console_map_verified') or ctx.args.layout == 'us':
        return
    keycodes = g.probe('keymap', timeout=60)['keycodes']
    bad = console_map_mismatches(keycodes, ctx.args.layout)
    ctx.result['console_map_keycodes'] = keycodes
    ctx.result['console_map_mismatches'] = bad
    if bad:
        raise Failure('console keymap differs from the %s table the harness types by:%s%s' % (ctx.args.layout, chr(10), chr(10).join(bad)))
    ctx.result['console_map_verified'] = True


def layout_in_effect_safe(g):
    layout = g.layout_in_effect()
    if layout.startswith('other:'):
        raise Failure('unexpected console keymap in effect: ' + layout)
    return layout


def hint_action(ctx):
    """Unknown form: park and poll for <run>/hint.json {"action": [...]} from the operator."""
    hint = ctx.dir / 'hint.json'
    for _ in range(int(ctx.args.hint_wait / 5)):
        if hint.exists():
            data = json.loads(hint.read_text())
            hint.rename(ctx.dir / ('hint-used-%d.json' % int(time.time())))
            return tuple(data['action'])
        ctx.guard()
        time.sleep(5)
    return None


def run_installer(ctx, g, secrets_, *, after_action=None, stuck_seconds=1500, deadline_seconds=6000):
    """Drive the native installer forms from an allowlist. Returns the last snapshot."""
    m = g.m
    rule_list = forms.rules(*LAYOUT_CHOICES[ctx.args.layout])
    start = time.monotonic()
    acted = None
    last_sig, last_change = None, time.monotonic()
    seen_vt7 = False
    menu_since = None
    steps = []
    while True:
        ctx.guard()
        if time.monotonic() - start > deadline_seconds:
            raise Failure('installer exceeded its deadline')
        snap = g.snap()
        vt = snap['active_vt']
        seen_vt7 = seen_vt7 or vt == 'tty7'
        if 'outcome.json' in snap['run_files'] or (seen_vt7 and vt == 'tty1' and snap['installer_unit'] != 'active'):
            ctx.result['installer_steps'] = steps
            return snap
        form = forms.parse_form(snap['vcs7'])
        sig = form.raw
        if sig != last_sig:
            last_sig, last_change = sig, time.monotonic()
        if after_action and after_action(snap, form, steps):
            ctx.result['installer_steps'] = steps
            return snap
        if ('frontend stopped' in form.raw or 'Running exit-installer' in form.raw) and time.monotonic() - last_change > 120                 and snap['installer_unit'] == 'active' and 'outcome.json' not in snap['run_files']:
            # the product promised a verdict within its grace periods (10 s frontend, 120 s backend); keep the
            # VM alive and let the caller diagnose instead of waiting out the generic 25 minute limit
            ctx.result['installer_stalled_after_finish'] = form.raw
            ctx.step('installer-stalled-after-finish', seconds=120)
            m.shot('stalled-after-finish')
            ctx.result['installer_steps'] = steps
            return snap
        if sig == acted:
            if time.monotonic() - last_change > stuck_seconds:
                raise Failure('installer screen unchanged for %ds' % stuck_seconds)
            time.sleep(2)
            continue
        if 'Choose the next step in the install process' in form.raw:
            # d-i main menu: Debian's default is "Finish the installation", but the supervisor judges the
            # install only after the installer exits. Never press anything here; give it time to exit on
            # its own, then stop driving and let the caller judge (and diagnose) the state.
            menu_since = menu_since or time.monotonic()
            if time.monotonic() - menu_since > 150:
                ctx.result['installer_stuck_at_main_menu'] = form.raw
                ctx.step('installer-stuck-at-main-menu', seconds=150)
                m.shot('stuck-main-menu')
                ctx.result['installer_steps'] = steps
                return snap
            time.sleep(3)
            continue
        decision = forms.decide(form, rule_list)
        if decision is None:
            (ctx.dir / 'unknown-form.txt').write_text(form.raw + '\n')
            m.shot('unknown-form')
            ctx.log('UNKNOWN FORM (kind=%s):\n%s' % (form.kind, form.raw))
            action = hint_action(ctx)
            if action is None:
                ctx.result['unknown_form'] = form.raw
                raise Failure('unknown installer form; nothing sent')
            rule = forms.Rule('operator-hint', '', action)
            ctx.step('operator-hint', action=action)
        else:
            rule, action = decision
        if action[0] != 'wait':
            steps.append({'t': round(time.monotonic() - start, 1), 'rule': rule.name, 'action': action[0],
                          'kind': form.kind, 'status': form.status})
            ctx.log('form %-22s %-7s %s' % (rule.name, action[0], form.status))
            m.shot('form-%02d-%s' % (len(steps), rule.name))
            apply_action(ctx, g, m, form, rule, action, secrets_, 'colemak')
            acted = sig
        else:
            time.sleep(3)


def phase_install(ctx):
    kill = getattr(ctx.args, 'kill', None)
    target, token = make_target(ctx)
    before = ctx.result['target']['sparse_digest_before']
    layout = ctx.args.layout
    secrets_ = {'login': vm.generated_secret(layout), 'luks': vm.generated_secret(layout)}
    secrets_path = ctx.dir / 'secrets.json'
    fd = os.open(secrets_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(secrets_, stream)
    ctx.result['secrets_file'] = 'secrets.json (0600, never copied to evidence)'
    ctx.result['layout_selected'] = layout
    ctx.result['secrets_meaningful_for_layout'] = {k: vm.wrong_layout_differs(v, layout) for k, v in secrets_.items()}
    m = boot_live(ctx, 'install', 'cdrom', target=target, serial_token=token, memory=4096)
    g = live_login(ctx, m)
    assert_live_state(ctx, g, 'install')
    rc, hashes = g.run('sha256sum /usr/bin/grimoire /usr/bin/wros-greeter /usr/bin/wros-installer', 30)
    live_hashes = {line.split()[1]: line.split()[0] for line in hashes.split('\n') if line.strip()}
    ctx.result['live_binary_sha256'] = live_hashes
    grim_open_settings(g)
    grim_to_panel(g, r'Storage')
    g.choose(r'Install WrOS to disk')
    m.key('ret')
    g.wait_vcs(1, r'Cancel and remain', 20, what='installer-confirm')
    m.key('down')
    m.key('ret')
    g.wait_vcs(7, r'WrOS . Install', 240, what='installer-first-form')
    ctx.step('installer-started')
    if getattr(ctx.args, 'diagnose', False):
        start_syslog_poller(g)
        ctx.step('diagnose-syslog-poller-started')

    def failure_hook(snap, form, steps):
        if not kill:
            return False
        wrote = any(s['rule'] in ('write-lvm', 'write-crypt', 'write-disks', 'partition-finish') for s in steps)
        mapped = [x for x in snap['mappers'] if x != 'control']
        when = getattr(ctx.args, 'kill_when', 'mapper')
        screen7 = chr(10).join(snap.get('vcs7') or [])
        at_stage = {'mapper': True, 'copying': 'Copying data to disk' in screen7, 'grub': 'Installing GRUB boot loader' in screen7}[when]
        if wrote and (mapped or snap['target_mounts']) and at_stage:
            ctx.step('failure-injection-point', kill=kill, mappers=mapped, mounts=snap['target_mounts'])
            if kill == 'backend':
                g.run("sudo -n pkill -KILL -x main-menu; sudo -n pkill -KILL -x debian-installer-startup; true", 20, check=False)
            elif kill == 'writer':
                g.run('sudo -n pkill -KILL -x grimoire; true', 20, check=False)
            return True
        return False

    snap = run_installer(ctx, g, secrets_, after_action=failure_hook)
    if kill:
        return finish_failure_injection(ctx, g, target, kill)
    if getattr(ctx.args, 'diagnose', False):
        diagnose(ctx, g, secrets_)
        if ctx.result.get('installer_stalled_after_finish'):
            hold_for_operator(ctx, g)
    outcome = snap['run_files'].get('outcome.json', {})
    ctx.result['outcome'] = outcome
    m.shot('install-complete')
    ctx.check('installer outcome is completed', outcome.get('outcome') == 'completed', outcome)
    ctx.check('host cleanup verified', outcome.get('host_cleanup_verified') is True, outcome)
    ctx.check('target_verified is true', outcome.get('target_verified') is True, outcome)
    screen = g.wait_vcs(1, r'Encrypted installation completed|installation completed', 120, what='completion-note')
    ctx.step('completion-note', screen=screen[-600:])
    ctx.check('no secret in serial transcript', not any(m.serial.contains(v) for v in secrets_.values()))
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    try:
        m.wait_exit(180)
    except TimeoutError:
        m.kill()
    after = vm.sparse_digest(target)
    ctx.check('target changed by the install', after != before)
    (ctx.dir / 'install-state.json').write_text(json.dumps({
        'disk': str(target), 'vars': str(ctx.dir / 'install-OVMF_VARS.fd'), 'secrets': str(secrets_path),
        'serial': fixture_serial(token), 'layout': layout, 'run_dir': str(ctx.dir),
        'username': 'writer', 'binary_sha256': live_hashes}, indent=1) + '\n')
    ctx.result['passed'] = True


def hold_for_operator(ctx, g, seconds=1800):
    """After a stall: keep the VM up and run read-only shell lines appended to <run>/cmd.txt, replies in out.txt.
    Ends on a line `quit`, an outcome.json appearing, or the time limit."""
    cmd, out = ctx.dir / 'cmd.txt', ctx.dir / 'out.txt'
    cmd.write_text('')
    out.write_text('HOLD %s' % ctx.dir + chr(10))
    ctx.step('hold-for-operator', seconds=seconds)
    done, deadline = 0, time.monotonic() + seconds
    while time.monotonic() < deadline:
        lines = cmd.read_text().split(chr(10))
        while done < len(lines) - 1:
            line = lines[done].strip()
            done += 1
            if line == 'quit':
                return
            if line:
                rc, text = g.run('{ %s; } </dev/null' % line, 120, check=False)
                with open(out, 'a') as stream:
                    stream.write('>>> %s%s[rc=%s]%s%s%s' % (line, chr(10), rc, chr(10), text, chr(10)))
        time.sleep(2)


def finish_failure_injection(ctx, g, target, kill):
    m = g.m
    deadline = time.monotonic() + 300
    outcome, last = None, None
    vt_history = []
    while time.monotonic() < deadline:
        try:
            snap = g.snap()
        except Exception as error:
            ctx.log('snapshot failed: %s' % error)
            time.sleep(3)
            continue
        last = snap
        vt_history.append([round(time.monotonic() - deadline + 300, 1), snap['active_vt'], sorted(set(p['comm'] for p in snap['procs'] if p['comm'] in ('grimoire', 'wros-input', 'wros-greeter', 'greetd', 'wros-installer'))), [x for x in snap['mappers'] if x != 'control']])
        outcome = snap['run_files'].get('outcome.json', outcome)
        if outcome and not [x for x in snap['mappers'] if x != 'control']:
            break
        time.sleep(3)
    ctx.result['outcome'] = outcome
    ctx.result['vt_history'] = vt_history
    ctx.result['final_snapshot'] = {k: last[k] for k in ('active_vt', 'procs', 'mappers', 'target_mounts', 'swaps', 'installer_unit')} if last else None
    m.shot('after-failure')
    rc, survivors = g.run("ps -eo pid,ppid,stat,comm,args | grep -E 'main-menu|debconf|partman|anna|udpkg|in[.]d-i|debian-installer|log-output|apt-install|di-run|wros-install-session' | grep -v -E 'grep|pkill'; true", 30, check=False)
    ctx.result['surviving_installer_processes'] = survivors
    if kill == 'backend':
        ctx.check('outcome is failed with verified cleanup', (outcome or {}).get('outcome') == 'failed' and (outcome or {}).get('host_cleanup_verified') is True, outcome)
        ctx.check('VT1 restored with a fresh writer', last['active_vt'] == 'tty1' and any(p['comm'] == 'wros-input' for p in last['procs']), last['active_vt'])
        screen = g.screen(1)
        ctx.check('Grimoire says installation did not complete', 'did not complete' in screen, screen[-400:])
    else:
        rc, jr = g.run("sudo -n journalctl -u wros-installer-session --no-pager -o short-monotonic 2>&1 | tail -60 | cut -c1-220", 30, check=False)
        ctx.result['installer_unit_journal'] = jr
        names = set(p['comm'] for p in last['procs'])
        ctx.check('outcome is interrupted with verified host cleanup',
                  (outcome or {}).get('outcome') == 'interrupted' and (outcome or {}).get('host_cleanup_verified') is True, outcome)
        ctx.check('the active VT shows the greeter (greetd and wros-greeter alive), not a writer',
                  last['active_vt'] == 'tty1' and {'greetd', 'wros-greeter'} <= names and not names & {'grimoire', 'wros-input'}, (last['active_vt'], sorted(names)))
        ctx.check('no grimoire and no wros-input process at any sample after the kill',
                  not any(set(row[2]) & {'grimoire', 'wros-input'} for row in vt_history), vt_history)
        early = [row for row in vt_history if row[1] == 'tty1' and row[3]]
        ctx.check('the installer did not issue the VT switch: tty1 was already active while target mappers were still open (its chvt runs only after the target stack is cleaned)',
                  bool(early), vt_history)
        ctx.result['vt_return_refused_in_journal'] = 'automatic VT return refused' in jr
        ctx.check('the installer journal shows no VT-restore failure', 'failed to restore active VT' not in jr, jr[-600:])
    ctx.check('no target mappers, mounts or swap left', not [x for x in last['mappers'] if x != 'control'] and not last['target_mounts'] and not last['swaps'])
    ctx.check('no d-i or installer-session process survives after the outcome is written', not survivors.strip(), survivors)
    rc, tree = g.run("ps -eo pid,ppid,stat,comm,args --no-headers | awk '$5 !~ /^[[]/' | cut -c1-150 | head -80", 30, check=False)
    ctx.result['process_list_after_outcome'] = tree
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    ctx.result['passed'] = True


# ----------------------------------------------------------------------------- failure diagnosis (read-only)

DIAG_GREP = re.compile(r'(?i:error|fail|apt|dpkg|systemd-cryptsetup|pkgsel|base-installer|retriever|udeb|pool)|E:')


def secret_variants(secrets_, typed_layout):
    """Every string a generated secret could appear as: as intended, and as the same physical keys
    would be read under each other layout (a layout mismatch between typing and the guest)."""
    variants = {}
    for name, value in secrets_.items():
        chords = vm.keystrokes(value, typed_layout)
        forms_ = {value}
        for layout, table in vm.LAYOUT_KEYS.items():
            text = ''
            for chord in chords:
                if chord == ['ret']:
                    text += chr(10)
                elif chord[0] == 'shift':
                    text += table[chord[1]][1]
                else:
                    text += table[chord[0]][0]
            forms_.add(text)
        variants[name] = sorted(forms_)
    return variants


def secret_readings(value, typed_layout):
    """{layout name: the string those physical keys produce under that layout}; labels only, for diagnosis."""
    chords = vm.keystrokes(value, typed_layout)
    out = {}
    for layout, table in vm.LAYOUT_KEYS.items():
        text = ''
        for chord in chords:
            text += table[chord[1]][1] if chord[0] == 'shift' else table[chord[0]][0]
        out[layout] = text
    return out


def fetch_guest_file(g, path):
    """Whole file as bytes over the serial shell (gzip+base64, read-only), or None when absent."""
    rc, out = g.run("sudo -n sh -c 'if [ -f %s ]; then gzip -c %s | base64 -w0; echo; else echo MISSING; fi'" % (path, path),
                    300, check=False)
    text = out.strip()
    if rc != 0 or text == 'MISSING' or not text:
        return None
    return gzip.decompress(base64.b64decode(text.splitlines()[-1]))


# Read-only snapshotter: the debian-installer root (/run/live/installer, with its syslog) lives only in the
# installer worker's private mount namespace and vanishes when the worker exits, so a post-mortem finds
# nothing. A short-lived `nsenter -m ... cat` once a second copies the syslog to RAM; nothing stays in the
# namespace between copies. Diagnosis only (--diagnose), never part of a qualification pass.
POLL_SCRIPT = """#!/bin/sh
while :; do
  P=$(systemctl show -p MainPID --value wros-installer-session.service 2>/dev/null)
  if [ -n "$P" ] && [ "$P" != 0 ]; then
    nsenter -m -t "$P" cat /run/live/installer/var/log/syslog > /dev/shm/rc-syslog.new 2>/dev/null \\
      && mv /dev/shm/rc-syslog.new /dev/shm/rc-syslog.snap
    nsenter -m -t "$P" cat /run/live/installer/var/log/partman > /dev/shm/rc-partman.new 2>/dev/null \\
      && mv /dev/shm/rc-partman.new /dev/shm/rc-partman.snap
  fi
  sleep 1
done
"""


def start_syslog_poller(g):
    g.shell.put('/dev/shm/rc-poll.sh', POLL_SCRIPT, '0700')
    g.run('setsid sudo -n sh /dev/shm/rc-poll.sh </dev/null >/dev/null 2>&1 & true', 20, check=False)



def diagnose(ctx, g, secrets_):
    """Collect the installer's logs and the left-behind state without touching the VM. Every item is
    scanned for the generated secrets (all layout readings) the moment it is collected; a file that
    matches is written to diag-quarantine/ instead (never to the run dir top level / evidence)."""
    ctx.step('diagnose-start')
    nl = chr(10)
    variants = secret_variants(secrets_, ctx.args.layout)
    leaks, written, sources = [], [], {}
    quarantine = ctx.dir / 'diag-quarantine'

    def keep(label, data):
        if isinstance(data, str):
            data = data.encode('utf-8', 'replace')
        fname = 'diag-' + re.sub(r'[^A-Za-z0-9._-]+', '_', label.strip('/'))
        if not fname.endswith(('.log', '.json', '.txt')):
            fname += '.txt'
        found = []
        for name, forms_ in variants.items():
            for form_ in forms_:
                needle = form_.encode()
                if needle in data:
                    for number, line in enumerate(data.split(nl.encode()), 1):
                        if needle in line:
                            found.append((name, number))
        if found:
            quarantine.mkdir(exist_ok=True)
            (quarantine / fname).write_bytes(data)
            leaks.append({'file': fname, 'secret': sorted({n for n, _ in found}), 'lines': sorted({ln for _, ln in found})[:20]})
            ctx.log('SECRET LEAK in %s lines %s' % (fname, sorted({ln for _, ln in found})[:20]))
        else:
            (ctx.dir / fname).write_bytes(data)
            written.append(fname)
        return fname

    def sh(label, command, timeout=90):
        try:
            rc, out = g.run('{ %s; } </dev/null' % command, timeout, check=False)
            keep(label, 'rc=%d%s%s' % (rc, nl, out))
        except Exception as error:
            keep(label, 'COLLECTION FAILED: %s%s' % (error, nl))
            try:
                g.m.serial.send(chr(3) + nl)       # free a wedged shell
                time.sleep(2)
            except Exception:
                pass

    # the installer's own files first: they are the evidence that matters most
    sh('installer-tree', "sudo -n find /run/wros-installer -ls 2>&1 | grep -a -v 'media-upper/native-installer/pool\\|media-upper/install'")
    rc, listing = g.run("{ sudo -n find /run/wros-installer -type f \\( -name '*.log' -o -name 'outcome.json' -o -name 'baseline.json' -o -name cmdline -o -name 'installer-syslog' -o -name 'installer-partman' \\) -size -50000k; } </dev/null", 60, check=False)
    wanted = [l.strip() for l in listing.splitlines() if l.strip().startswith('/run/wros-installer/')]
    ctx.step('diagnose-installer-files', files=wanted)
    g.run('sudo -n pkill -f rc-poll.sh; true', 20, check=False)
    for path in wanted + ['/dev/shm/rc-syslog.snap', '/dev/shm/rc-partman.snap', '/run/live/installer/var/log/syslog', '/run/live/installer/var/log/partman',
                          '/var/log/syslog', '/var/log/installer/syslog', '/var/log/partman']:
        try:
            data = fetch_guest_file(g, path)
        except Exception as error:
            keep('failed-' + path, 'COLLECTION FAILED: %s%s' % (error, nl))
            continue
        if data is None:
            keep('missing-' + path, 'absent in the guest: %s%s' % (path, nl))
            continue
        sources[keep(path, data)] = path
        if path.endswith(('syslog', 'rc-syslog.snap')):
            lines = data.decode('utf-8', 'replace').split(nl)
            keep(path + '-grep', nl.join('%d:%s' % (i + 1, l) for i, l in enumerate(lines) if DIAG_GREP.search(l)) + nl)
    for n in (7, 1):
        keep('vcs%d-final-screen' % n, nl.join(g.vcs(n)))
    g.m.shot('diagnose-screen')
    commands = {
        'swaps': 'cat /proc/swaps',
        'dmsetup-tree': 'sudo -n dmsetup ls --tree; echo; sudo -n dmsetup info -c',
        'lsblk': 'lsblk -o NAME,TYPE,FSTYPE,SIZE,MOUNTPOINTS',
        'findmnt-target': 'findmnt -R /run/live/installer/target; echo rc=$?',
        'mountinfo-installer-and-target': 'grep -a -E "installer|/target|vg-|mapper" /proc/self/mountinfo',
        'installer-root-listing': 'sudo -n ls -la /run/live /run/live/installer /run/live/installer/target 2>&1 | head -80',
        'recent-files': "sudo -n find /var /tmp /run /root /home -mmin -30 -type f -not -path '/run/wros-installer/work/media-upper/*' -not -path '/var/lib/dpkg/*' -not -path '/var/lib/apt/*' -printf '%TT %p %s\\n' 2>/dev/null | sort | tail -200",
        'syslog-candidates': "sudo -n find / -xdev \\( -name syslog -o -name partman -o -name 'syslog.*' -o -name 'debian-installer*' -o -name 'wros-install*' \\) -not -path '/proc/*' 2>/dev/null | head -60",
        'target-and-installer-apt-sources': 'sudo -n cat /run/live/installer/target/etc/apt/sources.list /run/live/installer/target/etc/apt/sources.list.d/* /run/live/installer/etc/apt/sources.list 2>&1 | head -60',
        'unit-status': 'systemctl status wros-installer-session.service --no-pager -l 2>&1 | head -60',
        'unit-journal': 'sudo -n journalctl -b --no-pager -u wros-installer-session.service 2>&1',
        'journal-installer-grep': 'sudo -n journalctl -b --no-pager 2>&1 | grep -a -i -E "wros-install|pkexec|backend|di-run|installer|cryptsetup|swap" | tail -400',
        'dmesg-tail': 'sudo -n dmesg | tail -100',
        'ps-wchan': 'ps -eo pid,ppid,stat,wchan:32,etimes,args --forest | cut -c1-220',
        'ps-installer-stacks': "for p in $(pgrep -f 'wros-install|debian-installer|main-menu|di-run|exit-installer|cryptsetup|umount|apt|dpkg|systemd-run'); do echo \"== $p $(tr '\0' ' ' </proc/$p/cmdline | cut -c1-160)\"; sudo -n cat /proc/$p/wchan; echo; sudo -n cat /proc/$p/stack 2>&1 | head -12; ls -l /proc/$p/fd 2>/dev/null | head -12; done",
        'unit-show': 'systemctl show wros-installer-session.service -p MainPID,ActiveState,SubState,ExecMainStatus,ControlPID,Result 2>&1',
        'run-wros-installer-ls': 'sudo -n ls -la --time-style=full-iso /run/wros-installer /run/wros-installer/work 2>&1 | head -60',
    }
    for label, command in commands.items():
        sh(label, command, 120)
    ctx.result['diagnose'] = {'files': sorted(written), 'leaked_files': leaks, 'sources': sources}
    ctx.step('diagnose-done', files=len(written), leaks=leaks)


# ----------------------------------------------------------------------------- installed boot


def load_state(path):
    return json.loads((Path(path) / 'install-state.json').read_text())


def boot_installed(ctx, st, name, *, disk=None, fmt='raw', variables=None, extra=None, memory=3072, reboot=False):
    m = vm.Machine(ctx.dir, name, memory=memory, reboot=reboot)
    ctx.machines.append(m)
    local_vars = ctx.dir / (name + '-OVMF_VARS.fd')
    shutil.copyfile(variables or st['vars'], local_vars)
    m.firmware(local_vars)
    m.virtio_disk(disk or st['disk'], fmt=fmt, serial=st.get('serial'), bootindex=1, ident='target')
    if extra:
        extra(m)
    m.serial_login_credential()
    m.start()
    ctx.step('qemu-started', name=name, command=' '.join(m.command))
    return m


def unlock(ctx, m, secrets_, layout, *, wrong_first=True, after_reset=None):
    if after_reset is not None:
        deadline = time.monotonic() + 120
        while m.qmp.count('RESET') < after_reset and time.monotonic() < deadline:
            time.sleep(0.5)
        time.sleep(3)
    text = m.wait_screen(r'Please unlock disk|' + RATATUI_PROMPT, 420, name='unlock-wait')
    if re.search(RATATUI_PROMPT, text, re.I):
        return ratatui_unlock(ctx, m, secrets_, layout, wrong_first=wrong_first)
    m.shot('unlock-prompt')
    ctx.step('unlock-prompt-seen', screen=text[-300:])
    ctx.check('unlock prompt is on tty1 only (no QA console argument)', True)
    time.sleep(3)  # let askpass switch the tty to no-echo: keys typed earlier are flushed and echoed on screen
    if wrong_first:
        # the real passphrase typed as if the keyboard were US QWERTY: with a correct non-US map it must fail
        m.type_text(secrets_['luks'] + '\n', 'us' if layout != 'us' else layout)
        try:
            text = m.wait_screen(r'No key a.ailable|bad password|incorrect|try again', 120, name='unlock-wrong')
        except TimeoutError:
            m.shot('unlock-qwerty-accepted-or-unclear')
            raise Failure('QWERTY-typed passphrase was not rejected at the LUKS prompt (initramfs layout disagrees with the installer layout?)')
        m.shot('unlock-rejected')
        ctx.result['unlock_wrong_screen'] = text[-400:]
        ctx.check('wrong passphrase rejected', True)
        # a NEW prompt after the error text: the first prompt line is still on the screen and must not satisfy this
        m.wait_screen(r'(?s)Please unlock disk.*Please unlock disk', 120, name='unlock-again')
        time.sleep(3)
    m.type_text(secrets_['luks'] + '\n', layout)
    ctx.step('luks-typed', layout=layout)


def greeter_login(ctx, g, m, user, password, layout, *, wrong_first=True):
    g.wait_vcs(1, r'Local account', 240, what='greeter')
    m.shot('greeter')
    if wrong_first:
        m.type_text(user + '\n', layout)
        g.wait_vcs(1, r'\[hidden\]', 30, what='greeter-password-prompt')
        m.type_text(password + '\n', 'us' if layout != 'us' else layout)    # QWERTY reading of the real password
        screen = g.wait_vcs(1, r'Login was not accepted', 60, what='greeter-rejected')
        m.shot('greeter-rejected')
        ctx.check('wrong greeter password rejected', True)
        if 'Enter to restart login' in screen:
            m.key('ret')
        g.wait_vcs(1, r'Local account', 30, what='greeter-again')
    m.type_text(user + '\n', layout)
    g.wait_vcs(1, r'\[hidden\]', 30, what='greeter-password-prompt-2')
    m.type_text(password + '\n', layout)
    screen = signed_in(g, m)
    ctx.check('correct greeter password accepted', 'Sign in' not in screen)


# Off the login screen and onto the writing app's.
WRITING_SCREEN = r'\A(?!.*Sign in)(?=.*(?:Resume writing|WrOS))'


def signed_in(g, m, label=''):
    """After the password: rc9's login opens the session at once; earlier login screens (rc1 before an
    update, or after going back) first show 'Open writing session' and wait for Enter."""
    screen = g.wait_vcs(1, 'Open writing session|' + WRITING_SCREEN, 90, what='greeter-signed-in' + label)
    if 'Open writing session' in screen:
        m.key('ret')
        screen = g.wait_vcs(1, WRITING_SCREEN, 90, what='grimoire-home' + label)
    return screen


def secret_scan(ctx, g, secrets_):
    """Generated secrets must not appear in logs, the journal or the installer logs of the target."""
    g.shell.put('/dev/shm/rcpat', '\n'.join(secrets_.values()) + '\n', '0600')
    rc, out = g.run('grep -rIlF -f /dev/shm/rcpat /var/log /var/lib/wros-update /etc 2>/dev/null | head; '
                    'journalctl -b --no-pager 2>/dev/null | grep -cF -f /dev/shm/rcpat; shred -u /dev/shm/rcpat', 120, check=False)
    ctx.result['secret_scan_output'] = out.strip()
    lines = [l for l in out.strip().split('\n') if l.strip()]
    ctx.check('no generated secret in /var/log, /etc or the journal', lines == ['0'], lines)


def phase_installed_boot(ctx):
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout = st['layout']
    user = st['username']
    m = boot_installed(ctx, st, 'installed')
    unlock(ctx, m, secrets_, layout)
    g = Guest(ctx, m, root_via_sudo=False)
    g.shell.login(user, secrets_['login'], 420)
    ctx.step('serial-login-as-installed-user')
    # sudo must demand a password, and a wrong one must fail
    rc, _ = g.run('sudo -n true', 20, check=False)
    ctx.check('sudo needs a password', rc != 0)
    g.run('id -u', 10)
    g.become_root(secrets_['login'])
    g.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
    greeter_login(ctx, g, m, user, secrets_['login'], layout)
    snap = g.snap()
    procs = {p['comm']: p for p in snap['procs']}
    ctx.check('grimoire and wros-input run as uid 1000 on tty1',
              all(procs.get(n, {}).get('uid', [''])[0] == '1000' for n in ('grimoire', 'wros-input')) and procs['grimoire']['stdin'] == '4:1', procs.get('grimoire'))
    ctx.check('no live autologin, no boot=live', 'boot=live' not in snap['cmdline'] and 'wros.live' not in snap['cmdline'] and 'console=ttyS' not in snap['cmdline'], snap['cmdline'])
    ctx.check('no autologin getty', not any('autologin' in ' '.join(p['cmd']) for p in snap['procs'] if p['comm'] == 'agetty'))
    rc, out = g.run("id wros-live 2>&1; test -e /run/wros-live && echo RUNLIVE; test -e /etc/sudoers.d/live && echo SUDOERSLIVE; "
                    "grep -c initial_session /etc/greetd/config.toml; dpkg-query -W wros; cat /etc/writingdeck/release; passwd -S root; "
                    "cat /etc/default/keyboard; ls /etc/writingdeck; pkexec --disable-internal-agent /bin/true 2>&1 | head -2", 60, check=False)
    ctx.result['installed_state'] = out
    ctx.check('no live user or runtime marker', 'no such user' in out.lower() and 'RUNLIVE' not in out and 'SUDOERSLIVE' not in out, out[:400])
    ctx.check('wros package is 0.1.0~rc1', re.search(r'^wros\s+0\.1\.0~rc1', out, re.M) is not None, out[:300])
    ctx.check('root is locked', re.search(r'root L', out) is not None)
    ctx.check('installed /etc/default/keyboard carries the installer-selected layout',
              (re.search(r'XKBVARIANT="?colemak', out) is not None) if layout.startswith('colemak')
              else (re.search(r'XKBVARIANT="?%s' % layout, out) is not None), out)
    rc, facts = g.run('echo "--- /etc/default/keyboard"; cat /etc/default/keyboard; echo "--- conf.d/wros-keymap"; cat /etc/initramfs-tools/conf.d/wros-keymap 2>&1; '
                      'echo "--- initramfs keymap/console files"; lsinitramfs /boot/initrd.img-$(uname -r) | grep -i -E "keymap|console-setup|keyboard|kbd|loadkeys|setupcon" ; '
                      'echo "--- installer syslog wros-keymaps"; grep -a -n "wros-keymaps" /var/log/installer/syslog 2>&1 | head -20; '
                      'echo "--- writingdeck-keyboard"; cat /etc/writingdeck/keyboard.json 2>&1; echo; ls -la /etc/writingdeck/installer-keyboard-pending 2>&1; '
                      'systemctl show -p Result,ExecMainStatus,ActiveState writingdeck-keyboard.service; journalctl -b --no-pager -u writingdeck-keyboard.service 2>&1 | tail -30; '
                      'echo "--- console keymap"; dumpkeys -1 2>/dev/null | grep -a -E "^plain\\s+keycode +(16|17|18|19|20) "', 90, check=False)
    ctx.result['keyboard_facts'] = facts
    (ctx.dir / 'keyboard-facts.txt').write_text(facts)
    ctx.check('installer syslog records the Colemak keymaps being offered', 'Colemak keymaps offered' in facts or not layout.startswith('colemak'), facts[:600])
    ctx.check('conf.d/wros-keymap carries KEYMAP=y', 'KEYMAP=y' in facts.split('--- initramfs')[0].split('--- conf.d/wros-keymap')[-1], facts[:600])
    installed_layout = g.layout_in_effect()
    ctx.step('console-keymap-after-login', layout=installed_layout)
    rc, kb = g.run('cat /etc/writingdeck/keyboard.json; echo; test -e /etc/writingdeck/installer-keyboard-pending && echo PENDING_STILL_PRESENT; '
                   'lsinitramfs /boot/initrd.img-$(uname -r) | grep -ci keymap; '
                   'sha256sum /usr/bin/grimoire /usr/bin/wros-greeter /usr/bin/wros-installer; '
                   'systemctl is-active live-config.service wros-live-session.service 2>&1; ls /run/wros-live 2>&1', 60, check=False)
    ctx.result['installed_keyboard_and_hashes'] = kb
    ctx.step('keyboard-adoption-observed', keyboard=kb[:700])
    ctx.check('installed binaries equal the live image binaries',
              all(('%s  %s' % (h, n)) in kb for n, h in st.get('binary_sha256', {}).items()) and bool(st.get('binary_sha256')), kb)
    receipt_bins = ctx.result.get('receipt_binaries_sha256') or {}
    want = {'/usr/bin/' + n: v.get('squashfs_sha256') for n, v in receipt_bins.items() if isinstance(v, dict)}
    ctx.result['installed_vs_receipt'] = want
    ctx.check('installed binaries equal the receipt squashfs hashes',
              bool(want) and all(('%s  %s' % (h, n)) in kb for n, h in want.items() if h), want)
    ctx.check('no live runtime state on the installed system', 'No such file' in kb.split('wros-live-session.service')[-1] or 'inactive' in kb, kb[-300:])
    # writing round trip
    phrase = 'rcboot' + secrets.token_hex(5)
    screen = grim_new_scene(g, 'rcscene', 'Chapter one ' + phrase, layout)
    m.shot('installed-after-save')
    hits = g.probe('find-text', phrase, '/home')
    ctx.step('installed-save-search', hits=hits)
    ctx.check('manuscript saved, owned by uid 1000, not world-accessible',
              bool(hits) and all(h[1] == 1000 and h[2] in ('0o600', '0o640', '0o644', '0o660') for h in hits)
              and all(int(h[2], 8) & 0o002 == 0 for h in hits), hits)
    sums = g.run('sha256sum ' + ' '.join(h[0] for h in hits), 30)[1]
    (ctx.dir / 'manuscript.json').write_text(json.dumps({'phrase': phrase, 'hits': hits, 'sha256': sums}))
    reopened = grim_logout_login(ctx, g, user, secrets_['login'], layout)
    m.shot('installed-reopened')
    ctx.check('manuscript content reopened after logout/login', phrase in reopened, reopened[-300:])
    # tty2 maintenance login
    m.key('ctrl', 'alt', 'f2')
    g.wait_vcs(2, r'login:', 30, what='tty2-login')
    m.type_text(user + '\n', layout)
    g.wait_vcs(2, r'[Pp]assword:', 30, what='tty2-password')
    m.type_text(secrets_['login'] + '\n', layout)
    g.wait_vcs(2, r'(?m)@\S+:\S*\$ *$', 30, what='tty2-shell')
    m.type_text('sudo -p PWPROMPT: id -u\n', layout)
    g.wait_vcs(2, r'PWPROMPT:', 30, what='tty2-sudo-prompt')
    m.type_text(secrets_['login'] + '\n', layout)
    screen2 = g.wait_vcs(2, r'(?m)^0\s*$', 30, what='tty2-sudo-result')
    ctx.check('tty2 maintenance login works and sudo needs the password', True)
    m.shot('tty2-maintenance')
    m.key('ctrl', 'alt', 'f1')
    secret_scan(ctx, g, secrets_)
    ctx.check('no secret in serial transcript', not any(m.serial.contains(v) for v in secrets_.values()))
    g.run('poweroff', 20, check=False)
    try:
        m.wait_exit(120)
    except TimeoutError:
        m.kill()
    ctx.result['passed'] = True


# ----------------------------------------------------------------------------- update / rescue


def session_ready(ctx, m, st, secrets_, layout, *, unlock_passes=1, greeter=True, after_reset=None):
    """Unlock (possibly after earlier reboots), log in on serial, become root, log in at the greeter."""
    user = st['username']
    for _ in range(unlock_passes):
        unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=after_reset)
    g = Guest(ctx, m, root_via_sudo=False)
    g.shell.login(user, secrets_['login'], 600)
    g.become_root(secrets_['login'])
    g.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
    if greeter:
        greeter_login(ctx, g, m, user, secrets_['login'], layout, wrong_first=False)
    return g


def updates_panel(ctx, g, layout):
    grim_open_settings(g, layout)
    grim_to_panel(g, r'Updates')
    g.wait_vcs(1, r'Find update media', 30, what='updates-panel')


def find_and_review(ctx, g, expect_review=True):
    m = g.m
    g.choose(r'Find update media')
    m.key('ret')
    if not expect_review:
        time.sleep(20)
        return g.screen(1)
    return g.wait_vcs(1, r'Continue to confirmation|Signed by key', 90, what='update-review')


def pinned_signers(g):
    rc, out = g.run("grep -v '^#' /usr/share/wros/update-signers | tr -d ' \\n'", 20, check=False)
    return out.strip()


def wait_reset(ctx, m, previous, timeout=600, what='reset', dump_every=30):
    deadline = time.monotonic() + timeout
    shots = 0
    nextdump = time.monotonic() + dump_every
    while time.monotonic() < deadline:
        ctx.guard()
        if m.qmp.count('RESET') > previous:
            return m.qmp.count('RESET')
        if not m.is_running():
            raise Failure('guest stopped while waiting for %s' % what)
        if time.monotonic() > nextdump:
            shots += 1
            m.shot('%s-wait-%02d' % (what, shots))
            nextdump = time.monotonic() + dump_every
        time.sleep(1)
    raise Failure('no reboot within %ds (%s)' % (timeout, what))


NOTHING_STAGED = 'test -e /system-update && echo MARKER; sudo -n ls /var/lib/wros-update/staged 2>/dev/null | wc -l'


def stage_update(ctx, g, layout):
    """Find media, Review, Continue, Confirm (Cancel proven first), Stage."""
    m = g.m
    updates_panel(ctx, g, layout)
    review = find_and_review(ctx, g)
    m.shot('update-review')
    ctx.result['review_screen'] = review
    rc, modes = g.run("stat -c '%U:%G %a %n' /run/wros-update /run/wros-update/media /run/wros-update/media/*; findmnt -n -o TARGET,SOURCE,FSTYPE,OPTIONS /run/wros-update/media/0; sudo -n -u writer ls /run/wros-update/media/0 | head -3", 30, check=False)
    ctx.result['media_dir_modes'] = modes
    ctx.step('media-dir-modes', modes=modes)
    ctx.check('media directories are traversable by the writer (0755)', re.search(r'root:root 755 /run/wros-update$', modes, re.M) is not None and re.search(r'root:root 755 /run/wros-update/media$', modes, re.M) is not None, modes)
    ctx.check('review shows wros %s -> %s' % (ctx.args.from_version, ctx.target),
              re.search(r'wros.*' + re.escape(ctx.args.from_version) + r'.*' + re.escape(ctx.target),
                        review, re.S) is not None, review[-900:])
    signer = pinned_signers(g)
    ctx.check('review shows the pinned signer', bool(signer) and signer.lower() in review.lower().replace(' ', ''),
              (signer, review[-500:]))
    g.choose(r'Continue to confirmation')
    m.key('ret')
    confirm = g.wait_vcs(1, r'Stage this update', 30, what='update-confirm')
    ctx.check('confirmation defaults to Cancel', re.search(r'▸ Cancel', confirm) is not None, confirm[-400:])
    m.shot('update-confirm')
    m.key('ret')                      # Cancel
    time.sleep(3)
    rc, out = g.run(NOTHING_STAGED, 20, check=False)
    ctx.check('Cancel at confirmation staged nothing', 'MARKER' not in out and out.strip().endswith('0'), out)
    ctx.step('confirm-cancel-ok')
    if 'Find update media' not in g.screen(1):
        updates_panel(ctx, g, layout)
    find_and_review(ctx, g)
    g.choose(r'Continue to confirmation')
    m.key('ret')
    g.wait_vcs(1, r'Stage this update', 30, what='update-confirm-2')
    m.key('down')
    ctx.check('Stage row highlighted', re.search(r'▸ Stage this update', g.screen(1)) is not None)
    m.key('ret')
    staged = g.wait_vcs(1, r'Restart now to install the staged update', 120, what='update-staged')
    m.shot('update-staged')
    rc, out = g.run("readlink /system-update; sudo -n find /var/lib/wros-update/staged -name '*.deb'", 20, check=False)
    ctx.result['staged_listing'] = out
    ctx.check('update is staged (marker and retained archive)', '/var/lib/wros-update/staged' in out and '.deb' in out, out)
    return staged


def migration_state(g, user, *, legacy):
    config = 'writingdeck' if legacy else 'wros'
    g.run("test -f /home/%s/.config/grimoire/settings.toml || "
          "runuser -u %s -- sh -c 'mkdir -p ~/.config/grimoire && printf \"# rc6 profile fixture\\n\" > ~/.config/grimoire/settings.toml'"
          % (user, user))
    _, out = g.run('sha256sum /etc/%s/keyboard.json /home/%s/.config/grimoire/settings.toml; '
                   'getent group %s; id -G %s' % (config, user, config, user))
    keymap, profile, group, memberships = out.strip().splitlines()
    return {'keyboard': keymap.split()[0], 'profile': profile.split()[0],
            'group': group.split(':')[2], 'memberships': memberships.split(),
            'layout': g.layout_in_effect()}


def check_migration(ctx, g, user, before):
    after = migration_state(g, user, legacy=False)
    ctx.result['name_migration'] = {'before': before, 'after': after}
    for key in ('keyboard', 'profile', 'group', 'layout'):
        ctx.check('renaming preserved ' + key, after[key] == before[key], after[key])
    ctx.check('writer kept every pre-update group ID', set(before['memberships']) <= set(after['memberships']))
    rc, _ = g.run('cmp /etc/writingdeck/keyboard.json /etc/wros/keyboard.json && '
                  'test "$(readlink -f /usr/share/writingdeck/keymaps)" = /usr/share/wros/keymaps && '
                  'test "$(systemctl show -p Id --value writingdeck-keyboard.service)" = wros-keyboard.service && '
                  'id -nG %s | grep -qw writingdeck && id -nG %s | grep -qw wros && '
                  'cmp <(grimoire wros help) <(grimoire writingdeck help)' % (user, user), check=False)
    ctx.check('the old configuration, keymap, service, group and CLI names are usable', rc == 0, rc)


def phase_update(ctx):
    variant = ctx.args.variant
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout = st['layout']
    manuscript_file = Path(ctx.args.install_run) / 'manuscript.json'
    manuscript = json.loads(manuscript_file.read_text()) if manuscript_file.exists() else None
    if not ctx.args.bundle:
        raise SystemExit('--bundle is required')
    bundle = Path(ctx.args.bundle)
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk or st['disk'], overlay,
                      base_format='qcow2' if ctx.args.base_disk else 'raw')
    media = vm.make_update_media(bundle, ctx.dir / 'update-media.img', ctx.args.media_fs, 48)
    debs = sorted(p.relative_to(bundle).as_posix() for p in bundle.rglob('*.deb'))
    ctx.check('bundle carries exactly one wros deb', len(debs) == 1 and debs[0].split('/')[-1].startswith('wros_'), debs)
    ctx.result['bundle_deb'] = debs[0]
    ctx.target = debs[0].split('/')[-1].split('_')[1]  # the version the bundle installs
    ctx.result['bundle_deb_sha256'] = vm.sha256_file(bundle / debs[0])
    ctx.result['media_fs'] = ctx.args.media_fs
    if variant == 'tamper':
        if ctx.args.media_fs != 'ext4':
            raise SystemExit('tamper needs ext4 media')
        vm.flip_byte_in_ext4(media, '/' + debs[0], 4096)
        ctx.step('media-tampered', deb=debs[0])

    def attach(machine):
        machine.usb_stick(media, readonly=True, ident='updatemedia')

    m = boot_installed(ctx, st, 'update', disk=overlay, fmt='qcow2', extra=attach, reboot=True)
    g = session_ready(ctx, m, st, secrets_, layout)
    rc, out = g.run('dpkg-query -W wros; cat /etc/writingdeck/release', 20)
    ctx.check('starting version is ' + ctx.args.from_version,
              re.search(r'^wros\s+' + re.escape(ctx.args.from_version) + r'$', out, re.M), out)
    before = migration_state(g, st['username'], legacy=True) if ctx.args.check_migration else None
    rc, eng = g.run("f=/usr/lib/wros-update/engine.py; test -f \"$f\" || exit 1; "
                    "grep -n 'dmask=0022,fmask=0133' \"$f\"; sha256sum \"$f\"", 30, check=False)
    ctx.result['engine_fix_probe'] = eng
    ctx.check('installed update engine contains the media permission fix (dmask=0022,fmask=0133)', 'dmask=0022,fmask=0133' in eng, eng)

    if variant == 'reviewpath':
        # supplementary: "Review a signed update bundle by path" with media the writer mounted themselves
        rc, out = g.run("sudo -n mkdir -p /mnt/rcbundle && sudo -n mount -o ro,nodev,nosuid,noexec /dev/sda /mnt/rcbundle && ls /mnt/rcbundle && stat -c '%a %U %n' /mnt /mnt/rcbundle", 30)
        ctx.result['manual_mount'] = out
        updates_panel(ctx, g, layout)
        g.choose(r'Review a signed update bundle by path')
        m.key('ret')
        time.sleep(2)
        first = g.screen(1)
        m.shot('reviewpath-prompt')
        ctx.result['reviewpath_prompt_screen'] = first
        m.type_text('/mnt/rcbundle', layout)
        time.sleep(1)
        m.shot('reviewpath-typed')
        m.key('ret')                       # Enter in the field moves to the 'Review update' row
        time.sleep(1)
        g.wait_vcs(1, r'▸ Review update', 20, what='reviewpath-row')
        m.key('ret')
        screen = g.wait_vcs(1, r'Continue to confirmation|Signed by key|not reviewed|refus', 90, what='reviewpath-result')
        m.shot('reviewpath-result')
        ctx.result['reviewpath_result_screen'] = screen
        ctx.check('path review shows wros 0.1.0~rc1 -> ' + ctx.target, re.search(r'wros.*0\.1\.0~rc1.*' + re.escape(ctx.target), screen, re.S) is not None, screen[-900:])
        signer = pinned_signers(g)
        ctx.check('path review shows the pinned signer', bool(signer) and signer.lower() in screen.lower().replace(' ', ''), (signer, screen[-500:]))
        rc, out = g.run(NOTHING_STAGED, 20, check=False)
        ctx.check('review by path staged nothing', 'MARKER' not in out and out.strip().endswith('0'), out)
        ctx.result['passed'] = True
        return

    if variant == 'tamper':
        updates_panel(ctx, g, layout)
        screen = find_and_review(ctx, g, expect_review=False)
        m.shot('tampered-review')
        ctx.result['tamper_screen'] = screen
        rc, out = g.run(NOTHING_STAGED, 20, check=False)
        ctx.check('tampered bundle refused: no review screen', 'Continue to confirmation' not in screen, screen[-500:])
        ctx.check('tampered bundle: nothing staged', 'MARKER' not in out and out.strip().endswith('0'), out)
        ctx.check('refusal is visible to the writer',
                  re.search(r'refus|not |fail|invalid|mismatch|verif|could not', screen, re.I) is not None, screen[-500:])
        ctx.result['passed'] = True
        return

    try:
        stage_update(ctx, g, layout)
    except Failure:
        if getattr(ctx.args, 'hold', False):
            ctx.step('holding-after-failure')
            hold_for_operator(ctx, g, 1500)
        raise

    if variant == 'cancel':
        g.choose(r'Cancel staged update')
        m.key('ret')
        time.sleep(4)
        rc, out = g.run(NOTHING_STAGED, 20, check=False)
        ctx.check('staged update cancelled (marker and archive gone)', 'MARKER' not in out, out)
        m.shot('update-cancelled')
        resets = m.qmp.count('RESET')
        g.run('systemctl reboot', 20, check=False)
        wait_reset(ctx, m, resets, 180, 'cancel-reboot')
        g2 = session_ready(ctx, m, st, secrets_, layout)
        rc, out = g2.run('dpkg-query -W wros; cat /etc/writingdeck/release', 20)
        ctx.check('nothing applied; still 0.1.0~rc1', '0.1.0~rc1' in out and ctx.target not in out, out)
        ctx.result['passed'] = True
        return

    if variant == 'badstage':
        g.run('systemctl poweroff', 20, check=False)
        m.wait_exit(180)
        maintenance_corrupt_staged(ctx, st, secrets_, overlay)
        m2 = boot_installed(ctx, st, 'badstage-boot', disk=overlay, fmt='qcow2', reboot=True)
        g2 = session_ready(ctx, m2, st, secrets_, layout)
        time.sleep(60)
        ctx.check('no reboot loop after refused apply', m2.qmp.count('RESET') == 0, m2.qmp.events[-5:])
        rc, out = g2.run('dpkg-query -W wros; cat /var/lib/wros-update/public/status.json; ls -l /system-update 2>&1', 30, check=False)
        ctx.result['badstage_state'] = out
        ctx.check('version unchanged after refused apply', '0.1.0~rc1' in out, out)
        updates_panel(ctx, g2, layout)
        screen = g2.screen(1)
        m2.shot('badstage-updates-panel')
        ctx.result['badstage_updates_screen'] = screen
        ctx.check('Settings reports the failure', re.search(r'Update state: (failed|interrupted)', screen) is not None, screen[-600:])
        ctx.result['passed'] = True
        return

    # ok / powercut: restart now to install
    g.choose(r'Restart now to install')
    resets = m.qmp.count('RESET')
    m.key('ret')
    ctx.step('restart-now-pressed')
    resets = wait_reset(ctx, m, resets, 300, 'restart-to-apply')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets)
    if variant == 'powercut':
        # the offline apply runs a few seconds after the unlock (the console font is unreadable to the OCR, so the
        # cut point is timed from the unlock and classified afterwards from the durable update journal)
        time.sleep(float(ctx.args.cut_at))
        m.shot('before-power-cut')
        m.kill()
        ctx.step('power-cut', resets=m.qmp.count('RESET'))
        m2 = boot_installed(ctx, st, 'after-cut', disk=overlay, fmt='qcow2', reboot=True)
        # a cut before the apply began leaves the trigger in place: the next boot applies the update again and
        # reboots by itself, which needs a second unlock. Either path is legitimate; record which one happened.
        unlock(ctx, m2, secrets_, layout, wrong_first=False)
        deadline2 = time.monotonic() + 150
        rebooted_again = False
        while time.monotonic() < deadline2:
            if m2.serial.contains('login: '):
                break
            if m2.qmp.count('RESET') > 0:
                rebooted_again = True
                unlock(ctx, m2, secrets_, layout, wrong_first=False, after_reset=m2.qmp.count('RESET'))
                break
            time.sleep(1)
        ctx.step('after-cut-boot', applied_again_and_rebooted=rebooted_again)
        ctx.result['after_cut_reapplied_and_rebooted'] = rebooted_again
        g2 = Guest(ctx, m2, root_via_sudo=False)
        g2.shell.login(st['username'], secrets_['login'], 600)
        g2.become_root(secrets_['login'])
        g2.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
        greeter_login(ctx, g2, m2, st['username'], secrets_['login'], layout, wrong_first=False)
        resets_after_login = m2.qmp.count('RESET')
        rc, out = g2.run('dpkg-query -W wros; cat /etc/writingdeck/release; cat /var/lib/wros-update/public/status.json; dpkg --audit; echo AUDIT_DONE', 60, check=False)
        ctx.result['after_cut_state'] = out
        rc, jr = g2.run("sudo -n cat /var/lib/wros-update/journal.jsonl 2>/dev/null | python3 -c \"import sys,json;print([json.loads(l).get('state') for l in sys.stdin if l.strip()])\"; sudo -n journalctl -b -1 --no-pager 2>/dev/null | grep -a -c 'wros-offline-update'", 30, check=False)
        ctx.result['cut_journal_states'] = jr
        ctx.step('cut-classification', journal_states_and_prev_boot_unit_lines=jr)
        time.sleep(90)
        ctx.check('no reboot loop after the power cut (at most the one apply reboot, then stable)', m2.qmp.count('RESET') == resets_after_login and resets_after_login <= 1, m2.qmp.count('RESET'))
        updates_panel(ctx, g2, layout)
        screen = g2.screen(1)
        m2.shot('after-cut-updates-panel')
        ctx.result['after_cut_updates_screen'] = screen
        ctx.check('Settings shows an update result',
                  re.search(r'Update state: (interrupted|recovered|completed|failed)', screen) is not None, screen[-600:])
        ctx.check('dpkg audit clean', 'AUDIT_DONE' in out and not re.search(r'not fully installed|half-', out), out)
        if manuscript:
            hits = g2.probe('find-text', manuscript['phrase'], '/home')
            ctx.check('manuscript intact', bool(hits), hits)
        ctx.result['passed'] = True
        return

    # ok: the update applies, the system reboots by itself, then unlock again
    try:
        resets = wait_reset(ctx, m, resets, int(getattr(ctx.args, 'apply_wait', 1500)), 'apply-then-reboot')
    except Failure:
        if getattr(ctx.args, 'hold', False):
            ctx.step('holding-after-apply-wait-failure')
            m.shot('apply-wait-timeout')
            gh = Guest(ctx, m, root_via_sudo=False)
            gh.shell.login(st['username'], secrets_['login'], 120)
            gh.become_root(secrets_['login'])
            hold_for_operator(ctx, gh, 1500)
        raise
    ctx.step('system-rebooted-itself-after-applying')
    g2 = session_ready(ctx, m, st, secrets_, layout, after_reset=resets)
    rc, out = g2.run('dpkg-query -W wros; cat /etc/writingdeck/release; cat /var/lib/wros-update/public/status.json; test -e /system-update && echo MARKER; dpkg --audit; echo AUDIT_DONE; sudo -n ls /var/lib/wros-update/staged 2>&1 | head -3', 30, check=False)
    ctx.result['after_update_state'] = out
    ctx.check('wros is ' + ctx.target, re.search(r'^wros\s+' + re.escape(ctx.target), out, re.M) is not None, out)
    ctx.check('release file carries ' + ctx.target, len(re.findall(re.escape(ctx.target), out)) >= 2, out)
    ctx.check('update marker consumed', 'MARKER' not in out, out)
    ctx.check('dpkg audit clean after the update', 'AUDIT_DONE' in out and not re.search(r'not fully installed|half-|unconfigured', out.split('AUDIT_DONE')[0].split('MARKER')[-1]), out[-400:])
    if before:
        check_migration(ctx, g2, st['username'], before)
    updates_panel(ctx, g2, layout)
    screen = g2.screen(1)
    m.shot('after-update-updates-panel')
    ctx.result['after_update_updates_screen'] = screen
    ctx.check('Settings shows the update completed', re.search(r'Update state: completed', screen) is not None, screen[-600:])
    time.sleep(60)
    ctx.check('no reboot loop (no further reboot after 60 s)', m.qmp.count('RESET') == resets)
    if manuscript:
        hits = g2.probe('find-text', manuscript['phrase'], '/home')
        ctx.check('manuscript intact after update', bool(hits), hits)
    g2.run('poweroff', 20, check=False)
    ctx.result['passed'] = True


OPEN_LUKS_SCRIPT = """dev=$(lsblk -rpno NAME,FSTYPE | awk '$2=="crypto_LUKS"{print $1}')
test "$(echo $dev | wc -w)" = 1 || { echo "LUKS device count: $dev"; exit 3; }
cryptsetup open $2 --key-file /dev/shm/rckey $dev $1
rc=$?
shred -u /dev/shm/rckey
vgchange -ay >/dev/null 2>&1
lsblk -rpno NAME,TYPE,FSTYPE
exit $rc
"""

LUKS_DEV = "dev=$(lsblk -rpno NAME,FSTYPE | awk '$2==\"crypto_LUKS\"{print $1}')"


def maintenance_corrupt_staged(ctx, st, secrets_, overlay):
    """Offline edit via a live maintenance boot: flip one byte of the retained staged .deb."""
    m = vm.Machine(ctx.dir, 'maint', memory=3072)
    ctx.machines.append(m)
    m.firmware(vm.fresh_vars(ctx.dir, 'maint-OVMF_VARS.fd'))
    m.cdrom(ctx.iso)
    m.virtio_disk(overlay, fmt='qcow2', ident='maintdisk')
    m.serial_login_credential()
    m.start()
    wait_for_firmware_menu(ctx, m, 'maint-grub')
    m.key('ret')
    g = live_login(ctx, m)
    g.shell.put('/dev/shm/rckey', secrets_['luks'], '0600')
    g.shell.put('/tmp/rcopen.sh', OPEN_LUKS_SCRIPT, '0644')
    rc, out = g.run('sudo -n sh /tmp/rcopen.sh rcmaint', 120)
    ctx.result['maintenance_lsblk'] = out
    rc, out2 = g.run("ls /dev/mapper", 20)
    candidates = [x for x in out2.split() if x not in ('control', 'rcmaint')]
    ctx.check('one LVM root volume found', len(candidates) >= 1, out2)
    root = '/dev/mapper/' + next((c for c in candidates if 'root' in c), candidates[0])
    g.run("sudo -n sh -c 'mkdir -p /mnt/rc && mount %s /mnt/rc'" % root, 30)
    rc, listing = g.run('sudo -n ls -R /mnt/rc/var/lib/wros-update/staged', 30)
    ctx.result['maintenance_staged'] = listing
    script = ("import glob\nfor f in glob.glob('/mnt/rc/var/lib/wros-update/staged/**/*.deb', recursive=True):\n"
              " d=bytearray(open(f,'rb').read()); d[len(d)//2]^=1; open(f,'wb').write(d); print('CORRUPTED',f)\n")
    g.shell.put('/tmp/corrupt.py', script, '0644')
    rc, out4 = g.run('sudo -n python3 /tmp/corrupt.py; sync', 30)
    ctx.check('staged archive corrupted offline', 'CORRUPTED' in out4, out4)
    g.run("sudo -n sh -c 'umount /mnt/rc; vgchange -an >/dev/null 2>&1; cryptsetup close rcmaint'", 60, check=False)
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    try:
        m.wait_exit(120)
    except TimeoutError:
        m.kill()


def phase_peek(ctx):
    """Read-only look at the installed filesystem of a (failed or finished) install run, via a qcow2
    overlay and a live maintenance boot. Never reads shadow, never prints a secret."""
    run = Path(ctx.args.install_run)
    secrets_ = json.loads((run / 'secrets.json').read_text())
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(run / 'target.img', overlay)
    m = vm.Machine(ctx.dir, 'peek', memory=3072)
    ctx.machines.append(m)
    m.firmware(vm.fresh_vars(ctx.dir, 'peek-OVMF_VARS.fd'))
    m.cdrom(ctx.iso)
    m.virtio_disk(overlay, fmt='qcow2', ident='peekdisk')
    m.serial_login_credential()
    m.start()
    wait_for_firmware_menu(ctx, m, 'peek-grub')
    m.key('ret')
    g = live_login(ctx, m)
    g.shell.put('/dev/shm/rckey', secrets_['luks'], '0600')
    g.shell.put('/tmp/rcopen.sh', OPEN_LUKS_SCRIPT, '0644')
    # DIAGNOSIS (labelled): which reading of the typed physical keys is the real passphrase?
    typed = ctx.args.layout
    readings = {}
    for which in ('luks', 'login'):
        for typed_as in ('colemak_dh', 'colemak', 'dvorak', 'us'):
            try:
                for read_as, text in secret_readings(secrets_[which], typed_as).items():
                    for suffix, tag in (('', ''), (chr(10), '+newline')):
                        readings.setdefault(text + suffix, []).append('%s typed-as-%s read-as-%s%s' % (which, typed_as, read_as, tag))
            except ValueError:
                pass
    readings = {labels[0] + (' (+%d same string)' % (len(labels) - 1) if len(labels) > 1 else ''): text for text, labels in readings.items()}
    g.shell.put('/tmp/rctest.sh', LUKS_DEV + chr(10) + 'cryptsetup open --test-passphrase --key-file "$1" $dev; echo rc=$?' + chr(10), '0644')
    matches = []
    for layout, text in readings.items():
        g.shell.put('/dev/shm/rckey_try', text, '0600')
        rc, res = g.run('sudo -n sh /tmp/rctest.sh /dev/shm/rckey_try', 60, check=False)
        ok = 'rc=0' in res
        if ok:
            ctx.step('luks-reading-test', candidate=layout, opens=ok)
        if ok:
            matches.append(layout)
            g.shell.put('/dev/shm/rckey', text, '0600')
            break
    g.run('shred -u /dev/shm/rckey_try', 20, check=False)
    ctx.result['luks_opened_by_reading'] = matches
    if not matches:
        raise Failure('no reading of the typed keys opens the volume')
    rc, out = g.run('sudo -n sh /tmp/rcopen.sh rcpeek --readonly', 120)
    ctx.result['peek_lsblk'] = out
    rc, out2 = g.run('ls /dev/mapper', 20)
    candidates = [x for x in out2.split() if x not in ('control', 'rcpeek')]
    root = '/dev/mapper/' + next((c for c in candidates if 'root' in c), candidates[0])
    g.run("sudo -n sh -c 'mkdir -p /mnt/rc && mount -o ro,noload %s /mnt/rc'" % root, 30)
    commands = {
        'passwd-system-accounts': "grep -E '^(root|_greetd|writer|rc|_wros-installer|wros-live|greetd|build[a-z]*):' /mnt/rc/etc/passwd; echo; grep -n greetd /mnt/rc/etc/group; echo; cut -d: -f1,3,6,7 /mnt/rc/etc/passwd | tail -12",
        'greetd-config': 'cat /mnt/rc/etc/greetd/config.toml; ls -la /mnt/rc/etc/greetd',
        'keyboard': 'cat /mnt/rc/etc/default/keyboard; cat /mnt/rc/etc/initramfs-tools/conf.d/wros-keymap 2>&1; ls -la /mnt/rc/etc/writingdeck',
        'initramfs-keymap': 'ls -la /mnt/rc/etc/console-setup /mnt/rc/etc/initramfs-tools/hooks /mnt/rc/etc/initramfs-tools/conf.d 2>&1; cat /mnt/rc/etc/initramfs-tools/conf.d/* 2>&1 | head -30; zcat /mnt/rc/etc/console-setup/cached_*.kmap.gz 2>/dev/null | grep -c keycode; ls -la /mnt/rc/boot 2>&1',
        'kmap-keycodes': "cat /mnt/rc/etc/writingdeck/keyboard.json; echo; cat /mnt/rc/etc/console-setup/cached_setup_keyboard.sh; zcat /mnt/rc/etc/console-setup/cached_UTF-8_del.kmap.gz | grep -E '^keycode +(16|17|18|19|20|21|22|23|24|25|31|32|33|34|35|36|37|38) '",
        'initrd-keymap': "u=$(grep -v '^#' /mnt/rc/etc/fstab | grep -E '[[:space:]]/boot[[:space:]]' | sed 's/[[:space:]].*//'); mkdir -p /mnt/rcb /tmp/ir; echo boot-device=$u; mount -o ro $u /mnt/rcb 2>&1; ls -la /mnt/rcb | head -12; unmkinitramfs /mnt/rcb/initrd.img-* /tmp/ir 2>&1 | tail -2; find /tmp/ir -iname '*keymap*' -o -iname '*kmap*' -o -name loadkeys -o -name setupcon | head; find /tmp/ir -name 'cached_*' | head; umount /mnt/rcb",
        'initrd-kmap-dump': "head -c 300 /tmp/ir/etc/console-setup/cached_UTF-8_del.kmap; echo; grep -E '^keycode +(16|17|18|19|20|21|22|23|24|25|30|31|32|33|34|35|36|37|38) ' /tmp/ir/etc/console-setup/cached_UTF-8_del.kmap | cut -c1-70; echo ---; zcat /mnt/rc/etc/console-setup/cached_UTF-8_del.kmap.gz | cmp - /tmp/ir/etc/console-setup/cached_UTF-8_del.kmap && echo kmap-identical-to-target-cache; echo ---; cat /tmp/ir/scripts/init-top/keymap; echo ---; cat /tmp/ir/conf/conf.d/wros-keymap; ls -la /tmp/ir/etc/console-setup /tmp/ir/etc/default 2>&1 | head -20; cat /tmp/ir/etc/default/keyboard 2>&1",
        'target-markers': 'ls -la /mnt/rc/wros-target-* /mnt/rc/etc/writingdeck 2>&1; cat /mnt/rc/etc/crypttab; cat /mnt/rc/etc/fstab',
        'installer-logs': 'ls -la /mnt/rc/var/log/installer 2>&1; grep -a -n "wros-keymaps" /mnt/rc/var/log/installer/syslog 2>&1 | head',
        'release': 'cat /mnt/rc/etc/writingdeck/release 2>&1; ls /mnt/rc/var/lib/dpkg/info | grep -i -E "^(wros|greetd)" | head',
        'finish-hooks-left-in-target': 'ls -la /mnt/rc/usr/lib/finish-install.d 2>&1 | head',
    }
    for label, command in commands.items():
        rc, text = g.run("sudo -n sh -c '%s'" % command.replace("'", "'" + chr(92) + "''"), 60, check=False)
        (ctx.dir / ('peek-%s.txt' % label)).write_text(text)
        ctx.step('peek-' + label, rc=rc, head=text[:300])
    g.run("sudo -n sh -c 'umount /mnt/rc; vgchange -an >/dev/null 2>&1; cryptsetup close rcpeek'", 60, check=False)
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    try:
        m.wait_exit(120)
    except TimeoutError:
        m.kill()
    ctx.result['passed'] = True


def phase_audit(ctx):
    """Dry run of the in-target proof on a real installed disk: unlock a qcow2 overlay of the kept disk, mount it
    read-write on the overlay with /boot and /boot/efi, bind /proc /sys /dev /run, drop a candidate supervisor script
    in a scratch path and run `target-audit`. The kept disk is never written."""
    run = Path(ctx.args.install_run)
    secrets_ = json.loads((run / 'secrets.json').read_text())
    script_dir = Path('/home/builder/rc-script')
    script = (script_dir / 'wros-install-session').read_bytes()
    expect = (script_dir / 'SHA256_WINDOWS_SIDE').read_text().strip()
    got = vm.sha256_file(script_dir / 'wros-install-session')
    ctx.result['candidate_script'] = {'commit': (script_dir / 'COMMIT').read_text().strip(), 'sha256': got, 'sha256_windows_side': expect}
    ctx.check('candidate script hash equals the Windows-side file', got == expect, (got, expect))
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(run / 'target.img', overlay)
    base_before = vm.sha256_file(run / 'target.img') if (run / 'target.img').stat().st_size < (1 << 20) else None
    m = vm.Machine(ctx.dir, 'audit', memory=3072)
    ctx.machines.append(m)
    m.firmware(vm.fresh_vars(ctx.dir, 'audit-OVMF_VARS.fd'))
    m.cdrom(ctx.iso)
    m.virtio_disk(overlay, fmt='qcow2', ident='auditdisk')
    m.serial_login_credential()
    m.start()
    wait_for_firmware_menu(ctx, m, 'audit-grub')
    m.key('ret')
    g = live_login(ctx, m)
    g.shell.put('/tmp/rcopen.sh', OPEN_LUKS_SCRIPT, '0644')
    g.shell.put('/dev/shm/rckey', secrets_['luks'], '0600')
    rc, out = g.run('sudo -n sh /tmp/rcopen.sh vda3_crypt', 120)
    rc, out2 = g.run('ls /dev/mapper', 20)
    candidates = [x for x in out2.split() if x not in ('control', 'vda3_crypt')]
    root = '/dev/mapper/' + next((c for c in candidates if 'root' in c), candidates[0])
    g.run("sudo -n sh -c 'mkdir -p /mnt/rc && mount %s /mnt/rc'" % root, 30)
    rc, fstab = g.run('sudo -n cat /mnt/rc/etc/fstab', 20)
    ctx.result['target_fstab'] = fstab
    mounts = []
    for line in fstab.splitlines():
        parts = line.split()
        if len(parts) >= 2 and not line.lstrip().startswith('#') and parts[1] in ('/boot', '/boot/efi'):
            mounts.append((parts[1], parts[0]))
    for mountpoint, device in sorted(mounts, key=lambda x: len(x[0])):
        g.run("sudo -n sh -c 'mkdir -p /mnt/rc%s && mount %s /mnt/rc%s'" % (mountpoint, device, mountpoint), 30)
    ctx.check('/boot and /boot/efi mounted from fstab', len(mounts) == 2, mounts)
    for d in ('proc', 'sys', 'dev'):
        g.run('sudo -n mount --rbind /%s /mnt/rc/%s' % (d, d), 30)
    # a fresh /run: binding the live system's /run would plant its /run/wros-live marker in the target and trip the hygiene proof
    g.run('sudo -n mount -t tmpfs -o mode=755 tmpfs /mnt/rc/run', 30)
    # carry the candidate script into a scratch path inside the target (never over the installed copy)
    g.shell.put('/dev/shm/cand-script', script, '0600')
    g.run("sudo -n sh -c 'mkdir -p /mnt/rc/root/rc-audit-scratch && cp /dev/shm/cand-script /mnt/rc/root/rc-audit-scratch/wros-install-session "
          "&& chmod 0700 /mnt/rc/root/rc-audit-scratch/wros-install-session'", 20)
    rc, sums = g.run('sudo -n sha256sum /mnt/rc/root/rc-audit-scratch/wros-install-session /mnt/rc/usr/libexec/wros-install-session', 20)
    ctx.result['script_hashes_in_target'] = sums
    ctx.check('scratch copy carries the candidate hash', got in sums.splitlines()[0], sums)
    nonce = secrets.token_hex(16)
    rc, audit = g.run('sudo -n chroot /mnt/rc /usr/bin/python3 -I /root/rc-audit-scratch/wros-install-session target-audit %s 2>&1' % nonce, 180, check=False)
    ctx.result['target_audit_exit_code'] = rc
    ctx.result['target_audit_output'] = audit
    (ctx.dir / 'target-audit-output.txt').write_text('exit code %d' % rc + chr(10) + audit)
    ctx.step('target-audit-ran', exit_code=rc, output=audit[:1500])
    # DRY-RUN-ONLY diagnostic, labelled: the same script with the 64 KiB command-reply limit raised in a scratch copy,
    # to learn whether anything after the first failing check would also fail. Never a pass for the product.
    relax = ("p = '/mnt/rc/root/rc-audit-scratch/'" + chr(10) + "d = open(p + 'wros-install-session').read()" + chr(10)
             + "n = d.replace('len(reply.stdout) <= 65536', 'len(reply.stdout) <= 67108864')" + chr(10)
             + "assert n != d" + chr(10) + "open(p + 'relaxed', 'w').write(n)" + chr(10))
    g.shell.put('/tmp/relax.py', relax, '0644')
    g.run('sudo -n python3 /tmp/relax.py && sudo -n chmod 0700 /mnt/rc/root/rc-audit-scratch/relaxed', 20)
    rc_d, diff = g.run("sudo -n diff /mnt/rc/root/rc-audit-scratch/wros-install-session /mnt/rc/root/rc-audit-scratch/relaxed", 20, check=False)
    ctx.result['relaxed_copy_diff'] = diff
    nonce2 = secrets.token_hex(16)
    rc_r, audit2 = g.run('sudo -n chroot /mnt/rc /usr/bin/python3 -I /root/rc-audit-scratch/relaxed target-audit %s 2>&1' % nonce2, 180, check=False)
    ctx.result['relaxed_audit_exit_code'] = rc_r
    ctx.result['relaxed_audit_output'] = audit2
    (ctx.dir / 'target-audit-relaxed-limit-output.txt').write_text('DRY-RUN-ONLY (reply limit raised in a scratch copy); exit code %d' % rc_r + chr(10) + diff + chr(10) + audit2)
    ctx.step('target-audit-relaxed-ran', exit_code=rc_r, output=audit2[:1800])
    rc_s, size = g.run("sudo -n chroot /mnt/rc sh -c 'lsinitramfs /boot/initrd.img-$(uname -r) | wc -lc'", 60, check=False)
    ctx.result['lsinitramfs_lines_bytes'] = size
    ctx.step('lsinitramfs-size', size=size)
    facts = {
        'conf-d-wros-keymap': 'ls -la /mnt/rc/etc/initramfs-tools/conf.d/; echo ---; cat /mnt/rc/etc/initramfs-tools/conf.d/wros-keymap 2>&1',
        'etc-default-keyboard': 'cat /mnt/rc/etc/default/keyboard',
        'initramfs-filtered': 'for f in /mnt/rc/boot/initrd.img-*; do echo "== $f"; chroot /mnt/rc lsinitramfs ${f#/mnt/rc} | grep -i -E "keymap|console-setup|cryptroot|keyboard|kbd|loadkeys"; done 2>&1',
        'installer-syslog-grep-wros-keymaps': 'ls -la /mnt/rc/var/log/installer/ 2>&1; grep -a -n "wros-keymaps" /mnt/rc/var/log/installer/syslog 2>&1',
    }
    for label, command in facts.items():
        rc2, text = g.run("sudo -n sh -c '%s'" % command.replace("'", "'" + chr(92) + "''"), 90, check=False)
        (ctx.dir / ('audit-%s.txt' % label)).write_text(text)
        ctx.step('audit-' + label, rc=rc2, head=text[:400])
    g.run("sudo -n sh -c 'umount -R /mnt/rc; vgchange -an >/dev/null 2>&1; cryptsetup close vda3_crypt; shred -u /dev/shm/cand-script'", 90, check=False)
    try:
        g.run('sudo -n /sbin/poweroff', 20, check=False)
    except Exception:
        pass
    try:
        m.wait_exit(120)
    except TimeoutError:
        m.kill()
    overlay.unlink()
    ctx.step('overlay-discarded')
    ctx.result['passed'] = rc == 0


INITRD_FACTS = ("ls -l --time-style=full-iso /boot/initrd.img-*; stat -c '%Y %n' /boot/initrd.img-*; sha256sum /boot/initrd.img-*; "
                "findmnt -n -o TARGET,SOURCE /boot")


def make_initramfs_trigger_pending(ctx, g):
    nl = chr(10)
    """Leave initramfs-tools with a pending trigger, the way an interrupted package step does. Returns the method."""
    status_cmd = "dpkg-query -W -f='${Status}' initramfs-tools"
    attempts = []
    # A real dpkg-written unfinished state: a tiny local package whose postinst activates update-initramfs,
    # installed with --no-triggers, so dpkg records initramfs-tools as triggers-pending in its status file.
    control = nl.join(['Package: rcprobe', 'Version: 1', 'Architecture: all', 'Maintainer: qa <qa@example.invalid>',
                       'Description: qa probe that activates update-initramfs', ''])
    postinst = nl.join(['#!/bin/sh', 'set -e', 'if [ "$1" = configure ]; then dpkg-trigger update-initramfs; fi', 'exit 0', ''])
    g.run('mkdir -p /tmp/rcp/DEBIAN && chmod 755 /tmp/rcp /tmp/rcp/DEBIAN', 20)
    g.shell.put('/tmp/rcp/DEBIAN/control', control, '0644')
    g.shell.put('/tmp/rcp/DEBIAN/postinst', postinst, '0755')
    rc, out = g.run("dpkg-deb --build /tmp/rcp /tmp/rcprobe.deb >/dev/null && dpkg --no-triggers -i /tmp/rcprobe.deb 2>&1 | tail -5; echo AUDIT-BEGIN; dpkg --audit; echo AUDIT-END; "
                    + status_cmd + "; echo; grep -A2 '^Package: initramfs-tools$' /var/lib/dpkg/status", 120, check=False)
    attempts.append(out)
    st_line = out.split('AUDIT-END')[-1].strip().splitlines()[0] if 'AUDIT-END' in out else ''
    method = 'dpkg --no-triggers -i of a local package whose postinst runs dpkg-trigger update-initramfs'
    if st_line.strip() == 'install ok installed':
        method = None
    out_last = out
    out = out_last
    status = out.split('AUDIT-END')[-1].strip().splitlines()[0] if 'AUDIT-END' in out else ''
    if method is None:
        method = 'dpkg-trigger could not leave it pending'
    status = out.split('AUDIT-END')[-1].strip().splitlines()[0] if 'AUDIT-END' in out else ''
    if status.strip() == 'install ok installed':
        # the trigger was only queued in Unincorp: write the state dpkg itself records for a pending trigger
        script = (
            "import re\n"
            "p='/var/lib/dpkg/status'\n"
            "t=open(p).read()\n"
            "parts=t.split('\\n\\n')\n"
            "for i,s in enumerate(parts):\n"
            " if s.startswith('Package: initramfs-tools\\n'):\n"
            "  s=s.replace('Status: install ok installed','Status: install ok triggers-pending')\n"
            "  if 'Triggers-Pending:' not in s:\n"
            "   s=s.rstrip('\\n')+'\\nTriggers-Pending: update-initramfs'\n"
            "  parts[i]=s\n"
            "open(p,'w').write('\\n\\n'.join(parts))\n"
            "print('EDITED')\n")
        g.shell.put('/tmp/rctrig.py', script, '0644')
        rc, out2 = g.run("python3 /tmp/rctrig.py; echo AUDIT-BEGIN; dpkg --audit; echo AUDIT-END; " + status_cmd + "; echo", 60, check=False)
        attempts.append(out2)
        status = out2.split('AUDIT-END')[-1].strip().splitlines()[0] if 'AUDIT-END' in out2 else ''
        method = 'dpkg-trigger left it queued; status entry set to triggers-pending'
    ctx.result['trigger_attempts'] = attempts
    ctx.result['trigger_method'] = method
    ctx.result['initramfs_tools_status_before_rescue'] = status
    ctx.check('initramfs-tools is not "install ok installed" before the rescue', status.strip() != 'install ok installed' and bool(status.strip()), attempts)
    return method


def phase_rescue(ctx):
    unfinished = getattr(ctx.args, 'unfinished', False)
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout = st['layout']
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk or st['disk'], overlay)
    before = None
    if unfinished:
        mp = boot_installed(ctx, st, 'prep', disk=overlay, fmt='qcow2')
        gp = session_ready(ctx, mp, st, secrets_, layout, greeter=False)
        rc, before = gp.run(INITRD_FACTS, 30, check=False)
        ctx.result['initrd_before'] = before
        make_initramfs_trigger_pending(ctx, gp)
        rc, stz = gp.run("grep -A12 '^Package: initramfs-tools$' /var/lib/dpkg/status | head -14; ls -la /var/lib/dpkg/updates; cat /var/lib/dpkg/triggers/Unincorp /var/lib/dpkg/triggers/File | head -20", 30, check=False)
        ctx.result['status_stanza_before_poweroff'] = stz
        gp.run('sync; poweroff', 20, check=False)
        mp.wait_exit(120)
        ctx.step('prep-powered-off')
    m = vm.Machine(ctx.dir, 'rescue', memory=3072)
    ctx.machines.append(m)
    m.firmware(vm.fresh_vars(ctx.dir, 'rescue-OVMF_VARS.fd'))
    m.cdrom(ctx.iso)
    m.virtio_disk(overlay, fmt='qcow2', ident='rescuedisk')
    m.serial_login_credential()
    m.start()
    wait_for_firmware_menu(ctx, m, 'rescue-grub')
    m.key('ret')
    g = live_login(ctx, m)
    rc, out = g.run('command -v wros-rescue; ls -l /usr/sbin/wros-rescue /usr/libexec/wros-rescue 2>&1; sudo -n wros-rescue --help 2>&1 | head -20', 30, check=False)
    ctx.result['rescue_tool'] = out
    ctx.check('wros-rescue is present on the live image', re.search(r'^-rwxr-xr-x .* /usr/sbin/wros-rescue$', out, re.M) is not None, out)
    g.shell.put('/tmp/rchead.sh', LUKS_DEV + chr(10) + 'echo $dev; dd if=$dev bs=1M count=16 2>/dev/null | sha256sum' + chr(10), '0644')
    head = 'sudo -n sh /tmp/rchead.sh'
    rc, header = g.run(head, 60)
    ctx.result['luks_header_before'] = header
    s = m.serial
    tag = secrets.token_hex(4)
    with s.lock:
        s.cursor = len(s.buffer)
    s.send("sudo wros-rescue; echo @@RESCUE_DONE_%s:$?\n" % tag)
    deadline = time.monotonic() + 900
    sent = False
    code = None
    pattern = re.compile(r'([Pp]ass(?:phrase|word)[^\n]*:|@@RESCUE_DONE_%s:[0-9]+)' % tag)
    while time.monotonic() < deadline:
        ctx.guard()
        try:
            hit = s.expect(pattern, 30)
        except TimeoutError:
            continue
        text = hit.group(1).decode('utf-8', 'replace')
        if text.startswith('@@RESCUE_DONE'):
            code = int(text.rsplit(':', 1)[1])
            break
        if not sent:
            time.sleep(0.5)
            s.send(secrets_['luks'] + '\n')
            sent = True
            ctx.step('rescue-passphrase-sent')
    ctx.check('wros-rescue finished', code is not None)
    ctx.result['rescue_exit_code'] = code
    tail = s.tail(4000)
    ctx.result['rescue_output_tail'] = tail
    ctx.result['rescue_output_lines_of_interest'] = [ln for ln in tail.splitlines()
                                                     if re.search(r'rename|/boot|Unlock|Found the installed|[Rr]epair|"status"|warning|refus|mount', ln)]
    ctx.check('passphrase was requested by wros-rescue', sent)
    ctx.check('wros-rescue finished with exit 0', code == 0, tail[-1500:])
    ctx.check('rescue did not warn about /boot/efi', 'could not mount /boot/efi' not in tail, tail[-600:])
    want = 'recovered' if unfinished else 'clean'
    ctx.check('rescue-repair status is "%s"' % want, ('"status": "%s"' % want) in tail, tail[-800:])
    rc, state = g.run("sudo -n sh -c 'ls /dev/mapper; echo TARGETMOUNTS=$(grep -c /target /proc/mounts)'", 20, check=False)
    ctx.result['after_rescue_live_state'] = state
    entries = [x.strip() for x in state.split('TARGETMOUNTS=')[0].split() if x.strip()]
    mounts = state.split('TARGETMOUNTS=')[-1].strip().splitlines()[0] if 'TARGETMOUNTS=' in state else '?'
    ctx.check('only /dev/mapper/control remains and no /target mount is left', entries == ['control'] and mounts == '0', state)
    rc, header2 = g.run(head, 60)
    ctx.check('LUKS header unchanged', header == header2, (header, header2))
    ctx.check('no secret in serial transcript', not s.contains(secrets_['luks']))
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    m.wait_exit(120)
    m2 = boot_installed(ctx, st, 'after-rescue', disk=overlay, fmt='qcow2', reboot=True)
    g2 = session_ready(ctx, m2, st, secrets_, layout)
    probe = (INITRD_FACTS + "; echo FACTS-END; dpkg --audit; echo AUDIT_DONE; dpkg-query -W -f='${Status}' initramfs-tools; echo; "
             "mkdir -p /mnt/rcroot; mount --bind / /mnt/rcroot; echo ROOTBOOT-BEGIN; ls -la /mnt/rcroot/boot; echo ROOTBOOT-END; umount /mnt/rcroot; "
             "dpkg-query -W wros")
    rc, out = g2.run(probe, 60, check=False)
    ctx.result['after_rescue_state'] = out
    ctx.check('disk boots, unlocks with the Colemak-DH passphrase and logs in after rescue', 'wros' in out)
    audit = out.split('FACTS-END')[-1].split('AUDIT_DONE')[0]
    ctx.check('dpkg --audit is clean after rescue', not audit.replace('dpkg-query', '').strip(), audit)
    status_line = out.split('AUDIT_DONE')[-1].strip().splitlines()[0] if 'AUDIT_DONE' in out else ''
    ctx.check('initramfs-tools is fully installed after rescue', status_line.strip() == 'install ok installed', status_line)
    rootboot = out.split('ROOTBOOT-BEGIN')[-1].split('ROOTBOOT-END')[0]
    ctx.check("the root filesystem's own /boot has no initrd (nothing was written under the unmounted /boot)", 'initrd' not in rootboot, rootboot)
    if unfinished:
        def facts(text):
            mtimes = dict((n, int(t)) for t, n in re.findall(r'^(\d{9,}) (/boot/initrd\.img-\S+)$', text, re.M))
            hashes = dict((n, h) for h, n in re.findall(r'^([0-9a-f]{64})  (/boot/initrd\.img-\S+)$', text, re.M))
            return mtimes, hashes
        mt0, h0 = facts(before)
        mt1, h1 = facts(out.split('FACTS-END')[0])
        ctx.result['initrd_mtime_hash_before'] = {'mtime': mt0, 'sha256': h0}
        ctx.result['initrd_mtime_hash_after'] = {'mtime': mt1, 'sha256': h1}
        ctx.check('/boot initrd was rebuilt: newer mtime and a different hash',
                  bool(mt0) and all(mt1.get(n, 0) > t for n, t in mt0.items()) and all(h1.get(n) not in (None, h) for n, h in h0.items()),
                  (mt0, mt1, h0, h1))
    ctx.result['passed'] = True


def menu_number(screen, label):
    """The number the recovery menu shows beside a choice ("  3  Show what happened")."""
    found = re.findall(r'(?m)^\s*(\d)\s+' + re.escape(label), screen)
    if not found:
        raise Failure('recovery menu has no choice %r:\n%s' % (label, screen[-800:]))
    return found[-1]


def feature_console_lock(ctx, g, st, secrets_, layout, *, suspend=False, signed_in=False):
    m, user = g.m, st['username']
    if not signed_in:
        greeter_login(ctx, g, m, user, secrets_['login'], layout, wrong_first=False)
    phrase = 'lockscene' + secrets.token_hex(4)
    second = 'locknote' + secrets.token_hex(4)
    grim_new_scene(g, 'Lock second scene', second, layout)
    ctx.check('the editor shows the disposable scene',
              phrase in grim_new_scene(g, 'Lock test scene', phrase, layout))
    m.key('f11')
    m.type_text('Lock second scene', layout)
    m.key('ret')
    g.wait_vcs(1, re.escape(second), 20, what='lock-secondary-buffer')
    editor = next(p for p in g.snap()['procs'] if p['comm'] == 'grimoire')
    grim_open_settings(g, layout)
    grim_to_panel(g, r'Battery and power')
    if suspend:
        g.choose(r'^Lock and suspend$')
        m.key('ret')
        g.wait_vcs(1, r'Confirm Suspend', 20, what='confirm-suspend')
        m.key('right')
        m.key('ret')
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline and m.qmp.count('SUSPEND') == 0:
            time.sleep(0.2)
        ctx.check('QEMU suspended after the editor requested the lock', m.qmp.count('SUSPEND') == 1)
        m.qmp.command('system_wakeup')
        time.sleep(4)  # Serial is resumed after the emulated hardware wakes.
        g.run('true', 30)
    else:
        g.choose(r'^Lock writing session$')
        m.key('ret')
    g.wait_vcs(8, r'Press Enter to unlock', 30, what='console-lock')
    ctx.check('tty8 owns input and tty1 has no reader or manuscript on screen',
              not any(p['comm'] == 'wros-input' for p in g.snap()['procs'])
              and not g.screen(1).strip()
              and g.run('fgconsole')[1].strip() == '8')
    m.key('alt', 'f1')
    ctx.check('the locker prevents switching back to tty1', g.run('fgconsole')[1].strip() == '8')
    wrong = 'lockprobe' + secrets.token_hex(7)
    m.key('ret')
    g.wait_vcs(8, r"Password:", 15, what='lock-password')
    m.type_text(wrong + '\n', layout)
    g.wait_vcs(8, r'\A(?!.*Password:).*Press Enter to unlock', 30, what='lock-password-rejected')
    m.key('ret')
    g.wait_vcs(8, r"Password:", 15, what='lock-password-retry')
    m.type_text(secrets_['login'] + '\n', layout)
    g.wait_vcs(1, r'Lock writing session|Wi-Fi and networking', 40, what='lock-return')
    procs = g.snap()['procs']
    ctx.check('Colemak unlock returned to the same editor with a fresh reader',
              any(p['pid'] == editor['pid'] and p['comm'] == 'grimoire' for p in procs)
              and any(p['ppid'] == editor['pid'] and p['comm'] == 'wros-input' for p in procs))
    m.key('esc')
    m.key('esc')
    m.key('esc')
    grim_editor(g)
    screen = g.wait_vcs(1, re.escape(phrase), 15, what='writing-after-unlock')
    ctx.check('both writing buffers survived the lock', second in screen)
    m.ctrl('s', layout)
    # A second lock in the same boot: the first lock's finished service must not block it (#93).
    grim_open_settings(g, layout)
    grim_to_panel(g, r'Battery and power')
    g.choose(r'^Lock writing session$')
    m.key('ret')
    g.wait_vcs(8, r'Press Enter to unlock', 30, what='second-console-lock')
    m.key('ret')
    g.wait_vcs(8, r"Password:", 15, what='second-lock-password')
    m.type_text(secrets_['login'] + '\n', layout)
    g.wait_vcs(1, r'Lock writing session|Wi-Fi and networking', 40, what='second-lock-return')
    ctx.check('a second lock in the same boot returned to the same editor',
              any(p['pid'] == editor['pid'] and p['comm'] == 'grimoire' for p in g.snap()['procs']))
    m.key('esc')
    m.key('esc')
    m.key('esc')
    g.shell.put('/dev/shm/lock-probe', wrong + '\n' + secrets_['login'] + '\n', '0600')
    rc, _ = g.run(
        '(set -o pipefail; p=/dev/shm/lock-probe; trap "rm -f $p" EXIT; '
        'grep -rlaF -f "$p" /home /var/log /var/lib/wros-update /etc >/dev/null; '
        'test "$?" = 1 || exit 11; journalctl --no-pager > /dev/shm/lock-journal || exit 12; '
        'grep -aqF -f "$p" /dev/shm/lock-journal; found=$?; rm -f /dev/shm/lock-journal; '
        'test "$found" = 1)', 90, check=False)
    ctx.check('neither lock password reached the saved book, recovery files or system logs', rc == 0, rc)


def phase_lock(ctx):
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk, overlay, base_format='qcow2')
    def attach(machine):
        if ctx.args.lock_fixture:
            media = vm.make_update_media(ctx.args.lock_fixture, ctx.dir / 'fixture.img', label='WROS-TEST')
            machine.usb_stick(media, readonly=True, ident='testfixture')
        if ctx.args.test_suspend:
            machine.args.extend(['-global', 'ICH9-LPC.disable_s3=0'])

    m = boot_installed(ctx, st, 'lock', disk=overlay, fmt='qcow2', extra=attach,
                       reboot=ctx.args.check_migration)
    g = session_ready(ctx, m, st, secrets_, st['layout'], greeter=False)
    if ctx.args.lock_fixture:
        before = migration_state(g, st['username'], legacy=True) if ctx.args.check_migration else None
        g.run('mkdir -p /mnt/testfixture && mount -o ro,nodev,nosuid,noexec LABEL=WROS-TEST /mnt/testfixture '
              '&& dpkg --force-confold -i /mnt/testfixture/wros_*.deb && systemctl daemon-reload '
              '&& systemctl restart systemd-logind', 120)
        if before:
            resets = m.qmp.count('RESET')
            g.run('(sleep 1; systemctl reboot) >/dev/null 2>&1 & echo reboot-requested')
            resets = wait_reset(ctx, m, resets, 120, 'fixture reboot')
            g = session_ready(ctx, m, st, secrets_, st['layout'], after_reset=resets, greeter=False)
            check_migration(ctx, g, st['username'], before)
    feature_console_lock(ctx, g, st, secrets_, st['layout'], suspend=ctx.args.test_suspend)
    ctx.result['passed'] = True


def feature_power_policy(ctx, g):
    service = 'org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager '
    _, capabilities = g.run('for mode in CanSuspend CanHibernate CanHybridSleep CanSuspendThenHibernate; '
                            'do printf "%s " "$mode"; busctl call ' + service + '"$mode" || exit 1; done')
    ctx.result['logind_sleep_capabilities'] = capabilities
    for mode in ('CanHibernate', 'CanHybridSleep', 'CanSuspendThenHibernate'):
        ctx.check(mode + ' is disabled', re.search(r'^' + mode + r' s "(?:no|na)"$', capabilities, re.M))
    properties = ('HandleLidSwitch HandleLidSwitchExternalPower HandleLidSwitchDocked '
                  'HandlePowerKey HandleRebootKey HandleSuspendKey HandleHibernateKey IdleAction')
    _, policy = g.run('busctl get-property ' + service + properties)
    ctx.result['logind_power_policy'] = policy
    ctx.check('logind leaves physical key, lid and idle actions to WrOS',
              policy.strip().splitlines() == ['s "ignore"'] * len(properties.split()), policy)
    rc, refusal = g.run('systemctl start systemd-suspend.service', 30, check=False)
    ctx.check('the suspend unit refuses to start without the protected lock',
              rc != 0 and 'systemd-suspend.service' in refusal and g.m.qmp.count('SUSPEND') == 0, refusal)


def phase_rc5(ctx):
    """rc5 on a disk the update phase brought to rc5: the boot menu, the recovery boot, the quiet
    console, the Shift-arrow keymap, settings in backups and the Google client for phone linking.
    --base-disk is the update run's disk.qcow2."""
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout, user = st['layout'], st['username']
    if not ctx.args.base_disk:
        raise SystemExit('--base-disk (the update run disk.qcow2) is required')
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk, overlay, base_format='qcow2')
    # rc7's checks (power policy, lock and sleep) hold for every later version too.
    rc7 = subprocess.run(['dpkg', '--compare-versions', ctx.args.expect, 'ge', '0.1.0~rc7']).returncode == 0
    def enable_sleep(machine):
        machine.args.extend(['-global', 'ICH9-LPC.disable_s3=0'])
    m = boot_installed(ctx, st, 'rc5', disk=overlay, fmt='qcow2', reboot=True,
                       extra=enable_sleep if rc7 else None)
    unlock(ctx, m, secrets_, layout, wrong_first=False)
    g = Guest(ctx, m, root_via_sudo=False)
    g.shell.login(user, secrets_['login'], 420)
    g.become_root(secrets_['login'])
    g.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
    g.wait_vcs(1, r'Local account', 240, what='greeter')
    time.sleep(5)  # wros-boot-ok runs once greetd has started
    rc, out = g.run("dpkg-query -W wros; grep -c \"menuentry 'WrOS recovery'\" /boot/grub/grub.cfg; "
                    "grep -c 'systemd.unit=wros-recovery.target' /boot/grub/grub.cfg; grep -c '(recovery mode)' /boot/grub/grub.cfg; "
                    "grep -c 'save_env wros_boot_pending' /boot/grub/grub.cfg; grep -E '^set timeout(_style)?=' /boot/grub/grub.cfg | head -4; "
                    "echo ENV; grub-editenv /boot/grub/grubenv list; echo SYSCTL; cat /proc/sys/kernel/printk; "
                    "echo BOOTOK; systemctl show -p Result,ExecMainStatus,ExecMainStartTimestampMonotonic wros-boot-ok.service; "
                    "echo LINKS; ls /etc/systemd/system/multi-user.target.wants/ /etc/systemd/system/system-update.target.wants/; "
                    "echo CLIENT; stat -c '%a %U %n' /usr/share/wros/google-client.json /usr/lib/wros-backup/link.py /usr/libexec/wros-recovery; "
                    "echo KEYMAP; M=/usr/share/writingdeck/keymaps/colemak.map; F=$(sed -n 's/^shift keycode 105 = //p' $M); "
                    "echo slot=$F; grep -h \"^string $F = \" $M; "
                    "echo LOADED; dumpkeys -C /dev/tty1 2>&1 | grep \"^string $F \"; "
                    "echo DIAG; dumpkeys -C /dev/tty1 2>&1 | grep -E '^(shift[[:space:]]+)?keycode 105 ' | head -3; "
                    "cat /etc/writingdeck/keyboard.json; journalctl -b --no-pager -u writingdeck-keyboard 2>&1 | tail -3", 90, check=False)
    ctx.result['rc5_state'] = out
    (ctx.dir / 'rc5-state.txt').write_text(out)
    expect = ctx.args.expect
    ctx.check('wros is %s' % expect, re.search(r'^wros\s+%s$' % re.escape(expect), out, re.M) is not None, out[:200])
    if expect != '0.1.0~rc5':
        # rc6: normal boots carry loglevel=3 (the recovery entry does not), and the Realtek
        # Wi-Fi options are installed.
        rc, more = g.run("cat /proc/cmdline; echo GRUB; grep -E '^\\s*linux\\s' /boot/grub/grub.cfg; "
                         "echo MODPROBE; cat /etc/modprobe.d/60-wros-wifi-power.conf", 30, check=False)
        ctx.result['rc6_state'] = more
        cmdline, rest = more.split('GRUB', 1)
        entries, modprobe = rest.split('MODPROBE', 1)
        ctx.check('this normal boot ran with loglevel=3', 'loglevel=3' in cmdline.split(), cmdline)
        recovery = [line for line in entries.splitlines() if 'wros-recovery.target' in line]
        ctx.check('the recovery entry keeps boot messages visible',
                  recovery and all('loglevel' not in line for line in recovery), entries[-600:])
        ctx.check('the Realtek Wi-Fi power options are installed',
                  'options rtw88_core disable_lps_deep=y' in modprobe and 'options rtw88_pci disable_aspm=y' in modprobe,
                  modprobe)
    counts = re.findall(r'(?m)^(\d+)$', out.split('set timeout')[0])
    ctx.check('grub.cfg has the WrOS recovery entry booting wros-recovery.target', counts[:2] == ['1', '1'], counts)
    ctx.check("Debian's root-shell recovery entries are gone", counts[2:3] == ['0'], counts)
    ctx.check('grub.cfg marks every boot pending (plain /boot)', counts[3:4] == ['1'], counts)
    env_block = out.split('ENV', 1)[1].split('SYSCTL', 1)[0]
    ctx.check('this boot reached the login screen and cleared the mark', 'wros_boot_pending' not in env_block, env_block)
    ctx.check('kernel messages stay off the console', out.split('SYSCTL', 1)[1].split()[0] == '3', out.split('SYSCTL', 1)[1][:40])
    ctx.check('wros-boot-ok ran and succeeded', 'Result=success' in out and 'ExecMainStatus=0' in out, out.split('BOOTOK', 1)[1][:300])
    links = out.split('LINKS', 1)[1].split('CLIENT', 1)[0]
    ctx.check('the postinst enabled wros-boot-ok for normal and update boots', links.count('wros-boot-ok.service') == 2, links)
    client = out.split('CLIENT', 1)[1].split('KEYMAP', 1)[0]
    ctx.check('the Google client and the linking module are installed', '644 root /usr/share/wros/google-client.json' in client
              and '644 root /usr/lib/wros-backup/link.py' in client and '755 root /usr/libexec/wros-recovery' in client, client)
    keymap, loaded = out.split('KEYMAP', 1)[1].split('LOADED', 1)
    ctx.check('the installed keymap sends Shift+Left as an xterm sequence',
              'slot=F' in keymap and '\\033[1;2D' in keymap, keymap)
    ctx.check("tty1's loaded keymap carries it", '\\033[1;2D' in loaded.split('DIAG')[0], loaded)

    # Settings in a backup, through the installed helper as the writer, with real restic.
    helper = ("runuser -u %s -- env -i HOME=/home/%s PATH=/usr/bin:/bin LC_ALL=C.UTF-8 python3 -I /usr/libexec/wros-backup"
              % (user, user))
    rc, out = g.run(
        "(set -e; B=/home/%s/rc5-book; R=/home/%s/rc5-repo; C=/home/%s/.config/grimoire; "
        "runuser -u %s -- mkdir -p $B/manuscript $C; echo 'rc5 scene' | runuser -u %s -- tee $B/manuscript/a.md >/dev/null; "
        "echo 'theme = \"rc5 original\"' | runuser -u %s -- tee $C/settings.toml >/dev/null; "
        "echo '{\"operation\":\"init\",\"repository\":\"'$R'\"}' | %s >/dev/null; "
        "echo '{\"operation\":\"backup\",\"project\":\"'$B'\"}' | %s > /tmp/rc5-backup.json; "
        "python3 -c 'import json;d=json.load(open(\"/tmp/rc5-backup.json\"))[\"data\"];s=d[\"snapshots\"][0];print(\"SNAP\",s[\"id\"],s[\"has_profile\"],d[\"phone_linking\"])')"
        % (user, user, user, user, user, user, helper, helper), 600, check=False)
    ctx.result['backup_with_settings'] = out[-600:]
    snap = re.search(r'SNAP ([0-9a-f]{64}) (\w+) (\w+)', out)
    ctx.check('a backup carries the settings and the build offers phone linking',
              snap is not None and snap.group(2) == 'True' and snap.group(3) == 'True', out[-400:])
    if snap:
        rc, out = g.run(
            "C=/home/%s/.config/grimoire; echo 'theme = \"changed\"' | runuser -u %s -- tee $C/settings.toml >/dev/null; "
            "echo '{\"operation\":\"restore\",\"snapshot\":\"%s\",\"destination\":\"/home/%s/rc5-restored\",\"settings\":true}' | %s "
            "| python3 -c 'import json,sys;print(\"NOTICE\",json.load(sys.stdin)[\"data\"][\"notice\"])'; cat $C/settings.toml; "
            "ls -d $C/before-restore-* && cat $C/before-restore-*/settings.toml; cat /home/%s/rc5-restored/manuscript/a.md"
            % (user, user, snap.group(1), user, helper, user), 600, check=False)
        ctx.result['restore_with_settings'] = out[-800:]
        ctx.check('restoring with settings puts them back and keeps the current ones',
                  'theme = "rc5 original"' in out and 'theme = "changed"' in out and 'rc5 scene' in out
                  and 'Settings restored' in out, out[-600:])

    if rc7:
        feature_power_policy(ctx, g)
        ctx.check('canonical keyboard files and unit exist alongside rc6 aliases',
                  g.run('test -f /etc/wros/keyboard.json && test -f /usr/share/wros/keymaps/colemak.map && '
                        'systemctl is-enabled wros-keyboard.service && test -L /etc/writingdeck/keyboard.json',
                        check=False)[0] == 0)
        feature_console_lock(ctx, g, st, secrets_, layout, suspend=True)

    # The recovery boot has no serial shell, and tty1 uses the installed console font, not the
    # VGA font the screen reader knows: take console-setup's cached copy of that font.
    rc, b64 = g.run('f=$(ls /etc/console-setup/cached_*.psf.gz 2>/dev/null | head -1); test -n "$f" && base64 -w0 "$f"',
                    30, check=False)
    font = None
    if rc == 0 and b64.strip():
        font_path = ctx.dir / 'tty1-font.psf.gz'
        font_path.write_bytes(base64.b64decode(b64.strip()))
        font = vm.Font(font_path)
    ctx.step('tty1-font', copied=font is not None)

    def wait_tty(pattern, timeout, name):
        regex = re.compile(pattern, re.I | re.S)
        deadline, last = time.monotonic() + timeout, ''
        while time.monotonic() < deadline:
            last = vm.png_text(m.shot(name), font)
            if regex.search(last):
                return last
            time.sleep(2)
        raise Failure('screen text %r not seen; last screen:\n%s' % (pattern, last[-600:]))

    # A boot that never reaches the login screen: the next boot shows the menu with WrOS recovery.
    g.run('grub-editenv /boot/grub/grubenv set wros_boot_pending=1; sync', 30, check=False)
    resets = m.qmp.count('RESET')
    g.shell.serial.send('systemctl reboot\n')
    deadline = time.monotonic() + 120
    while m.qmp.count('RESET') <= resets and time.monotonic() < deadline:
        time.sleep(0.5)
    # The menu waits 30 s after a failed boot. End (repeated until the menu is surely up; it is
    # harmless before) selects the last entry, WrOS recovery; any key also stops the countdown.
    for _ in range(14):
        time.sleep(1)
        m.key('end')
    menu = m.screen_text('grub-after-failed-boot')
    ctx.result['grub_menu_screen'] = menu[-600:]
    ctx.step('grub-menu-after-failed-boot', seen='WrOS recovery' in menu or 'Debian' in menu)
    m.shot('grub-recovery-selected')
    m.key('ret')
    unlock(ctx, m, secrets_, layout, wrong_first=False)
    screen = wait_tty(r'Password for ' + re.escape(user), 300, 'recovery-password')
    m.shot('recovery-password')
    ctx.check('the WrOS recovery boot asks for the writer\'s password', 'WrOS recovery' in screen, screen[-400:])
    m.type_text(secrets_['login'] + '\n', 'us' if layout != 'us' else layout)  # QWERTY reading: wrong
    wait_tty(r'did not match', 60, 'recovery-wrong')
    ctx.check('a wrong password is refused', True)
    m.type_text(secrets_['login'] + '\n', layout)
    screen = wait_tty(r'Choose:', 90, 'recovery-menu')
    m.shot('recovery-menu')
    ctx.result['recovery_menu'] = screen[-900:]
    ctx.check('the menu offers repair, restore, what happened, login, restart and power off, and no shell',
              all(label in screen for label in ('Repair system packages', 'Restore a book', 'Show what happened',
                                               'Continue to the normal login', 'Power off'))
              and 'shell' not in screen.lower(), screen[-900:])
    m.type_text(menu_number(screen, 'Show what happened') + '\n', layout)
    screen = wait_tty(r'runs WrOS ' + re.escape(ctx.args.expect), 60, 'recovery-what')
    m.shot('recovery-what-happened')
    ctx.check('Show what happened names the installed version', True)
    m.type_text(menu_number(screen, 'Continue to the normal login') + '\n', layout)
    g2 = Guest(ctx, m, root_via_sudo=False)
    g2.shell.login(user, secrets_['login'], 300)
    g2.become_root(secrets_['login'])
    g2.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
    g2.wait_vcs(1, r'Local account', 240, what='greeter-after-recovery')
    time.sleep(5)
    rc, out = g2.run('grub-editenv /boot/grub/grubenv list; systemctl is-active greetd; cat /proc/cmdline', 30, check=False)
    ctx.result['after_recovery'] = out
    ctx.check('continuing from recovery reaches the login screen and clears the mark',
              'wros_boot_pending' not in out and 'active' in out and 'systemd.unit=wros-recovery.target' in out, out)
    m.shot('login-after-recovery')
    ctx.result['passed'] = True


GOBACK_GUEST = r'''
import importlib.machinery, json, sys
engine = importlib.machinery.SourceFileLoader('engine', '/usr/lib/wros-update/engine.py').load_module()
verb = sys.argv[1]
if verb == 'stage':
    review = engine.review_bundle(sys.argv[2])
    print(json.dumps(engine.stage_update(sys.argv[2], review['review_id'])))
elif verb == 'back':
    print(json.dumps(engine.stage_rollback()))
else:
    print(json.dumps(engine.inspect_recovery()))
'''


def phase_goback(ctx):
    """Going back a version on real disks: rc5 -> rc5.1 -> rc5.2 through the installed engine,
    then back to rc5.1. --bundle and --bundle2 are the rc5.1 and rc5.2 test bundles;
    --base-disk is the update run's disk.qcow2 (rc5)."""
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout, user = st['layout'], st['username']
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk, overlay, base_format='qcow2')
    sticks = []
    for i, bundle in enumerate((ctx.args.bundle, ctx.args.bundle2)):
        sticks.append(vm.make_update_media(Path(bundle), ctx.dir / ('stick%d.img' % i), 'ext4', 48))

    def attach(machine):
        for i, stick in enumerate(sticks):
            machine.usb_stick(stick, ident='usb%d' % i, serial='RCSTICK%d' % i, readonly=True)

    m = boot_installed(ctx, st, 'goback', disk=overlay, fmt='qcow2', reboot=True, extra=attach)

    def login(after_reset=None):
        unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=after_reset)
        g = Guest(ctx, m, root_via_sudo=False)
        g.shell.login(user, secrets_['login'], 600)
        g.become_root(secrets_['login'])
        g.shell.put('/root/goback.py', GOBACK_GUEST.encode(), '0600')
        return g

    def reboot_through_update(g):
        resets = m.qmp.count('RESET')
        g.shell.serial.send('systemctl reboot\n')
        # Wait for each reset: the last boot's unlock prompt can still be on the screen.
        unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets + 1)  # the update boot
        return login(after_reset=resets + 2)                                          # the new version

    def state(g, label):
        rc, out = g.run("dpkg-query -W -f='${Version}\\n' wros; find /var/lib/wros-update/installed -name '*.deb' -printf '%f\\n' 2>&1; "
                        "echo PREVIOUS; find /var/lib/wros-update/previous -name '*.deb' -printf '%f\\n' 2>&1; "
                        "echo STATUS; python3 -I /root/goback.py status", 60, check=False)
        ctx.result['state_' + label] = out
        return out

    g = login()
    g.run('mkdir -p /mnt/b0 /mnt/b1; for i in 0 1; do d=$(readlink -f /dev/disk/by-id/usb-*RCSTICK$i*); mount -o ro $d /mnt/b$i; done; ls /mnt/b0 /mnt/b1', 60)
    for i, version in ((0, '0.1.0~rc5.1'), (1, '0.1.0~rc5.2')):
        rc, out = g.run('python3 -I /root/goback.py stage /mnt/b%d' % i, 300, check=False)
        ctx.check('the installed engine stages %s' % version, '"staged"' in out, out[-400:])
        g = reboot_through_update(g)
        g.run('mkdir -p /mnt/b0 /mnt/b1; for i in 0 1; do d=$(readlink -f /dev/disk/by-id/usb-*RCSTICK$i*); mount -o ro $d /mnt/b$i; done', 60, check=False)
        out = state(g, version)
        ctx.check('%s is installed' % version, out.splitlines()[0].strip() == version, out[:200])
    out = state(g, 'before-going-back')
    kept = out.split('PREVIOUS', 1)[1].split('STATUS', 1)[0]  # kept packages are named by their hash
    ctx.check('rc5.2 keeps rc5.1 as the previous version',
              '.deb' in kept and '"previous_version": "0.1.0~rc5.1"' in out, out[-600:])
    rc, out = g.run('python3 -I /root/goback.py back', 300, check=False)
    ctx.check('going back is staged', '"rollback": true' in out, out[-400:])
    g = reboot_through_update(g)
    out = state(g, 'after-going-back')
    ctx.check('going back installed rc5.1 again', out.splitlines()[0].strip() == '0.1.0~rc5.1', out[:200])
    ctx.check('after going back, rc5.1 is the installed tree and nothing older is offered',
              '.deb' in out.split('PREVIOUS', 1)[0] and '.deb' not in out.split('PREVIOUS', 1)[1].split('STATUS', 1)[0]
              and '"previous_version": null' in out and '"went_back": true' in out, out[-700:])
    m.shot('after-going-back')
    ctx.result['passed'] = True


def phase_autoback(ctx):
    """Going back by itself: install a test version whose writing app exits at once (--bundle, rc5.3),
    log in at the greeter on two boots, and expect the next boot to go back to the installed version
    without being asked. --base-disk is a disk with WrOS rc5.x installed."""
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout, user = st['layout'], st['username']
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk, overlay, base_format='qcow2')
    stick = vm.make_update_media(Path(ctx.args.bundle), ctx.dir / 'stick.img', 'ext4', 48)
    m = boot_installed(ctx, st, 'autoback', disk=overlay, fmt='qcow2', reboot=True,
                       extra=lambda machine: machine.usb_stick(stick, ident='usb0', serial='RCSTICK0', readonly=True))

    def login(after_reset=None):
        unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=after_reset)
        g = Guest(ctx, m, root_via_sudo=False)
        g.shell.login(user, secrets_['login'], 600)
        g.become_root(secrets_['login'])
        g.shell.put('/root/goback.py', GOBACK_GUEST.encode(), '0600')
        g.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
        return g

    def state(g, label):
        rc, out = g.run("dpkg-query -W -f='${Version}\\n' wros; echo PROBATION; cat /var/lib/wros-update/probation.json 2>&1; "
                        "echo; echo STATUS; python3 -I /root/goback.py status", 60, check=False)
        ctx.result['state_' + label] = out
        return out

    def broken_session(g, label):
        """A greeter login whose session ends at once; the login is recorded for this boot."""
        g.wait_vcs(1, r'Local account', 240, what='greeter-' + label)
        m.type_text(user + '\n', layout)
        g.wait_vcs(1, r'\[hidden\]', 30, what='greeter-password-' + label)
        m.type_text(secrets_['login'] + '\n', layout)
        # A login screen from before rc9 waits for Enter; rc9's opens the session at once (and this
        # broken one ends at once, so the screen may go straight back to the login).
        until = time.monotonic() + 15
        while time.monotonic() < until:
            if 'Open writing session' in g.screen(1):
                m.key('ret')
                break
            time.sleep(0.5)
        deadline, out = time.monotonic() + 90, ''
        while time.monotonic() < deadline:
            rc, out = g.run('cat /var/lib/wros-update/probation.json', 20, check=False)
            if '"started_boot": "' in out:
                break
            time.sleep(3)
        ctx.check('the login is recorded while the new version is on probation (%s)' % label,
                  '"started_boot": "' in out, out[-300:])
        # The session's stderr is not journalled: check the installed app is the broken test build,
        # and that the session ended back at the greeter.
        rc, out = g.run('grep -c "deliberately broken" /usr/bin/grimoire; journalctl -b --no-pager | grep -Ec "WROS_SESSION|WRITINGDECK_SESSION"',
                        30, check=False)
        ctx.check('the broken test app ran in a writing session (%s)' % label,
                  out.split()[:1] == ['1'] and out.split()[1:2] != ['0'], out)
        g.wait_vcs(1, r'Local account', 120, what='greeter-back-' + label)
        m.shot('greeter-after-broken-session-' + label)

    g = login()
    g.run('mkdir -p /mnt/b0; mount -o ro "$(readlink -f /dev/disk/by-id/usb-*RCSTICK0*)" /mnt/b0', 60)
    before = state(g, 'before').splitlines()[0].strip()
    rc, out = g.run('python3 -I /root/goback.py stage /mnt/b0', 300, check=False)
    ctx.check('the installed engine stages the broken test version', '"staged"' in out, out[-400:])
    resets = m.qmp.count('RESET')
    g.shell.serial.send('systemctl reboot\n')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets + 1)  # the update boot
    g = login(after_reset=resets + 2)
    out = state(g, 'broken-installed')
    ctx.check('0.1.0~rc5.3 is installed and on probation',
              out.splitlines()[0].strip() == '0.1.0~rc5.3' and '"failed_logins": 0' in out
              and '"probation": true' in out, out[-700:])

    broken_session(g, 'first')
    resets = m.qmp.count('RESET')
    g.shell.serial.send('systemctl reboot\n')
    g = login(after_reset=resets + 1)
    out = state(g, 'after-first')
    ctx.check('one failed start is counted at the next boot, and nothing goes back yet',
              out.splitlines()[0].strip() == '0.1.0~rc5.3' and '"failed_logins": 1' in out, out[-700:])

    broken_session(g, 'second')
    resets = m.qmp.count('RESET')
    g.shell.serial.send('systemctl reboot\n')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets + 1)  # reconcile stages the go-back
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets + 2)  # the offline go-back
    g = login(after_reset=resets + 3)
    out = state(g, 'after-going-back')
    ctx.check('the second failed start went back to %s by itself' % before,
              out.splitlines()[0].strip() == before and '"went_back_automatically": true' in out
              and 'No such file' in out.split('PROBATION', 1)[1].split('STATUS', 1)[0], out[-700:])
    rc, out = g.run('journalctl -b -1 -u wros-update-reconcile --no-pager | tail -5', 30, check=False)
    ctx.result['reconcile_journal'] = out
    greeter_login(ctx, g, m, user, secrets_['login'], layout, wrong_first=False)
    screen = g.wait_vcs(1, r'went back to the previous version', 90, what='went-back-notice')
    m.shot('went-back-notice')
    ctx.check('the writing app says it went back by itself', 'did not start twice' in screen, screen[-400:])
    ctx.result['passed'] = True


# The Ratatui start screen (docs/RATATUI_BOOT.md).
RATATUI_PROMPT = r'Type your disk password'
LOGO_TOP = ('+----------------+----------------+', '+--------+--------+')


def ratatui_unlock(ctx, m, secrets_, layout, *, wrong_first=False, after_reset=None, name='ratatui'):
    """Unlock at the Ratatui disk password screen; optionally a QWERTY-typed (wrong) password first."""
    if after_reset is not None:
        deadline = time.monotonic() + 120
        while m.qmp.count('RESET') < after_reset and time.monotonic() < deadline:
            time.sleep(0.5)
        time.sleep(3)
    text = m.wait_screen(RATATUI_PROMPT, 420, name=name + '-wait')
    m.shot(name + '-prompt')
    ctx.result[name + '_screen'] = text
    ctx.check(name + ': the Ratatui screen shows the logo', any(top in text for top in LOGO_TOP), text[-1500:])
    ctx.check(name + ': Debian\'s prompt is not on screen', 'Please unlock disk' not in text)
    time.sleep(2)
    if wrong_first:
        m.type_text(secrets_['luks'] + '\n', 'us' if layout != 'us' else layout)
        # The OCR can read the f of "of" as [.
        text = m.wait_screen(r"didn.t unlock the disk.*Try 2 o. 3", 120, name=name + '-wrong')
        m.shot(name + '-wrong')
        ctx.check(name + ': a wrong password is refused and counted', True, text[-600:])
        ctx.check(name + ': the typed password is not shown', secrets_['luks'] not in text)
        time.sleep(2)
    m.type_text(secrets_['luks'] + '\n', layout)
    ctx.step(name + '-typed', layout=layout)


def splash_record(g):
    """How this boot's progress screen went: whether it ran, and when it stopped against the login."""
    return g.run('systemctl show wros-boot-splash.service -p ConditionResult -p ExecMainStartTimestampMonotonic '
                 '-p InactiveEnterTimestampMonotonic -p Result; '
                 'systemctl show greetd.service -p ExecMainStartTimestampMonotonic | sed s/^/greetd/', 60)[1]


def splash_ran_before_login(record):
    values = dict(line.split('=', 1) for line in record.splitlines() if '=' in line)
    started = int(values.get('ExecMainStartTimestampMonotonic', '0') or 0)
    stopped = int(values.get('InactiveEnterTimestampMonotonic', '0') or 0)
    login = int(values.get('greetdExecMainStartTimestampMonotonic', '0') or 0)
    return started > 0 and 0 < stopped <= login and values.get('Result') == 'success'


def signed_in_again(ctx, m, st, secrets_):
    """After an unlock: the serial root shell, and the login screen on the console."""
    g = Guest(ctx, m, root_via_sudo=False)
    g.shell.login(st['username'], secrets_['login'], 600)
    g.become_root(secrets_['login'])
    g.shell.put('/tmp/rcg.py', GUEST.read_bytes(), '0644')
    g.wait_vcs(1, r'Local account', 240, what='greeter-after-unlock')
    return g


def reboot(ctx, g, m, what):
    resets = m.qmp.count('RESET')
    g.run('(sleep 1; systemctl reboot) >/dev/null 2>&1 & echo reboot-requested')
    return wait_reset(ctx, m, resets, 180, what)


def boot_image(g):
    """What the current boot image holds of the start screen."""
    return g.run('lsinitramfs /boot/initrd.img-$(uname -r) | grep -E "wros-boot$|cryptsetup/askpass" | sort', 120,
                 check=False)[1]


def console_has_green(path):
    from PIL import Image
    with Image.open(path) as picture:
        return any(g > r + 40 and g > b + 40 for r, g, b in picture.convert('RGB').getdata())


def native_pixels(path):
    """Anti-aliased font edges distinguish pixels from the 16-colour console."""
    from PIL import Image
    with Image.open(path) as picture:
        return len(picture.convert('RGB').getcolors(maxcolors=1 << 24)) > 64


def wait_boot_prepared(ctx, g, *, enabled):
    deadline = time.monotonic() + 900
    while time.monotonic() < deadline:
        ctx.guard()
        status = json.loads(g.run('/usr/libexec/wros-boot-settings status')[1])
        if status['enabled'] == enabled and not status['pending']:
            ctx.check('the background boot-image rebuild completed', not status['failed'], status)
            return
        time.sleep(2)
    raise Failure('the background start-screen rebuild did not finish')


def phase_bootscreen(ctx):
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    layout = st['layout']
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk, overlay, base_format='qcow2')
    m = boot_installed(ctx, st, 'bootscreen', disk=overlay, fmt='qcow2', reboot=True)

    # 1. The owner chose graphical defaults for every stage of startup.
    g = session_ready(ctx, m, st, secrets_, layout, greeter=False)
    image = boot_image(g)
    ctx.check('default: the boot image has native graphics and keeps Debian behind it',
              'cryptsetup/askpass' in image and 'wros-boot' in image and 'askpass.debian' in image, image)
    record = splash_record(g)
    ctx.check('default: the progress screen ran before login', splash_ran_before_login(record), record)
    ctx.check('wros-boot is installed', g.run('test -x /usr/lib/wros/wros-boot && echo yes')[1].strip() == 'yes')

    # 2. Accept a theme and choose the start screen through the installed app and polkit.
    greeter_login(ctx, g, m, st['username'], secrets_['login'], layout, wrong_first=False)
    grim_editor(g)
    m.key('f9')
    g.wait_vcs(1, r'THEME', 30, what='boot-theme-picker')
    for _ in range(20):
        if '● Custom' in g.screen(1):
            break
        m.key('down')
    else:
        raise Failure('the theme picker never selected Custom')
    m.key('ret')
    g.wait_vcs(1, r'CUSTOM THEME', 30, what='custom-boot-theme')
    m.type_text('00ff00', layout)
    m.key('esc')
    grim_open_settings(g, layout)
    grim_to_panel(g, r'^Display')
    g.wait_vcs(1, r'Start screen: WrOS', 30, what='start-screen-on')
    wait_boot_prepared(ctx, g, enabled=True)
    theme = g.run('stat -c "%a %U" /etc/wros/boot-theme.toml; cat /etc/wros/boot-theme.toml')[1]
    ctx.check('Settings kept a root-owned copy of the active custom green theme',
              '644 root' in theme and 'name = "Custom"' in theme and 'accent = "#00ff00"' in theme, theme)
    rc, extracted = g.run('mkdir /tmp/rc-boot-image && unmkinitramfs /boot/initrd.img-$(uname -r) /tmp/rc-boot-image '
                          '&& find /tmp/rc-boot-image -path "*/etc/wros/boot-theme.toml" '
                          '-exec cmp -s /etc/wros/boot-theme.toml {} \\; -print', 180)
    ctx.check('the exact Settings theme reached the rebuilt boot image',
              'etc/wros/boot-theme.toml' in extracted, extracted)
    image = boot_image(g)
    ctx.result['boot_image_on'] = image
    ctx.check('switch on: the boot image has wros-boot, the wrapper and Debian\'s prompt',
              'usr/lib/wros/wros-boot' in image and 'askpass.debian' in image, image)

    # 3. The Ratatui screen: a wrong password, then the right one, then the login screen.
    resets = reboot(ctx, g, m, 'reboot-ratatui')
    ratatui_unlock(ctx, m, secrets_, layout, wrong_first=True, after_reset=resets, name='ratatui')
    ctx.check('the custom accent reaches the real unlock console in green',
              console_has_green(ctx.dir / 'ratatui-prompt.png'))
    ctx.check('disk unlock is native pixel output', native_pixels(ctx.dir / 'ratatui-prompt.png'))
    try:
        text = m.wait_screen(r'Starting WrOS', 8, interval=0.5, name='progress')
        m.shot('progress-screen')
        ctx.result['progress_screen'] = text
    except TimeoutError:
        ctx.result['progress_screen'] = 'not caught on screen (the start may have been too quick)'
    g = signed_in_again(ctx, m, st, secrets_)
    record = splash_record(g)
    ctx.result['splash_record'] = record
    ctx.check('the progress screen ran and stopped before the login screen started',
              splash_ran_before_login(record), record)
    m.shot('login-after-ratatui')
    ctx.check('the same green accent reaches the login console', console_has_green(ctx.dir / 'login-after-ratatui.png'))
    ctx.check('login is native pixel output', native_pixels(ctx.dir / 'login-after-ratatui.png'))
    ctx.check('the Ratatui unlock opens the disk and reaches the login screen', True)
    login = g.screen(1)
    ctx.check('the shared logo sits above the complete login form',
              any(top in login and login.index(top) < login.index('Local account') for top in LOGO_TOP)
              and 'Password' in login, login)

    greeter_login(ctx, g, m, st['username'], secrets_['login'], layout, wrong_first=False)
    m.shot('native-writing')
    ctx.check('the writing session defaults to native pixels',
              g.probe('vcs', '1')['graphics'] and native_pixels(ctx.dir / 'native-writing.png'))
    m.key('ctrl', 'alt', 'f2')
    time.sleep(2)
    ctx.check('native writing releases the display for another VT', g.run('fgconsole')[1].strip() == '2')
    m.key('ctrl', 'alt', 'f1')
    time.sleep(2)
    ctx.check('native writing redraws after a VT switch', g.run('fgconsole')[1].strip() == '1'
              and native_pixels(m.shot('native-after-vt')))
    feature_console_lock(ctx, g, st, secrets_, layout, signed_in=True)

    # 4. Esc: Debian's text prompt, which unlocks.
    resets = reboot(ctx, g, m, 'reboot-esc')
    m.wait_screen(RATATUI_PROMPT, 420, name='esc-wait')
    time.sleep(2)
    m.key('esc')
    unlock(ctx, m, secrets_, layout, wrong_first=False)
    shutil.copyfile(ctx.dir / 'unlock-prompt.png', ctx.dir / 'esc-text-prompt.png')
    m.shot('after-esc-unlock')
    g = signed_in_again(ctx, m, st, secrets_)
    ctx.check('Esc goes to Debian\'s prompt, which unlocks', True)

    # The ordinary text-start menu entry always keeps Debian's prompt behind the switch.
    g.run('grub-reboot wros-text-start', 30)
    resets = reboot(ctx, g, m, 'reboot-text-start')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets)
    shutil.copyfile(ctx.dir / 'unlock-prompt.png', ctx.dir / 'text-start-prompt.png')
    m.shot('text-start-unlock')
    g = signed_in_again(ctx, m, st, secrets_)
    cmdline = g.run('cat /proc/cmdline')[1].split()
    ctx.check('WrOS (text start) uses Debian\'s prompt and the ordinary login',
              'wros.unlock=text' in cmdline and not any(word.startswith('systemd.unit=') for word in cmdline), cmdline)
    ctx.check('text start skips the progress screen', 'ConditionResult=no' in splash_record(g))

    # 5. Kill the renderer after it has switched to pixels, bypassing every Drop
    # guard. Only the separate recovery executable can restore Debian's prompt.
    crash_hook = r'''#!/bin/sh
case "$1" in prereqs) echo wros-boot; exit 0;; esac
cp "$DESTDIR/usr/lib/wros/wros-boot" "$DESTDIR/usr/lib/wros/wros-boot.real"
cat > "$DESTDIR/usr/lib/wros/wros-boot" <<'CRASH'
#!/bin/sh
/usr/lib/wros/wros-boot.real "$@" &
renderer=$!
sleep 20
kill -KILL "$renderer"
wait "$renderer"
[ "$?" = 137 ] && echo killed > /run/wros-native-crash-fired
exit 20
CRASH
chmod 0755 "$DESTDIR/usr/lib/wros/wros-boot"
'''
    g.shell.put('/etc/initramfs-tools/hooks/zz-rc-native-crash', crash_hook, '0755')
    g.run('update-initramfs -u', 900)
    resets = reboot(ctx, g, m, 'reboot-native-crash')
    m.wait_screen(RATATUI_PROMPT, 420, name='native-before-crash')
    ctx.check('the forced crash happens after native pixels appeared', native_pixels(m.shot('native-before-crash')))
    m.wait_screen(r'Please unlock disk', 90, name='native-crash-debian')
    unlock(ctx, m, secrets_, layout, wrong_first=False)
    shutil.copyfile(ctx.dir / 'unlock-prompt.png', ctx.dir / 'native-crash-text-prompt.png')
    g = signed_in_again(ctx, m, st, secrets_)
    ctx.check('SIGKILL of the graphical unlock still reaches Debian and opens the disk',
              g.run('cat /run/wros-native-crash-fired', 30)[1].strip() == 'killed')
    g.run('rm -f /etc/initramfs-tools/hooks/zz-rc-native-crash', 30)

    # A start screen that cannot draw at all also falls back.
    g.run('cp -p /usr/lib/wros/wros-boot /root/wros-boot.good && cp /usr/bin/false /usr/lib/wros/wros-boot '
          '&& update-initramfs -u', 900)
    resets = reboot(ctx, g, m, 'reboot-failing')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets)
    shutil.copyfile(ctx.dir / 'unlock-prompt.png', ctx.dir / 'failing-start-text-prompt.png')
    m.shot('after-failing-start-screen')
    g = signed_in_again(ctx, m, st, secrets_)
    ctx.check('a failing start screen falls back to Debian\'s prompt, which unlocks', True)
    g.run('cp -p /root/wros-boot.good /usr/lib/wros/wros-boot', 60)

    # 6. Switch off again: the boot image is Debian's.
    greeter_login(ctx, g, m, st['username'], secrets_['login'], layout, wrong_first=False)
    grim_open_settings(g, layout)
    grim_to_panel(g, r'^Display')
    g.wait_vcs(1, r'Start screen: WrOS', 30, what='start-screen-off-choice')
    g.choose(r'^Start screen: WrOS')
    m.key('ret')
    g.wait_vcs(1, r'Start screen: plain text', 30, what='start-screen-off')
    wait_boot_prepared(ctx, g, enabled=False)
    image = boot_image(g)
    ctx.check('switch off again: the boot image has no start screen',
              'cryptsetup/askpass' in image and 'wros-boot' not in image and 'askpass.debian' not in image, image)
    resets = reboot(ctx, g, m, 'reboot-off')
    unlock(ctx, m, secrets_, layout, wrong_first=False, after_reset=resets)
    shutil.copyfile(ctx.dir / 'unlock-prompt.png', ctx.dir / 'switch-off-text-prompt.png')
    signed_in_again(ctx, m, st, secrets_)
    ctx.check('switch off: Debian\'s prompt unlocks as before', True)
    ctx.result['passed'] = True


PHASES = {'firmware': phase_firmware, 'live': phase_live, 'install': phase_install,
          'install-failure': phase_install, 'installed-boot': phase_installed_boot,
          'update': phase_update, 'rescue': phase_rescue, 'peek': phase_peek, 'audit': phase_audit,
          'rc5': phase_rc5, 'lock': phase_lock, 'goback': phase_goback, 'autoback': phase_autoback,
          'bootscreen': phase_bootscreen}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('phase', choices=sorted(PHASES))
    parser.add_argument('--iso')
    parser.add_argument('--receipt')
    parser.add_argument('--install-run')
    parser.add_argument('--bundle')
    parser.add_argument('--bundle2', help='goback: the second test bundle (rc5.2)')
    parser.add_argument('--expect', default='0.1.0~rc5', help='rc5: the version the base disk should run')
    parser.add_argument('--secboot', action='store_true')
    parser.add_argument('--layout', default='dvorak', choices=['dvorak', 'colemak', 'colemak_dh'])
    parser.add_argument('--kill', choices=['backend', 'writer'])
    parser.add_argument('--kill-when', default='mapper', choices=['mapper', 'copying', 'grub'], help='failure injection point: first mapper, while copying the system, or during the GRUB install')
    parser.add_argument('--variant', default='ok', choices=['ok', 'cancel', 'tamper', 'powercut', 'badstage', 'reviewpath'])
    parser.add_argument('--base-disk')
    parser.add_argument('--from-version', default='0.1.0~rc1', help='update: the expected installed version')
    parser.add_argument('--check-migration', action='store_true', help='update: compare writer settings, groups and keyboard across the rename')
    parser.add_argument('--lock-fixture', help='lock: install a disposable local test package from this directory')
    parser.add_argument('--test-suspend', action='store_true', help='lock: require an actual QEMU suspend and wake')
    parser.add_argument('--media-fs', default='ext4', choices=['ext4', 'vfat'])
    parser.add_argument('--cut-delay', default='2')
    parser.add_argument('--hint-wait', type=int, default=900)
    parser.add_argument('--apply-wait', type=int, default=1500)
    parser.add_argument('--cut-at', default='9', help='powercut: seconds after the passphrase is typed')
    parser.add_argument('--unfinished', action='store_true', help='rescue: leave initramfs-tools trigger-pending first (repair path)')
    parser.add_argument('--hold', action='store_true', help='on a stage failure keep the VM up for read-only operator commands (cmd.txt)')
    parser.add_argument('--diagnose', action='store_true', help='install phase: collect installer logs and state read-only before stopping')
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise SystemExit('run as the unprivileged builder')
    ctx = Ctx(args.phase + (('-' + args.kill) if args.kill else '') + (('-' + args.variant) if args.phase == 'update' else ''), args)
    error = None
    try:
        PHASES[args.phase](ctx)
    except Failure as failure:
        error = 'Failure: %s' % failure
        ctx.log(error)
    except Exception:
        error = traceback.format_exc()
        ctx.log(error)
    finally:
        for machine in ctx.machines:
            try:
                if machine.is_running():
                    machine.shot('failure-final' if error else 'final')
            except Exception:
                pass
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
