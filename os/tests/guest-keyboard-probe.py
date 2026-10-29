#!/usr/bin/python3 -I
"""Keymap integration only, injected into a disposable VM-demo overlay."""
import json
from pathlib import Path
import re
import subprocess
import time


def command(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=12)


def marker(value):
    with open('/dev/ttyS0', 'w') as output:
        output.write(value + '\n')


def symbol():
    dump = command('/usr/bin/dumpkeys', '--console=/dev/tty1', '--keys-only', '--numeric')
    match = re.search(r'^\s*keycode\s+18\s*=\s*\+?(0x[0-9a-fA-F]+)', dump, re.MULTILINE)
    assert match, 'actual E keycode must be present'
    return int(match[1], 16) & 0xff


def helper(*args):
    return json.loads(command('/usr/libexec/writingdeck-keyboard', *args))


def main():
    assert Path('/etc/writingdeck/VM-DEMO-ONLY').is_file()
    config = Path('/etc/writingdeck/keyboard.json')
    pending = config.with_name('installer-keyboard-pending')
    system = Path('/etc/default/keyboard')
    assert json.loads(config.read_text()) == {'version': 1, 'layout': 'colemak', 'generation': 'installer'}
    assert not pending.exists()
    assert symbol() == ord('f'), 'boot service must adopt installer choice before login'
    marker('WROS_KEYBOARD_INSTALLER_BOOT_PASS')
    helper('preview', 'qwerty')
    assert symbol() == ord('e')
    time.sleep(16)
    assert symbol() == ord('f'), 'independent timer must restore installer choice'
    marker('WROS_KEYBOARD_INSTALLER_ROLLBACK_PASS')
    preview = helper('preview', 'qwerty')
    helper('confirm', preview['token'])
    committed = config.read_bytes()
    system.write_text('XKBLAYOUT="de"\nXKBVARIANT=""\n')
    pending.touch()
    helper('boot')
    assert config.read_bytes() == committed and symbol() == ord('e')
    assert not pending.exists()
    marker('WROS_KEYBOARD_COMMITTED_PRESERVED_PASS')
    # Simulate console-setup having applied another system map. This verifies
    # preservation, not the correctness of a German or custom keyboard map.
    command('/usr/bin/loadkeys', '--unicode', '--console=/dev/tty1',
            '/usr/share/writingdeck/keymaps/colemak.map')
    config.write_text('{"version":1,"layout":"qwerty","generation":"initial"}')
    pending.touch()
    assert helper('boot')['status'] == 'preserved-system-layout'
    rejected = subprocess.run(['/usr/libexec/writingdeck-keyboard', 'preview', 'qwerty'],
                              capture_output=True, text=True, timeout=12)
    assert rejected.returncode != 0 and 'selected keyboard is preserved' in rejected.stderr
    assert symbol() == ord('f') and pending.exists()
    marker('WROS_KEYBOARD_SYSTEM_MAP_PRESERVED_PASS')
    marker('WROS_KEYBOARD_PASS')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        marker('WROS_KEYBOARD_FAIL ' + repr(error))
    finally:
        subprocess.run(['/usr/bin/systemctl', 'poweroff'], check=False)
