"""Settings › Battery and power › Restart the writing app, in a disposable installed VM.

Boots a qcow2 overlay of an installed disk (never the disk itself), installs the working tree's
release grimoire from a read-only WROS-TRIAL image, signs in, and chooses the row the way a
writer does. The app must come back in place (same process, same login), with exactly one
keyboard reader and at most one colour process, the old reader gone, and typing must still
reach the book.
"""
import argparse
import json
from pathlib import Path
import time
import traceback
import rc_vm_phases as phases

TYPED = 'Typed after the writing app restarted'


def pids(g, name):
    _, out = g.run(f'pgrep -x {name} || true', check=False)
    return sorted(int(p) for p in out.split() if p.isdigit())


def trial(ctx):
    a = ctx.args
    state = phases.load_state(a.install_run)
    credentials = json.loads(Path(state['secrets']).read_text())
    user, layout = state['username'], state['layout']
    disk = ctx.dir / 'disk.qcow2'
    phases.vm.create_overlay(a.base_disk, disk, base_format='qcow2')

    def attach(machine):
        machine.usb_stick(a.fixture, readonly=True, ident='trial')

    m = phases.boot_installed(ctx, state, 'restart-app', disk=disk, fmt='qcow2', extra=attach)
    g = phases.session_ready(ctx, m, state, credentials, layout, greeter=False)
    ctx.guest = g
    _, out = g.run('mkdir -p /mnt/trial && mount -o ro,nodev,nosuid LABEL=WROS-TRIAL /mnt/trial && '
                   'install -m755 /mnt/trial/grimoire /usr/bin/grimoire && umount /mnt/trial && '
                   'sha256sum /usr/bin/grimoire')
    ctx.result['installed_grimoire'] = out.strip()
    phases.greeter_login(ctx, g, m, user, credentials['login'], layout, wrong_first=False)
    time.sleep(3)
    app, readers, colours = pids(g, 'grimoire'), pids(g, 'wros-input'), pids(g, 'wros-colours')
    ctx.result['before'] = {'grimoire': app, 'wros-input': readers, 'wros-colours': colours}
    ctx.check('the writing app and one keyboard reader run', len(app) == 1 and len(readers) == 1,
              ctx.result['before'])

    phases.grim_open_settings(g, layout)
    phases.grim_to_panel(g, r'^Battery and power(\s*›)?$')
    g.wait_vcs(1, r'Restart the writing app', 30, what='power-page')
    g.choose(r'^Restart the writing app')
    m.key('ret')
    time.sleep(4)
    g.wait_vcs(1, phases.WRITING_SCREEN, 90, what='after-restart')
    time.sleep(3)
    after = {'grimoire': pids(g, 'grimoire'), 'wros-input': pids(g, 'wros-input'),
             'wros-colours': pids(g, 'wros-colours')}
    ctx.result['after'] = after
    ctx.check('the same writing app process came back, so the login stayed', after['grimoire'] == app,
              (app, after))
    _, exe = g.run(f'readlink /proc/{app[0]}/exe', check=False)
    ctx.check('it runs the installed app again', exe.strip() == '/usr/bin/grimoire', exe)
    ctx.check('exactly one keyboard reader, and not the old one',
              len(after['wros-input']) == 1 and after['wros-input'] != readers, (readers, after))
    ctx.check('no second colour process', len(after['wros-colours']) <= 1, after)
    _, greeter = g.run('pgrep -x wros-greeter || echo none', check=False)
    ctx.check('the login screen did not come back', greeter.strip() == 'none', greeter)

    # Typing still reaches the book.
    phases.grim_new_scene(g, 'Restarted', TYPED, layout)
    _, found = g.run(f"grep -rlF '{TYPED}' /home/{user} 2>/dev/null | head -1", check=False)
    ctx.check('typing after the restart is saved in the book', bool(found.strip()), found)
    ctx.check('the guest powered off cleanly', phases.poweroff(g) == 0)
    ctx.result['passed'] = True


def main():
    import os
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', type=Path, required=True)
    parser.add_argument('--base-disk', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True, help='read-only WROS-TRIAL image with grimoire')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    ctx = phases.Ctx('restart-app', args)
    error = None
    try:
        trial(ctx)
    except Exception:
        error = traceback.format_exc()
        ctx.log(error)
        if hasattr(ctx, 'guest') and ctx.guest.m.is_running():
            try:
                ctx.guest.m.shot('failure-final')
            except Exception as failure:
                ctx.log('Failure-state capture unavailable: ' + type(failure).__name__)
    finally:
        ctx.finish(error)
    return 0 if ctx.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
