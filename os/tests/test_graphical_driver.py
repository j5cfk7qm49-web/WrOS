"""The private VM client waits for an action that is temporarily unavailable."""
import contextlib
import io
import json
from pathlib import Path
import runpy
import sys
import unittest
from unittest import mock

CLIENT = Path(__file__).with_name('graphical_driver.py')


class GraphicalDriver(unittest.TestCase):
    def test_a_pending_system_request_must_finish_before_a_lock_is_sent(self):
        snapshots = [json.dumps({'type': 'snapshot', 'nodes': [
            {'id': 'lock', 'label': 'Lock writing session', 'actions': actions}
        ]}) + '\n' for actions in ([], ['activate'])]

        class Connection:
            request = None
            reads = 0
            reads_before_send = None

            def settimeout(self, _):
                pass

            def connect(self, _):
                pass

            def makefile(self, _):
                return self

            def readline(self):
                self.reads += 1
                if snapshots:
                    return snapshots.pop(0)
                return json.dumps({'type': 'ack', 'id': self.request['id'], 'status': 'delivered'}) + '\n'

            def sendall(self, data):
                self.reads_before_send = self.reads
                self.request = json.loads(data)

        connection = Connection()
        with mock.patch('socket.socket', return_value=connection), \
             mock.patch.object(sys, 'argv', [str(CLIENT), json.dumps({'choose': 'Lock writing session'}), 'send-only']), \
             contextlib.redirect_stdout(io.StringIO()):
            runpy.run_path(CLIENT, run_name='__main__')
        self.assertEqual(connection.reads_before_send, 2)
        self.assertEqual(connection.request['input'], {'kind': 'act', 'node': 'lock', 'action': 'activate'})


if __name__ == '__main__':
    unittest.main()
