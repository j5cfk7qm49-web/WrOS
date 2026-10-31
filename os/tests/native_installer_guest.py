#!/usr/bin/env python3
"""Root fixture for marked QEMU guests; never an installed launcher.

The maintained Debian live plugin prepares the backend. Its normal startup and
menu run over the inherited native stream, without invoking console wrappers.
Only the separate frontend owns tty7. A transient systemd cgroup and private
mount/PID namespaces contain every process and mount until this probe stops.
The optional target is a new fixed-size virtual disk with the fixture nonce;
ordinary diskless operation retains its strict rejection of writable media.
"""
import ctypes
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import pwd
import re
import resource
import select
import shlex
import shutil
import socket
import stat
import subprocess
import sys
import termios
import time

QA = Path('/run/wros-native-qa')
WORK = Path('/run/wros-native-probe')
UNIT = 'wros-native-probe.service'
DI = Path('/run/live/installer')
FRONTEND = Path('/usr/bin/wros-installer')
storage_spec = importlib.util.spec_from_file_location('wros_fixture_storage', Path(__file__).with_name('fixture_storage.py'))
storage = importlib.util.module_from_spec(storage_spec)
storage_spec.loader.exec_module(storage)


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


def guard(allow_synthetic_target=False):
    require(os.geteuid() == 0, 'Guest fixture needs root inside its disposable VM')
    token = (QA/'fixture-token').read_text().strip()
    require(len(token) == 32 and all(c in '0123456789abcdef' for c in token), 'Invalid fixture marker')
    require(Path('/sys/class/dmi/id/product_name').read_text().strip() == 'WrOS-Installer-Fixture'
            and Path('/sys/class/dmi/id/product_serial').read_text().strip() == token,
            'This is not the marked disposable QEMU guest')
    require('wros.native_probe='+token in Path('/proc/cmdline').read_text().split(),
            'Guest kernel fixture marker is absent')
    require(Path('/run/live/medium/live/filesystem.squashfs').is_file(), 'Expected a live guest')
    identity = None
    if allow_synthetic_target:
        description = QA/'fixture-target.json'
        require(description.is_file() and not description.is_symlink() and description.stat().st_size <= 1024,
                'Bounded generated target description required')
        identity = storage.inspect_target(json.loads(description.read_bytes()), token)
        device = os.stat('/dev/vda')
        require(stat.S_ISBLK(device.st_mode) and os.major(device.st_rdev) == identity['major']
                and os.minor(device.st_rdev) == identity['minor'],
                'Generated target device node differs')
    for block in Path('/sys/class/block').iterdir():
        require(block_is_readonly_or_unconfigured_loop(block)
                or (identity is not None and storage.belongs_to_target(block.name, identity)),
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
    return manifest, identity


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


def validate_review_preseed(contents, installing=False):
    require(len(contents) <= 16*1024 and b'\0' not in contents and b'\r' not in contents,
            'Invalid review fixture defaults')
    allowed = {'anna/choose_modules', 'keyboard-configuration/xkb-keymap', 'netcfg/enable',
               'ethdetect/module_select',
               'netcfg/get_hostname', 'netcfg/get_domain', 'clock-setup/utc', 'clock-setup/ntp',
               'time/zone', 'passwd/user-fullname', 'passwd/username', 'passwd/user-password-crypted',
               'partman-auto/disk', 'partman-auto/method', 'partman-auto/choose_recipe',
               'partman-auto-lvm/guided_size'}
    extra = {'apt-setup/use_mirror': ('boolean', 'false'),
             'apt-setup/services-select': ('multiselect', ''),
             'pkgsel/upgrade': ('select', 'none'), 'tasksel/first': ('multiselect', ''),
             'popularity-contest/participate': ('boolean', 'false'),
             'partman-auto-crypto/erase_disks': ('boolean', 'false'),
             'cdrom-detect/eject': ('boolean', 'false'),
             'debian-installer/add-kernel-opts': ('string', 'console=tty1 console=ttyS0,115200')}
    owners = {'tasksel/first': 'tasksel', 'popularity-contest/participate': 'popularity-contest'}
    if installing:
        allowed.update(extra)
    values = {}
    for line in contents.decode('utf-8').splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        parts = line.split(None, 3)
        if installing and len(parts) == 3 and parts[1] in extra and extra[parts[1]] == ('multiselect', ''):
            parts.append('')
        require(len(parts) == 4 and parts[0] == (owners.get(parts[1], 'd-i') if installing else 'd-i') and parts[1] in allowed
                and parts[1] not in values, 'Unexpected review fixture default')
        values[parts[1]] = (parts[2], parts[3])
    password = values.get('passwd/user-password-crypted', ('password', '!'))
    if installing:
        require(all(values.get(name, value) == value for name, value in extra.items()),
                'Unexpected full-install fixture default')
        require(password[0] == 'password' and re.fullmatch(r'\$6\$[./A-Za-z0-9]{1,16}\$[./A-Za-z0-9]{86}', password[1]) is not None,
                'Only a generated hashed login credential is accepted')
    require(values.get('partman-auto/disk') == ('string', '/dev/vda')
            and values.get('partman-auto/method') == ('string', 'crypto')
            and values.get('ethdetect/module_select', ('select', 'no ethernet card')) == ('select', 'no ethernet card')
            and (installing or password == ('password', '!')),
            'Review fixture may only select the fresh disk, never unlock credentials or approve writes')


def cleanup_target_stack(
    identity,
    target_root=DI/'target',
    sysroot=Path('/sys/class/block'),
    mountinfo_path=Path('/proc/self/mountinfo'),
    swaps_path=Path('/proc/swaps'),
    runner=None,
):
    """Retire any target mounts, active swaps, LVM LVs, and LUKS2 containers."""
    if identity is None:
        return {
            'mounts_unmounted': [],
            'swaps_disabled': [],
            'mappers_closed': [],
            'clean': True,
        }
    run_cmd = runner or command
    mountinfo_text = mountinfo_path.read_text(encoding='utf-8') if mountinfo_path.exists() else ''
    swaps_text = swaps_path.read_text(encoding='utf-8') if swaps_path.exists() else ''
    plan = storage.teardown_plan(
        identity,
        mountinfo_text,
        swaps_text,
        target_root=str(target_root),
        sysroot=sysroot,
    )
    unmounted = []
    for mountpoint in plan['mounts']:
        run_cmd('/bin/umount', mountpoint)
        unmounted.append(mountpoint)
    disabled_swaps = []
    for swap in plan['swaps']:
        require(storage.belongs_to_target(swap['block'], identity, sysroot=sysroot),
                'Refusing to disable swap on non-target block device')
        run_cmd('/sbin/swapoff', swap['path'])
        disabled_swaps.append(swap['path'])
    closed_mappers = []
    for mapper in plan['mappers']:
        require(storage.belongs_to_target(mapper['block'], identity, sysroot=sysroot),
                'Refusing to close non-target device-mapper node')
        if mapper['kind'] == 'crypt':
            run_cmd('/sbin/cryptsetup', 'close', mapper['dm_name'])
        else:
            run_cmd('/sbin/dmsetup', 'remove', '--retry', mapper['dm_name'])
        closed_mappers.append(mapper['dm_name'])
    # Re-read state to verify zero leaked target mounts, swaps, or mappers.
    after_mounts = storage.target_mountpoints(
        mountinfo_path.read_text(encoding='utf-8') if mountinfo_path.exists() else '',
        target_root=str(target_root),
    )
    after_swaps = storage.target_swaps(
        swaps_path.read_text(encoding='utf-8') if swaps_path.exists() else '',
        identity,
        sysroot=sysroot,
    )
    after_mappers = storage.target_mapper_teardown_order(identity, sysroot=sysroot)
    require(not after_mounts and not after_swaps and not after_mappers,
            'Target mounts, swaps or device-mapper nodes remained active after cleanup')
    return {
        'mounts_unmounted': unmounted,
        'swaps_disabled': disabled_swaps,
        'mappers_closed': closed_mappers,
        'clean': True,
    }


def stage_target_proof(token):
    source = (QA/'native_target_audit.py').read_text()
    require(0 < len(source.encode()) <= 65536, 'Bounded fixed target audit required')
    directory = DI/'usr/lib/finish-install.d'
    audit = directory/'94wros-fixture-proof'
    finish = directory/'97wros-fixture-finished'
    require(not audit.exists() and not finish.exists(), 'Fixture completion hooks already exist')
    audit.write_text('#!/bin/sh\nset -e\numask 077\n'
        'if in-target --pass-stdout /usr/bin/python3 -I -c '+shlex.quote(source)
        +' --fixture-token '+shlex.quote(token)+' > /wros-target-proof; then\n'
        '  exit 0\nelse\n  exit 10\nfi\n')
    finish.write_text('#!/bin/sh\nset -e\numask 077\n'
                      'test -s /wros-target-proof || exit 10\n'
                      'printf "%s\\n" '+shlex.quote(token)+' > /wros-target-finished\n')
    audit.chmod(0o755); finish.chmod(0o755)


def verified_target_proof(token, manifest):
    path = DI/'wros-target-proof'
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o077
            and 0 < info.st_size <= 65536, 'Private bounded target proof required')
    require((DI/'wros-target-finished').read_text() == token+'\n', 'Finish marker differs')
    proof = json.loads(path.read_bytes())
    require(proof.get('passed') is True and proof.get('root_locked') is True
            and proof.get('native_authentication_preserved') is True
            and proof.get('encrypted_root_chain', {}).get('format') == 'LUKS2'
            and proof.get('plaintext_fixture_credentials_in_logs') is False
            and proof.get('kernel_sha256') == manifest['file_sha256']['vmlinuz'],
            'Target proof did not satisfy its required fields')
    target = str(DI/'target')
    require(not any(line.split()[4] == target or line.split()[4].startswith(target+'/')
                    for line in Path('/proc/self/mountinfo').read_text().splitlines()),
            'Target mounts remained after Debian finish hooks')
    return proof


def diagnose():
    # Read only the marked guest's own backend logs and process metadata. No
    # question/password database, manuscript or arbitrary path is inspected.
    guard((QA/'fixture-target.json').exists())
    for process in Path('/proc').glob('[0-9]*'):
        try:
            arguments = (process/'cmdline').read_bytes().split(b'\0')
            if (not arguments or arguments[0] != b'/usr/bin/python3'
                    or str(QA/'native_installer_guest.py').encode() not in arguments
                    or not any(value in arguments for value in (b'worker', b'worker-target', b'worker-review', b'worker-install'))):
                continue
            require('/system.slice/'+UNIT in (process/'cgroup').read_text(), 'Diagnostic process differs')
            namespace = os.open(process/'ns/mnt', os.O_RDONLY | os.O_CLOEXEC)
            pid_namespace = os.open(process/'ns/pid', os.O_RDONLY | os.O_CLOEXEC)
            try:
                subprocess.run(['/usr/bin/nsenter', '--mount=/proc/self/fd/'+str(namespace), '--',
                                '/usr/bin/tail', '-c', '16000', str(DI/'var/log/syslog')],
                               pass_fds=(namespace,), stdin=subprocess.DEVNULL, timeout=15, check=False)
                subprocess.run(['/usr/bin/nsenter', '--mount=/proc/self/fd/'+str(namespace),
                                '--pid=/proc/self/fd/'+str(pid_namespace), '--',
                                '/bin/ps', '-eo', 'pid,ppid,stat,wchan:24,comm'],
                               pass_fds=(namespace, pid_namespace), stdin=subprocess.DEVNULL, timeout=15, check=False)
            finally:
                os.close(pid_namespace)
                os.close(namespace)
            return
        except (FileNotFoundError, ProcessLookupError):
            continue
    print('No active installer worker remained for read-only diagnostics.')


def writing_readers_retired():
    # A stopped greetd session can retain an already-exited zombie briefly.
    # pidfd readiness proves exit; a live reader must still block the handoff.
    for process in Path('/proc').glob('[0-9]*'):
        try:
            if (process/'comm').read_text().strip() not in ('grimoire', 'wros-input', 'wros-greeter'):
                continue
            descriptor = os.pidfd_open(int(process.name))
            try:
                if not select.select([descriptor], [], [], 0)[0]:
                    return False
            finally:
                os.close(descriptor)
        except (FileNotFoundError, ProcessLookupError):
            pass
    return True


def installer_readers_retired():
    for process in Path('/proc').glob('[0-9]*'):
        try:
            if (process/'comm').read_text().strip() not in ('wros-installer', 'main-menu', 'cdebconf'):
                continue
            descriptor = os.pidfd_open(int(process.name))
            try:
                if not select.select([descriptor], [], [], 0)[0]:
                    return False
            finally:
                os.close(descriptor)
        except (FileNotFoundError, ProcessLookupError):
            pass
    return True


def handoff_to_writing_session(
    identity=None,
    target_root=DI/'target',
    sysroot=Path('/sys/class/block'),
    mountinfo_path=Path('/proc/self/mountinfo'),
    swaps_path=Path('/proc/swaps'),
    runner=None,
):
    """Retire any installer target state and return tty1 to greetd writing session."""
    run_cmd = runner or command
    require(installer_readers_retired(), 'Installer frontend or backend is still running')
    cleanup = cleanup_target_stack(
        identity,
        target_root=target_root,
        sysroot=sysroot,
        mountinfo_path=mountinfo_path,
        swaps_path=swaps_path,
        runner=run_cmd,
    )
    run_cmd('/bin/chvt', '1')
    run_cmd('/usr/bin/systemctl', 'start', 'greetd.service')
    return cleanup


def worker(allow_synthetic_target=False, review=False, installing=False):
    manifest, identity = guard(allow_synthetic_target)
    require(os.getpid() == 1, 'Worker must run in the private PID namespace')
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    require(ctypes.CDLL(None).prctl(4, 0, 0, 0, 0) == 0, 'Worker dump protection failed')
    record = {'installation_complete': False,
              'scope': 'Marked fresh-target encrypted installation fixture' if installing
                       else 'Marked fresh-target menu probe only' if identity else 'Diskless menu/transport probe only',
              'writable_disks': 1 if identity else 0, 'fixture_target': identity,
              'backend_kernel': manifest['kernel_release'], 'module_sha256': manifest['module_sha256']}
    state(record)
    candidate = QA/'wros-installer'
    if candidate.exists():
        expected = (QA/'frontend.sha256').read_text().strip()
        require(candidate.is_file() and not candidate.is_symlink()
                and re.fullmatch('[0-9a-f]{64}', expected) is not None and digest(candidate) == expected,
                'Compiled fixture frontend differs from its read-only receipt')
        staged = WORK/'wros-installer'
        shutil.copyfile(candidate, staged); staged.chmod(0o755)
        require(digest(staged) == expected, 'Copied fixture frontend differs')
        bind(staged, FRONTEND, True)
        command('/bin/mount', '-o', 'remount,bind,ro,exec', str(FRONTEND))
        record['frontend_override_sha256'] = expected
        state(record)
    # Debian base-installer later uses a nonrecursive CD bind. Its entire
    # view must therefore be one filesystem, rather than a tree of submounts.
    # The live ISO is the read-only lower layer. The separate QA capsule must
    # retain its indexed native-installer/ prefix as regular RAM-backed files.
    view = WORK/'medium'; view.mkdir()
    upper, overlay_work = WORK/'media-upper', WORK/'media-work'
    upper.mkdir(); upper.chmod(0o755); overlay_work.mkdir()
    medium = Path('/run/live/medium')
    media_info = (medium/'.disk/info').read_bytes()
    squashfs_size = (medium/'live/filesystem.squashfs').stat().st_size
    # Hide Debian's GTK preference through an opaque overlay directory.
    # No upstream package or lower-layer file is modified.
    gtk = upper/'install/gtk'; gtk.mkdir(parents=True)
    os.setxattr(gtk, 'trusted.overlay.opaque', b'y')
    options = ('nosuid,nodev,lowerdir='+str(medium)
               +',upperdir='+str(upper)+',workdir='+str(overlay_work))
    command('/bin/mount', '-t', 'overlay', '-o', options, 'overlay', str(view))
    # Expose only the declared archive paths. Capsule authentication catalogs
    # remain on the QA medium; apt-cdrom must not mistake them for flat repos.
    (view/'native-installer').mkdir(mode=0o755)
    shutil.copytree(QA/'native-installer/pool', view/'native-installer/pool')
    for name, target in [('initrd.gz', 'install/initrd.gz'),
                         ('vmlinuz', 'install/vmlinuz'),
                         ('Packages', 'dists/trixie/main/debian-installer/binary-amd64/Packages'),
                         ('Packages.gz', 'dists/trixie/main/debian-installer/binary-amd64/Packages.gz'),
                         ('media-Release', 'dists/trixie/Release')]:
        destination = view/target
        require(not destination.is_symlink(), 'Native media layout must use regular files')
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(QA/'native-installer'/name, destination)
    # Seal the published VFS mount through the same maintained read-only bind
    # operation as the rest of the fixture; do not reconfigure overlayfs.
    bind(view, view, True)
    rebound = WORK/'media-rebound'
    bind(view, rebound, True)
    # This is an invariant over the builder's authenticated, canonical index,
    # not a replacement package parser. Check its exact retriever paths, not
    # merely that an archive exists somewhere in the merged filesystem.
    index = (QA/'native-installer/Packages').read_bytes()
    require(0 < len(index) <= 32*1024*1024, 'Bounded canonical package index required')
    paths = re.findall(r'^Filename: (\S+)$', index.decode('utf-8'), re.MULTILINE)
    expected = {'native-installer/'+name for name in manifest['file_sha256'] if name.startswith('pool/')}
    require(len(paths) == len(expected) and set(paths) == expected,
            'Canonical package index paths differ from the capsule')
    require(all((rebound/name).is_file() and not (rebound/name).is_symlink()
                and digest(rebound/name) == manifest['file_sha256'][name.removeprefix('native-installer/')]
                for name in paths), 'Debian media rebind lost an indexed package archive')
    require((rebound/'.disk/info').read_bytes() == media_info
            and (rebound/'live/filesystem.squashfs').stat().st_size == squashfs_size
            and digest(rebound/'install/initrd.gz') == manifest['file_sha256']['initrd.gz']
            and digest(rebound/'install/vmlinuz') == manifest['file_sha256']['vmlinuz']
            and digest(rebound/'dists/trixie/main/debian-installer/binary-amd64/Packages') == manifest['file_sha256']['Packages']
            and not (rebound/'install/gtk/initrd.gz').exists()
            and not (rebound/'native-installer/Packages').exists(),
            'Debian nonrecursive media bind lost its native or ordinary payload')
    command('/bin/umount', str(rebound)); rebound.rmdir()
    record['media_nonrecursive_bind_verified'] = True
    bind(view, medium, True)
    require(not (medium/'install/gtk/initrd.gz').exists()
            and digest(medium/'install/initrd.gz') == manifest['file_sha256']['initrd.gz']
            and digest(medium/'dists/trixie/main/debian-installer/binary-amd64/Packages') == manifest['file_sha256']['Packages'],
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
    require((DI/'usr/lib/apt-setup/generators/40cdrom').is_file(),
            'Maintained offline CD-ROM APT provider is absent')
    # Debian must retain this externally supplied media view instead of
    # unmounting it and probing the underlying physical ISO again.
    with (DI/'preseed.cfg').open('ab') as output:
        output.write(b'\nd-i cdrom-detect/hybrid boolean true\n')
    # Debian loads crypto later through anna. Apply the fixed-version native
    # passphrase adapter at partman startup, after those maintained files exist.
    hook = DI/'lib/partman/init.d/00wros-native-passphrase'
    hook.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(QA/'native-crypto-passphrase', hook)
    hook.chmod(0o755)
    if review:
        require(identity is not None, 'Review requires the generated target')
        defaults = (QA/'review.preseed').read_bytes()
        validate_review_preseed(defaults, installing)
        shipping = (QA/'installer.preseed').read_bytes()
        require(len(shipping) <= 16*1024 and b'\0' not in shipping, 'Bounded shipping defaults required')
        with (DI/'preseed.cfg').open('ab') as output:
            output.write(b'\n'+shipping+b'\n'+defaults+b'\n')
        if installing:
            stage_target_proof((QA/'fixture-token').read_text().strip())
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
            if installing:
                code = backend.wait(timeout=5)
                require(code in (0, 11), 'Backend did not exit through the maintained finish path')
                proof = verified_target_proof((QA/'fixture-token').read_text().strip(), manifest)
                record.update({'target_configuration_verified': True, 'target_mounts_retired': True,
                               'target_proof': proof, 'backend_exit': code})
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
            if identity is not None:
                cleanup = cleanup_target_stack(identity)
                record['target_cleanup'] = cleanup
                state(record)


def main():
    require(len(sys.argv) == 2 and sys.argv[1] in ('start', 'worker', 'start-target', 'worker-target',
                                                'start-review', 'worker-review', 'start-install', 'worker-install',
                                                'handoff-writing', 'inspect'),
            'Fixed fixture operation required')
    if sys.argv[1] == 'inspect':
        diagnose()
        return
    if sys.argv[1] == 'handoff-writing':
        _, identity = guard((QA/'fixture-target.json').exists())
        handoff_to_writing_session(identity=identity)
        return
    installing = sys.argv[1].endswith('-install')
    review = installing or sys.argv[1].endswith('-review')
    target = review or sys.argv[1].endswith('-target')
    if sys.argv[1].startswith('worker'):
        worker(target, review, installing)
        return
    guard(target)
    require(not WORK.exists(), 'This one-shot guest probe already ran')
    command('/usr/bin/systemctl', 'stop', 'greetd.service')
    # Stopping greetd must retire its editor and dedicated input reader before
    # any new TUI is started. No signal is sent to an unrelated process.
    deadline = time.monotonic()+10
    while not writing_readers_retired():
        require(time.monotonic() < deadline, 'The previous writing session has not retired')
        time.sleep(.1)
    WORK.mkdir(mode=0o700)
    command('/usr/bin/systemd-run', '--unit='+UNIT, '--collect',
            '--property=KillMode=control-group', '--property=TimeoutStopSec=5s',
            '--property=RuntimeMaxSec='+('3600s' if installing else '1200s' if review else '600s'), '--property=PrivateMounts=yes',
            '--property=PrivateTmp=yes', '--property=LimitCORE=0',
            '/usr/bin/unshare', '--mount', '--pid', '--uts', '--ipc', '--fork', '--kill-child', '--mount-proc',
            '/usr/bin/python3', '-I', str(QA/'native_installer_guest.py'),
            'worker-install' if installing else 'worker-review' if review else 'worker-target' if target else 'worker')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print('Diskless native installer fixture stopped: '+str(error), file=sys.stderr)
        raise SystemExit(1) from None
