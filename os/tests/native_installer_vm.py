#!/usr/bin/env python3
"""Exercise Debian's real installer menus in a diskless, no-NIC WrOS guest.

This fixture has no target disk. It does not qualify partitioning, passwords,
installation, completion or reboot into a target. Its separate read-only QA CD
contains a verified capsule and maintained Debian launcher source, not a grant
installed in the live image.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import socket
import subprocess
import sys
import tempfile
import time

from installed_boot import qmp

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
from writingdeck_source import fingerprint


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


SNAPSHOT = r'''
import json,os,pathlib,re,subprocess,sys,time
P=pathlib.Path
deadline=time.monotonic()+45
needle=sys.argv[1]
while True:
    status=P('/run/wros-native-probe/status.json')
    record=json.loads(status.read_bytes()) if status.exists() else {}
    screen=P('/dev/vcs7').read_bytes().decode('latin1') if P('/dev/vcs7').exists() else ''
    if needle in screen and 'Enter continue' in screen and record.get('ready'): break
    assert subprocess.run(['systemctl','is-active','--quiet','wros-native-probe.service']).returncode==0,'Backend service stopped'
    assert time.monotonic()<deadline,'Installer UI readiness'
    time.sleep(.2)
processes=[]
for p in P('/proc').glob('[0-9]*'):
    try:
        name=(p/'comm').read_text().strip()
        if name not in ('wros-installer','main-menu','grimoire','wros-input','wros-greeter'):continue
        uid=next(x.split()[1:] for x in (p/'status').read_text().splitlines() if x.startswith('Uid:'))
        processes.append({'name':name,'pid':int(p.name),'uids':uid,'cgroup':(p/'cgroup').read_text()})
    except (FileNotFoundError,ProcessLookupError):pass
front=[p for p in processes if p['name']=='wros-installer']
assert len(front)==1 and front[0]['uids']==['1000']*4
assert 'wros-native-probe.service' in front[0]['cgroup']
assert not any(p['name'] in ('grimoire','wros-input','wros-greeter') for p in processes)
assert all((p/'ro').read_text().strip()=='1' or (re.fullmatch(r'loop[0-9]+',p.name)
    and (p/'size').read_text().strip()=='0' and not (p/'loop/backing_file').exists())
    for p in P('/sys/class/block').iterdir())
assert 'wros.native_probe=' in P('/proc/cmdline').read_text(),'Private sanitized cmdline escaped'
assert os.uname().nodename=='debian','Backend hostname change escaped its UTS namespace'
assert not P('/run/live/installer/usr/bin/main-menu').exists(),'Private backend mount escaped'
record.update({'screen':screen,'processes':processes,'outside_namespace_unchanged':True})
print(sys.argv[2]+json.dumps(record),flush=True)
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('iso', 'kernel', 'initrd', 'capsule', 'launcher', 'artifacts-directory'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise SystemExit('Run the QEMU fixture as a normal user')
    for path in (args.iso, args.kernel, args.initrd):
        if not path.is_file() or path.is_symlink():
            raise SystemExit('Only regular image files are accepted')
    for path in (args.capsule, args.launcher):
        if not path.is_dir() or path.is_symlink():
            raise SystemExit('Only regular prepared input directories are accepted')
    args.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='native-', dir=args.artifacts_directory)); run.chmod(0o700)
    token = secrets.token_hex(16)
    (run/'fixture-token').write_text(token+'\n')
    (run/'snapshot.py').write_text(SNAPSHOT)
    source = fingerprint()
    capsule_hash = digest(args.capsule/'manifest.json')
    result = {'passed': False, 'source_fingerprint': source, 'iso_sha256_before': digest(args.iso),
              'capsule_manifest_sha256': capsule_hash, 'network_devices': 0,
              'host_disks_exposed': 0, 'writable_disks': 0, 'phases': [],
              'scope': 'Real Debian startup/menu through native UI in a diskless guest. No target installation or hardware qualification.'}
    launcher_files = ['plugins/live', 'debconf_kernel.sh', 'debian/copyright']
    result['maintained_launcher_file_sha256'] = {name: digest(args.launcher/name) for name in launcher_files}
    payload = run/'qa.iso'
    command = ['xorriso', '-outdev', str(payload), '-volid', 'WROS_NATIVE_QA',
               '-map', str(args.capsule), '/native-installer',
               '-map', str(run/'fixture-token'), '/fixture-token',
               '-map', str(run/'snapshot.py'), '/snapshot.py',
               '-map', str(ROOT/'os/tests/native_installer_guest.py'), '/native_installer_guest.py']
    for name in launcher_files:
        command += ['-map', str(args.launcher/name), '/launcher/'+name]
    subprocess.run(command+['-commit', '-end'], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    result['qa_iso_sha256'] = digest(payload)
    for name, original in (('kernel', args.kernel), ('initrd', args.initrd)):
        extracted = run/name
        subprocess.run(['xorriso', '-osirrox', 'on', '-indev', str(args.iso),
                        '-extract', '/live/'+original.name, str(extracted)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if digest(extracted) != digest(original):
            raise SystemExit('Direct boot artifact differs from the actual live ISO')
        result[name+'_sha256'] = digest(extracted)
    serial, control = run/'serial.sock', run/'qmp.sock'
    append = ('boot=live components wros.live=1 console=ttyS0,115200 console=tty1 '
              'systemd.show_status=no panic=1 wros.native_probe='+token)
    command = ['qemu-system-x86_64', '-nodefaults', '-vga', 'std', '-accel', 'tcg,thread=multi', '-machine', 'q35',
               '-cpu', 'Nehalem', '-m', '3072', '-smp', '2', '-nic', 'none',
               '-smbios', 'type=1,manufacturer=WrOS-QA,product=WrOS-Installer-Fixture,serial='+token,
               '-display', 'none', '-no-reboot', '-kernel', str(run/'kernel'),
               '-initrd', str(run/'initrd'), '-append', append,
               '-drive', 'if=ide,media=cdrom,format=raw,readonly=on,file='+str(args.iso.resolve()),
               '-drive', 'if=ide,media=cdrom,format=raw,readonly=on,file='+str(payload),
               '-serial', 'unix:'+str(serial)+',server=on,wait=on',
               '-qmp', 'unix:'+str(control)+',server=on,wait=off']
    content = bytearray(); cursor = 0
    started = time.monotonic()
    with (run/'qemu.log').open('w') as log, (run/'serial.log').open('wb') as capture:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            deadline = time.monotonic()+15
            while not serial.exists():
                if process.poll() is not None or time.monotonic()>deadline:
                    raise RuntimeError('Guest serial endpoint did not start')
                time.sleep(.1)
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(.5); connection.connect(str(serial))

                def expect(pattern, seconds=180):
                    nonlocal cursor
                    deadline = time.monotonic()+seconds
                    while time.monotonic()<deadline:
                        match = re.search(pattern, content[cursor:])
                        if match:
                            cursor += match.end()
                            return match
                        try:
                            chunk = connection.recv(65536)
                        except socket.timeout:
                            continue
                        if not chunk: break
                        content.extend(chunk); capture.write(chunk); capture.flush()
                    raise RuntimeError('Guest did not reach the expected fixture state')

                def shell(command, seconds=120):
                    marker = 'WROS_NATIVE_'+secrets.token_hex(12)+'='
                    start = len(content)
                    connection.sendall((command+'; printf "\\n'+marker+'%s\\n" "$?"\n').encode())
                    status = expect(rb'(?:^|[\r\n])'+marker.encode()+rb'([0-9]+)[\r\n]', seconds)
                    if status.group(1) != b'0':
                        expect(rb':~\$ ', 30)
                        connection.sendall(b'sudo -n journalctl -u wros-native-probe.service --no-pager -n 30; sudo -n tail -c 12000 /run/wros-native-probe/prepare.log /run/wros-native-probe/backend.log; printf "\\nWROS_NATIVE_DIAGNOSTIC_END\\n"\n')
                        expect(rb'(?:^|[\r\n])WROS_NATIVE_DIAGNOSTIC_END[\r\n]', 30)
                        raise RuntimeError('Guest fixture command failed')
                    output = bytes(content[start:cursor])
                    expect(rb':~\$ ', 30)
                    return output

                def snapshot(needle, name):
                    marker = 'WROS_SNAPSHOT_'+secrets.token_hex(12)+'='
                    output = shell('sudo -n /usr/bin/python3 -I /run/wros-native-qa/snapshot.py '
                                   +"'"+needle+"' "+marker, 75)
                    matched = re.search(rb'(?:^|[\r\n])'+marker.encode()+rb'([^\r\n]+)', output)
                    if matched is None: raise RuntimeError('Native snapshot missing')
                    result['phases'].append({'phase':name, **json.loads(matched.group(1))})
                    qmp(control, 'screendump', {'filename':str(run/(name+'.ppm'))})

                expect(rb'login:'); connection.sendall(b'wros-live\n'); expect(rb'Password:')
                connection.sendall(b'live\n'); expect(rb':~\$ ', 45)
                shell('sudo -n mkdir /run/wros-native-qa && sudo -n mount -o ro,nosuid,nodev,noexec /dev/disk/by-label/WROS_NATIVE_QA /run/wros-native-qa')
                shell('sudo -n /usr/bin/python3 -I /run/wros-native-qa/native_installer_guest.py start')
                snapshot('Select a language', 'language')
                qmp(control, 'send-key', {'keys':[{'type':'qcode','data':'ret'}], 'hold-time':40})
                snapshot('Select your location', 'location')
                # Stop the standard service, proving that its frontend and
                # backend leave together, before restarting authenticated entry.
                shell('sudo -n systemctl stop wros-native-probe.service')
                cleanup = r'''import json,pathlib,subprocess; P=pathlib.Path; names=[(p/'comm').read_text().strip() for p in P('/proc').glob('[0-9]*') if (p/'comm').exists()]; assert not any(n in names for n in ('wros-installer','main-menu')); assert not P('/run/live/installer/usr/bin/main-menu').exists(); assert 'wros.native_probe=' in P('/proc/cmdline').read_text(); print('WROS_NATIVE_CLEANUP_PASSED')'''
                shell('sudo -n python3 -I -c '+"'"+cleanup.replace("'", "'\"'\"'")+"'")
                result['cgroup_stop_retired_both_processes'] = True
                result['private_mounts_did_not_escape'] = True
                connection.sendall(b'sudo -n /sbin/poweroff\n')
                deadline = time.monotonic()+60
                while process.poll() is None and time.monotonic()<deadline:
                    try:
                        chunk = connection.recv(65536)
                    except socket.timeout: continue
                    if chunk:
                        content.extend(chunk); capture.write(chunk); capture.flush()
                    else: time.sleep(.05)
                process.wait(timeout=1)
                result['passed'] = process.returncode == 0
        except Exception as error:
            result['error'] = type(error).__name__+': '+str(error)
            if control.exists():
                try: qmp(control, 'screendump', {'filename':str(run/'failed-screen.ppm')})
                except (OSError, RuntimeError): pass
        finally:
            if process.poll() is None:
                process.terminate()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
            result['iso_sha256_after'] = digest(args.iso)
            result['iso_unchanged'] = result['iso_sha256_after'] == result['iso_sha256_before']
            result['source_unchanged'] = source == fingerprint()
            result['capsule_unchanged'] = capsule_hash == digest(args.capsule/'manifest.json')
            result['passed'] &= result['iso_unchanged'] and result['source_unchanged'] and result['capsule_unchanged']
            result['seconds'] = round(time.monotonic()-started, 2)
            (run/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='phases'},indent=2))
    print('Private diskless artifacts: '+str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
