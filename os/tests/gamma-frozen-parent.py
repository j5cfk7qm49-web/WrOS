"""Disposable test parent: no book, input device, credentials or real display."""
import json
import os
import signal
import subprocess
import sys

worker = subprocess.Popen([sys.argv[1], '--wros-colour-worker'],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)


def request(value):
    worker.stdin.write(json.dumps(value) + '\n')
    worker.stdin.flush()
    reply = json.loads(worker.stdout.readline())
    assert 'Ok' in reply, reply
    return reply['Ok']


status = request('Status')['DisplayStatus']
assert status['available'] and not status['enabled'], status
preview = request({'Preview': {'enabled': True, 'kelvin': 2200, 'strength': 100}})['DisplayPreview']
assert 1 <= preview['seconds'] <= 15
print(json.dumps({'worker': worker.pid}), flush=True)
os.kill(os.getpid(), signal.SIGSTOP)
raise AssertionError('the fixture parent must remain stopped until the test ends it')
