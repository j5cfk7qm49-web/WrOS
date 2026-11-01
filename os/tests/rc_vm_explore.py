#!/usr/bin/env python3
"""Operator tool: boot the live ISO (diskless, or with a fresh target) and execute commands
appended to <run>/cmd.txt, writing replies to <run>/out.txt. For learning real screens only;
the qualification phases do not use it.

Commands (one per line): key <qcode+qcode...> | type <layout> <text> | shot <name> | vcs <n> |
sh <command> | wait <seconds> | quit
"""
import argparse
import json
import re
import sys
import time
from pathlib import Path

import rc_vm as vm
import rc_vm_phases as ph


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--iso', required=True)
    parser.add_argument('--target', action='store_true')
    args = parser.parse_args()
    ctx = ph.Ctx('explore', argparse.Namespace(iso=args.iso, receipt=None))
    target = token = None
    if args.target:
        target, token = ph.make_target(ctx)
    m = ph.boot_live(ctx, 'explore', 'cdrom', target=target, serial_token=token)
    g = ph.live_login(ctx, m)
    cmd, out = ctx.dir / 'cmd.txt', ctx.dir / 'out.txt'
    cmd.write_text('')
    out.write_text('READY %s\n' % ctx.dir)
    done = 0
    while True:
        lines = cmd.read_text().split('\n')
        while done < len(lines) - 1 or (done < len(lines) and lines[done].strip()):
            line = lines[done].strip()
            done += 1
            if not line:
                continue
            verb, _, rest = line.partition(' ')
            reply = ''
            try:
                if verb == 'key':
                    m.key(*rest.split('+'))
                elif verb == 'type':
                    layout, _, text = rest.partition(' ')
                    m.type_text(text.replace('\\n', '\n'), layout)
                elif verb == 'shot':
                    reply = str(m.shot(rest))
                elif verb == 'vcs':
                    reply = '\n'.join(g.vcs(int(rest)))
                elif verb == 'sh':
                    rc, text = g.run(rest, 120, check=False)
                    reply = 'rc=%d\n%s' % (rc, text)
                elif verb == 'wait':
                    time.sleep(float(rest))
                elif verb == 'quit':
                    ctx.finish()
                    return 0
            except Exception as error:
                reply = 'ERROR %s' % error
            with out.open('a') as stream:
                stream.write('>>> %s\n%s\n' % (line, reply))
        time.sleep(0.5)


if __name__ == '__main__':
    sys.exit(main())
