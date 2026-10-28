#!/usr/bin/env python3
"""Real Debian cdebconf + native TUI in a private, pre-mounted RAM fixture.

The caller binds fixture/cdebconf.conf over /etc/cdebconf.conf in a private
mount namespace. Run non-root with no network. No target disk or real account.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'os/tests'))
from test_installer_pty import InstallerTerminal
from test_installer_pty import BINARY
from writingdeck_source import fingerprint


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture',type=Path)
    parser.add_argument('package_root',type=Path)
    parser.add_argument('--report',type=Path,required=True)
    parser.add_argument('--passthrough-module',type=Path,required=True)
    parser.add_argument('--config-file',type=Path,default=Path('/etc/cdebconf.conf'))
    args=parser.parse_args()
    source_before=fingerprint()
    fixture,package=args.fixture.resolve(),args.package_root.resolve()
    if os.geteuid()==0 or fixture.is_symlink() or not (fixture/'WROS-SYNTHETIC-ONLY').is_file():
        raise SystemExit('A prepared private non-root synthetic fixture is required.')
    if (fixture/'cdebconf.conf').read_bytes()!=args.config_file.read_bytes():
        raise SystemExit('The private cdebconf configuration differs from the prepared fixture.')
    secret=secrets.token_hex(1500).encode()+b'\\n\\literal'
    typed=b' '+secrets.token_hex(1500).encode()+b'\\n\\literal '
    (fixture/'secret').write_bytes(secret)
    (fixture/'secret').chmod(0o600)
    (fixture/'typed').write_bytes(typed)
    (fixture/'typed').chmod(0o600)
    templates='''Template: fixture/name
Type: string
Description: Fixture account name
 Choose a name for this synthetic account.
 .
 This is a second paragraph with café and λ.

Template: fixture/password
Type: password
Description: Fixture account password
 This synthetic credential must remain hidden.

Template: fixture/repeated
Type: password
Description: Fixture prefilled password
 This tests the backend-to-frontend secret direction.

Template: fixture/back
Type: note
Description: Return from this fixture step
 Escape must return the documented backup status.

Template: fixture/confirm
Type: boolean
Default: true
Description: Write synthetic fixture changes?
 No real disk is opened by this test.

Template: fixture/layout
Type: select
Choices-C: us, fr, uk
Choices: English, Français, British
Default: fr
Description: Fixture keyboard layout
 Choose an existing canonical value.

Template: fixture/progress
Type: text
Description: Installing the synthetic fixture
 This progress description has multiple paragraphs.
 .
 A second progress paragraph.

Template: fixture/final
Type: note
Description: Synthetic backend completed
 The Debian backend accepted all answers.
'''
    (fixture/'fixture.templates').write_text(templates,encoding='utf-8')
    script='''#!/bin/sh
set -eu
. /usr/share/debconf/confmodule
db_x_loadtemplatefile fixture.templates fixture
db_capb backup
db_input critical fixture/name
db_go
db_get fixture/name
test "$RET" = "fixture-user"
db_input critical fixture/back
if db_go; then exit 41; fi
db_input critical fixture/password
db_go
db_get fixture/password
test "$RET" = "$(cat typed)"
db_set fixture/password ""
db_set fixture/repeated "$(cat secret)"
db_input critical fixture/repeated
db_go
db_get fixture/repeated
test "$RET" = "$(cat secret)" || { printf 'Synthetic answer length %s; expected %s\n' "${#RET}" "$(wc -c < secret)" >&2; exit 43; }
db_set fixture/repeated ""
db_input critical fixture/confirm
db_go
db_get fixture/confirm
test "$RET" = false
db_input critical fixture/layout
db_go
db_get fixture/layout
test "$RET" = fr
db_set fixture/name ""
db_fset fixture/name seen false
db_input critical fixture/name
db_go
db_get fixture/name
test "$RET" = ""
db_progress START 0 10 fixture/progress
db_progress STEP 5
db_progress SET 10
db_progress STOP
db_input critical fixture/final
db_go
printf complete > backend-completed
db_stop
'''
    postinst=fixture/'fixture.postinst';postinst.write_text(script);postinst.chmod(0o700)
    checks=[]
    started=time.monotonic()
    terminal=InstallerTerminal(fixture,80,24)
    # Python timeout sockets use O_NONBLOCK. The inherited C stdio descriptor
    # must be blocking; otherwise cdebconf can see EAGAIN as a protocol error.
    terminal.peer.setblocking(True)
    backend=None
    log=fixture/'backend.log'
    try:
        environment={'PATH':'/usr/bin:/bin','HOME':str(fixture),'LANG':'C.UTF-8','LC_ALL':'C.UTF-8',
            'LD_LIBRARY_PATH':':'.join(str(package/path) for path in
                                     ('usr/lib/cdebconf','usr/lib/x86_64-linux-gnu','usr/lib')),
            'DEBCONF_READFD':str(terminal.peer.fileno()),'DEBCONF_WRITEFD':str(terminal.peer.fileno()),
            'DEBCONF_DEBUG':'0','DEBIAN_PRIORITY':'low'}
        with log.open('wb') as output:
            backend=subprocess.Popen([str(package/'usr/lib/cdebconf/debconf'),'-fpassthrough','-ofixture','/bin/sh',str(postinst)],
                stdin=subprocess.DEVNULL,stdout=output,stderr=output,env=environment,cwd=fixture,
                pass_fds=(terminal.peer.fileno(),),start_new_session=True)
        terminal.screen_shows('Fixture account name')
        terminal.screen_shows('second paragraph')
        if str(args.passthrough_module.resolve()) not in Path(f'/proc/{backend.pid}/maps').read_text():
            raise AssertionError('The expected module was not loaded by the real backend')
        checks.append('real-multiline-debian-template')
        terminal.send(b'fixture-user\r')
        terminal.screen_shows('Return from this fixture step')
        terminal.send(b'\x1b')
        terminal.screen_shows('Fixture account password')
        terminal.send(typed+b'\r')
        terminal.screen_shows('Fixture prefilled password')
        terminal.send(b'\r')
        terminal.screen_shows('Write synthetic fixture changes?')
        if secret in terminal.output or typed in terminal.output: raise AssertionError('Synthetic credential reached terminal output')
        terminal.screen_shows('▸ No');terminal.send(b'\r')
        terminal.screen_shows('Fixture keyboard layout')
        terminal.screen_shows('▸ Français');terminal.send(b'\r')
        terminal.screen_shows('Fixture account name')
        terminal.send(b'\r')
        terminal.screen_shows('Synthetic backend completed')
        terminal.send(b'\r')
        terminal.until(lambda:(fixture/'backend-completed').is_file(),'backend accepted canonical answers')
        backend.wait(timeout=5)
        if backend.returncode!=0: raise AssertionError('Debian backend failed')
        checks.extend(['real-back-navigation-status','real-password-roundtrip-hidden','prefilled-secret-with-literal-backslashes',
                       'prefilled-password-over-one-stdio-chunk','typed-password-preserves-leading-and-trailing-spaces',
                       'typed-password-over-one-terminal-read',
                       'explicit-no-overrides-backend-true-default',
                       'backend-cleared-default-does-not-reuse-earlier-answer',
                       'canonical-translated-choice','real-progress-commands','backend-success-observed'])
        if secret in log.read_bytes() or typed in log.read_bytes(): raise AssertionError('Synthetic credential reached backend log')
        checks.append('credential-absent-from-backend-log')
        if fingerprint()!=source_before: raise AssertionError('Source changed during backend verification')
        report={'recorded_at_utc':datetime.now(timezone.utc).isoformat(),'passed':True,
                'scope':'Debian cdebconf passthrough and native TUI synthetic questions; no installation or target disk',
                'checks':checks,'seconds':round(time.monotonic()-started,3),
                'source_fingerprint':source_before,
                'native_frontend_binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                'module_observed_in_backend_mapping':True,
                'backend_binary_sha256':hashlib.sha256((package/'usr/lib/cdebconf/debconf').read_bytes()).hexdigest(),
                'passthrough_module_sha256':hashlib.sha256(args.passthrough_module.read_bytes()).hexdigest(),
                'limitations':['Privileged live launcher, backend-death supervision and actual live-image installation remain unqualified.']}
        args.report.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'passed':True,'checks':len(checks),'seconds':report['seconds']}))
    except Exception:
        if backend is not None and backend.poll() is None:
            print('Synthetic backend wait: '+Path(f'/proc/{backend.pid}/wchan').read_text(),file=sys.stderr)
        if log.is_file() and secret not in log.read_bytes() and typed not in log.read_bytes():
            print('Synthetic backend diagnostics: '+log.read_text(errors='replace')[:4000],file=sys.stderr)
        failure={'recorded_at_utc':datetime.now(timezone.utc).isoformat(),'passed':False,
                 'scope':'Synthetic cdebconf integration attempt','checks_completed':checks,
                 'backend_exit_code':None if backend is None else backend.poll()}
        args.report.with_name('native-installer-backend-failed-'+str(time.time_ns())+'.json').write_text(json.dumps(failure,indent=2)+'\n')
        raise
    finally:
        # This harness is the supervisor: frontend/backend failure cannot leave
        # the synthetic config script or cdebconf running in the background.
        if backend is not None and backend.poll() is None:
            os.killpg(backend.pid,signal.SIGTERM)
            try: backend.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(backend.pid,signal.SIGKILL);backend.wait(timeout=3)
        terminal.close()
        (fixture/'secret').unlink(missing_ok=True)
        (fixture/'typed').unlink(missing_ok=True)


if __name__=='__main__': main()
