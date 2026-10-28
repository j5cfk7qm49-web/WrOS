#!/usr/bin/env python3
"""Boot a read-only live ISO in a fresh diskless, no-NIC QEMU fixture."""
import argparse
import base64
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

PROBE = r'''
import hashlib,json,os,pathlib,subprocess,sys,tomllib
P=pathlib.Path
assert os.geteuid()==0
assert {'boot=live','wros.live=1'} <= set(P('/proc/cmdline').read_text().split())
marker=json.loads(P('/run/wros-live/session.json').read_bytes())
assert marker['user']=='wros-live' and marker['uid']==1000
assert P('/run/live/medium/live/filesystem.squashfs').is_file()
assert os.statvfs('/run/live/medium').f_flag & os.ST_RDONLY
base=tomllib.loads(P('/etc/greetd/config.toml').read_text())
assert 'initial_session' not in base
runtime=tomllib.loads(P('/run/wros-live/greetd.toml').read_text())
assert ('initial_session' in runtime)==marker['automatic']
assert subprocess.check_output(['passwd','-S','root']).split()[1]==b'L'
assert P('/etc/systemd/system-generators/live-config-getty-generator').is_symlink()
assert os.readlink('/etc/systemd/system-generators/live-config-getty-generator')=='/dev/null'
processes={}
for p in P('/proc').glob('[0-9]*'):
    try:
        name=(p/'comm').read_text().strip()
        if name not in ('grimoire','wros-input','wros-greeter','agetty'): continue
        status=(p/'status').read_text().splitlines()
        uid=next(s.split()[1:] for s in status if s.startswith('Uid:'))
        cmd=(p/'cmdline').read_bytes().split(b'\0')
        if name=='agetty': assert not any(b'autologin' in s for s in cmd)
        processes.setdefault(name,[]).append({'pid':int(p.name),'uid':uid,
          'exe_sha256':hashlib.sha256((p/'exe').read_bytes()).hexdigest()})
    except (FileNotFoundError,ProcessLookupError): pass
screen=P('/dev/vcs1').read_bytes().decode('latin1')
phase=sys.argv[1]
if phase=='manual':
    assert not marker['automatic'] and not processes.get('grimoire')
    assert len(processes.get('wros-greeter',[]))==1
    assert processes['wros-greeter'][0]['uid'][0]!='0'
    assert 'Local account' in screen and 'WrOS' in screen
else:
    for name in ('grimoire','wros-input'):
        assert len(processes.get(name,[]))==1
        assert processes[name][0]['uid']==['1000']*4
    if phase=='home': assert 'WrOS' in screen and 'Live session' in screen and 'Resume writing' in screen
    if phase=='power': assert 'Live session' in screen and 'personal lock unavailable' in screen
print(sys.argv[2]+json.dumps({'phase':phase,'marker':marker,'processes':processes,
 'screen':screen,'shipping_initial_session_absent':True,'root_locked':True,
 'other_gettys_authenticated':True}),flush=True)
'''


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('iso', 'kernel', 'initrd', 'artifacts-directory'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--manual', action='store_true')
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise SystemExit('Run the diskless QEMU fixture as a normal user')
    for path in (args.iso, args.kernel, args.initrd):
        if path.is_symlink() or not path.is_file():
            raise SystemExit('Only existing regular image artifacts are accepted')
    args.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='live-', dir=args.artifacts_directory)); run.chmod(0o700)
    (run/'DISPOSABLE_DISKLESS_FIXTURE').write_text('No network interface or writable disk.\n')
    source = fingerprint()
    result = {'passed': False, 'source_fingerprint': source, 'manual_requested': args.manual,
              'iso_sha256_before': digest(args.iso), 'network_devices': 0,
              'host_disks_exposed': 0, 'writable_disks': 0, 'phases': [],
              'probe_sha256': hashlib.sha256(PROBE.encode()).hexdigest(),
              'scope': 'Diskless live startup and native UI only; no installation or hardware qualification'}
    # Prove direct-boot artifacts are the bytes in this actual ISO.
    for name, original in (('kernel', args.kernel), ('initrd', args.initrd)):
        extracted = run/name
        subprocess.run(['xorriso', '-osirrox', 'on', '-indev', str(args.iso),
                        '-extract', '/live/'+original.name, str(extracted)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if digest(extracted) != digest(original):
            raise SystemExit('Live boot artifact differs from ISO')
        result[name+'_sha256'] = digest(extracted)
    serial, control = run/'serial.sock', run/'qmp.sock'
    append = 'boot=live components wros.live=1 console=ttyS0,115200 console=tty1 systemd.show_status=no panic=1'
    if args.manual:
        append += ' live-config.noautologin'
    command = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35',
               '-cpu', 'Nehalem', '-m', '2048', '-smp', '2', '-nic', 'none',
               '-display', 'none', '-no-reboot', '-kernel', str(run/'kernel'),
               '-initrd', str(run/'initrd'), '-append', append,
               '-drive', 'if=ide,media=cdrom,format=raw,readonly=on,file='+str(args.iso.resolve()),
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
                    raise RuntimeError('Live guest serial endpoint did not start')
                time.sleep(0.1)
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(0.5); connection.connect(str(serial))

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
                        if not chunk:
                            break
                        content.extend(chunk); capture.write(chunk); capture.flush()
                    raise RuntimeError('Live guest did not reach the expected fixture state')

                def keys(values):
                    for key in values:
                        qmp(control, 'send-key', {'keys':[{'type':'qcode','data':key}], 'hold-time':40})
                        time.sleep(0.12)

                def chord(*values):
                    qmp(control, 'send-key', {'keys':[{'type':'qcode','data':v} for v in values], 'hold-time':40})
                    time.sleep(0.2)

                def probe(phase):
                    nonce = 'WROS_LIVE_'+secrets.token_hex(12)+'='
                    invocation = 'sudo -n /usr/bin/python3 -I /tmp/wros-live-fixture.py '+phase+' '+nonce+'\n'
                    connection.sendall(invocation.encode())
                    match = expect(rb'(?:^|[\r\n])'+nonce.encode()+rb'([^\r\n]+)[\r\n]', 45)
                    reply = json.loads(match.group(1)); result['phases'].append(reply)
                    return reply

                expect(rb'login:')
                connection.sendall(b'wros-live\n'); expect(rb'Password:')
                # Debian's standard public live credential; never a personal secret.
                connection.sendall(b'live\n'); expect(rb':~\$ ', 45)
                encoded = base64.b64encode(PROBE.encode()).decode()
                upload = "umask 077; base64 -d > /tmp/wros-live-fixture.py <<'WROS_FIXTURE_END'\n"
                upload += '\n'.join(encoded[i:i+512] for i in range(0,len(encoded),512))
                upload += '\nWROS_FIXTURE_END\n'
                connection.sendall(upload.encode()); time.sleep(0.5)
                if args.manual:
                    probe('manual')
                    qmp(control, 'screendump', {'filename':str(run/'manual-login.ppm')})
                    keys(['w','r','o','s','minus','l','i','v','e','ret'])
                    time.sleep(0.5); keys(['l','i','v','e','ret']); time.sleep(3)
                first = probe('session')
                if 'Live session' not in first['screen'] or 'Resume writing' not in first['screen']:
                    keys(['f12']); time.sleep(1)
                probe('home')
                qmp(control, 'screendump', {'filename':str(run/'live-home.ppm')})
                keys(['esc']); chord('ctrl','p')
                keys(['w','r','o','s','spc','s','e','t','t','i','n','g','s','ret'])
                time.sleep(1); keys(['home','down','down','ret']); time.sleep(0.5)
                keys(['home','down','down','ret']); time.sleep(1)
                probe('power')
                qmp(control, 'screendump', {'filename':str(run/'live-power.ppm')})
                connection.sendall(b'sudo -n /sbin/poweroff\n')
                process.wait(timeout=60)
                result['passed'] = process.returncode == 0
        except Exception as error:
            result['error'] = type(error).__name__+': '+str(error)
            if control.exists():
                try:
                    qmp(control, 'screendump', {'filename':str(run/'failed-screen.ppm')})
                except (OSError, RuntimeError):
                    pass
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait()
            result['iso_sha256_after'] = digest(args.iso)
            result['iso_unchanged'] = result['iso_sha256_after'] == result['iso_sha256_before']
            result['source_unchanged'] = source == fingerprint()
            result['passed'] &= result['iso_unchanged'] and result['source_unchanged']
            result['seconds'] = round(time.monotonic()-started, 2)
            (run/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='phases'},indent=2))
    print('Private virtual artifacts: '+str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
