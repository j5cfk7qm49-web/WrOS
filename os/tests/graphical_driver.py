"""Private taria client for a disposable VM's debug executable only."""
import json
import socket
import sys
import time

s = socket.socket(socket.AF_UNIX)
s.settimeout(25)
s.connect('/run/user/1000/wros-vm-test.sock')
stream = s.makefile('r')
tree = None
while tree is None:
    message = json.loads(stream.readline())
    if message['type'] == 'snapshot':
        tree = message

def find(value, label):
    if isinstance(value, dict):
        if str(value.get('label', '')).startswith(label) and value.get('actions'):
            return value['id']
        children = value.values()
    elif isinstance(value, list):
        children = value
    else:
        return None
    return next((node for item in children if (node := find(item, label))), None)

if len(sys.argv) == 1:
    print(json.dumps(tree))
else:
    request = json.loads(sys.argv[1])
    if 'choose' in request:
        node = find(tree, request['choose'])
        deadline = time.monotonic() + 25
        while node is None:
            remaining = deadline - time.monotonic()
            assert remaining > 0, 'row stayed unavailable: ' + request['choose']
            s.settimeout(remaining)
            line = stream.readline()
            assert line, 'disconnected while waiting for row: ' + request['choose']
            message = json.loads(line)
            if message['type'] == 'snapshot':
                tree = message
                node = find(tree, request['choose'])
        request = {'kind': 'act', 'node': node, 'action': 'activate'}
    stamp = time.monotonic_ns() // 1000
    s.sendall((json.dumps({'type': 'input', 'id': stamp, 'input': request}) + '\n').encode())
    if len(sys.argv) > 2 and sys.argv[2] == 'send-only':
        # A lock prevents the next frame until authentication. Wait for dequeue
        # instead of sleeping and disconnecting while the input is still queued.
        # The VM controller separately proves that tty8 actually became locked.
        while True:
            message = json.loads(stream.readline())
            if message.get('type') == 'ack' and message.get('id') == stamp:
                assert message['status'] == 'delivered', message
                print('dequeued')
                break
    else:
        while True:
            message = json.loads(stream.readline())
            if message.get('after_input') == stamp:
                print(json.dumps(message))
                break
