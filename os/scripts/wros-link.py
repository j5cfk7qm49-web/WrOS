"""Link Google Drive from the writer's phone: one QR code, two ways back to this laptop.

Loaded by /usr/libexec/wros-backup for its "link-phone" operation and run as the writer.

The laptop shows a QR code for https://…/WrOS/l/#<challenge>.<address>.<user code>. The phone
signs in once, with full Drive access, and Google answers the site's /link/ page with a code for
the laptop and a short-lived token for the page. The page saves the code in the writer's Drive
as "WrOS linking <key>", then offers two ways back:
- Wi-Fi: "Finish linking" sends the code and this session's challenge to the laptop's listener.
- Anywhere: the writer approves the laptop's own limited sign-in (Google's device page, with the
  user code this laptop shows). That sign-in can see only files WrOS made, so it finds the saved
  code, reads it and deletes it.
Whichever arrives first is exchanged for a refresh token with the PKCE verifier, which never
leaves this process. The limited sign-in is set aside, never revoked: both clients belong to one
Google project, and revoking either grant revokes both.

It also holds a small QR encoder (byte mode, error correction level L, versions 1-10), because
WrOS updates replace only the wros package and cannot bring a QR library with them.
"""
import base64
import hashlib
import hmac
import ipaddress
import json
import secrets
import socket
import time
import urllib.error
import urllib.parse
import urllib.request

# ---------------------------------------------------------------------------------------- QR

# Error correction level L, versions 1-10 (ISO/IEC 18004 table 9).
ECC_PER_BLOCK = (None, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18)
BLOCKS = (None, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4)
MAX_VERSION = 10


def raw_modules(version):
    """Data and error correction modules of a version, after every function pattern."""
    result = (16 * version + 128) * version + 64
    if version >= 2:
        alignments = version // 7 + 2
        result -= (25 * alignments - 10) * alignments - 55
        if version >= 7:
            result -= 36
    return result


def data_codewords(version):
    return raw_modules(version) // 8 - ECC_PER_BLOCK[version] * BLOCKS[version]


def gf_multiply(x, y):
    """Multiplication in GF(2^8) modulo x^8 + x^4 + x^3 + x^2 + 1."""
    z = 0
    for i in reversed(range(8)):
        z = (z << 1) ^ ((z >> 7) * 0x11D)
        z ^= ((y >> i) & 1) * x
    return z


def rs_divisor(degree):
    result = [0] * (degree - 1) + [1]
    root = 1
    for _ in range(degree):
        for j in range(degree):
            result[j] = gf_multiply(result[j], root)
            if j + 1 < degree:
                result[j] ^= result[j + 1]
        root = gf_multiply(root, 0x02)
    return result


def rs_remainder(data, divisor):
    result = [0] * len(divisor)
    for byte in data:
        factor = byte ^ result.pop(0)
        result.append(0)
        for i, coefficient in enumerate(divisor):
            result[i] ^= gf_multiply(coefficient, factor)
    return result


def alignment_positions(version):
    if version == 1:
        return []
    count = version // 7 + 2
    step = (version * 8 + count * 3 + 5) // (count * 4 - 4) * 2
    positions = [version * 4 + 10 - i * step for i in range(count - 1)]
    return [6] + list(reversed(positions))


def codewords(text):
    """The data codewords, the smallest version that holds them, and the interleaved final bytes."""
    data = text.encode()
    for version in range(1, MAX_VERSION + 1):
        count_bits = 8 if version <= 9 else 16
        capacity = data_codewords(version) * 8
        if 4 + count_bits + len(data) * 8 <= capacity:
            break
    else:
        raise ValueError('text too long for a version 10 QR code')
    bits = [0, 1, 0, 0] + [(len(data) >> i) & 1 for i in reversed(range(count_bits))]
    for byte in data:
        bits += [(byte >> i) & 1 for i in reversed(range(8))]
    bits += [0] * min(4, capacity - len(bits))
    bits += [0] * (-len(bits) % 8)
    padded = [int(''.join(map(str, bits[i:i + 8])), 2) for i in range(0, len(bits), 8)]
    pad = 0xEC
    while len(padded) < capacity // 8:
        padded.append(pad)
        pad ^= 0xEC ^ 0x11
    blocks_total, ecc = BLOCKS[version], ECC_PER_BLOCK[version]
    raw = raw_modules(version) // 8
    short_blocks = blocks_total - raw % blocks_total
    short_length = raw // blocks_total
    divisor = rs_divisor(ecc)
    blocks, at = [], 0
    for i in range(blocks_total):
        size = short_length - ecc + (0 if i < short_blocks else 1)
        block = padded[at:at + size]
        at += size
        remainder = rs_remainder(block, divisor)
        if i < short_blocks:
            block = block + [None]
        blocks.append(block + remainder)
    result = []
    for i in range(len(blocks[0])):
        for j, block in enumerate(blocks):
            if i != short_length - ecc or j >= short_blocks:
                result.append(block[i])
    return version, [b for b in result if b is not None]


def format_bits(mask):
    data = (1 << 3) | mask  # error correction level L is 01
    remainder = data
    for _ in range(10):
        remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537)
    return ((data << 10) | remainder) ^ 0x5412


def version_bits(version):
    remainder = version
    for _ in range(12):
        remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1F25)
    return (version << 12) | remainder


MASKS = (
    lambda x, y: (x + y) % 2 == 0,
    lambda x, y: y % 2 == 0,
    lambda x, y: x % 3 == 0,
    lambda x, y: (x + y) % 3 == 0,
    lambda x, y: (x // 3 + y // 2) % 2 == 0,
    lambda x, y: x * y % 2 + x * y % 3 == 0,
    lambda x, y: (x * y % 2 + x * y % 3) % 2 == 0,
    lambda x, y: ((x + y) % 2 + x * y % 3) % 2 == 0,
)


class _Grid:
    def __init__(self, version):
        self.version, self.size = version, version * 4 + 17
        self.dark = [[False] * self.size for _ in range(self.size)]
        self.function = [[False] * self.size for _ in range(self.size)]

    def set(self, x, y, dark):
        self.dark[y][x] = dark
        self.function[y][x] = True

    def function_patterns(self):
        size = self.size
        for i in range(size):
            self.set(6, i, i % 2 == 0)
            self.set(i, 6, i % 2 == 0)
        for cx, cy in ((3, 3), (size - 4, 3), (3, size - 4)):
            for dy in range(-4, 5):
                for dx in range(-4, 5):
                    x, y = cx + dx, cy + dy
                    if 0 <= x < size and 0 <= y < size:
                        distance = max(abs(dx), abs(dy))
                        self.set(x, y, distance not in (2, 4))
        positions = alignment_positions(self.version)
        last = len(positions) - 1
        for i, ax in enumerate(positions):
            for j, ay in enumerate(positions):
                if (i, j) in ((0, 0), (0, last), (last, 0)):
                    continue
                for dy in range(-2, 3):
                    for dx in range(-2, 3):
                        self.set(ax + dx, ay + dy, max(abs(dx), abs(dy)) != 1)
        self.format(0)  # reserved now, drawn for real once the mask is known
        if self.version >= 7:
            bits = version_bits(self.version)
            for i in range(18):
                bit = (bits >> i) & 1 == 1
                a, b = size - 11 + i % 3, i // 3
                self.set(a, b, bit)
                self.set(b, a, bit)

    def format(self, mask):
        bits, size = format_bits(mask), self.size
        bit = lambda i: (bits >> i) & 1 == 1  # noqa: E731
        for i in range(6):
            self.set(8, i, bit(i))
        self.set(8, 7, bit(6))
        self.set(8, 8, bit(7))
        self.set(7, 8, bit(8))
        for i in range(9, 15):
            self.set(14 - i, 8, bit(i))
        for i in range(8):
            self.set(size - 1 - i, 8, bit(i))
        for i in range(8, 15):
            self.set(8, size - 15 + i, bit(i))
        self.set(8, size - 8, True)

    def place(self, data):
        size, index = self.size, 0
        right = size - 1
        while right >= 1:
            if right == 6:
                right = 5
            for vertical in range(size):
                for j in range(2):
                    x = right - j
                    upward = (right + 1) & 2 == 0
                    y = size - 1 - vertical if upward else vertical
                    if not self.function[y][x] and index < len(data) * 8:
                        self.dark[y][x] = (data[index >> 3] >> (7 - (index & 7))) & 1 == 1
                        index += 1
            right -= 2

    def masked(self, mask):
        rule = MASKS[mask]
        grid = [row[:] for row in self.dark]
        for y in range(self.size):
            for x in range(self.size):
                if not self.function[y][x] and rule(x, y):
                    grid[y][x] = not grid[y][x]
        return grid


def penalty(grid):
    """The standard's four penalty rules; the mask with the lowest total is used."""
    size, total = len(grid), 0
    lines = [row for row in grid] + [[grid[y][x] for y in range(size)] for x in range(size)]
    for line in lines:
        run = 1
        for i in range(1, size + 1):
            if i < size and line[i] == line[i - 1]:
                run += 1
                continue
            if run >= 5:
                total += run - 2
            run = 1
        padded = [False] * 4 + line + [False] * 4
        for i in range(len(padded) - 10):
            window = padded[i:i + 11]
            if window in ([True, False, True, True, True, False, True, False, False, False, False],
                          [False, False, False, False, True, False, True, True, True, False, True]):
                total += 40
    for y in range(size - 1):
        for x in range(size - 1):
            if grid[y][x] == grid[y][x + 1] == grid[y + 1][x] == grid[y + 1][x + 1]:
                total += 3
    dark, area = sum(map(sum, grid)), size * size
    total += ((abs(dark * 20 - area * 10) + area - 1) // area - 1) * 10
    return total


def qr_matrix(text, mask=None):
    """The QR code for text as rows of booleans (True is dark), without a quiet zone."""
    version, data = codewords(text)
    grid = _Grid(version)
    grid.function_patterns()
    grid.place(data)
    masks = [mask] if mask is not None else range(8)
    best = None
    for candidate in masks:
        grid.format(candidate)
        result = grid.masked(candidate)
        score = penalty(result) if mask is None else 0
        if best is None or score < best[0]:
            best = (score, result)
    return best[1]


def qr_rows(text, quiet=2):
    """The QR code as strings of 0 (light) and 1 (dark), with a light quiet zone around it."""
    grid = qr_matrix(text)
    width = len(grid) + 2 * quiet
    blank = '0' * width
    rows = [blank] * quiet
    rows += ['0' * quiet + ''.join('1' if cell else '0' for cell in row) + '0' * quiet for row in grid]
    return rows + [blank] * quiet


# ----------------------------------------------------------------------------------- linking

PAGES = 'https://j5cfk7qm49-web.github.io/WrOS'
RETURN_URL = PAGES + '/link/'
TOKEN_URL = 'https://oauth2.googleapis.com/token'
DEVICE_URL = 'https://oauth2.googleapis.com/device/code'
REVOKE_URL = 'https://oauth2.googleapis.com/revoke'
DRIVE_FILES = 'https://www.googleapis.com/drive/v3/files'
DEVICE_GRANT = 'urn:ietf:params:oauth:grant-type:device_code'
FULL_SCOPE = 'https://www.googleapis.com/auth/drive'
LIMITED_SCOPE = 'https://www.googleapis.com/auth/drive.file'
LINK_SECONDS = 25 * 60   # Google's device code lasts 30 minutes
MAILBOX_SECONDS = 180    # after the laptop is approved, the phone has already saved the code
MAILBOX_PREFIX = 'WrOS linking '


def challenge(verifier):
    return base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b'=').decode()


def private(address):
    """A home or school network address (10/8, 172.16/12, 192.168/16), never the internet's."""
    try:
        ip = ipaddress.IPv4Address(address)
    except ValueError:
        return False
    return any(ip in ipaddress.IPv4Network(n) for n in ('10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16'))


def pack_address(address, port):
    raw = ipaddress.IPv4Address(address).packed + port.to_bytes(2, 'big')
    return base64.urlsafe_b64encode(raw).decode()


def unpack_address(packed):
    raw = base64.urlsafe_b64decode(packed)
    return str(ipaddress.IPv4Address(raw[:4])), int.from_bytes(raw[4:], 'big')


def local_address():
    """The address this laptop uses on its network, found without sending anything."""
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        probe.connect(('10.255.255.255', 9))
        address = probe.getsockname()[0]
    except OSError:
        return None
    finally:
        probe.close()
    return address if private(address) else None


class Listener:
    """Accept the phone's "Finish linking" callback for this session."""

    PAGE = ('<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">'
            '<title>WrOS</title><body style="font:20px system-ui;background:#111;color:#eee;margin:32px 16px">'
            '<p>{}</p></body>')

    def __init__(self, host):
        self.server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server.bind((host, 0))
        self.server.listen(4)
        self.address = self.server.getsockname()

    def wait(self, seconds, state):
        """(code, answer) for a well-formed /linked request within `seconds`, else (None, None)."""
        self.server.settimeout(max(seconds, 0.01))
        try:
            client, _ = self.server.accept()
        except (socket.timeout, OSError):
            return None, None
        client.settimeout(5)
        try:
            request = b''
            while b'\r\n\r\n' not in request and len(request) < 8192:
                chunk = client.recv(2048)
                if not chunk:
                    break
                request += chunk
            line = request.split(b'\r\n', 1)[0].decode('ascii', 'replace').split(' ')
            if len(line) == 3 and line[0] == 'GET':
                target = urllib.parse.urlsplit(line[1])
                query = urllib.parse.parse_qs(target.query)
                code = query.get('code', [''])[0]
                returned_state = query.get('state', [''])[0]
                if (target.path == '/linked' and hmac.compare_digest(returned_state.encode(), state.encode())
                        and 10 <= len(code) <= 512 and all(32 < ord(ch) < 127 for ch in code)):
                    return code, lambda text: self.answer(client, 200, text)
            self.answer(client, 404, 'This is not a WrOS linking address.')
        except (OSError, ValueError):
            client.close()
        return None, None

    def answer(self, client, status, text):
        body = self.PAGE.format(text.replace('&', '&amp;').replace('<', '&lt;')).encode()
        head = (f'HTTP/1.1 {status} {"OK" if status == 200 else "Not Found"}\r\n'
                'Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\n'
                f'Content-Length: {len(body)}\r\nConnection: close\r\n\r\n').encode()
        try:
            client.sendall(head + body)
        except OSError:
            pass
        finally:
            client.close()

    def close(self):
        self.server.close()


def http(method, url, form=None, token=None):
    """(status, decoded JSON or raw bytes); never raises for an HTTP error status."""
    data = urllib.parse.urlencode(form).encode() if form is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    if token:
        request.add_header('Authorization', 'Bearer ' + token)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status, body = response.status, response.read(1024 * 1024)
    except urllib.error.HTTPError as error:
        status, body = error.code, error.read(1024 * 1024)
    except (urllib.error.URLError, OSError, ValueError):
        return 0, {'error': 'network'}
    if url.endswith('?alt=media'):
        return status, body
    try:
        return status, json.loads(body or b'{}')
    except ValueError:
        return status, {}


class Link:
    """One linking: the offer on screen, then whichever way the phone's approval arrives first."""

    def __init__(self, clients, *, http=http, clock=time.monotonic, sleep=time.sleep, listener=None,
                 offer=lambda offer: None, verifier=None):
        self.linking, self.mailbox = clients['linking'], clients['mailbox']
        self.http, self.clock, self.sleep = http, clock, sleep
        self.listener, self.offer = listener, offer
        self.verifier = verifier or secrets.token_urlsafe(48)
        self.challenge = challenge(self.verifier)
        self.key = MAILBOX_PREFIX + self.challenge[:16]
        self.answer = None

    def run(self):
        status, device = self.http('POST', DEVICE_URL, {'client_id': self.mailbox['client_id'],
                                                        'scope': LIMITED_SCOPE})
        if status != 200 or not isinstance(device.get('user_code'), str):
            raise ValueError('Google did not start the sign-in; check the network and try again.')
        address = pack_address(*self.listener.address) if self.listener else '-'
        url = f"{PAGES}/l/#{self.challenge}.{address}.{device['user_code']}"
        self.offer({'link': url, 'code': device['user_code'], 'qr': qr_rows(url)})
        interval = max(int(device.get('interval', 5)), 5)
        deadline = self.clock() + min(LINK_SECONDS, int(device.get('expires_in', LINK_SECONDS)))
        device_open, limited, approved_at = True, None, None
        while self.clock() < deadline:
            started = self.clock()
            if self.listener:
                code, answer = self.listener.wait(interval, self.challenge)
                if code:
                    self.answer = answer
                    token = self.exchange(code)
                    if token is not None:
                        return token, 'wifi'
                    answer('WrOS did not link with this approval. Scan the laptop code again.')
                    self.answer = None
            if limited:
                for code in self.collect(limited):
                    token = self.exchange(code)
                    if token is not None:
                        return token, 'drive'
                if self.clock() - approved_at > MAILBOX_SECONDS:
                    raise ValueError('The laptop was approved, but the phone had not saved its approval '
                                     'in Drive. Scan the code again.')
            elif device_open:
                status, reply = self.http('POST', TOKEN_URL, {
                    'client_id': self.mailbox['client_id'], 'client_secret': self.mailbox['client_secret'],
                    'device_code': device['device_code'], 'grant_type': DEVICE_GRANT})
                if status == 200 and isinstance(reply.get('access_token'), str):
                    limited, approved_at = reply['access_token'], self.clock()
                    continue
                if reply.get('error') == 'slow_down':
                    interval += 5
                elif reply.get('error') not in ('authorization_pending', 'network'):
                    device_open = False  # declined or expired: the Wi-Fi way may still answer
            remaining = interval - (self.clock() - started)
            if remaining > 0:
                self.sleep(remaining)
        raise ValueError('Linking did not finish in 25 minutes. Start it again when the phone is ready.')

    def collect(self, limited, *, discard=False):
        """Read and remove the phone's matching mailbox entries."""
        query = urllib.parse.urlencode({'q': f"name = '{self.key}' and trashed = false",
                                        'fields': 'files(id)', 'spaces': 'drive'})
        status, listing = self.http('GET', f'{DRIVE_FILES}?{query}', token=limited)
        files = listing.get('files', []) if status == 200 and isinstance(listing, dict) else []
        codes = []
        for entry in files:
            url = f"{DRIVE_FILES}/{entry['id']}"
            if not discard:
                status, body = self.http('GET', f'{url}?alt=media', token=limited)
                if status != 200 or not isinstance(body, bytes):
                    continue
                code = body.decode('utf-8', 'replace').strip()
                if 10 <= len(code) <= 512 and all(32 < ord(ch) < 127 for ch in code):
                    codes.append(code)
            self.http('DELETE', url, token=limited)
        return codes

    def exchange(self, code):
        status, token = self.http('POST', TOKEN_URL, {
            'code': code, 'client_id': self.linking['client_id'], 'client_secret': self.linking['client_secret'],
            'redirect_uri': RETURN_URL, 'grant_type': 'authorization_code', 'code_verifier': self.verifier})
        if status == 400 and token.get('error') == 'invalid_grant':
            return None
        if status != 200 or not isinstance(token.get('refresh_token'), str) or not token.get('access_token'):
            self.fail('WrOS did not link: Google refused the approval. Scan the code on the laptop again.')
            raise ValueError(f"Google refused the approval ({token.get('error', status)}). Start linking again.")
        expiry = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(time.time() + int(token.get('expires_in', 3600))))
        return {'access_token': token['access_token'], 'token_type': 'Bearer',
                'refresh_token': token['refresh_token'], 'expiry': expiry}

    def finish(self, token, via):
        """After the laptop has stored the link: tell the phone, remove the saved code, stop listening."""
        if self.answer:
            self.answer('Google Drive is linked. You can close this page.')
            self.answer = None
        if via == 'wifi':
            self.collect(token['access_token'], discard=True)  # the phone saved a copy for the other way
        if self.listener:
            self.listener.close()

    def fail(self, message):
        if self.answer:
            self.answer(message)
            self.answer = None
        if self.listener:
            self.listener.close()
