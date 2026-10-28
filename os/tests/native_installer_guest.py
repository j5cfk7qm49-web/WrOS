#!/usr/bin/env python3
"""Root fixture for a marked, diskless QEMU guest; never an installed launcher.

The maintained Debian live plugin prepares the backend. Its normal startup and
menu run over the inherited native stream, without invoking console wrappers.
Only the separate frontend owns tty7. A transient systemd cgroup and private
mount/PID namespaces contain every process and mount until this probe stops.
"""
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import resource
import shutil
import socket
import subprocess
import sys
import termios
import time

QA = Path('/run/wros-native-qa')
WORK = Path('/run/wros-native-probe')
UNIT = 'wros-native-probe.service'
DI = Path('/run/live/installer')
FRONTEND = Path('/usr/bin/wros-installer')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def block_is_readonly_or_unconfigured_loop(block):
    return ((block/'ro').read_text().strip() == '1'
            or (re.fullmatch(r'loop[0-9]+', block.name) is not None
                and (block/'size').read_text().strip() == '0'
                and not (block/'loop/backing_file').exists()))


def guard():
    require(os.geteuid() == 0, 'Guest fixture needs root inside its disposable VM')
    token = (QA/'fixture-token').read_text().strip()
    require(len(token) == 32 and all(c in '0123456789abcdef' for c in token), 'Invalid fixture marker')
    require(Path('/sys/class/dmi/id/product_name').read_text().strip() == 'WrOS-Installer-Fixture'
            and Path('/sys/class/dmi/id/product_serial').read_text().strip() == token,
            'This is not the marked disposable QEMU guest')
    require('wros.native_probe='+token in Path('/proc/cmdline').read_text().split(),
            'Guest kernel fixture marker is absent')
    require(Path('/run/live/medium/live/filesystem.squashfs').is_file(), 'Expected a live guest')
    for block in Path('/sys/class/block').iterdir():
        require(block_is_readonly_or_unconfigured_loop(block),
                'Writable block device is forbidden in this probe: '+block.name)
    require(os.statvfs(QA).f_flag & os.ST_RDONLY, 'Fixture payload must be mounted read-only')
    manifest = json.loads((QA/'native-installer/manifest.json').read_bytes())
    require(manifest['passed'] and manifest['kernel_release'] == os.uname().release,
            'Backend kernel must match the running live kernel')
    for name, expected in manifest['file_sha256'].items():
        relative = Path(name)
        require(not relative.is_absolute() and '..' not in relative.parts, 'Unsafe capsule member')
        path = QA/'native-installer'/relative
        require(path.is_file() and not path.is_symlink() and digest(path) == expected,
                'Read-only capsule member differs: '+name)
    require(digest(Path('/run/live/medium/dists/trixie/Release'))
            == manifest['base_media_release_sha256'], 'Base live media differs')
    return manifest


def command(*args):
    subprocess.run(list(args), check=True, stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd='/')


def bind(source, target, readonly=False, recursive=False):
    if source.is_dir():
        target.mkdir(parents=True, exist_ok=True)
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            target.touch()
    command('/bin/mount', '--rbind' if recursive else '--bind', str(source), str(target))
    if readonly:
        command('/bin/mount', '-o', 'remount,bind,ro', str(target))


def state(value):
    temporary = WORK/'status.new'
    temporary.write_text(json.dumps(value)+'\n')
    temporary.chmod(0o600)
    temporary.replace(WORK/'status.json')


def worker():
    manifest = guard()
    require(os.getpid() == 1, 'Worker must run in the private PID namespace')
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    require(ctypes.CDLL(None).prctl(4, 0, 0, 0, 0) == 0, 'Worker dump protection failed')
    record = {'installation_complete': False, 'scope': 'Diskless menu/transport probe only', 'writable_disks': 0,
              'backend_kernel': manifest['kernel_release'], 'module_sha256': manifest['module_sha256']}
    state(record)
    # Preserve the original live root/pool, replacing only the installer's
    # private view. Neither this view nor /proc/cmdline escapes the namespace.
    view = WORK/'medium'; view.mkdir()
    command('/bin/mount', '-t', 'tmpfs', '-o', 'mode=755,nosuid,nodev', 'tmpfs', str(view))
    medium = Path('/run/live/medium')
    for source in medium.iterdir():
        if source.name == 'install':
            # Debian's live plugin prefers install/gtk/initrd.gz even with
            # --text. The native view must expose only the matching backend.
            shutil.copytree(source, view/source.name, symlinks=True,
                            ignore=shutil.ignore_patterns('gtk'))
        elif source.name == 'dists':
            shutil.copytree(source, view/source.name, symlinks=True)
        elif source.is_symlink():
            (view/source.name).symlink_to(os.readlink(source))
        else:
            bind(source, view/source.name, True)
    bind(QA/'native-installer', view/'native-installer', True)
    # The installer sees the matching initrd and the verified subset index.
    # These overmounts are private even though their lower directories are RO.
    for name, target in [('initrd.gz', 'install/initrd.gz'),
                         ('Packages', 'dists/trixie/main/debian-installer/binary-amd64/Packages'),
                         ('Packages.gz', 'dists/trixie/main/debian-installer/binary-amd64/Packages.gz'),
                         ('media-Release', 'dists/trixie/Release')]:
        bind(QA/'native-installer'/name, view/target, True)
    # Carry the verified file and directory overmounts into the installer's
    # view. A nonrecursive bind would expose the copied old initrd underneath.
    bind(view, medium, True, recursive=True)
    require(not (medium/'install/gtk/initrd.gz').exists()
            and digest(medium/'install/initrd.gz') == manifest['file_sha256']['initrd.gz']
            and digest(medium/'native-installer/Packages') == manifest['file_sha256']['Packages'],
            'Private media view does not expose the exact matching backend and pool')
    launcher = Path('/usr/share/debian-installer-launcher')
    launcher.mkdir(parents=True, exist_ok=True)
    staged_launcher = WORK/'launcher'
    shutil.copytree(QA/'launcher', staged_launcher)
    # QA media stays noexec; only these fixed, hashed upstream scripts are
    # copied into the disposable guest's private runtime before execution.
    (staged_launcher/'debconf_kernel.sh').rename(staged_launcher/'kernel-upstream.sh')
    (staged_launcher/'debconf_kernel.sh').write_text(
        '#!/bin/sh\nexport DEBIAN_FRONTEND=noninteractive\n'
        'exec /usr/bin/debconf --frontend=noninteractive /bin/sh -x '
        '/usr/share/debian-installer-launcher/kernel-upstream.sh\n')
    (staged_launcher/'debconf_kernel.sh').chmod(0o755)
    bind(staged_launcher, launcher, True)
    command('/bin/mount', '-o', 'remount,bind,ro,exec', str(launcher))
    # live_prepare performs Debian's own unpack and separate kernel check.
    # Debian calls prepare in an if-condition, deliberately suppressing shell
    # errexit within the plugin. Preserve that calling convention.
    prepare = '. /usr/share/debian-installer-launcher/plugins/live; if live_prepare --text; then exit 0; else exit 1; fi'
    preparation_log = WORK/'prepare.log'
    with preparation_log.open('xb') as log:
        preparation_log.chmod(0o600)
        # Only the separate kernel-check child uses host noninteractive debconf.
        # Preparation's ordinary output must never enter its command protocol.
        subprocess.run(['/bin/sh', '-ec', prepare], check=True, stdin=subprocess.DEVNULL,
                       stdout=log, stderr=subprocess.STDOUT, cwd='/')
    require(digest(DI/'usr/lib/cdebconf/frontend/passthrough.so') == manifest['module_sha256'],
            'Unpacked native transport differs')
    require({p.name for p in (DI/'lib/modules').iterdir()} == {os.uname().release},
            'Unpacked kernel modules differ')
    bind(medium, DI/'cdrom', True, recursive=True)
    for name in ('dev', 'dev/pts', 'proc', 'sys', 'tmp'):
        bind(Path('/'+name), DI/name)
    command('/bin/mount', '-t', 'tmpfs', '-o', 'mode=755,nosuid,nodev', 'tmpfs', str(DI/'run'))
    (DI/'run/lock').mkdir()
    # The live /dev/log symlink resolves inside the chroot's private /run.
    (DI/'run/systemd/journal').mkdir(parents=True)
    # Do not expose arbitrary boot-time preseeding or early shell commands.
    # Keep the fixture marker for guard() if this process is inspected later.
    sanitized = WORK/'cmdline'
    sanitized.write_text('console=tty1\n')
    bind(sanitized, Path('/proc/cmdline'), True)
    bind(sanitized, DI/'proc/cmdline', True)
    require((DI/'proc/cmdline').read_bytes() == b'console=tty1\n', 'Backend boot arguments were not filtered')
    account = pwd.getpwnam('wros-live')
    require(account.pw_uid == account.pw_gid == 1000, 'Unexpected public live account')
    frontend_socket, backend_socket = socket.socketpair()
    tty = os.open('/dev/tty7', os.O_RDWR | os.O_NOCTTY)

    def frontend_setup():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        os.setgroups([]); os.setgid(account.pw_gid); os.setuid(account.pw_uid)

    frontend = None
    backend = None
    log_path = WORK/'backend.log'
    # Debian confmodule reserves low descriptors for its own pipes. Keep the
    # outer native transport clear of those descriptors across startup scripts.
    fd = fcntl.fcntl(backend_socket.fileno(), fcntl.F_DUPFD_CLOEXEC, 64)
    require(64 <= fd < 1024, 'Backend transport descriptor unavailable')
    backend_socket.close()
    environment = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'HOME': '/', 'TERM': 'linux',
                   'LANG': 'C.UTF-8', 'LIVE_INSTALLER_MODE': '1', 'DEBIAN_FRONTEND': 'passthrough',
                   'DEBCONF_READFD': str(fd), 'DEBCONF_WRITEFD': str(fd), 'MENU': '/usr/bin/main-menu'}
    # Startup scripts may create separate short-lived debconf processes. The
    # supervisor retains the socket while startup and menu use it in order.
    with log_path.open('xb') as log:
        log_path.chmod(0o600)
        try:
            frontend = subprocess.Popen([str(FRONTEND), '--frontend-fd', str(frontend_socket.fileno())],
                stdin=tty, stdout=tty, stderr=tty, pass_fds=(frontend_socket.fileno(),),
                preexec_fn=frontend_setup, env={'PATH': '/usr/bin:/bin', 'TERM': 'linux',
                    'HOME': account.pw_dir, 'USER': account.pw_name, 'LANG': 'C.UTF-8'}, cwd='/')
            frontend_socket.close(); os.close(tty)
            command('/bin/chvt', '7')
            shell = '/sbin/debian-installer-startup; exec /lib/debian-installer/menu'
            backend = subprocess.Popen(['/usr/sbin/chroot', str(DI), '/bin/sh', '-ec', shell],
                stdin=subprocess.DEVNULL, stdout=log, stderr=log, env=environment,
                pass_fds=(fd,), cwd='/')
            os.close(fd)
            record.update({'frontend_pid': frontend.pid, 'backend_pid': backend.pid,
                           'frontend_uid': 1000, 'frontend_tty': 7, 'ready': True,
                           'backend_boot_arguments_filtered': True,
                           'frontend_sha256': digest(FRONTEND)})
            state(record)
            while frontend.poll() is None and backend.poll() is None:
                time.sleep(0.1)
            record.update({'frontend_exit': frontend.poll(), 'backend_exit': backend.poll(),
                           'ready': False})
            state(record)
        finally:
            for process in (backend, frontend):
                if process is not None and process.poll() is None:
                    process.terminate()
            for process in (backend, frontend):
                if process is not None:
                    try:
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.wait()


def main():
    require(len(sys.argv) == 2 and sys.argv[1] in ('start', 'worker'), 'Fixed fixture operation required')
    if sys.argv[1] == 'worker':
        worker()
        return
    guard()
    require(not WORK.exists(), 'This one-shot guest probe already ran')
    command('/usr/bin/systemctl', 'stop', 'greetd.service')
    # Stopping greetd must retire its editor and dedicated input reader before
    # any new TUI is started. No signal is sent to an unrelated process.
    for process in Path('/proc').glob('[0-9]*'):
        try:
            require((process/'comm').read_text().strip() not in ('grimoire', 'wros-input', 'wros-greeter'),
                    'The previous writing session has not retired')
        except (FileNotFoundError, ProcessLookupError):
            pass
    WORK.mkdir(mode=0o700)
    command('/usr/bin/systemd-run', '--unit='+UNIT, '--collect',
            '--property=KillMode=control-group', '--property=TimeoutStopSec=5s',
            '--property=RuntimeMaxSec=600s', '--property=PrivateMounts=yes',
            '--property=PrivateTmp=yes', '--property=LimitCORE=0',
            '/usr/bin/unshare', '--mount', '--pid', '--uts', '--ipc', '--fork', '--kill-child', '--mount-proc',
            '/usr/bin/python3', '-I', str(QA/'native_installer_guest.py'), 'worker')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print('Diskless native installer fixture stopped: '+str(error), file=sys.stderr)
        raise SystemExit(1) from None
