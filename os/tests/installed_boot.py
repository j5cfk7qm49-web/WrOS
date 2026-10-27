"""Actual UEFI/PAM/tty1 acceptance for a disposable installer-created disk."""
import json
import os
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


def verify(run, secret=None):
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
              'offline_wifi_packages_verified': False, 'password_logged': False}
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

                # The serial login remains the maintenance shell. Independently
                # authenticate tty1 through the actual virtual keyboard/PAM path.
                for char in 'fixture\n':
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'ret' if char == '\n' else char}], 'hold-time': 30})
                    time.sleep(0.06)
                # Inspect the actual console and echo flag before sending any
                # password keystrokes. A guessed delay is not input ownership.
                deadline = time.monotonic() + 30
                while True:
                    start = len(content)
                    connection.sendall(b'sudo -n /bin/sh -c \'cat /dev/vcs1; stty -F /dev/tty1 -a; printf "\\nWROS_CONSOLE_STATE_END\\n"\'\n')
                    expect(b'\r\nWROS_CONSOLE_STATE_END\r\n', 10)
                    console = content[start:]
                    expect(b':~$ ', 20)
                    if b'Password:' in console and b'-echo' in console.split():
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError('tty1 did not hand input to a non-echoing PAM password prompt')
                    time.sleep(0.5)
                for char in secret + '\n':
                    qmp(control, 'send-key', {'keys': [{'type': 'qcode', 'data': 'ret' if char == '\n' else char}], 'hold-time': 30})
                    time.sleep(0.06)
                time.sleep(12)
                connection.sendall(b'sudo -n journalctl -t writingdeck-session --no-pager -n 1\n')
                expect(b'WRITINGDECK_SESSION uid=1000 tty=/dev/tty1', 40)
                result['tty1_nonroot_session_verified'] = True
                # sudo's pty can consume input until its child has exited.
                # Wait for Bash before issuing the next command, even after
                # the expected child output has already arrived.
                expect(b':~$ ', 20)
                deadline = time.monotonic() + 40
                while True:
                    start = len(content)
                    connection.sendall(b'sudo -n /bin/sh -c \'cat /dev/vcs1; printf "\\nWROS_UI_STATE_END\\n"\'\n')
                    expect(b'\r\nWROS_UI_STATE_END\r\n', 10)
                    console = content[start:]
                    expect(b':~$ ', 20)
                    if b'WRITINGDECK OS' in console and b'Resume writing' in console:
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
