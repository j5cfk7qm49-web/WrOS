#!/usr/bin/env python3
"""Operator tool: attach to a running phase VM whose driver is gone (sockets are server mode) and run
read-only commands over the already-logged-in echo-free serial shell.
usage: rc_vm_attach.py <run-dir> <name> <command> [<command> ...]   (name = machine name, e.g. install)
Output goes to stdout; every reply is scanned by the caller. Nothing is sent that the caller did not give."""
import re
import sys
from pathlib import Path

import rc_vm as vm


def main():
    run, name = Path(sys.argv[1]), sys.argv[2]
    ser = vm.Serial(run / (name + '-serial.sock'), run / ('attach-%s-serial.log' % name))
    ser.connect()
    sh = vm.Shell(ser)
    for command in sys.argv[3:]:
        rc, out = sh.run(command, 120, check=False)
        print('>>> %s\n[rc=%s]\n%s' % (command, rc, out))


if __name__ == '__main__':
    main()
