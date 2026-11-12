"""The brightness and volume keys (#80) in a disposable installed VM.

Boots a qcow2 overlay of an installed disk (never the disk itself) with an AC97 sound card,
installs Debian's triggerhappy and the WrOS key files from a read-only WROS-TRIAL image laid
out at their packaged paths, then presses the keyboard's volume keys through QEMU at the login
screen and in the writing session. The volume must move one step per press, Mute must toggle,
the handler must run as the unprivileged wros-keys account, and the app's marker must change.
A VM has no backlight, so brightness is covered by the unit tests and the hardware checklist.
"""
import argparse
import json
from pathlib import Path
import time
import traceback
import rc_vm_phases as phases

LOGIN_READY = phases.GREETER_READY


def sound(g):
    _, out = g.run('/usr/libexec/wros-sound status')
    return json.loads(out.strip().splitlines()[-1])


def marker(g):
    _, out = g.run('stat -c %Y.%y /run/wros-keys/changed 2>/dev/null || echo none', check=False)
    return out.strip()


def press(g, m, qcode, before, deadline=8.0):
    """Press a key and wait for the volume or mute to change; the first press after boot is slow."""
    m.key(qcode)
    end = time.monotonic() + deadline
    while True:
        time.sleep(0.5)
        now = sound(g)
        if now != before or time.monotonic() > end:
            return now


def volume_keys(ctx, g, m, where):
    g.run('/usr/libexec/wros-sound set 50 >/dev/null')
    start = sound(g)
    before = marker(g)
    up = press(g, m, 'volumeup', start)
    ctx.check(where + ': Volume up raises the volume one step',
              up['volume_percent'] - start['volume_percent'] in range(4, 7), (start, up))
    ctx.check(where + ': the app is told about the change', marker(g) != before, before)
    down = press(g, m, 'volumedown', up)
    ctx.check(where + ': Volume down lowers it one step',
              up['volume_percent'] - down['volume_percent'] in range(4, 7), (up, down))
    muted = press(g, m, 'audiomute', down)
    ctx.check(where + ': Mute silences', muted['muted'] and muted['volume_percent'] == down['volume_percent'],
              muted)
    back = press(g, m, 'audiomute', muted)
    ctx.check(where + ': Mute again brings the sound back', not back['muted'], back)


def trial(ctx):
    a = ctx.args
    state = phases.load_state(a.install_run)
    credentials = json.loads(Path(state['secrets']).read_text())
    disk = ctx.dir / 'disk.qcow2'
    phases.vm.create_overlay(a.base_disk, disk, base_format='qcow2')

    def attach(machine):
        machine.usb_stick(a.fixture, readonly=True, ident='trial')
        machine.args += ['-audiodev', 'none,id=snd0', '-device', 'AC97,audiodev=snd0']

    m = phases.boot_installed(ctx, state, 'media-keys', disk=disk, fmt='qcow2', extra=attach)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False)
    ctx.guest = g
    g.run('mkdir -p /mnt/trial && mount -o ro,nodev,nosuid LABEL=WROS-TRIAL /mnt/trial')
    _, out = g.run('dpkg -i /mnt/trial/debs/*.deb 2>&1 | tail -3; echo rc=$?', 300, check=False)
    ctx.log(out)
    g.run('cp -a /mnt/trial/root/. / && umount /mnt/trial')
    g.run('systemd-sysusers wros-keys.conf && systemd-tmpfiles --create wros-keys.conf && '
          'systemctl daemon-reload')
    # As at boot: the socket first, then the daemon (dpkg started the daemon alone, holding the path).
    g.run('systemctl stop triggerhappy.service triggerhappy.socket; rm -f /run/thd.socket; '
          'systemctl start triggerhappy.socket && systemctl start triggerhappy.service')
    time.sleep(2)

    _, active = g.run('systemctl is-active triggerhappy.service', check=False)
    ctx.check('triggerhappy is running', active.strip() == 'active', active)
    _, owner = g.run('ps -o user= -C thd', check=False)
    ctx.check('triggerhappy runs as wros-keys, not root', owner.split() == ['wros-keys'], owner)
    _, groups = g.run('id -nG wros-keys', check=False)
    ctx.check('wros-keys may use only the backlight and the sound card',
              sorted(groups.split()) == ['audio', 'video', 'wros-keys'], groups)
    _, switch = g.run('cat /sys/module/video/parameters/brightness_switch_enabled 2>/dev/null || echo absent',
                      check=False)
    ctx.result['brightness_switch_enabled'] = switch.strip()
    ctx.check('the kernel leaves brightness keys to WrOS', switch.strip() in ('N', 'absent'), switch)
    first = sound(g)
    ctx.check('the VM has a sound card with a volume control', first['available'], first)
    g.run('/usr/libexec/wros-sound set 50 >/dev/null')
    _, out = g.run('runuser -u wros-keys -- /usr/libexec/wros-media-key volume-up; echo rc=$?', check=False)
    by_hand = sound(g)
    ctx.check('the handler changes the volume as wros-keys', 'rc=0' in out and by_hand['volume_percent'] > 50,
              (out, by_hand))

    g.wait_vcs(1, LOGIN_READY, 120, what='login-before-keys')
    greeter = g.run('pgrep -xo wros-greeter')[1].strip()
    volume_keys(ctx, g, m, 'login screen')
    g.wait_vcs(1, LOGIN_READY, 30, what='login-after-keys')
    after = g.run('pgrep -xo wros-greeter', check=False)[1].strip()
    ctx.check('the login screen ignores the keys and keeps running', after == greeter,
              {'before': greeter, 'after': after})

    phases.greeter_login(ctx, g, m, state['username'], credentials['login'], state['layout'], wrong_first=False)
    time.sleep(5)
    volume_keys(ctx, g, m, 'writing session')
    ctx.check('the guest powered off cleanly', phases.poweroff(g) == 0)
    ctx.result['passed'] = True


def main():
    import os
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', type=Path, required=True)
    parser.add_argument('--base-disk', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True,
                        help='read-only WROS-TRIAL image: debs/ and root/ laid out at packaged paths')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    ctx = phases.Ctx('media-keys', args)
    error = None
    try:
        trial(ctx)
    except Exception:
        error = traceback.format_exc()
        ctx.log(error)
        if hasattr(ctx, 'guest') and ctx.guest.m.is_running():
            try:
                ctx.guest.m.shot('failure-final')
                _, diagnostic = ctx.guest.run('systemctl status triggerhappy --no-pager; ps -eo pid,user,comm; '
                                              'ls -l /run/wros-keys; journalctl -b --no-pager -u triggerhappy -n40',
                                              check=False)
                (ctx.dir / 'failure-state.log').write_text(diagnostic)
            except Exception as failure:
                ctx.log('Failure-state capture unavailable: ' + type(failure).__name__)
    finally:
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
