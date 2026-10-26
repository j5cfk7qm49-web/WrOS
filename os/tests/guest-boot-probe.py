#!/usr/bin/python3 -I
"""VM-only integration probe: never installed in a production configuration."""
import json
from pathlib import Path
import re
import subprocess
import time


def command(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=12)


def marker(text):
    with open('/dev/ttyS0', 'w') as out:
        out.write(text + '\n')


def primary_symbol(code):
    dump = command('/usr/bin/dumpkeys', '--console=/dev/tty1', '--keys-only', '--numeric')
    match = re.search(r'^\s*keycode\s+' + str(code) + r'\s*=\s*\+?(0x[0-9a-fA-F]+)', dump, re.MULTILINE)
    if not match:
        raise RuntimeError('keycode not found in dumpkeys: ' + repr([line for line in dump.splitlines() if re.search(r'keycode\s+18\s*=', line)][:2]))
    return int(match[1], 16) & 0xff


def main():
    assert Path('/etc/writingdeck/VM-DEMO-ONLY').exists(), 'probe requires explicit isolated VM profile'
    deadline = time.monotonic() + 100
    pid = None
    while time.monotonic() < deadline:
        processes = []
        for p in Path('/proc').glob('[0-9]*'):
            try:
                if (p / 'comm').read_text().strip() == 'grimoire':
                    processes.append(p)
            except (OSError, ProcessLookupError):
                pass
        try:
            screen = Path('/dev/vcs1').read_bytes().decode('latin1')
        except OSError:
            screen = ''
        if processes and 'WRITINGDECK OS' in screen and 'Resume writing' in screen:
            pid = processes[0]
            break
        time.sleep(0.25)
    assert pid is not None, 'tty1 did not render the real Grimoire dashboard'
    status = (pid / 'status').read_text()
    uid = re.search(r'^Uid:\s+(\d+)', status, re.MULTILINE).group(1)
    assert uid != '0', 'Grimoire must never run as root'
    rss = re.search(r'^VmRSS:\s+(\d+)', status, re.MULTILINE).group(1)
    marker('WRITINGDECK_VM_UI_PASS uid=' + uid + ' rss_kib=' + rss)
    # The active local writer is eligible for only the narrow keyboard action.
    command('/usr/bin/pkcheck', '--action-id', 'org.writingdeck.keyboard', '--process', pid.name)
    marker('WRITINGDECK_VM_POLKIT_PASS')
    assert primary_symbol(18) == ord('e'), 'initial QWERTY E key'
    # Real Linux console keymap change + real independent systemd timer.
    preview = json.loads(command('/usr/libexec/writingdeck-keyboard', 'preview', 'colemak-dh-ansi'))
    assert primary_symbol(18) == ord('f'), 'Colemak physical E must produce logical F'
    assert len(preview['token']) == 32
    time.sleep(16)
    assert primary_symbol(18) == ord('e'), 'unconfirmed preview must revert without Grimoire involvement'
    marker('WRITINGDECK_VM_KEYBOARD_ROLLBACK_PASS')
    preview = json.loads(command('/usr/libexec/writingdeck-keyboard', 'preview', 'colemak'))
    command('/usr/libexec/writingdeck-keyboard', 'confirm', preview['token'])
    assert json.loads(Path('/etc/writingdeck/keyboard.json').read_text())['layout'] == 'colemak'
    command('/usr/libexec/writingdeck-keyboard', 'boot')
    assert primary_symbol(18) == ord('f'), 'committed layout must apply again at boot'
    marker('WRITINGDECK_VM_KEYBOARD_COMMIT_PASS')
    grammar = json.loads(subprocess.check_output([
        '/usr/sbin/runuser', '-u', 'writer', '--', '/usr/bin/python3',
        '/usr/libexec/writingdeck-vm-fixtures/guest-checks-probe.py'], text=True, stderr=subprocess.STDOUT, timeout=120))
    assert grammar['uid'] != 0 and len(grammar['checks']) == 6
    marker('WRITINGDECK_VM_GRAMMAR_RESULT ' + json.dumps(grammar))
    marker('WRITINGDECK_VM_GRAMMAR_PASS uid=' + str(grammar['uid']) + ' peak_rss_kib=' + str(grammar['kernel_peak_rss_kib']) )
    prose = json.loads(subprocess.check_output([
        '/usr/sbin/runuser', '-u', 'writer', '--', '/usr/bin/python3',
        '/usr/libexec/writingdeck-vm-fixtures/guest-prose-probe.py'], text=True, stderr=subprocess.STDOUT, timeout=250))
    assert prose['uid'] != 0 and len(prose['engines']) == 3 and prose['wordnet_senses'] == 21
    marker('WRITINGDECK_VM_PROSE_RESULT ' + json.dumps(prose))
    marker('WRITINGDECK_VM_PROSE_PASS uid=' + str(prose['uid']))
    marker('WRITINGDECK_VM_PASS')
    time.sleep(4)  # Host captures an actual QEMU display screenshot via QMP.


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        marker('WRITINGDECK_VM_FAIL ' + repr(exc))
        time.sleep(4)
    finally:
        subprocess.run(['/usr/bin/systemctl', 'poweroff'], check=False)
