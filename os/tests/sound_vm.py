"""Sound in the writing session (#118) in a disposable installed VM.

Boots a qcow2 overlay of an installed disk (never the disk itself) with an AC97 sound card and a
NAT network added after boot, installs the sound packages the wros package now depends on from Debian, puts the
working tree's wros-sound in place, and signs in. Then, as the writer: PipeWire and WirePlumber
run in the session, eSpeak NG speaks through them, the visualizer's own pw-record command hears
what the session plays, and the Sound page's helper and the key handler move the volume PipeWire
reports.
"""
import argparse
import json
from pathlib import Path
import re
import time
import traceback
import rc_vm_phases as phases

PACKAGES = ('pipewire pipewire-bin pipewire-alsa pipewire-pulse wireplumber dbus-user-session '
            'playerctl espeak-ng')


def as_writer(g, user, command, timeout=60, check=True):
    """Run in the writer's session: their runtime directory and user bus."""
    wrapped = ("runuser -u {u} -- env XDG_RUNTIME_DIR=/run/user/$(id -u {u}) "
               "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u {u})/bus sh -c {c}").format(
        u=user, c=json.dumps(command))
    return g.run(wrapped, timeout, check=check)


def sink_volume(g, user):
    _, out = as_writer(g, user, 'wpctl get-volume @DEFAULT_AUDIO_SINK@', check=False)
    found = re.search(r'Volume: ([0-9.]+)', out)
    return float(found.group(1)) if found else None


def trial(ctx):
    a = ctx.args
    state = phases.load_state(a.install_run)
    credentials = json.loads(Path(state['secrets']).read_text())
    user = state['username']
    disk = ctx.dir / 'disk.qcow2'
    phases.vm.create_overlay(a.base_disk, disk, base_format='qcow2')

    def attach(machine):
        machine.usb_stick(a.fixture, readonly=True, ident='trial')
        machine.args += ['-audiodev', 'none,id=snd0', '-device', 'AC97,audiodev=snd0',
                         '-device', 'pcie-root-port,id=hotplug0,chassis=9,slot=9']

    m = phases.boot_installed(ctx, state, 'sound', disk=disk, fmt='qcow2', extra=attach, reboot=True)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], greeter=False)
    ctx.guest = g
    # The network joins after boot, so the firmware and start-up behave as on any boot.
    m.qmp.command('netdev_add', type='user', id='net0')
    m.qmp.command('device_add', driver='virtio-net-pci', netdev='net0', id='nic0', bus='hotplug0')
    # WrOS updates from signed bundles and keeps no Debian sources; this test brings its own.
    # printf turns each \\n into a line break inside the guest.
    sources = ('deb http://deb.debian.org/debian trixie main\\n'
               'deb http://deb.debian.org/debian trixie-updates main\\n'
               'deb http://security.debian.org/debian-security trixie-security main\\n')
    apt = '-o Dir::Etc::sourcelist=/tmp/sound.list -o Dir::Etc::sourceparts=-'
    _, out = g.run(f"printf '{sources}' > /tmp/sound.list; "
                   'for i in $(seq 1 30); do getent hosts deb.debian.org >/dev/null && break; sleep 2; done; '
                   f'apt-get {apt} update -qq 2>&1 | tail -3; '
                   f'DEBIAN_FRONTEND=noninteractive apt-get {apt} install -y --no-install-recommends {PACKAGES} '
                   '> /tmp/sound-apt.log 2>&1; echo rc=$?; tail -3 /tmp/sound-apt.log', 900, check=False)
    ctx.log(out)
    ctx.check('the sound packages install from Debian 13', 'rc=0' in out, out[-600:])
    g.run('mkdir -p /mnt/trial && mount -o ro,nodev,nosuid LABEL=WROS-TRIAL /mnt/trial && '
          'install -m755 /mnt/trial/wros-sound /usr/libexec/wros-sound && umount /mnt/trial')

    # The user bus and PipeWire start with the next session, as after an update's restart.
    resets = m.qmp.count('RESET')
    g.run('systemctl reboot', check=False)
    g = phases.session_ready(ctx, m, state, credentials, state['layout'], after_reset=resets + 1)
    ctx.guest = g
    time.sleep(8)
    _, units = as_writer(g, user, 'systemctl --user is-active pipewire wireplumber pipewire-pulse', check=False)
    ctx.check('PipeWire, WirePlumber and the Pulse server run in the writer session',
              units.split() == ['active', 'active', 'active'], units)
    _, status = as_writer(g, user, 'wpctl status', check=False)
    ctx.result['wpctl_status'] = status[-1500:]
    ctx.check('PipeWire found the sound card', sink_volume(g, user) is not None, status[-800:])

    # The visualizer's capture, exactly as grimoire runs it, while eSpeak NG speaks.
    _, heard = as_writer(
        g, user,
        "(sleep 1; espeak-ng 'The lighthouse keeper counted ships every night' ) & "
        "timeout 6 pw-record -P '{ stream.capture.sink = true }' --format s16 --rate 48000 --channels 2 "
        "/tmp/heard.wav; python3 -c \"import array,wave; w=wave.open('/tmp/heard.wav'); "
        "a=array.array('h', w.readframes(w.getnframes())); print('peak', max(map(abs, a), default=0))\"; wait",
        60, check=False)
    ctx.log('capture: ' + heard)
    peak = re.search(r'peak (\d+)', heard)
    ctx.check('eSpeak NG speaks and the visualizer capture hears it', peak is not None and int(peak.group(1)) > 500,
              heard[-400:])

    # The Sound page's helper (as the writer) and the key handler path both move PipeWire's volume.
    as_writer(g, user, '/usr/libexec/wros-sound set 30 >/dev/null')
    time.sleep(1)
    low = sink_volume(g, user)
    as_writer(g, user, '/usr/libexec/wros-sound set 80 >/dev/null')
    time.sleep(1)
    high = sink_volume(g, user)
    ctx.check('the Sound page moves the volume PipeWire reports', low is not None and high is not None and high > low,
              (low, high))
    _, reply = as_writer(g, user, '/usr/libexec/wros-sound status')
    ctx.check('the Sound page reads the card while PipeWire holds it',
              json.loads(reply.strip().splitlines()[-1])['available'], reply)
    ctx.check('the guest powered off cleanly', phases.poweroff(g) == 0)
    ctx.result['passed'] = True


def main():
    import os
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install-run', type=Path, required=True)
    parser.add_argument('--base-disk', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True, help='read-only WROS-TRIAL image with wros-sound')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the unprivileged builder')
    ctx = phases.Ctx('sound', args)
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
