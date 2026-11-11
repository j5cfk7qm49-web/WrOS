"""Phone linking (os/scripts/wros-link.py): the QR code, the link it carries, and both ways the
phone's approval can come back. Google, Drive and the phone are fakes; nothing leaves the machine."""
import importlib.machinery
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
link = importlib.machinery.SourceFileLoader('wros_link', str(ROOT / 'os/scripts/wros-link.py')).load_module()

try:
    import qrcode
    import qrcode.util
except ImportError:  # the reference encoder is only in the build chroot
    qrcode = None

SAMPLE = ('https://j5cfk7qm49-web.github.io/WrOS/l/#ftbxKRcY58ctEI9ndMY6vdJlmRMR46YS5qMKvgzbGQk'
          '.wKgpQSB8.KXP-QKN-VLWD')


def reference(text, mask):
    code = qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_L, border=0, mask_pattern=mask)
    code.add_data(qrcode.util.QRData(text.encode(), mode=qrcode.util.MODE_8BIT_BYTE))
    code.make(fit=True)
    return [[bool(cell) for cell in row] for row in code.modules]


@unittest.skipUnless(qrcode, 'needs python3-qrcode as the reference encoder')
class QrCode(unittest.TestCase):
    def test_every_mask_matches_the_reference_encoder_for_short_and_link_sized_text(self):
        for text in ('WrOS', 'https://example.org/', SAMPLE, 'x' * 200):
            for mask in range(8):
                self.assertEqual(link.qr_matrix(text, mask=mask), reference(text, mask), (text[:20], mask))

    def test_the_chosen_mask_is_the_one_with_the_lowest_penalty(self):
        chosen = link.qr_matrix(SAMPLE)
        penalties = [link.penalty(link.qr_matrix(SAMPLE, mask=m)) for m in range(8)]
        self.assertIn(chosen, [link.qr_matrix(SAMPLE, mask=m) for m in range(8)
                               if penalties[m] == min(penalties)])


class QrSize(unittest.TestCase):
    def test_the_link_fits_the_laptop_console(self):
        # 1366x768 with an 8x16 font is 170x48 cells; each module is two cells wide and one tall.
        rows = link.qr_rows(SAMPLE)
        self.assertLessEqual(len(rows), 45)
        self.assertLessEqual(len(rows[0]), 90)
        self.assertEqual({len(r) for r in rows}, {len(rows[0])})
        self.assertTrue(set(''.join(rows)) <= {'0', '1'})
        self.assertEqual(rows[0], '0' * len(rows[0]))  # the quiet zone is light

    def test_text_too_long_for_version_ten_is_refused(self):
        with self.assertRaises(ValueError):
            link.qr_matrix('x' * 300)


CLIENTS = {'linking': {'client_id': 'web-id', 'client_secret': 'web-secret'},
           'mailbox': {'client_id': 'tv-id', 'client_secret': 'tv-secret'}}


class Google:
    """Google's token, device and Drive endpoints, scripted."""

    def __init__(self, device_answers=(), files=None, exchange=None):
        self.calls, self.device_answers = [], list(device_answers)
        self.files = dict(files or {})
        self.exchange = exchange or (200, {'access_token': 'full-access', 'refresh_token': 'full-refresh',
                                           'expires_in': 3599, 'scope': link.FULL_SCOPE})

    def __call__(self, method, url, form=None, token=None):
        self.calls.append((method, url, dict(form or {}), token))
        if url == link.DEVICE_URL:
            return 200, {'device_code': 'device-1', 'user_code': 'KXP-QKN-VLWD', 'interval': 5,
                         'expires_in': 1800, 'verification_url': 'https://www.google.com/device'}
        if url == link.TOKEN_URL and form.get('grant_type') == link.DEVICE_GRANT:
            return self.device_answers.pop(0) if self.device_answers else (428, {'error': 'authorization_pending'})
        if url == link.TOKEN_URL:
            return self.exchange
        if url.startswith(link.DRIVE_FILES + '?'):
            query = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)['q'][0]
            hits = [{'id': i} for i, (name, _) in self.files.items() if repr(name) in query or f"'{name}'" in query]
            return 200, {'files': hits}
        if url.startswith(link.DRIVE_FILES + '/') and url.endswith('?alt=media'):
            return 200, self.files[url[len(link.DRIVE_FILES) + 1:-10]][1].encode()
        if url.startswith(link.DRIVE_FILES + '/') and method == 'DELETE':
            self.files.pop(url[len(link.DRIVE_FILES) + 1:])
            return 204, {}
        raise AssertionError(f'unexpected call {method} {url}')

    def used(self, url):
        return [c for c in self.calls if c[1].startswith(url)]


class Listener:
    def __init__(self, codes=()):
        self.codes, self.answers, self.closed = list(codes), [], False
        self.address = ('192.168.1.20', 31020)

    def wait(self, seconds, state):
        if self.codes:
            return self.codes.pop(0), self.answers.append
        return None, None

    def close(self):
        self.closed = True


class Clock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class Linking(unittest.TestCase):
    def run_link(self, google, listener, verifier='v' * 64):
        self.clock, self.offers = Clock(), []
        session = link.Link(CLIENTS, http=google, clock=self.clock, sleep=self.clock.sleep,
                            listener=listener, offer=self.offers.append, verifier=verifier)
        return session, session.run()

    def test_the_address_packs_into_eight_characters_and_only_private_ones_are_offered(self):
        packed = link.pack_address('192.168.1.20', 31020)
        self.assertEqual(len(packed), 8)
        self.assertEqual(link.unpack_address(packed), ('192.168.1.20', 31020))
        self.assertTrue(link.private('10.1.2.3') and link.private('172.20.0.9'))
        self.assertFalse(link.private('8.8.8.8') or link.private('127.0.0.1') or link.private('100.64.0.1'))

    def test_the_wifi_answer_wins_and_the_saved_code_is_cleaned_up_with_full_access(self):
        challenge = link.challenge('v' * 64)
        google = Google(files={'f1': ('WrOS linking ' + challenge[:16], 'code-from-phone'),
                               'f2': ('WrOS linking ' + challenge[:16], 'duplicate-code')})
        listener = Listener(codes=['code-from-phone'])
        session, (token, via) = self.run_link(google, listener)
        self.assertEqual(via, 'wifi')
        self.assertEqual(token['refresh_token'], 'full-refresh')
        exchange = [c for c in google.used(link.TOKEN_URL) if c[2].get('grant_type') == 'authorization_code'][0]
        self.assertEqual(exchange[2]['code_verifier'], 'v' * 64)
        self.assertEqual(exchange[2]['client_id'], 'web-id')
        self.assertEqual(exchange[2]['redirect_uri'], link.RETURN_URL)
        session.finish(token, via)
        self.assertEqual(google.files, {})  # the phone's saved copy is gone
        self.assertTrue(listener.closed)
        self.assertEqual(google.used(link.REVOKE_URL), [])
        offer = self.offers[0]
        self.assertEqual(offer['code'], 'KXP-QKN-VLWD')
        self.assertTrue(offer['link'].startswith(link.PAGES + '/l/#' + challenge + '.'))
        self.assertTrue(offer['link'].endswith('.KXP-QKN-VLWD'))
        self.assertEqual(offer['qr'], link.qr_rows(offer['link']))

    def test_without_wifi_the_laptops_own_sign_in_collects_the_saved_code(self):
        challenge = link.challenge('v' * 64)
        google = Google(device_answers=[(428, {'error': 'authorization_pending'}),
                                        (200, {'access_token': 'limited', 'refresh_token': 'limited-r',
                                               'scope': link.LIMITED_SCOPE})],
                        files={'f9': ('WrOS linking ' + challenge[:16], ' code-from-phone \n')})
        session, (token, via) = self.run_link(google, Listener())
        self.assertEqual(via, 'drive')
        self.assertEqual(google.files, {})  # read, then deleted with the limited access
        exchange = [c for c in google.used(link.TOKEN_URL) if c[2].get('grant_type') == 'authorization_code'][0]
        self.assertEqual(exchange[2]['code'], 'code-from-phone')
        self.assertEqual(google.used(link.REVOKE_URL), [])  # revoking it would revoke the new access too
        drive_calls = [c for c in google.calls if c[1].startswith(link.DRIVE_FILES)]
        self.assertTrue(all(c[3] == 'limited' for c in drive_calls))

    def test_linking_stops_when_neither_way_answers_in_time(self):
        with self.assertRaisesRegex(ValueError, 'did not finish'):
            self.run_link(Google(), Listener())
        self.assertGreaterEqual(self.clock.now - 1000, link.LINK_SECONDS)

    def test_a_refused_approval_keeps_the_session_open(self):
        listener = Listener(codes=['stale-code', 'code-from-phone'])

        class GoogleWithStaleCode(Google):
            def __call__(self, method, url, form=None, token=None):
                if url == link.TOKEN_URL and form.get('code') == 'stale-code':
                    return 400, {'error': 'invalid_grant'}
                return super().__call__(method, url, form, token)

        session, (token, via) = self.run_link(GoogleWithStaleCode(), listener)
        self.assertTrue(any('did not link' in answer for answer in listener.answers))
        self.assertEqual(via, 'wifi')
        session.finish(token, via)
        self.assertTrue(listener.closed)

    def test_a_device_sign_in_the_writer_declines_leaves_the_wifi_way_open(self):
        google = Google(device_answers=[(403, {'error': 'access_denied'})])
        listener = Listener()
        session = link.Link(CLIENTS, http=google, clock=Clock(), sleep=lambda s: listener.codes.append('late-code'),
                            listener=listener, offer=lambda o: None, verifier='v' * 64)
        token, via = session.run()
        self.assertEqual(via, 'wifi')


class RealListener(unittest.TestCase):
    def test_it_takes_one_code_from_linked_and_refuses_everything_else(self):
        import socket
        server = link.Listener('127.0.0.1')
        try:
            def ask(request):
                client = socket.create_connection(server.address, timeout=5)
                client.sendall(request.encode())
                return client

            state = link.challenge('v' * 64)
            for target in ('/elsewhere', '/linked?code=4%2F0AVG-code',
                           '/linked?code=4%2F0AVG-code&state=wrong'):
                with ask(f'GET {target} HTTP/1.1\r\nHost: x\r\n\r\n') as other:
                    code, answer = server.wait(2, state)
                    self.assertIsNone(code)
                    self.assertIn(b'404', other.recv(4096))
            client = ask(f'GET /linked?code=4%2F0AVG-code&state={state} HTTP/1.1\r\nHost: x\r\n\r\n')
            self.addCleanup(client.close)
            code, answer = server.wait(2, state)
            self.assertEqual(code, '4/0AVG-code')
            answer('Google Drive is linked. You can close this page.')
            reply = client.recv(4096)
            self.assertIn(b'200', reply.split(b'\r\n')[0])
            self.assertIn(b'Google Drive is linked', reply)
        finally:
            server.close()


import urllib.parse  # noqa: E402  (used by the Google fake)

if __name__ == '__main__':
    unittest.main()
