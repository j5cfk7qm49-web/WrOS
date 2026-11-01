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
        return self.shell.run(command, timeout, check)

    def probe(self, *args, timeout=90):
        rc, out = self.shell.run('%spython3 -I /tmp/rcg.py %s' % (self.prefix, ' '.join(args)), timeout, check=True)
        for line in out.splitlines():
            if line.startswith('RCJSON'):
                return json.loads(line[6:])
        raise Failure('probe %s produced no RCJSON: %s' % (args, out[-300:]))

    def vcs(self, n):
        return self.probe('vcs', str(n)).get('lines', [])

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
        seen = None
        for _ in range(limit):
            current = self.selected_row(n)
            if current and regex.search(current):
                return current
            if current == seen:
                self.m.key('down')       # did not move: keep pushing to the end
            seen = current
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
    g.wait_vcs(1, r'Writing.*Interface.*System.*Accounts', 30, what='settings-root')


def grim_to_panel(g, panel_regex):
    g.choose(r'^System$')
    g.m.key('ret')
    g.wait_vcs(1, r'Wi-Fi and networking', 30, what='system-panels')
    g.choose(panel_regex)
    g.m.key('ret')
    time.sleep(1)


def grim_new_scene(g, name, text, layout='us'):
    """Dashboard (or editor) -> new scene named `name` -> type text -> save. Returns the screen."""
    m = g.m
    grim_editor(g)
    m.key('ctrl', 'k')
    time.sleep(0.8)
    m.type_text('new scene', layout)
    g.wait_vcs(1, r'\u25b8 New scene|\u2192 New scene|New scene', 20, what='palette-new-scene')
    m.key('ret')
    g.wait_vcs(1, r'NEW SCENE', 20, what='new-scene-dialog')
    m.type_text(name, layout)
    m.key('ret')
    g.wait_vcs(1, r'Start writing', 20, what='new-scene-editor')
    m.type_text(text, layout)
    m.key('ctrl', 's')
    time.sleep(2)
    return g.screen(1)


def grim_logout_login(ctx, g, user, password, layout):
    g.m.key('ctrl', 'q')
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
        m.type_text(action[1], layout_in_effect_safe(g))
        m.key('ret')
    elif name == 'secret':
        ctx.check('password field is masked', form.selected == '[hidden]', form.raw[-200:])
        effective = layout_in_effect_safe(g)
        ctx.result.setdefault('secret_entries', []).append({'rule': rule.name, 'layout_in_effect': effective})
        m.type_text(secrets_[action[1]], effective)
        shown = '\n'.join(g.vcs(7))
        ctx.check('typed secret never rendered', secrets_[action[1]] not in shown)
        m.key('ret')
    else:
        raise Failure('unknown action ' + str(action))


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
        if wrote and (mapped or snap['target_mounts']):
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


def finish_failure_injection(ctx, g, target, kill):
    m = g.m
    deadline = time.monotonic() + 300
    outcome, last = None, None
    while time.monotonic() < deadline:
        try:
            snap = g.snap()
        except Exception as error:
            ctx.log('snapshot failed: %s' % error)
            time.sleep(3)
            continue
        last = snap
        outcome = snap['run_files'].get('outcome.json', outcome)
        if outcome and not [x for x in snap['mappers'] if x != 'control']:
            break
        time.sleep(3)
    ctx.result['outcome'] = outcome
    ctx.result['final_snapshot'] = {k: last[k] for k in ('active_vt', 'procs', 'mappers', 'target_mounts', 'swaps', 'installer_unit')} if last else None
    m.shot('after-failure')
    if kill == 'backend':
        ctx.check('outcome is failed with verified cleanup', (outcome or {}).get('outcome') == 'failed' and (outcome or {}).get('host_cleanup_verified') is True, outcome)
        ctx.check('VT1 restored with a fresh writer', last['active_vt'] == 'tty1' and any(p['comm'] == 'wros-input' for p in last['procs']), last['active_vt'])
        screen = g.screen(1)
        ctx.check('Grimoire says installation did not complete', 'did not complete' in screen, screen[-400:])
    else:
        ctx.check('outcome is interrupted', (outcome or {}).get('outcome') == 'interrupted', outcome)
        ctx.check('VT7 stays active (no auto-return after writer death)', last['active_vt'] == 'tty7', last['active_vt'])
        ctx.check('no new writer on tty1', not any(p['comm'] in ('grimoire', 'wros-input') for p in last['procs']), last['procs'])
    ctx.check('no target mappers, mounts or swap left', not [x for x in last['mappers'] if x != 'control'] and not last['target_mounts'] and not last['swaps'])
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
    text = m.wait_screen(r'Please unlock disk', 420, name='unlock-wait')
    m.shot('unlock-prompt')
    ctx.step('unlock-prompt-seen', screen=text[-300:])
    ctx.check('unlock prompt is on tty1 only (no QA console argument)', True)
    if wrong_first:
        # the real passphrase typed as if the keyboard were US QWERTY: with a correct non-US map it must fail
        m.type_text(secrets_['luks'] + '\n', 'us' if layout != 'us' else layout)
        try:
            text = m.wait_screen(r'No key available|incorrect|try again', 120, name='unlock-wrong')
        except TimeoutError:
            m.shot('unlock-qwerty-accepted-or-unclear')
            raise Failure('QWERTY-typed passphrase was not rejected at the LUKS prompt (initramfs layout disagrees with the installer layout?)')
        m.shot('unlock-rejected')
        ctx.result['unlock_wrong_screen'] = text[-400:]
        ctx.check('wrong passphrase rejected', True)
        m.wait_screen(r'Please unlock disk', 120, name='unlock-again')
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
    screen = g.wait_vcs(1, r'Authenticated\.|Open writing session', 90, what='greeter-authenticated')
    ctx.check('correct greeter password accepted', 'Authenticated' in screen or 'Open writing session' in screen)
    m.key('ret')
    g.wait_vcs(1, r'Resume writing|WrOS', 90, what='grimoire-home')


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
    g.wait_vcs(2, r'\$ ?$', 30, what='tty2-shell')
    m.type_text('sudo -p PWPROMPT: id -u\n', layout)
    g.wait_vcs(2, r'PWPROMPT:', 30, what='tty2-sudo-prompt')
    m.type_text(secrets_['login'] + '\n', layout)
    screen2 = g.wait_vcs(2, r'\n0\s*\n|^0$', 30, what='tty2-sudo-result')
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


NOTHING_STAGED = 'test -e /system-update && echo MARKER; ls /var/lib/wros-update/staged 2>/dev/null | wc -l'


def stage_update(ctx, g, layout):
    """Find media, Review, Continue, Confirm (Cancel proven first), Stage."""
    m = g.m
    updates_panel(ctx, g, layout)
    review = find_and_review(ctx, g)
    m.shot('update-review')
    ctx.result['review_screen'] = review
    ctx.check('review shows wros 0.1.0~rc1 -> 0.1.0~rc2',
              re.search(r'wros.*0\.1\.0~rc1.*0\.1\.0~rc2', review, re.S) is not None, review[-900:])
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
    rc, out = g.run('readlink /system-update; ls /var/lib/wros-update/staged', 20, check=False)
    ctx.result['staged_listing'] = out
    ctx.check('update is staged (marker and retained archive)', '/var/lib/wros-update/staged' in out and '.deb' in out, out)
    return staged


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
    vm.create_overlay(ctx.args.base_disk or st['disk'], overlay)
    media = vm.make_update_media(bundle, ctx.dir / 'update-media.img', ctx.args.media_fs, 48)
    debs = sorted(p.relative_to(bundle).as_posix() for p in bundle.rglob('*.deb'))
    ctx.check('bundle carries exactly one wros deb', len(debs) == 1 and debs[0].split('/')[-1].startswith('wros_'), debs)
    ctx.result['bundle_deb'] = debs[0]
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
    ctx.check('starting version is 0.1.0~rc1', '0.1.0~rc1' in out, out)

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

    stage_update(ctx, g, layout)

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
        ctx.check('nothing applied; still 0.1.0~rc1', '0.1.0~rc1' in out and 'rc2' not in out, out)
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
        regex = re.compile(r'wros|update|dpkg|Unpacking|Setting up|Preparing|apply', re.I)
        deadline = time.monotonic() + 600
        seen = None
        while time.monotonic() < deadline:
            text = m.screen_text('apply-watch')
            if regex.search(text) and not re.search(r'Please unlock', text):
                seen = text
                break
            time.sleep(1)
        ctx.check('apply phase observed on the console before the power cut', seen is not None, m.screen_text('apply-final')[-600:])
        ctx.result['apply_screen'] = seen
        time.sleep(float(ctx.args.cut_delay))
        m.shot('before-power-cut')
        m.kill()
        ctx.step('power-cut', resets=m.qmp.count('RESET'))
        m2 = boot_installed(ctx, st, 'after-cut', disk=overlay, fmt='qcow2', reboot=True)
        g2 = session_ready(ctx, m2, st, secrets_, layout)
        rc, out = g2.run('dpkg-query -W wros; cat /etc/writingdeck/release; cat /var/lib/wros-update/public/status.json; dpkg --audit; echo AUDIT_DONE', 60, check=False)
        ctx.result['after_cut_state'] = out
        time.sleep(90)
        ctx.check('no reboot loop after the power cut', m2.qmp.count('RESET') == 0)
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
    resets = wait_reset(ctx, m, resets, 1500, 'apply-then-reboot')
    ctx.step('system-rebooted-itself-after-applying')
    g2 = session_ready(ctx, m, st, secrets_, layout, after_reset=resets)
    rc, out = g2.run('dpkg-query -W wros; cat /etc/writingdeck/release; cat /var/lib/wros-update/public/status.json; test -e /system-update && echo MARKER', 30, check=False)
    ctx.result['after_update_state'] = out
    ctx.check('wros is 0.1.0~rc2', re.search(r'^wros\s+0\.1\.0~rc2', out, re.M) is not None, out)
    ctx.check('release file carries 0.1.0~rc2', len(re.findall(r'0\.1\.0~rc2', out)) >= 2, out)
    ctx.check('update marker consumed', 'MARKER' not in out, out)
    updates_panel(ctx, g2, layout)
    screen = g2.screen(1)
    m.shot('after-update-updates-panel')
    ctx.result['after_update_updates_screen'] = screen
    ctx.check('Settings shows the update completed', re.search(r'Update state: completed', screen) is not None, screen[-600:])
    ctx.check('no further reboot', m.qmp.count('RESET') == resets)
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
        'passwd-system-accounts': "awk -F: '$3>=100 || $1==\"root\"' /mnt/rc/etc/passwd; echo; grep -n greetd /mnt/rc/etc/passwd /mnt/rc/etc/group",
        'greetd-config': 'cat /mnt/rc/etc/greetd/config.toml; ls -la /mnt/rc/etc/greetd',
        'keyboard': 'cat /mnt/rc/etc/default/keyboard; cat /mnt/rc/etc/initramfs-tools/conf.d/wros-keymap 2>&1; ls -la /mnt/rc/etc/writingdeck',
        'target-markers': 'ls -la /mnt/rc/wros-target-* /mnt/rc/etc/writingdeck 2>&1; cat /mnt/rc/etc/crypttab; cat /mnt/rc/etc/fstab',
        'installer-logs': 'ls -la /mnt/rc/var/log/installer 2>&1; grep -a -n "wros-keymaps" /mnt/rc/var/log/installer/syslog 2>&1 | head',
        'release': 'cat /mnt/rc/etc/writingdeck/release 2>&1; ls /mnt/rc/var/lib/dpkg/info | grep -i -E "^(wros|greetd)" | head',
        'finish-hooks-left-in-target': 'ls -la /mnt/rc/usr/lib/finish-install.d 2>&1 | head',
    }
    for label, command in commands.items():
        rc, text = g.run("sudo -n sh -c '%s'" % command.replace("'", "'\''"), 60, check=False)
        (ctx.dir / ('peek-%s.txt' % label)).write_text(text)
        ctx.step('peek-' + label, rc=rc, head=text[:300])
    g.run("sudo -n sh -c 'umount /mnt/rc; vgchange -an >/dev/null 2>&1; cryptsetup close rcpeek'", 60, check=False)
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    try:
        m.wait_exit(120)
    except TimeoutError:
        m.kill()
    ctx.result['passed'] = True


def phase_rescue(ctx):
    st = load_state(ctx.args.install_run)
    secrets_ = json.loads(Path(st['secrets']).read_text())
    overlay = ctx.dir / 'disk.qcow2'
    vm.create_overlay(ctx.args.base_disk or st['disk'], overlay)
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
    ctx.check('wros-rescue is present on the live image', out.strip().startswith('/'), out)
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
    ctx.result['rescue_output_tail'] = s.tail(4000)
    ctx.check('passphrase was requested by wros-rescue', sent)
    ctx.check('wros-rescue finished with exit 0', code == 0, ctx.result['rescue_output_tail'][-1500:])
    rc, state = g.run("sudo -n sh -c 'ls /dev/mapper; grep -c /target /proc/mounts'", 20, check=False)
    ctx.check('no mapping or target mount left open', 'wros-rescue-root' not in state, state)
    rc, header2 = g.run(head, 60)
    ctx.check('LUKS header unchanged', header == header2, (header, header2))
    ctx.check('no secret in serial transcript', not s.contains(secrets_['luks']))
    g.run('sudo -n /sbin/poweroff', 20, check=False)
    m.wait_exit(120)
    m2 = boot_installed(ctx, st, 'after-rescue', disk=overlay, fmt='qcow2', reboot=True)
    g2 = session_ready(ctx, m2, st, secrets_, st['layout'])
    rc, out = g2.run('dpkg-query -W wros; dpkg --audit; echo DONE', 30, check=False)
    ctx.result['after_rescue_state'] = out
    ctx.check('disk boots and logs in after rescue', 'wros' in out)
    ctx.result['passed'] = True


PHASES = {'firmware': phase_firmware, 'live': phase_live, 'install': phase_install,
          'install-failure': phase_install, 'installed-boot': phase_installed_boot,
          'update': phase_update, 'rescue': phase_rescue, 'peek': phase_peek}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('phase', choices=sorted(PHASES))
    parser.add_argument('--iso')
    parser.add_argument('--receipt')
    parser.add_argument('--install-run')
    parser.add_argument('--bundle')
    parser.add_argument('--secboot', action='store_true')
    parser.add_argument('--layout', default='dvorak', choices=['dvorak', 'colemak', 'colemak_dh'])
    parser.add_argument('--kill', choices=['backend', 'writer'])
    parser.add_argument('--variant', default='ok', choices=['ok', 'cancel', 'tamper', 'powercut', 'badstage'])
    parser.add_argument('--base-disk')
    parser.add_argument('--media-fs', default='ext4', choices=['ext4', 'vfat'])
    parser.add_argument('--cut-delay', default='2')
    parser.add_argument('--hint-wait', type=int, default=900)
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
