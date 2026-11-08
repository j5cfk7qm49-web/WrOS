"""Reusable QEMU/QMP/serial/screen helpers for the WrOS release-candidate VM qualification.

Everything here drives a *disposable* VM from outside: QMP send-key for physical
keystrokes (so the guest keyboard layout matters), QMP screendump for evidence and
for the firmware/initramfs text that has no other channel, and one serial shell for
reading guest state. Nothing here changes product media: the serial login channel is
created by a QEMU SMBIOS OEM-string systemd credential (host command line only), not
by a kernel argument or a file on the media.

The module is import-safe on a host without QEMU; only Machine.start() needs it.
"""
import base64
import gzip
import hashlib
import json
import os
import queue
import re
import secrets
import shutil
import socket
import struct
import subprocess
import threading
import time
from pathlib import Path

OVMF_DIR = Path('/usr/share/OVMF')
OVMF_CODE = OVMF_DIR / 'OVMF_CODE_4M.fd'
OVMF_VARS = OVMF_DIR / 'OVMF_VARS_4M.fd'
OVMF_CODE_SECBOOT = OVMF_DIR / 'OVMF_CODE_4M.secboot.fd'
OVMF_VARS_MS = OVMF_DIR / 'OVMF_VARS_4M.ms.fd'
FONT_CANDIDATES = ('/usr/share/consolefonts/Lat15-VGA16.psf.gz',
                   '/usr/share/consolefonts/Uni2-VGA16.psf.gz')

# --------------------------------------------------------------------------- hashing


def sha256_file(path, chunk=1 << 20):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        while block := stream.read(chunk):
            h.update(block)
    return h.hexdigest()


def sparse_digest(path, block=4096, window=1 << 22):
    """Digest of a (possibly 16 GiB sparse) raw image that ignores zero blocks.

    Reads only data extents and skips all-zero 4 KiB blocks, so an untouched sparse
    target hashes in milliseconds and a hole is equivalent to written zeroes.
    """
    h = hashlib.sha256()
    zero = bytes(block)
    with open(path, 'rb') as stream:
        end = os.fstat(stream.fileno()).st_size
        h.update(b'size=%d\0' % end)
        offset = 0
        while offset < end:
            try:
                start = os.lseek(stream.fileno(), offset, os.SEEK_DATA)
            except OSError:  # ENXIO: no more data
                break
            stop = os.lseek(stream.fileno(), start, os.SEEK_HOLE)
            position = start
            while position < stop:
                stream.seek(position)
                data = stream.read(min(window, stop - position))
                if not data:
                    break
                for index in range(0, len(data), block):
                    piece = data[index:index + block]
                    if piece != zero[:len(piece)]:
                        h.update(struct.pack('<Q', position + index))
                        h.update(piece)
                position += len(data)
            offset = stop
    return h.hexdigest()


def free_bytes(path):
    return shutil.disk_usage(path).free


# --------------------------------------------------------------------------- keyboard layouts

_ROWS = [  # qcode, unshifted, shifted (US physical keyboard)
    ('1', '1', '!'), ('2', '2', '@'), ('3', '3', '#'), ('4', '4', '$'), ('5', '5', '%'),
    ('6', '6', '^'), ('7', '7', '&'), ('8', '8', '*'), ('9', '9', '('), ('0', '0', ')'),
    ('minus', '-', '_'), ('equal', '=', '+'),
    ('bracket_left', '[', '{'), ('bracket_right', ']', '}'), ('backslash', '\\', '|'),
    ('semicolon', ';', ':'), ('apostrophe', "'", '"'), ('grave_accent', '`', '~'),
    ('comma', ',', '<'), ('dot', '.', '>'), ('slash', '/', '?'), ('spc', ' ', ' '),
] + [(c, c, c.upper()) for c in 'abcdefghijklmnopqrstuvwxyz']
US_KEYS = {q: (u, s) for q, u, s in _ROWS}

_QWERTY_POSITIONS = {'q': 'q', 'w': 'w', 'e': 'e', 'r': 'r', 't': 't', 'y': 'y', 'u': 'u', 'i': 'i',
                     'o': 'o', 'p': 'p', 'a': 'a', 's': 's', 'd': 'd', 'f': 'f', 'g': 'g', 'h': 'h',
                     'j': 'j', 'k': 'k', 'l': 'l', ';': 'semicolon', 'z': 'z', 'x': 'x', 'c': 'c',
                     'v': 'v', 'b': 'b', 'n': 'n', 'm': 'm'}
# What Colemak prints for each US-QWERTY physical key (unshifted).
_COLEMAK_BY_POSITION = dict(zip('qwertyuiopasdfghjkl;zxcvbnm', 'qwfpgjluy;arstdhneiozxcvbkm'))
# us(colemak_dh) as xkeyboard-config defines it (the installer's "Colemak Mod-DH (ANSI)"): DH home/top rows plus
# the angle mod on the bottom row (US z/x/c/v/b keys print x/c/d/v/z). Measured from the console map in the
# installer: keycodes 44-48 give x c d v z. (Without the angle mod it would be z x c d v.)
_COLEMAK_DH_BY_POSITION = dict(zip('qwertyuiopasdfghjkl;zxcvbnm', 'qwfpbjluy;arstgmneioxcdvzkh'))


def _remap(table):
    keys = dict(US_KEYS)
    for position, produced in table.items():
        qcode = _QWERTY_POSITIONS[position]
        if produced == ';':
            keys[qcode] = (';', ':')
        else:
            keys[qcode] = (produced, produced.upper())
    return keys


_DVORAK = {
    'q': ("'", '"'), 'w': (',', '<'), 'e': ('.', '>'), 'r': ('p', 'P'), 't': ('y', 'Y'), 'y': ('f', 'F'),
    'u': ('g', 'G'), 'i': ('c', 'C'), 'o': ('r', 'R'), 'p': ('l', 'L'), 'bracket_left': ('/', '?'),
    'bracket_right': ('=', '+'), 'a': ('a', 'A'), 's': ('o', 'O'), 'd': ('e', 'E'), 'f': ('u', 'U'),
    'g': ('i', 'I'), 'h': ('d', 'D'), 'j': ('h', 'H'), 'k': ('t', 'T'), 'l': ('n', 'N'),
    'semicolon': ('s', 'S'), 'apostrophe': ('-', '_'), 'z': (';', ':'), 'x': ('q', 'Q'), 'c': ('j', 'J'),
    'v': ('k', 'K'), 'b': ('x', 'X'), 'n': ('b', 'B'), 'm': ('m', 'M'), 'comma': ('w', 'W'),
    'dot': ('v', 'V'), 'slash': ('z', 'Z'), 'minus': ('[', '{'), 'equal': (']', '}')}
DVORAK_KEYS = dict(US_KEYS)
DVORAK_KEYS.update(_DVORAK)

LAYOUT_KEYS = {'us': US_KEYS, 'dvorak': DVORAK_KEYS, 'colemak': _remap(_COLEMAK_BY_POSITION),
               'colemak_dh': _remap(_COLEMAK_DH_BY_POSITION)}


def keystrokes(text, layout='us'):
    """Return, for each character, the physical-key chord (list of qcodes) that
    produces it on the named layout. Raises on characters the layout cannot type."""
    keys = LAYOUT_KEYS[layout]
    out = {}
    for qcode, (plain, shifted) in keys.items():
        out.setdefault(plain, [qcode])
    for qcode, (plain, shifted) in keys.items():
        out.setdefault(shifted, ['shift', qcode])
    chords = []
    for char in text:
        if char == '\n':
            chords.append(['ret'])
        elif char in out:
            chords.append(list(out[char]))
        else:
            raise ValueError('layout %s cannot type %r' % (layout, char))
    return chords


def wrong_layout_differs(text, layout):
    """True when typing `text` on `layout` would produce different characters than
    the same physical keys on US QWERTY (i.e. the layout test is meaningful)."""
    return keystrokes(text, layout) != keystrokes(text, 'us')


def generated_secret(layout='colemak', length=24):
    """A random lowercase hex secret that is guaranteed to contain keys Colemak moves."""
    while True:
        value = secrets.token_hex(length // 2)
        if layout == 'us' or sum(value.count(c) for c in 'def') >= 3:
            return value


# --------------------------------------------------------------------------- QMP


class Qmp:
    """One persistent QMP connection (QEMU serves a single client): commands plus a
    timestamped event list, so RESET/SHUTDOWN events are not lost between calls."""

    def __init__(self, path):
        self.path = str(path)
        self.sock = None
        self.file = None
        self.events = []
        self.replies = queue.Queue()
        self.lock = threading.Lock()

    def _connect(self):
        sock = socket.socket(socket.AF_UNIX)
        sock.settimeout(15)
        sock.connect(self.path)
        file = sock.makefile('rwb', buffering=0)
        json.loads(file.readline())
        file.write(json.dumps({'execute': 'qmp_capabilities'}).encode() + b'\n')
        while True:
            message = json.loads(file.readline())
            if 'return' in message:
                break
        sock.settimeout(None)
        self.sock, self.file = sock, file
        while not self.replies.empty():
            self.replies.get_nowait()
        threading.Thread(target=self._reader, args=(file,), daemon=True).start()

    def _reader(self, file):
        try:
            while True:
                line = file.readline()
                if not line:
                    break
                message = json.loads(line)
                if 'event' in message:
                    self.events.append((time.monotonic(), message['event'], message.get('data')))
                else:
                    self.replies.put(message)
        except (OSError, ValueError):
            pass
        finally:
            self.replies.put(None)
            if self.file is file:
                self.sock = self.file = None

    def command(self, name, **arguments):
        with self.lock:
            if self.sock is None:
                self._connect()
            while not self.replies.empty():
                self.replies.get_nowait()
            self.file.write(json.dumps({'execute': name, 'arguments': arguments}).encode() + b'\n')
            try:
                reply = self.replies.get(timeout=60)
            except queue.Empty:
                raise RuntimeError('QMP %s: no reply' % name)
            if reply is None:
                raise RuntimeError('QMP %s: connection closed' % name)
            if 'error' in reply:
                raise RuntimeError('QMP %s: %s' % (name, reply['error']))
            return reply.get('return')

    def count(self, event):
        return sum(1 for _, name, _ in self.events if name == event)

    def send_key(self, qcodes, hold=40):
        self.command('send-key', keys=[{'type': 'qcode', 'data': q} for q in qcodes], **{'hold-time': hold})

    def screendump(self, path):
        self.command('screendump', filename=str(path), format='png')

    def quit(self):
        try:
            self.command('quit')
        except (OSError, RuntimeError, ValueError):
            pass


# --------------------------------------------------------------------------- screen OCR (PSF glyph matching)


class Font:
    """8x16 PSF console font. Only ASCII glyph indices 32..126 are used."""

    def __init__(self, path=None):
        path = Path(path) if path else next((Path(p) for p in FONT_CANDIDATES if Path(p).exists()), None)
        if path is None:
            raise FileNotFoundError('no PSF console font found')
        data = path.read_bytes()
        if data[:2] == b'\x1f\x8b':
            data = gzip.decompress(data)
        if data[:2] == b'\x36\x04':  # PSF1
            mode, height = data[2], data[3]
            count = 512 if mode & 1 else 256
            header, width = 4, 8
        elif data[:4] == b'\x72\xb5\x4a\x86':  # PSF2
            _magic, _version, header, _flags, count, _charsize, height, width = struct.unpack('<8I', data[:32])
        else:
            raise ValueError('not a PSF font')
        self.width, self.height = width, height
        stride = (width + 7) // 8
        self.glyphs = {}
        for code in range(32, 127):
            raw = data[header + code * height * stride: header + (code + 1) * height * stride]
            rows = []
            for y in range(height):
                value = int.from_bytes(raw[y * stride:(y + 1) * stride], 'big')
                rows.append(tuple((value >> (stride * 8 - 1 - x)) & 1 for x in range(width)))
            self.glyphs[chr(code)] = tuple(rows)

    def render(self, text, columns, rows, fg=(170, 170, 170), bg=(0, 0, 0)):
        from PIL import Image
        image = Image.new('RGB', (columns * self.width, rows * self.height), bg)
        px = image.load()
        for line_no, line in enumerate(text.split('\n')[:rows]):
            for col, char in enumerate(line[:columns]):
                glyph = self.glyphs.get(char)
                if not glyph:
                    continue
                for y, row in enumerate(glyph):
                    for x, bit in enumerate(row):
                        if bit:
                            px[col * self.width + x, line_no * self.height + y] = fg
        return image


def _glyph_int(glyph):
    return int.from_bytes(bytes(255 if bit else 0 for row in glyph for bit in row), 'big')


class NativeFont(Font):
    """Templates from wros-graphics' font_atlas example, including its styles."""
    def __init__(self, atlas, width, height, glyphs=None):
        self.width, self.height = width, height
        pixels = atlas.convert('L')
        self.glyphs, self._ints = {}, []
        rows = atlas.height // height // 3
        for style in range(3):
            for index, char in enumerate((glyphs or [chr(c) for c in range(32, 127)] + ['▸', '●'])[:16 * rows]):
                left, top = index % 16 * width, (index // 16 + style * rows) * height
                glyph = tuple(tuple(pixels.getpixel((left + x, top + y)) > 40
                                    for x in range(width)) for y in range(height))
                if style == 0:
                    self.glyphs[char] = glyph
                self._ints.append((char, _glyph_int(glyph)))


def native_font():
    from PIL import Image
    path = os.environ.get('WROS_NATIVE_FONT')
    if not path:
        return None
    cache = getattr(native_font, 'cache', None)
    if cache is None:
        data = Path(path).read_bytes()
        match = re.search(rb'# cells (\d+) (\d+)\n', data[:100])
        if not match:
            raise ValueError('native font atlas has no cell geometry')
        codes = re.search(rb'# codepoints ([0-9 ]+)\n', data[:4096])
        glyphs = [chr(int(code)) for code in codes[1].split()] if codes else None
        cache = native_font.cache = NativeFont(Image.open(path), int(match[1]), int(match[2]), glyphs)
    return cache


def ocr(image, font=None, offset=(0, 0)):
    """Return the screen as text rows. Cells that match no glyph well become '?'."""
    from PIL import Image, ImageChops
    font = font or Font()
    image = image.convert('RGB')
    w, h = image.size
    cw, ch = font.width, font.height
    colors = image.getcolors(maxcolors=1 << 24)
    background = max(colors)[1]
    mask = ImageChops.difference(image, Image.new('RGB', image.size, background)).convert('L')
    raw = mask.point(lambda v: 255 if v > 40 else 0).tobytes()
    table = getattr(font, '_ints', None)
    if table is None:
        table = font._ints = [(char, _glyph_int(glyph)) for char, glyph in font.glyphs.items()]
    cols, rows = (w - offset[0]) // cw, (h - offset[1]) // ch
    empty = bytes(cw)
    lines = []
    for r in range(rows):
        chars = []
        for c in range(cols):
            x0, y0 = offset[0] + c * cw, offset[1] + r * ch
            parts = [raw[(y0 + y) * w + x0:(y0 + y) * w + x0 + cw] for y in range(ch)]
            cell_bytes = b''.join(parts)
            if isinstance(font, NativeFont) and any(cell_bytes):
                # Selection backgrounds differ from the page. Match glyph edges
                # against each cell's own background rather than the whole page.
                tile = image.crop((x0, y0, x0 + cw, y0 + ch))
                local_bg = max(tile.getcolors(maxcolors=cw * ch))[1]
                contrast = ImageChops.difference(tile, Image.new('RGB', tile.size, local_bg)).convert('L')
                cutoff = contrast.getextrema()[1] * 40 / 255
                cell_bytes = contrast.point(lambda v: 255 if v > cutoff else 0).tobytes()
            if not any(cell_bytes):
                chars.append(' ')
                continue
            cell = int.from_bytes(cell_bytes, 'big')
            on = cell.bit_count() // 8
            best, best_d = '?', 10 ** 9
            for char, glyph in table:
                d = (cell ^ glyph).bit_count() // 8
                if d < best_d:
                    best, best_d = char, d
            chars.append(best if best_d <= max(6, on // 3) else '?')
        lines.append(''.join(chars).rstrip())
    return lines


def png_text(path, font=None):
    from PIL import Image
    picture = Image.open(path)
    if font is not None:
        return '\n'.join(ocr(picture, font))
    candidates = ['\n'.join(ocr(picture))]
    if native_font() is not None:
        candidates.append('\n'.join(ocr(picture, native_font())))
    # Firmware and Debian's prompt retain the console font; native screens use
    # the exact rasterizer templates. Keep the more readable interpretation.
    return max(candidates, key=lambda text: sum(map(len, re.findall(r'[A-Za-z]{3,}', text))) - text.count('?') * 2)


def image_signature(path):
    from PIL import Image
    image = Image.open(path).convert('L').resize((80, 48))
    return hashlib.sha256(image.tobytes()).hexdigest()


# --------------------------------------------------------------------------- serial shell


class Serial:
    """A unix-socket serial console with a reader thread, a transcript file and expect()."""

    def __init__(self, path, transcript, alive=lambda: True):
        self.path = str(path)
        self.buffer = bytearray()
        self.cursor = 0
        self.lock = threading.Lock()
        self.transcript = open(transcript, 'wb')
        self.alive = alive
        self.sock = None
        self.closed = False

    def connect(self, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if os.path.exists(self.path):
                try:
                    self.sock = socket.socket(socket.AF_UNIX)
                    self.sock.connect(self.path)
                    self.sock.settimeout(0.5)
                    break
                except OSError:
                    self.sock.close()
                    self.sock = None
            time.sleep(0.1)
        if self.sock is None:
            raise RuntimeError('serial socket did not appear')
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        while not self.closed:
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                continue
            except OSError:
                break
            if not chunk:
                break
            with self.lock:
                self.buffer.extend(chunk)
                self.transcript.write(chunk)
                self.transcript.flush()

    def send(self, data):
        if isinstance(data, str):
            data = data.encode()
        self.sock.sendall(data)

    def expect(self, pattern, timeout=120, consume=True):
        if isinstance(pattern, (str, bytes)):
            pattern = re.compile(re.escape(pattern if isinstance(pattern, bytes) else pattern.encode()))
        elif isinstance(pattern.pattern, str):
            pattern = re.compile(pattern.pattern.encode(), pattern.flags & ~re.UNICODE)
        deadline = time.monotonic() + timeout
        while True:
            with self.lock:
                match = pattern.search(self.buffer, self.cursor)
                if match:
                    if consume:
                        self.cursor = match.end()
                    return match
            if time.monotonic() > deadline:
                raise TimeoutError('serial: %r not seen within %ss' % (pattern.pattern, timeout))
            if not self.alive():
                with self.lock:
                    match = pattern.search(self.buffer, self.cursor)
                    if match:
                        self.cursor = match.end()
                        return match
                raise RuntimeError('guest stopped while waiting for %r' % (pattern.pattern,))
            time.sleep(0.05)

    def tail(self, n=2000):
        with self.lock:
            return bytes(self.buffer[-n:]).decode('utf-8', 'replace')

    def contains(self, needle):
        data = needle.encode() if isinstance(needle, str) else needle
        with self.lock:
            return data in self.buffer

    def close(self):
        self.closed = True
        try:
            if self.sock:
                self.sock.close()
        finally:
            self.transcript.close()


class Shell:
    """Marker-delimited commands over a login shell on a serial console."""

    def __init__(self, serial):
        self.serial = serial

    def login(self, user, password, timeout=300):
        s = self.serial
        s.expect(re.compile(r'login: ?'), timeout)
        s.send(user + '\n')
        s.expect(re.compile(r'[Pp]assword: ?'), 30)
        s.send(password + '\n')
        # Plain, echo-free, prompt-free bash: only our markers appear in output.
        time.sleep(1.5)
        s.send("exec env PS1= TERM=dumb HISTFILE= bash --noediting --norc --noprofile\n")
        time.sleep(1)
        s.send('stty -echo -onlcr; echo RCSHELL_READY_%s\n' % user)
        s.expect(re.compile(r'^RCSHELL_READY_%s\r?$' % re.escape(user), re.M), 45)

    def run(self, command, timeout=60, check=True, quiet_secret=None):
        tag = secrets.token_hex(6)
        # printf '\n...' keeps the end marker at a line start even when the command's output has no final newline
        command = command.rstrip().rstrip(';')
        # ONE line: sudo's pty relay swallows input already queued on the tty, so a second line would be lost.
        self.serial.send("echo @@S%s; { %s; }; rc_=$?; printf '\\n@@E%s:%%s\\n' $rc_\n" % (tag, command, tag))
        match = self.serial.expect(re.compile(r'^@@E%s:([0-9]+)\r?$' % tag, re.M), timeout, consume=False)
        with self.serial.lock:
            blob = bytes(self.serial.buffer)
        start = re.search(rb'^@@S%s\r?\n' % tag.encode(), blob, re.M)
        out = blob[start.end():match.start()].decode('utf-8', 'replace') if start else ''
        with self.serial.lock:
            self.serial.cursor = match.end()
        rc = int(match.group(1))
        out = out.replace('\r', '')
        if check and rc != 0:
            raise RuntimeError('guest command failed (%d): %s\n%s' % (rc, command[:200], out[-1500:]))
        return rc, out

    def put(self, path, data, mode='0600'):
        if isinstance(data, str):
            data = data.encode()
        encoded = base64.b64encode(data).decode()
        if len(encoded) > 6000:
            # a long heredoc overruns the serial tty (XOFF stalls the socket); send acknowledged chunks instead
            self.run('umask 077; : > %s.b64' % path, 30)
            for i in range(0, len(encoded), 3000):
                self.run("printf %%s '%s' >> %s.b64" % (encoded[i:i + 3000], path), 30)
            self.run('umask 077; base64 -d %s.b64 > %s && rm -f %s.b64 && chmod %s %s && sha256sum %s' % (path, path, path, mode, path, path), 60)
            return
        tag = 'RCEND' + secrets.token_hex(4)
        body = '\n'.join(encoded[i:i + 400] for i in range(0, len(encoded), 400))
        self.serial.send("umask 077; base64 -d > %s <<'%s'\n%s\n%s\n" % (path, tag, body, tag))
        self.run('chmod %s %s && sha256sum %s' % (mode, path, path), 60)

    def sudo_python(self, script_path, *args, timeout=60):
        rc, out = self.run('sudo -n python3 -I %s %s' % (script_path, ' '.join(args)), timeout, check=False)
        return rc, out


# --------------------------------------------------------------------------- machine


class Machine:
    """One disposable QEMU instance. Builds an explicit command line (recorded in
    `command` for the result.json), never attaches a host disk and never a NIC."""

    def __init__(self, run, name, memory=4096, smp=2, accel='kvm', reboot=False):
        self.run = Path(run)
        self.name = name
        self.memory, self.smp, self.accel = memory, smp, accel
        self.reboot = reboot
        self.serial_sock = self.run / (name + '-serial.sock')
        self.qmp_sock = self.run / (name + '-qmp.sock')
        self.args = []
        self.process = None
        self.serial = None
        self.qmp = Qmp(self.qmp_sock)
        self.log = None
        self.command = None
        self.credentials = []

    # drive builders -- each returns self for chaining
    def firmware(self, vars_copy, secure_boot=False):
        code = OVMF_CODE_SECBOOT if secure_boot else OVMF_CODE
        self.args += ['-drive', 'if=pflash,format=raw,unit=0,readonly=on,file=%s' % code,
                      '-drive', 'if=pflash,format=raw,unit=1,file=%s' % vars_copy]
        if secure_boot:
            self.args += ['-global', 'driver=cfi.pflash01,property=secure,value=on']
        return self

    def cdrom(self, iso, bootindex=1):
        self.args += ['-drive', 'if=none,id=cd0,media=cdrom,format=raw,readonly=on,file=%s' % iso,
                      '-device', 'ide-cd,drive=cd0,bootindex=%d' % bootindex]
        return self

    def usb_stick(self, path, fmt='raw', readonly=False, bootindex=None, ident='usb0', serial=None):
        self.args += ['-drive', 'if=none,id=%s,format=%s,%sfile=%s' % (ident, fmt, 'readonly=on,' if readonly else '', path)]
        if not any(a.startswith('qemu-xhci') for a in self.args):  # once, however many sticks
            self.args += ['-device', 'qemu-xhci,id=xhci']
        device = 'usb-storage,bus=xhci.0,drive=%s' % ident
        if bootindex is not None:
            device += ',bootindex=%d' % bootindex
        if serial:
            device += ',serial=%s' % serial
        self.args += ['-device', device]
        return self

    def virtio_disk(self, path, fmt='raw', serial=None, bootindex=None, ident='target'):
        self.args += ['-drive', 'if=none,id=%s,format=%s,file=%s' % (ident, fmt, path)]
        device = 'virtio-blk-pci,drive=%s' % ident
        if serial:
            device += ',serial=%s' % serial
        if bootindex is not None:
            device += ',bootindex=%d' % bootindex
        self.args += ['-device', device]
        return self

    def serial_login_credential(self, tty='ttyS0'):
        # systemd-getty-generator honours this SMBIOS OEM-string credential; it exists only on
        # the QEMU command line, so the installed/ISO configuration stays as shipped.
        self.credentials.append('getty.ttys.serial=%s' % tty)
        return self

    def command_line(self):
        smbios = []
        for cred in self.credentials:
            smbios += ['-smbios', 'type=11,value=io.systemd.credential:%s' % cred]
        cpu = 'host' if self.accel == 'kvm' else 'Nehalem'
        return (['qemu-system-x86_64', '-name', self.name, '-accel', self.accel, '-machine', 'q35',
                 '-cpu', cpu, '-m', str(self.memory), '-smp', str(self.smp), '-nic', 'none',
                 '-display', 'none', '-vga', 'std'] + ([] if self.reboot else ['-no-reboot']) + ['-boot', 'menu=off',
                 '-serial', 'unix:%s,server=on,wait=off' % self.serial_sock,
                 '-qmp', 'unix:%s,server=on,wait=off' % self.qmp_sock] + smbios + self.args)

    def start(self):
        if os.geteuid() == 0:
            raise RuntimeError('run QEMU as the unprivileged builder')
        for sock in (self.serial_sock, self.qmp_sock):
            sock.unlink(missing_ok=True)
        self.command = self.command_line()
        (self.run / (self.name + '-command.json')).write_text(json.dumps(self.command, indent=1) + '\n')
        self.log = open(self.run / (self.name + '-qemu.log'), 'wb')
        self.process = subprocess.Popen(self.command, stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log)
        deadline = time.monotonic() + 20
        while not (self.qmp_sock.exists() and self.serial_sock.exists()):
            if self.process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('QEMU did not start: %s' % (self.run / (self.name + '-qemu.log')))
            time.sleep(0.1)
        self.serial = Serial(self.serial_sock, self.run / (self.name + '-serial.log'), self.is_running)
        self.serial.connect()
        return self

    def is_running(self):
        return self.process is not None and self.process.poll() is None

    def key(self, *qcodes, hold=40, gap=0.15):
        self.qmp.send_key(list(qcodes), hold)
        time.sleep(gap)

    def ctrl(self, letter, layout='us'):
        """Ctrl+<letter> as the application sees it: press the physical key that produces `letter` on `layout`."""
        chord = keystrokes(letter, layout)[0]
        self.key('ctrl', *chord)

    def type_text(self, text, layout='us', gap=0.12):
        for chord in keystrokes(text, layout):
            self.qmp.send_key(chord, 40)
            time.sleep(gap)

    def shot(self, name):
        path = self.run / (name + '.png')
        self.qmp.screendump(path)
        return path

    def wait_exit(self, timeout=120):
        deadline = time.monotonic() + timeout
        while self.is_running():
            if time.monotonic() > deadline:
                raise TimeoutError('QEMU did not exit')
            time.sleep(0.3)
        return self.process.returncode

    def kill(self):
        """QMP quit == power cut (no guest shutdown)."""
        self.qmp.quit()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()

    def stop(self):
        if self.is_running():
            try:
                self.shot('final-screen')
            except Exception:
                pass
            self.process.terminate()
            try:
                self.process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        if self.serial:
            self.serial.close()
        if self.log:
            self.log.close()

    # screen helpers -----------------------------------------------------
    def screen_text(self, name='screen'):
        path = self.shot(name)
        return png_text(path)

    def wait_screen(self, pattern, timeout=300, interval=2.0, name='wait'):
        """Wait for OCR'd framebuffer text matching a regex (tty1/initramfs, no serial)."""
        regex = re.compile(pattern, re.I | re.S)
        deadline = time.monotonic() + timeout
        last = ''
        while time.monotonic() < deadline:
            if not self.is_running():
                raise RuntimeError('guest stopped while waiting for screen text %r' % pattern)
            last = self.screen_text(name)
            if regex.search(last):
                return last
            time.sleep(interval)
        raise TimeoutError('screen text %r not seen; last screen:\n%s' % (pattern, last))


# --------------------------------------------------------------------------- disks


def create_sparse_target(path, size_bytes):
    with open(path, 'xb') as stream:
        stream.truncate(size_bytes)
    os.chmod(path, 0o600)


def create_overlay(base, overlay, base_format='raw'):
    subprocess.run(['qemu-img', 'create', '-q', '-f', 'qcow2', '-b', str(base), '-F', base_format, str(overlay)],
                   check=True)


def fresh_vars(run, name='OVMF_VARS.fd', source=OVMF_VARS):
    path = Path(run) / name
    shutil.copyfile(source, path)
    path.chmod(0o600)
    return path


def make_update_media(bundle_dir, image, fs='ext4', size_mb=48, label='WROS-UPDATE'):
    """Build a small disposable filesystem image labelled WROS-UPDATE holding the bundle
    directory's files at the filesystem root. No root privileges needed."""
    bundle = Path(bundle_dir)
    if not (bundle / 'InRelease').is_file() and not (bundle / 'Release').is_file():
        raise ValueError('not an update bundle directory: %s' % bundle)
    image = Path(image)
    with open(image, 'xb') as stream:
        stream.truncate(size_mb << 20)
    if fs == 'ext4':
        subprocess.run(['mke2fs', '-q', '-t', 'ext4', '-L', label, '-d', str(bundle), str(image)], check=True)
    elif fs == 'vfat':
        subprocess.run(['mkfs.vfat', '-n', label, str(image)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(['mcopy', '-i', str(image), '-s', '-p'] + [str(p) for p in sorted(bundle.iterdir())] + ['::/'],
                       check=True)
    else:
        raise ValueError(fs)
    return image


def flip_byte_in_ext4(image, path_in_fs, offset=1024):
    """Flip one byte of a file inside an ext4 image using debugfs (offline, no mount)."""
    tmp = Path(str(image) + '.deb.tmp')
    subprocess.run(['debugfs', '-R', 'dump %s %s' % (path_in_fs, tmp), str(image)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    data = bytearray(tmp.read_bytes())
    data[offset] ^= 0x01
    tmp.write_bytes(bytes(data))
    subprocess.run(['debugfs', '-w', '-R', 'rm %s' % path_in_fs, str(image)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run(['debugfs', '-w', '-R', 'write %s %s' % (tmp, path_in_fs), str(image)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    tmp.unlink()
