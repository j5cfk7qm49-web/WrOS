#!/usr/bin/env python3
"""Exercise Debian's real installer menus in a no-NIC WrOS guest.

The default is diskless. Review mode creates its own target and requires every
byte to stay unchanged. A separate explicit install mode may write only to that
new marked target, then requires encrypted UEFI boot and native authentication.
The read-only QA CD contains a verified capsule and maintained Debian launcher;
none of this grants a production launcher in the live image.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time

from installed_boot import qmp, verify as verify_boot
from fixture_storage import TARGET_BYTES, fixture_serial

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
from writingdeck_source import fingerprint


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


SNAPSHOT = r'''
import hashlib,importlib.util,json,os,pathlib,re,select,subprocess,sys,time
P=pathlib.Path
seconds=int(sys.argv[4]) if len(sys.argv)>4 else 45
assert 10<=seconds<=1800
deadline=time.monotonic()+seconds
needle=sys.argv[1]
previous=sys.argv[3] if len(sys.argv)>3 else ''
installing=len(sys.argv)>5 and sys.argv[5]=='install'
diagnostic_at=time.monotonic()+180
while True:
    status=P('/run/wros-native-probe/status.json')
    record=json.loads(status.read_bytes()) if status.exists() else {}
    screen=P('/dev/vcs7').read_bytes().decode('latin1') if P('/dev/vcs7').exists() else ''
    screen_hash=hashlib.sha256(screen.encode('latin1')).hexdigest()
    if installing and record.get('target_configuration_verified') and record.get('target_mounts_retired'):break
    if needle in screen and 'Enter continue' in screen and record.get('ready') and screen_hash!=previous: break
    assert subprocess.run(['systemctl','is-active','--quiet','wros-native-probe.service']).returncode==0,'Backend service stopped'
    assert time.monotonic()<deadline,'Installer UI readiness'
    if installing and time.monotonic()>=diagnostic_at:
        diagnostic=subprocess.run(['/usr/bin/python3','-I','/run/wros-native-qa/native_installer_guest.py','inspect'],
            stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=45)
        print('WROS_NATIVE_PROGRESS_DIAGNOSTIC\n'+diagnostic.stdout[-20000:].decode(errors='replace'),flush=True)
        diagnostic_at=time.monotonic()+180
    time.sleep(.2)
processes=[]
for p in P('/proc').glob('[0-9]*'):
    try:
        name=(p/'comm').read_text().strip()
        if name not in ('wros-installer','main-menu','grimoire','wros-input','wros-greeter'):continue
        descriptor=os.pidfd_open(int(p.name))
        try:
            if select.select([descriptor],[],[],0)[0]:continue
        finally:os.close(descriptor)
        uid=next(x.split()[1:] for x in (p/'status').read_text().splitlines() if x.startswith('Uid:'))
        processes.append({'name':name,'pid':int(p.name),'uids':uid,'cgroup':(p/'cgroup').read_text()})
    except (FileNotFoundError,ProcessLookupError):pass
front=[p for p in processes if p['name']=='wros-installer']
completed=installing and record.get('target_configuration_verified') and record.get('target_mounts_retired')
assert len(front)<=1 if completed else len(front)==1
assert all(p['uids']==['1000']*4 and 'wros-native-probe.service' in p['cgroup'] for p in front)
assert not any(p['name'] in ('grimoire','wros-input','wros-greeter') for p in processes)
spec=importlib.util.spec_from_file_location('fixture_storage',P('/run/wros-native-qa/fixture_storage.py'))
storage=importlib.util.module_from_spec(spec);spec.loader.exec_module(storage)
identity=record.get('fixture_target')
assert all((p/'ro').read_text().strip()=='1' or (re.fullmatch(r'loop[0-9]+',p.name)
    and (p/'size').read_text().strip()=='0' and not (p/'loop/backing_file').exists())
    or (identity is not None and storage.belongs_to_target(p.name,identity)) for p in P('/sys/class/block').iterdir())
assert 'wros.native_probe=' in P('/proc/cmdline').read_text(),'Private sanitized cmdline escaped'
assert os.uname().nodename=='debian','Backend hostname change escaped its UTS namespace'
assert not P('/run/live/installer/usr/bin/main-menu').exists(),'Private backend mount escaped'
record.update({'screen':screen,'screen_sha256':screen_hash,'processes':processes,'outside_namespace_unchanged':True,'completion_observed':bool(completed)})
print(sys.argv[2]+json.dumps(record),flush=True)
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('iso', 'kernel', 'initrd', 'capsule', 'launcher', 'artifacts-directory'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--synthetic-target', action='store_true',
                        help='Create the fixed new virtual disk and stop before any partitioning question')
    parser.add_argument('--partition-review', action='store_true',
                        help='With the fresh target only, decline the first disk-write confirmation')
    parser.add_argument('--install-target', action='store_true',
                        help='Explicitly install only to this fixture\'s new marked virtual disk; no host disk argument exists')
    parser.add_argument('--frontend', type=Path,
                        help='Stage a newly compiled non-root frontend only inside this disposable service namespace')
    args = parser.parse_args()
    if args.partition_review and not args.synthetic_target:
        raise SystemExit('Partition review requires the freshly generated synthetic target')
    if args.install_target and (not args.synthetic_target or args.partition_review):
        raise SystemExit('Full installation requires the new synthetic target and a separate explicit mode')
    if os.geteuid() == 0:
        raise SystemExit('Run the QEMU fixture as a normal user')
    for path in (args.iso, args.kernel, args.initrd):
        if not path.is_file() or path.is_symlink():
            raise SystemExit('Only regular image files are accepted')
    if args.frontend and (not args.frontend.is_file() or args.frontend.is_symlink()):
        raise SystemExit('Only a regular compiled fixture frontend is accepted')
    for path in (args.capsule, args.launcher):
        if not path.is_dir() or path.is_symlink():
            raise SystemExit('Only regular prepared input directories are accepted')
    args.artifacts_directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='native-', dir=args.artifacts_directory)); run.chmod(0o700)
    token = secrets.token_hex(16)
    crypt_secret = 'wros-qa-'+secrets.token_hex(16) if args.install_target else None
    login_secret = 'wros-user-'+secrets.token_hex(16) if args.install_target else None
    if args.install_target:
        credential = run/'.fixture-secrets'
        credential.write_text(json.dumps({'crypt':crypt_secret, 'login':login_secret}))
        credential.chmod(0o600)
        shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.fd', run/'OVMF_VARS.fd')
    (run/'fixture-token').write_text(token+'\n')
    (run/'snapshot.py').write_text(SNAPSHOT)
    target = None
    if args.synthetic_target:
        target = run/'target.raw'
        with target.open('xb') as stream:
            stream.truncate(TARGET_BYTES)
            stream.seek(TARGET_BYTES-4096)
            stream.write(b'WrOS generated target sentinel; no user data.\n')
        target.chmod(0o600)
        (run/'fixture-target.json').write_text(json.dumps({'bytes':TARGET_BYTES,'serial':fixture_serial(token)})+'\n')
    source = fingerprint()
    capsule_hash = digest(args.capsule/'manifest.json')
    result = {'passed': False, 'source_fingerprint': source, 'iso_sha256_before': digest(args.iso),
              'capsule_manifest_sha256': capsule_hash, 'network_devices': 0,
              'host_disks_exposed': 0, 'writable_disks': 1 if target else 0, 'phases': [],
              'scope': 'Real Debian startup/menu and containment only; no disk-write consent, target installation or hardware qualification.'}
    if args.install_target:
        result['scope'] = 'Native encrypted installation into the fixed fresh virtual target; authenticated target boot and production completion UI remain separate.'
        result['firmware_variables_created_by_fixture'] = True
        result['firmware_variables_sha256_before'] = digest(run/'OVMF_VARS.fd')
    if target:
        result.update({'target_created_by_fixture':True,'target_bytes':TARGET_BYTES,'target_sha256_before':digest(target)})
    if args.partition_review or args.install_target:
        # No format, overwrite, LVM/crypto or new-label confirmations are seeded.
        # The only target is the newly generated and nonce-verified vda above.
        (run/'review.preseed').write_text('''# Private negative-answer fixture only; never shipping defaults.
d-i anna/choose_modules seen true
d-i keyboard-configuration/xkb-keymap select us
d-i netcfg/enable boolean false
d-i ethdetect/module_select select no ethernet card
d-i netcfg/get_hostname string wros-native-review
d-i netcfg/get_domain string invalid
d-i clock-setup/utc boolean true
d-i clock-setup/ntp boolean false
d-i time/zone string Etc/UTC
d-i passwd/user-fullname string Disposable target review
d-i passwd/username string fixture
d-i passwd/user-password-crypted password !
d-i partman-auto/disk string /dev/vda
d-i partman-auto/method string crypto
d-i partman-auto/choose_recipe select atomic
d-i partman-auto-lvm/guided_size string max
''')
        if args.install_target:
            password_hash = subprocess.check_output(['openssl','passwd','-6','-stdin'],
                input=login_secret+'\n', text=True).strip()
            defaults = (run/'review.preseed').read_text().replace(
                'd-i passwd/user-password-crypted password !',
                'd-i passwd/user-password-crypted password '+password_hash)
            defaults += '''d-i apt-setup/use_mirror boolean false
d-i apt-setup/services-select multiselect
d-i pkgsel/upgrade select none
tasksel tasksel/first multiselect
popularity-contest popularity-contest/participate boolean false
d-i partman-auto-crypto/erase_disks boolean false
d-i cdrom-detect/eject boolean false
d-i debian-installer/add-kernel-opts string console=tty1 console=ttyS0,115200
'''
            (run/'review.preseed').write_text(defaults)
            result['encrypted_install_requested'] = True
        else:
            result['partition_review_requested'] = True
    if args.frontend:
        result['frontend_override_sha256'] = digest(args.frontend)
        (run/'frontend.sha256').write_text(result['frontend_override_sha256']+'\n')
    launcher_files = ['plugins/live', 'debconf_kernel.sh', 'debian/copyright']
    result['maintained_launcher_file_sha256'] = {name: digest(args.launcher/name) for name in launcher_files}
    payload = run/'qa.iso'
    command = ['xorriso', '-outdev', str(payload), '-volid', 'WROS_NATIVE_QA',
               '-map', str(args.capsule), '/native-installer',
               '-map', str(run/'fixture-token'), '/fixture-token',
               '-map', str(run/'snapshot.py'), '/snapshot.py',
               '-map', str(ROOT/'os/tests/fixture_storage.py'), '/fixture_storage.py',
               '-map', str(ROOT/'os/config/installer/native-crypto-passphrase'), '/native-crypto-passphrase',
               '-map', str(ROOT/'os/tests/native_installer_guest.py'), '/native_installer_guest.py']
    if target:
        command += ['-map', str(run/'fixture-target.json'), '/fixture-target.json']
    if args.partition_review or args.install_target:
        command += ['-map', str(run/'review.preseed'), '/review.preseed',
                    '-map', str(ROOT/'os/config/installer.preseed'), '/installer.preseed']
    if args.install_target:
        command += ['-map', str(ROOT/'os/tests/native_target_audit.py'), '/native_target_audit.py']
    if args.frontend:
        command += ['-map', str(args.frontend.resolve()), '/wros-installer',
                    '-map', str(run/'frontend.sha256'), '/frontend.sha256']
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
    if target:
        command += ['-drive', 'if=none,id=wros-target,format=raw,file='+str(target),
                    '-device', 'virtio-blk-pci,drive=wros-target,serial='+fixture_serial(token)]
    if args.install_target:
        command += ['-drive','if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
                    '-drive','if=pflash,format=raw,file='+str(run/'OVMF_VARS.fd')]
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
                        connection.sendall(b'sudo -n journalctl -u wros-native-probe.service --no-pager -n 30; sudo -n tail -c 12000 /run/wros-native-probe/prepare.log /run/wros-native-probe/backend.log; sudo -n python3 -I /run/wros-native-qa/native_installer_guest.py inspect; printf "\\nWROS_NATIVE_DIAGNOSTIC_END\\n"\n')
                        expect(rb'(?:^|[\r\n])WROS_NATIVE_DIAGNOSTIC_END[\r\n]', 60)
                        raise RuntimeError('Guest fixture command failed')
                    output = bytes(content[start:cursor])
                    expect(rb':~\$ ', 30)
                    return output

                def snapshot(needle, name, previous='', seconds=45):
                    marker = 'WROS_SNAPSHOT_'+secrets.token_hex(12)+'='
                    output = shell('sudo -n /usr/bin/python3 -I /run/wros-native-qa/snapshot.py '
                                   +"'"+needle+"' "+marker+" '"+previous+"' "+str(seconds)
                                   +(' install' if args.install_target else ''), seconds+30)
                    matched = re.search(rb'(?:^|[\r\n])'+marker.encode()+rb'([^\r\n]+)', output)
                    if matched is None: raise RuntimeError('Native snapshot missing')
                    observation = json.loads(matched.group(1))
                    if observation.get('media_nonrecursive_bind_verified') is not True:
                        raise RuntimeError('Debian media rebind was not verified before native input')
                    result['media_nonrecursive_bind_verified'] = True
                    if args.frontend and observation['frontend_sha256'] != result['frontend_override_sha256']:
                        raise RuntimeError('Observed fixture frontend differs from the compiled input')
                    result['phases'].append({'phase':name, **observation})
                    qmp(control, 'screendump', {'filename':str(run/(name+'.ppm'))})
                    return observation

                def key(name):
                    qmp(control, 'send-key', {'keys':[{'type':'qcode','data':name}], 'hold-time':40})
                    time.sleep(.15)

                expect(rb'login:'); connection.sendall(b'wros-live\n'); expect(rb'Password:')
                connection.sendall(b'live\n'); expect(rb':~\$ ', 45)
                shell('sudo -n mkdir /run/wros-native-qa && sudo -n mount -o ro,nosuid,nodev,noexec /dev/disk/by-label/WROS_NATIVE_QA /run/wros-native-qa')
                shell('sudo -n /usr/bin/python3 -I /run/wros-native-qa/native_installer_guest.py '
                      + ('start-install' if args.install_target else 'start-review' if args.partition_review else 'start-target' if target else 'start'))
                snapshot('Select a language', 'language')
                key('ret')
                observation = snapshot('Select your location', 'location')
                if args.install_target:
                    key('ret')
                    deadline = time.monotonic()+3500
                    approvals = []
                    for step in range(80):
                        remaining = int(deadline-time.monotonic())
                        if remaining < 10:
                            raise RuntimeError('Encrypted installation exceeded its bounded deadline')
                        observation = snapshot('', 'install-'+str(step), observation['screen_sha256'],
                                               min(600 if step == 0 else 1800, remaining))
                        if observation.get('completion_observed'):
                            proof = observation['target_proof']
                            image = json.loads((ROOT/'docs/evidence/encrypted-media-image-build.json').read_bytes())
                            if image['iso_sha256'] != result['iso_sha256_before']:
                                raise RuntimeError('Target binary expectations differ from the actual image')
                            expected = {name: row['staged_sha256'] for name,row in image['application_inputs']['applications'].items()}
                            if proof['binary_sha256'] != expected:
                                raise RuntimeError('Installed application bytes differ from the image')
                            result.update({'target_configuration_verified':True,
                                           'installed_binaries_match_image':True,'target_proof':proof,
                                           'explicit_fresh_target_approvals':approvals})
                            break
                        if observation['fixture_target']['serial'] != fixture_serial(token):
                            raise RuntimeError('Fixture target changed before input')
                        screen = observation['screen']
                        selected_no = re.search(r'\x1a No(?:\s|\xb3|$)', screen) is not None
                        write_prompts = ('Write the changes to disks and configure LVM?',
                                         'Write the changes to disk and configure encrypted volumes?',
                                         'Keep current partition layout and configure LVM?',
                                         'Keep current partition layout and configure encrypted volumes?',
                                         'Write the changes to disks?')
                        prompt = next((value for value in write_prompts if value in screen), None)
                        if prompt and selected_no and 'Yes' in screen and ('vda' in screen or 'LVM' in screen or 'encrypted volumes' in prompt):
                            approvals.append(prompt)
                            key('down'); key('ret')
                        elif 'Encryption passphrase:' in screen or 'Re-enter passphrase to verify:' in screen:
                            if '[hidden]' not in screen:
                                raise RuntimeError('Encryption credential field is not hidden')
                            for char in crypt_secret:
                                key('minus' if char == '-' else char)
                            key('ret')
                        elif ('Create new empty partition table on this device?' in screen
                                and 'undo this operation later' in screen and selected_no and 'Yes' in screen):
                            key('down'); key('ret')
                        elif ('This is an overview of your currently configured partitions' in screen
                                and 'vda' in screen and 'Finish partitioning and write changes to disk' in screen):
                            for _ in range(48):key('down')
                            choice = snapshot('Finish partitioning and write changes to disk', 'partition-finish-'+str(step))
                            if '\x1a Finish partitioning and write changes to disk' not in choice['screen']:
                                raise RuntimeError('Partition finish selection differs')
                            key('ret')
                        elif ('Install the GRUB boot loader to your primary drive?' in screen
                                and 'only operating system' in screen and selected_no):
                            approvals.append('Install GRUB on the only fresh target')
                            key('down'); key('ret')
                        elif ('Update NVRAM variables to automatically boot into Debian?' in screen
                                and selected_no and 'Yes' in screen
                                and result['firmware_variables_created_by_fixture']):
                            approvals.append('Configure only this fixture\'s fresh OVMF variables')
                            key('down'); key('ret')
                        elif ('Use a network mirror?' in screen or 'Force GRUB installation to the EFI removable media path?' in screen) and selected_no:
                            key('ret')
                        elif any(value in screen.lower() for value in ('installer components to load', 'keymap to use:')):
                            key('ret')
                        elif 'Driver needed by your Ethernet card:' in screen and '\x1a no ethernet card' in screen:
                            key('ret')
                        elif 'Ethernet card not found' in screen and 'No Ethernet card was found on the system.' in screen:
                            key('ret')
                        else:
                            shell('sudo -n python3 -I /run/wros-native-qa/native_installer_guest.py inspect')
                            raise RuntimeError('Unrecognized full-install form; no answer sent')
                    else:
                        raise RuntimeError('Verified encrypted target finish was not observed')
                if args.partition_review:
                    key('ret')
                    deadline = time.monotonic()+900
                    for step in range(40):
                        if time.monotonic()>deadline:
                            raise RuntimeError('Partition review exceeded its bounded deadline')
                        # The first transition loads the authenticated offline
                        # udebs and probes hardware under software emulation.
                        # Its measured progress exceeds three minutes; retain
                        # the overall review deadline and never answer while
                        # the screen is only a progress indicator.
                        remaining = int(deadline-time.monotonic())
                        wait = min(600 if step == 0 else 180, remaining)
                        if wait < 10:
                            raise RuntimeError('Partition review exceeded its bounded deadline')
                        observation = snapshot('', 'review-'+str(step), observation['screen_sha256'], wait)
                        screen = observation['screen']
                        selected_no = re.search(r'\x1a No(?:\s|\xb3|$)', screen) is not None
                        if 'Write the changes to disk' in screen and selected_no and 'Yes' in screen:
                            if 'vda' not in screen:
                                raise RuntimeError('Write review did not identify the generated target')
                            result['write_confirmation_seen'] = True
                            result['confirmation_default_no_verified'] = True
                            key('ret')
                            snapshot('', 'write-declined', observation['screen_sha256'], 180)
                            result['negative_answer_sent'] = True
                            break
                        if ('Create new empty partition table on this device?' in screen
                                and 'undo this operation later' in screen and selected_no and 'Yes' in screen):
                            key('down'); key('ret')
                            result['in_memory_partition_draft_acknowledged'] = True
                        elif any(value in screen.lower() for value in ('installer components to load', 'keymap to use:')):
                            key('ret')
                        elif ('Driver needed by your Ethernet card:' in screen
                                and '\x1a no ethernet card' in screen):
                            key('ret')
                        elif 'Ethernet card not found' in screen and 'No Ethernet card was found on the system.' in screen:
                            key('ret')
                        else:
                            raise RuntimeError('Unrecognized pre-confirmation form; no answer sent')
                    else:
                        raise RuntimeError('No disk-write confirmation was observed')
                # Stop the standard service, proving that its frontend and
                # backend leave together, before restarting authenticated entry.
                if args.install_target:
                    shell('if sudo -n systemctl is-active --quiet wros-native-probe.service; then sudo -n systemctl stop wros-native-probe.service; fi')
                else:
                    shell('sudo -n systemctl stop wros-native-probe.service')
                cleanup = r'''import json,pathlib,subprocess; P=pathlib.Path; names=[(p/'comm').read_text().strip() for p in P('/proc').glob('[0-9]*') if (p/'comm').exists()]; assert not any(n in names for n in ('wros-installer','main-menu')); assert not P('/run/live/installer/usr/bin/main-menu').exists(); assert 'wros.native_probe=' in P('/proc/cmdline').read_text(); print('WROS_NATIVE_CLEANUP_PASSED')'''
                shell('sudo -n python3 -I -c '+"'"+cleanup.replace("'", "'\"'\"'")+"'")
                result['finish_retired_both_processes' if args.install_target else 'cgroup_stop_retired_both_processes'] = True
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
                if args.install_target and (crypt_secret.encode() in content or login_secret.encode() in content):
                    raise RuntimeError('Generated fixture credential appeared in serial output')
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
            if args.install_target:
                result['firmware_variables_sha256_after_install'] = digest(run/'OVMF_VARS.fd')
            result['passed'] &= result['iso_unchanged'] and result['source_unchanged'] and result['capsule_unchanged']
            if args.frontend:
                result['compiled_frontend_unchanged'] = digest(args.frontend) == result['frontend_override_sha256']
                result['passed'] &= result['compiled_frontend_unchanged']
            if target:
                result['target_sha256_after'] = digest(target)
                result['target_unchanged'] = result['target_sha256_before'] == result['target_sha256_after']
                if args.install_target:
                    result['target_changed'] = not result['target_unchanged']
                    result['passed'] &= result['target_changed'] and result.get('target_configuration_verified',False)
                else:
                    result['passed'] &= result['target_unchanged']
            result['seconds'] = round(time.monotonic()-started, 2)
            (run/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    if args.install_target and result['passed']:
        result['backend_installation_verified'] = True
        result['passed'] = False
        boot_started = time.monotonic()
        try:
            subprocess.run(['qemu-img','convert','-f','raw','-O','qcow2',str(target),str(run/'target.qcow2')],
                           check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
            subprocess.run(['qemu-img','compare','-f','raw','-F','qcow2',str(target),str(run/'target.qcow2')],
                           check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
            shutil.copyfile(run/'review.preseed',run/'preseed.cfg')
            boot = verify_boot(run, secret=login_secret, native_session=True, unlock_secret=crypt_secret)
            result['installed_boot'] = boot
            required = ('installed_boot_verified','encrypted_root_unlock_verified','authentication_verified',
                        'encrypted_wrong_key_rejected',
                        'tty1_nonroot_session_verified','writing_ui_verified','offline_wifi_packages_verified',
                        'native_login_verified','native_wrong_password_rejected','sleep_guard_verified',
                        'native_logout_verified','input_reader_nonroot_verified')
            expected = {name: result['target_proof']['binary_sha256'][name] for name in ('grimoire','wros-greeter')}
            identities = (boot.get('running_image_binary_sha256') == expected
                          and boot.get('running_editor_exe_sha256') == expected['grimoire'])
            result['running_target_binaries_match_proof'] = identities
            result['passed'] = all(boot.get(name) for name in required) and identities and not boot['password_logged']
            if result['passed']:
                (run/'.fixture-secrets').unlink()
        except Exception as error:
            result['boot_error'] = type(error).__name__+': '+str(error)
        finally:
            result['boot_seconds'] = round(time.monotonic()-boot_started,2)
            result['source_unchanged'] &= source == fingerprint()
            result['passed'] &= result['source_unchanged']
            (run/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='phases'},indent=2))
    print('Private disposable artifacts: '+str(run))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
