#!/usr/bin/env python3
"""Measure isolated graphical sessions. Never opens the writer's book or settings.

Headless cage measures software rendering, not the laptop's graphics chip. The
optional key test measures a virtual key to a changed screenshot, including the
capture overhead. Native DRM needs an active tty1 and separate hardware latency
measurement. No passwords, privileged helpers or persistent display choices.
"""
import argparse
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import select
import signal
import statistics
import struct
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
COLEMAK = dict(zip('qwertyuiopasdfghjkl;zxcvbnm',
                  (16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 30, 31, 32, 33, 34,
                   35, 36, 37, 38, 39, 44, 45, 46, 47, 48, 49, 50)))
COLEMAK = {char: COLEMAK[position] for position, char in
           zip('qwertyuiopasdfghjkl;zxcvbnm', 'qwfpgjluy;arstdhneiozxcvbkm')}


def environment(directory, headless, backend):
    home = directory / 'home'
    runtime = directory / 'run'
    home.mkdir(mode=0o700)
    runtime.mkdir(mode=0o700)
    env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / '.config'),
           'XDG_DATA_HOME': str(home / '.local/share'), 'XDG_STATE_HOME': str(home / '.local/state'),
           'XDG_RUNTIME_DIR': str(runtime), 'PATH': '/usr/local/bin:/usr/bin:/bin',
           'LANG': 'C.UTF-8', 'WROS_SESSION': backend, 'WROS_READY_FILE': str(directory / 'ready'),
           'XKB_DEFAULT_LAYOUT': 'us', 'XKB_DEFAULT_VARIANT': 'colemak',
           'XKB_DEFAULT_OPTIONS': 'compose:ralt'}
    if headless:
        env.update(WLR_BACKENDS='headless', WLR_RENDERER='pixman',
                   WLR_LIBINPUT_NO_DEVICES='1', LIBGL_ALWAYS_SOFTWARE='1')
    else:
        # libseat uses the existing authenticated session, never a new root session.
        for name in ('XDG_SESSION_ID', 'XDG_SEAT', 'XDG_VTNR', 'DBUS_SESSION_BUS_ADDRESS'):
            if name in os.environ:
                env[name] = os.environ[name]
    return env


def command(backend, binary, book):
    if backend == 'drm':
        return [str(binary), str(book)]
    client = [str(binary), str(book)]
    if backend == 'foot':
        client = ['foot', '--config=' + str(ROOT / 'os/config/foot.ini'), '--', *client]
    return ['cage', '-s', '--', *client]


def process_members(pid, proc=Path('/proc')):
    entries = []
    for path in proc.glob('[0-9]*/stat'):
        try:
            text = path.read_text()
            left, right = text.index('('), text.rindex(')')
            fields = text[right + 2:].split()
            rss = int(fields[21]) * os.sysconf('SC_PAGE_SIZE') // 1024
            entries.append((int(path.parent.name), int(fields[1]), int(fields[2]),
                            path, text[left + 1:right], rss))
        except (OSError, ValueError, IndexError):
            continue  # A short-lived child may have exited between the reads.
    selected = {pid}
    while True:
        expanded = selected | {p for p, parent, group, *_ in entries if group == pid or parent in selected}
        if expanded == selected:
            break
        selected = expanded
    return [entry for entry in entries if entry[0] in selected]


def process_memory(pid, proc=Path('/proc')):
    """RSS/PSS for this run's process tree, including foot's new PTY session."""
    processes = []
    for _, _, _, path, name, rss in process_members(pid, proc):
        pss = None
        try:
            for line in path.with_name('smaps_rollup').read_text().splitlines():
                if line.startswith('Pss:'):
                    pss = int(line.split()[1])
        except OSError:
            pass
        processes.append({'name': name, 'rss_kib': rss, 'pss_kib': pss})
    return {'rss_kib': sum(p['rss_kib'] for p in processes),
            'pss_kib': sum(p['pss_kib'] for p in processes) if processes and
                       all(p['pss_kib'] is not None for p in processes) else None,
            'processes': processes}


def stop(child, private_group=True):
    # start_new_session below makes the child's pid its private process-group id.
    # foot's child owns another session. Retain pidfds before stopping cage so
    # its editor cannot outlive the disposable home or signal a reused PID.
    readers = []
    for pid, *_ in process_members(child.pid):
        try:
            readers.append(os.pidfd_open(pid))
        except ProcessLookupError:
            pass
    try:
        if private_group:
            os.killpg(child.pid, signal.SIGTERM)
        else:
            child.terminate()
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if private_group:
            os.killpg(child.pid, signal.SIGKILL)
        else:
            child.kill()
        child.wait(timeout=5)
    finally:
        for reader in readers:
            try:
                signal.pidfd_send_signal(reader, signal.SIGTERM)
            except ProcessLookupError:
                pass
        pending = [fd for fd in readers if not select.select([fd], [], [], 1)[0]]
        for reader in pending:
            try:
                signal.pidfd_send_signal(reader, signal.SIGKILL)
            except ProcessLookupError:
                pass
        for reader in readers:
            os.close(reader)


def keyboard(directory, env):
    xml = ROOT / 'os/tests/protocols/virtual-keyboard-unstable-v1.xml'
    for mode, file in [('client-header', 'virtual-keyboard.h'), ('private-code', 'virtual-keyboard.c')]:
        subprocess.run(['wayland-scanner', mode, str(xml), str(directory / file)], check=True)
    helper = directory / 'keys'
    subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', '-I', str(directory),
                    str(ROOT / 'os/tests/wayland-keys.c'), str(directory / 'virtual-keyboard.c'),
                    '-lwayland-client', '-lxkbcommon', '-o', str(helper)], check=True)
    runtime = Path(env['XDG_RUNTIME_DIR'])
    sockets = [p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')]
    if len(sockets) != 1:
        raise RuntimeError('expected one private compositor socket')
    env = env | {'WAYLAND_DISPLAY': sockets[0]}
    child = subprocess.Popen([str(helper)], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        expect_keyboard(child, 'ready\n')
    except RuntimeError:
        child.kill()
        child.wait()
        raise
    return child, env


def expect_keyboard(child, expected):
    if not select.select([child.stdout], [], [], 5)[0] or child.stdout.readline() != expected:
        raise RuntimeError('the test keyboard did not answer within five seconds')


def key(child, code, control=0):
    child.stdin.write(f'{code} {control}\n')
    child.stdin.flush()
    expect_keyboard(child, 'sent\n')


def key_latency(directory, env, samples):
    from PIL import Image
    child, env = keyboard(directory, env)
    try:
        key(child, COLEMAK['k'], 1)
        for char in 'brass':
            key(child, COLEMAK[char])
        key(child, 28)
        time.sleep(.3)
        key(child, COLEMAK['d'], 1)  # Focus mode removes animated decorations.
        time.sleep(.3)

        def capture():
            result = subprocess.run(['grim', '-'], env=env, capture_output=True, check=True, timeout=4)
            # Exclude the bottom status row's clock; compare actual editor pixels.
            with Image.open(io.BytesIO(result.stdout)) as picture:
                return picture.convert('RGB').crop((0, 0, picture.width, picture.height - 24)).tobytes()

        delays, overhead = [], []
        for _ in range(samples):
            before = capture()
            started = time.monotonic()
            if capture() != before:
                raise RuntimeError('the editor was not stationary before the key test')
            overhead.append((time.monotonic() - started) * 1000)
            started = time.monotonic()
            key(child, COLEMAK['a'])
            deadline = started + 3
            while capture() == before:
                if time.monotonic() > deadline:
                    raise RuntimeError('the virtual key did not change editor pixels')
            delays.append((time.monotonic() - started) * 1000)
        return {'samples': samples, 'median_ms': round(statistics.median(delays), 3),
                'maximum_ms': round(max(delays), 3),
                'capture_median_ms': round(statistics.median(overhead), 3),
                'method': 'virtual Colemak key to changed Wayland screenshot; includes capture overhead'}
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def measure(backend, binary, directory, args):
    env = environment(directory, args.headless, 'graphical' if backend == 'foot' else backend)
    book = directory / 'book'
    subprocess.run([str(binary), 'wros', 'tutorial-demo', str(book)], env=env,
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=20)
    with (directory / 'renderer.log').open('w') as log:
        started = time.monotonic()
        # Native output needs the authenticated controlling tty. It shares the
        # caller's foreground group; stop() signals only its captured process tree.
        child = subprocess.Popen(command(backend, binary, book), env=env, start_new_session=backend != 'drm',
                                 stdin=None if backend == 'drm' else subprocess.DEVNULL,
                                 stdout=log, stderr=log)
        try:
            ready = directory / 'ready'
            while not ready.exists():
                if child.poll() is not None or time.monotonic() - started > 30:
                    raise RuntimeError('no first-frame marker: ' + (directory / 'renderer.log').read_text()[-2000:])
                time.sleep(.005)
            startup = (time.monotonic() - started) * 1000
            if backend == 'drm' and struct.unpack('i', fcntl.ioctl(0, 0x4B3B, bytes(4)))[0] != 1:
                raise RuntimeError('DRM fell back to text; this is not a native graphical measurement')
            time.sleep(args.settle)
            result = {'first_draw_ms': round(startup, 3), 'memory': process_memory(child.pid)}
            if args.latency and backend != 'drm':
                result['key_to_screenshot'] = key_latency(directory, env, args.latency)
            else:
                result['key_to_screenshot'] = None
            return result
        finally:
            stop(child, private_group=backend != 'drm')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    for backend in ('foot', 'wgpu', 'drm'):
        parser.add_argument('--' + backend + '-binary', type=Path)
    parser.add_argument('--backend', action='append', choices=['foot', 'wgpu', 'drm'], required=True)
    parser.add_argument('--headless', action='store_true')
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--settle', type=float, default=1)
    parser.add_argument('--latency', type=int, default=0, metavar='SAMPLES')
    parser.add_argument('--package', type=Path, help='optional exact update package whose byte size is recorded')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('run as the writer, never root')
    if not 1 <= args.runs <= 20 or not 0 <= args.latency <= 100 or not 0 <= args.settle <= 60:
        parser.error('use 1..20 runs, 0..100 key samples, and 0..60 seconds to settle')
    if args.headless and 'drm' in args.backend:
        parser.error('native DRM needs tty1; headless cage cannot measure it')
    if not args.headless and (not os.isatty(0) or os.ttyname(0) != '/dev/tty1'):
        parser.error('screen trials need an authenticated tty1; use --headless in the chroot')
    args.output = args.output.resolve()
    args.output.mkdir(mode=0o700)  # Never overwrite an earlier run.
    result = {'headless': args.headless, 'first_draw_method': 'app first-frame marker; 5 ms polling',
              'kernel': os.uname().release,
              'trial_script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'package_bytes': args.package.stat().st_size if args.package else None,
              'native_latency': 'requires a hardware measurement; Wayland capture does not cover direct DRM',
              'backends': {}, 'passed': False}
    try:
        for backend in args.backend:
            binary = (getattr(args, backend + '_binary') or args.binary).resolve()
            features = json.loads(subprocess.check_output([str(binary), '--wros-renderers'], text=True))
            if backend != 'foot' and not features.get(backend):
                raise RuntimeError(f'{backend} was not compiled in {binary}')
            record = {'binary_bytes': binary.stat().st_size,
                      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                      'compiled_renderers': features, 'runs': []}
            result['backends'][backend] = record
            for number in range(args.runs):
                with tempfile.TemporaryDirectory(prefix='wros-trial-') as temp:
                    run = measure(backend, binary, Path(temp), args)
                record['runs'].append(run)
                print(f'{backend} {number + 1}: first draw {run["first_draw_ms"]} ms, '
                      f'RSS {run["memory"]["rss_kib"]} KiB', flush=True)
            record['median_first_draw_ms'] = statistics.median(r['first_draw_ms'] for r in record['runs'])
        result['passed'] = True
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        result['error'] = str(error)
        print(str(error), flush=True)
    finally:
        (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
