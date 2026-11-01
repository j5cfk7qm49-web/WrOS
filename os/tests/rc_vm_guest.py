#!/usr/bin/python3
"""Guest-side read-only probe for the WrOS RC VM qualification. Run as root in the VM.

Never writes anything except to stdout. Output lines are prefixed `RCJSON` so the host
can find them in serial noise. Modes: vcs N | snap | keymap | tree PATH | find-text TEXT ROOT
"""
import json
import os
import pathlib
import re
import struct
import subprocess
import sys

P = pathlib.Path
WATCH = {'grimoire', 'wros-input', 'wros-greeter', 'agetty', 'wros-installer', 'greetd', 'pkexec',
         'wros-install-session', 'systemd-run', 'unshare', 'debconf-communicate', 'main-menu',
         'debian-installer', 'partman', 'mount', 'cryptsetup'}


def emit(obj):
    print('RCJSON' + json.dumps(obj, separators=(',', ':')), flush=True)


def vcs(n):
    try:
        header = P('/dev/vcsa%d' % n).open('rb').read(4)
        rows, cols = header[0], header[1]
        raw = P('/dev/vcsu%d' % n).read_bytes()
    except OSError as error:
        return {'error': str(error), 'lines': []}
    chars = [chr(c) if 0 < c < 0x110000 and not (0xd800 <= c < 0xe000) else ' '
             for c in struct.unpack('<%dI' % (len(raw) // 4), raw)]
    lines = [''.join(chars[i * cols:(i + 1) * cols]).rstrip() for i in range(rows)]
    return {'rows': rows, 'cols': cols, 'lines': lines}


def text(path, limit=1 << 20):
    try:
        return P(path).read_text(errors='replace')[:limit]
    except OSError:
        return None


def processes():
    found = []
    for entry in P('/proc').glob('[0-9]*'):
        try:
            comm = (entry / 'comm').read_text().strip()
            if comm not in WATCH:
                continue
            status = dict(line.split(':\t', 1) for line in (entry / 'status').read_text().splitlines() if ':\t' in line)
            cmd = (entry / 'cmdline').read_bytes().split(b'\0')
            try:
                exe = os.readlink(entry / 'exe')
            except OSError:
                exe = None
            try:
                stdin = os.stat(entry / 'fd/0')
                dev = '%d:%d' % (os.major(stdin.st_rdev), os.minor(stdin.st_rdev)) if stdin.st_rdev else None
            except OSError:
                dev = None
            found.append({'pid': int(entry.name), 'ppid': int(status['PPid'].strip()), 'comm': comm,
                          'uid': status['Uid'].split(), 'cgroup': (text(entry / 'cgroup') or '').strip(),
                          'exe': exe, 'stdin': dev,
                          'cmd': [c.decode('utf-8', 'replace') for c in cmd[:8] if c]})
        except (OSError, KeyError, ValueError):
            continue
    return found


def run(*cmd, timeout=10):
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return done.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None


def snap():
    runs = {}
    base = P('/run/wros-installer')
    if base.is_dir():
        for item in sorted(base.iterdir()):
            if item.is_file() and item.stat().st_size < 1 << 20:
                try:
                    runs[item.name] = json.loads(item.read_text())
                except ValueError:
                    runs[item.name] = {'unparsed': item.read_text(errors='replace')[:2000]}
    mounts = [line.split()[4] for line in (text('/proc/self/mountinfo') or '').splitlines()
              if len(line.split()) > 5 and ('/run/live/installer' in line or '/target' in line.split()[4])]
    state = {
        'active_vt': (text('/sys/class/tty/tty0/active') or '').strip(),
        'hostname': run('hostname'),
        'cmdline': (text('/proc/cmdline') or '').strip(),
        'vcs1': vcs(1)['lines'], 'vcs7': vcs(7)['lines'],
        'procs': processes(),
        'run_files': runs,
        'run_dir_exists': base.exists(),
        'installer_unit': run('systemctl', 'is-active', 'wros-installer-session.service'),
        'mappers': sorted(os.listdir('/dev/mapper')) if P('/dev/mapper').is_dir() else [],
        'target_mounts': mounts,
        'swaps': (text('/proc/swaps') or '').splitlines()[1:],
        'block': run('lsblk', '-dn', '-o', 'NAME,SIZE,TYPE,RO,SERIAL'),
    }
    for unit in ('wros-update-reconcile.service', 'system-update.target', 'greetd.service'):
        state['unit:' + unit] = run('systemctl', 'is-active', unit)
    return state


def keymap():
    dump = run('dumpkeys', '-1', timeout=20) or ''
    result = {}
    for line in dump.splitlines():
        match = re.match(r'keycode\s+(\d+)\s*=\s*(\S+)', line)
        if match and match.group(1) in ('16', '17', '18', '19', '20', '30', '31', '32', '33', '34'):
            result[match.group(1)] = match.group(2)
    return {'keycodes': result, 'kbd_file': text('/etc/default/keyboard')}


def tree(path):
    out = []
    for root, dirs, files in os.walk(path):
        for name in files:
            full = P(root, name)
            try:
                st = full.lstat()
                out.append([str(full), st.st_uid, oct(st.st_mode & 0o7777), st.st_size])
            except OSError:
                pass
        if len(out) > 4000:
            break
    return out


def find_text(needle, roots):
    hits = []
    for root in roots:
        for base, dirs, files in os.walk(root):
            dirs[:] = [d for d in dirs if d not in ('.cache', 'target', 'proc')]
            for name in files:
                full = P(base, name)
                try:
                    if full.stat().st_size < 4 << 20 and needle.encode() in full.read_bytes():
                        st = full.stat()
                        hits.append([str(full), st.st_uid, oct(st.st_mode & 0o7777)])
                except OSError:
                    pass
    return hits


def main():
    mode = sys.argv[1]
    if mode == 'vcs':
        emit(vcs(int(sys.argv[2])))
    elif mode == 'snap':
        emit(snap())
    elif mode == 'keymap':
        emit(keymap())
    elif mode == 'tree':
        emit(tree(sys.argv[2]))
    elif mode == 'find-text':
        emit(find_text(sys.argv[2], sys.argv[3:]))
    else:
        raise SystemExit('unknown mode')


if __name__ == '__main__':
    main()
