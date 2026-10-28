"""Actual UEFI/PAM/tty1 acceptance for a disposable installer-created disk."""
import json
import os
import re
from pathlib import Path
import socket
import subprocess
import time


def qmp(path, command, arguments=None):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(10)
        connection.connect(str(path))
        stream = connection.makefile('rwb', buffering=0)
        json.loads(stream.readline())
        for message in ({'execute': 'qmp_capabilities'},
                        {'execute': command, 'arguments': arguments or {}}):
            stream.write(json.dumps(message).encode() + b'\n')
            while True:
                reply = json.loads(stream.readline())
                if 'error' in reply:
                    raise RuntimeError(str(reply['error']))
                if 'return' in reply:
                    break


def verify(run, secret=None, native_session=False):
    if os.geteuid() == 0:
        raise RuntimeError('Run QEMU acceptance as a non-root builder')
    run = Path(run)
    disk = run / 'target.qcow2'
    if disk.is_symlink() or not disk.is_file() or not (run / 'preseed.cfg').is_file():
        raise RuntimeError('Only an existing disposable install fixture is accepted')
    serial_path, control = run / 'boot-serial.sock', run / 'boot-qmp.sock'
    for path in (serial_path, control):
        path.unlink(missing_ok=True)
    args = ['qemu-system-x86_64', '-accel', 'tcg,thread=multi', '-machine', 'q35', '-cpu', 'Nehalem',
            '-m', '1024', '-smp', '2', '-nic', 'none', '-display', 'none', '-no-reboot',
            '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
            '-drive', 'if=pflash,format=raw,file=' + str(run / 'OVMF_VARS.fd'),
            '-drive', 'if=virtio,format=qcow2,file=' + str(disk),
            '-serial', 'unix:' + str(serial_path) + ',server=on,wait=on',
            '-qmp', 'unix:' + str(control) + ',server=on,wait=off']
    result = {'installed_boot_verified': False, 'authentication_verified': False,
              'tty1_nonroot_session_verified': False, 'writing_ui_verified': False,
              'offline_wifi_packages_verified': False, 'password_logged': False,
              'native_session_requested': native_session, 'native_login_verified': False,
              'native_wrong_password_rejected': False, 'sleep_guard_verified': False,
              'native_logout_verified': False, 'input_reader_nonroot_verified': False}
    content = bytearray()
    with (run / 'boot-qemu.log').open('w') as log, (run / 'boot-serial.log').open('wb') as capture:
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while not serial_path.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Installed boot serial endpoint did not start')
                time.sleep(0.1)
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(0.5)
                connection.connect(str(serial_path))

                cursor = 0

                def expect(needle, timeout=180):
                    nonlocal cursor
                    needles = (needle,) if isinstance(needle, bytes) else needle
                    deadline = time.monotonic() + timeout
                    while time.monotonic() < deadline:
                        matches = [(found, len(value)) for value in needles
                                   if (found := content.find(value, cursor)) >= 0]
                        if matches:
                            found, length = min(matches)
                            cursor = found + length
                            return
                        try:
                            data = connection.recv(65536)
                        except socket.timeout:
                            continue
                        if not data:
                            break
                        content.extend(data)
                        capture.write(data)
                        capture.flush()
                    raise RuntimeError('Installed boot did not reach expected authentication/session state')

                expect(b'login:')
                result['installed_boot_verified'] = True
                qmp(control, 'screendump', {'filename': str(run / 'login-prompt.ppm')})
                if secret is None:
                    return result

                connection.sendall(b'fixture\n')
                expect(b'Password:')
                connection.sendall(b'wrong-fixture-password\n')
                expect(b'Login incorrect')
                expect(b'login:')
                connection.sendall(b'fixture\n')
                expect(b'Password:')
                connection.sendall(secret.encode() + b'\n')
                expect(b':~$ ')
                connection.sendall(b'printf "\\nWROS_AUTH_UID="; id -u; printf "\\nWROS_NO_PASSWORD_ADMIN="; sudo -n true >/dev/null 2>&1; printf "%s\\n" "$?"; printf "\\nWROS_AUTH_CHECK_DONE\\n"\n')
                expect(b'\r\nWROS_AUTH_CHECK_DONE\r\n', 30)
                if b'WROS_AUTH_UID=1000' not in content or b'WROS_NO_PASSWORD_ADMIN=1' not in content:
                    raise RuntimeError('Installed account is root or has a passwordless admin bypass')
                result['authentication_verified'] = True
                expect(b':~$ ', 20)

                connection.sendall(b'sudo -p WROS_ADMIN_PROMPT: true\n')
                # Bash may disable bracketed paste and return to column zero
                # with a lone CR. Require the prompt at the start of the line,
                # so its occurrence in the echoed sudo command cannot match.
                expect((b'\r\nWROS_ADMIN_PROMPT:', b'\rWROS_ADMIN_PROMPT:'), 20)
                connection.sendall(secret.encode() + b'\n')
                expect(b':~$ ', 20)

                def keys(text):
                    for char in text:
                        qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'ret' if char == '\n' else char}], 'hold-time': 30})
                        time.sleep(0.06)

                def console_until(predicate, description, timeout=45):
                    deadline = time.monotonic() + timeout
                    while True:
                        start = len(content)
                        connection.sendall(b'sudo -n /bin/sh -c \'cat /dev/vcs1; stty -F /dev/tty1 -a; printf "\\nWROS_CONSOLE_STATE_END\\n"\'\n')
                        expect(b'\r\nWROS_CONSOLE_STATE_END\r\n', 15)
                        console = content[start:]
                        expect(b':~$ ', 20)
                        if predicate(console):
                            return console
                        if time.monotonic() > deadline:
                            raise RuntimeError(description)
                        time.sleep(0.5)

                # The serial login remains the authenticated maintenance shell.
                # tty1 uses only the installed stack, not a test-injected service.
                if native_session:
                    console_until(lambda c: b'Local account' in c and b'WrOS' in c,
                                  'Installed native greeter did not start automatically')
                    keys('fixture\n')
                    console_until(lambda c: b'[hidden]' in c and b'-echo' in c.split(),
                                  'Native PAM prompt is not hidden')
                    keys('badfixture\n')
                    refused = console_until(lambda c: b'Login was not accepted' in c,
                                            'Native PAM did not reject incorrect password')
                    result['native_wrong_password_rejected'] = True
                    if b'Enter to restart login' in refused:
                        keys('\n')
                    console_until(lambda c: b'Local account' in c,
                                  'Native login did not recover from rejected authentication')
                keys('fixture\n')
                # Inspect the console and echo flag before any password input.
                console_until(lambda c: (b'[hidden]' if native_session else b'Password:') in c
                              and b'-echo' in c.split(), 'tty1 PAM prompt is not non-echoing')
                keys(secret)
                if native_session:
                    console_until(lambda c: b'[hidden]' in c and secret.encode() not in c,
                                  'Native login exposed the typed password')
                keys('\n')
                if native_session:
                    console_until(lambda c: b'Open writing session' in c and b'Authenticated.' in c,
                                  'Installed native PAM did not authenticate')
                    result['native_login_verified'] = True
                    keys('\n')
                deadline = time.monotonic() + 45
                while True:
                    start = len(content)
                    # A logger marker is telemetry, not authoritative process
                    # ownership; a fresh installed journal may omit it. Require
                    # the actual writer process/TTY and rendered UI instead.
                    connection.sendall(b'if test "$(ps -C grimoire -o uid= | xargs)" = 1000 && test "$(ps -C grimoire -o tty= | xargs)" = tty1; then echo WROS_WRITER_PROCESS_PASS; fi; printf "\\nWROS_WRITER_STATE_END\\n"\n')
                    expect(b'\r\nWROS_WRITER_STATE_END\r\n', 20)
                    expect(b':~$ ', 20)
                    process_verified = b'\r\nWROS_WRITER_PROCESS_PASS\r\n' in content[start:]
                    connection.sendall(b'sudo -n /bin/sh -c \'cat /dev/vcs1; printf "\\nWROS_UI_STATE_END\\n"\'\n')
                    expect(b'\r\nWROS_UI_STATE_END\r\n', 10)
                    console = content[start:]
                    expect(b':~$ ', 20)
                    if process_verified and b'WRITINGDECK OS' in console and b'Resume writing' in console:
                        result['tty1_nonroot_session_verified'] = True
                        result['writing_ui_verified'] = True
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError('Authenticated tty1 did not render the actual writing dashboard')
                    time.sleep(0.5)
                packages = (b'firmware-iwlwifi', b'firmware-realtek', b'firmware-atheros',
                            b'firmware-ath9k-htc', b'firmware-brcm80211', b'firmware-mediatek')
                connection.sendall(b"dpkg-query -W -f='WROS_WIFI=${Package}:${Status}\\n' " + b' '.join(packages) +
                                   b"; test -x /usr/bin/nmtui && test -d /lib/modules/$(uname -r)/kernel/drivers/net/wireless && printf '\\nWROS_WIFI_TOOLS_PASS\\n'; printf '\\nWROS_WIFI_STATE_END\\n'\n")
                expect(b'\r\nWROS_WIFI_STATE_END\r\n', 30)
                expect(b':~$ ', 20)
                if not all(b'WROS_WIFI=' + name + b':install ok installed' in content for name in packages) or b'\r\nWROS_WIFI_TOOLS_PASS\r\n' not in content:
                    raise RuntimeError('Actual installed system lacks the required offline Wi-Fi payload')
                result['offline_wifi_packages_verified'] = True
                qmp(control, 'screendump', {'filename': str(run / 'installed-writing-session.ppm')})
                if native_session:
                    start = len(content)
                    connection.sendall(b'if test "$(ps -C grimoire -o uid= | xargs)" = 1000 && test "$(ps -C wros-input -o uid= | xargs)" = 1000; then echo WROS_INPUT_UID_PASS; fi; sudo -n /bin/sh -c \'sha256sum /proc/$(pgrep -x grimoire)/exe\'; sha256sum /usr/bin/grimoire /usr/bin/wros-greeter; printf "\\nWROS_IDENTITY_STATE_END\\n"\n')
                    expect(b'\r\nWROS_IDENTITY_STATE_END\r\n', 20)
                    expect(b':~$ ', 20)
                    if b'\r\nWROS_INPUT_UID_PASS\r\n' not in content[start:]:
                        raise RuntimeError('Installed editor/input reader is not the non-root writer')
                    result['input_reader_nonroot_verified'] = True
                    hashes = re.findall(rb'([0-9a-f]{64})  /usr/bin/(grimoire|wros-greeter)\r', content[start:])
                    result['running_image_binary_sha256'] = {name.decode(): value.decode() for value, name in hashes}
                    loaded = re.findall(rb'([0-9a-f]{64})  /proc/[0-9]+/exe\r', content[start:])
                    result['running_editor_exe_sha256'] = loaded[0].decode() if len(loaded) == 1 else None
                    # Inspect logind and masks; never issue an actual sleep request.
                    start = len(content)
                    connection.sendall(b'sudo -n /bin/sh -c \'for u in sleep suspend hibernate hybrid-sleep suspend-then-hibernate; do test "$(systemctl is-enabled "$u.target" 2>/dev/null)" = masked || exit 1; done; echo WROS_SLEEP_MASKS_PASS; busctl call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager CanSuspend\'; printf "\\nWROS_SLEEP_STATE_END\\n"\n')
                    expect(b'\r\nWROS_SLEEP_STATE_END\r\n', 30)
                    expect(b':~$ ', 20)
                    if b'\r\nWROS_SLEEP_MASKS_PASS\r\n' not in content[start:] or b'\r\ns "na"\r\n' not in content[start:]:
                        raise RuntimeError('Installed sleep guard is not effective')
                    result['sleep_guard_verified'] = True
                    # Exit through the actual home menu, then check login and no
                    # surviving editor/input process. Keep no authenticated shell.
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'ctrl'},
                                                    {'type': 'qcode', 'data': 'q'}], 'hold-time': 40})
                    console_until(lambda c: b'Local account' in c and b'WrOS' in c,
                                  'Native logout did not return to login')
                    connection.sendall(b'if ! pgrep -x grimoire >/dev/null && ! pgrep -x wros-input >/dev/null; then echo WROS_LOGOUT_PASS; fi; printf "\\nWROS_LOGOUT_STATE_END\\n"\n')
                    expect(b'\r\nWROS_LOGOUT_STATE_END\r\n', 20)
                    expect(b':~$ ', 20)
                    if b'\r\nWROS_LOGOUT_PASS\r\n' not in content:
                        raise RuntimeError('Native logout left editor/input processes')
                    result['native_logout_verified'] = True
                if secret.encode() in content:
                    result['password_logged'] = True
                    raise RuntimeError('Synthetic unlock password appeared in output; keep private output for diagnosis')
                connection.sendall(b'sudo /sbin/poweroff\n')
                process.wait(timeout=60)
                return result
        finally:
            if secret is not None and secret.encode() in content:
                result['password_logged'] = True
            (run / 'boot-result.json').write_text(json.dumps(result, indent=2) + '\n')
            if process.poll() is None:
                if control.exists():
                    try:
                        qmp(control, 'screendump', {'filename': str(run / 'boot-last-screen.ppm')})
                    except (OSError, RuntimeError):
                        pass
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
