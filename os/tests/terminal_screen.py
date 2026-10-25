"""Small ANSI cell tracker for Ratatui PTY assertions, not a full emulator.

Tracks cursor/erase/UTF-8 text and ignores styling/private terminal modes.
Input events and CPR replies still come from the actual shared PTY harness.
"""
import codecs
import unicodedata


class Screen:
    def __init__(self, width, height):
        self.width, self.height = width, height
        self.cells = [[' '] * width for _ in range(height)]
        self.x = self.y = 0
        self.saved = (0, 0)
        self.pending = ''
        self.decoder = codecs.getincrementaldecoder('utf-8')(errors='replace')

    @property
    def text(self):
        return '\n'.join(''.join(row) for row in self.cells)

    def _linefeed(self):
        self.y += 1
        if self.y >= self.height:
            self.cells.pop(0)
            self.cells.append([' '] * self.width)
            self.y = self.height - 1

    def _csi(self, parameters, command):
        if parameters.startswith(('?', '>', '<', '=')):
            return
        try:
            values = [int(n) if n else 0 for n in parameters.split(';')]
        except ValueError:
            return
        first = values[0] if values else 0
        n = first or 1
        if command in ('H', 'f'):
            self.y = min(self.height - 1, max(0, n - 1))
            self.x = min(self.width - 1, max(0, (values[1] if len(values) > 1 and values[1] else 1) - 1))
        elif command == 'G':
            self.x = min(self.width - 1, max(0, n - 1))
        elif command == 'A':
            self.y = max(0, self.y - n)
        elif command == 'B':
            self.y = min(self.height - 1, self.y + n)
        elif command == 'C':
            self.x = min(self.width - 1, self.x + n)
        elif command == 'D':
            self.x = max(0, self.x - n)
        elif command == 'J':
            for row in range(self.height):
                for col in range(self.width):
                    if first in (2, 3) or (first == 0 and (row, col) >= (self.y, self.x)) or (first == 1 and (row, col) <= (self.y, self.x)):
                        self.cells[row][col] = ' '
        elif command == 'K':
            for col in range(self.width):
                if first == 2 or (first == 0 and col >= self.x) or (first == 1 and col <= self.x):
                    self.cells[self.y][col] = ' '
        elif command == 's':
            self.saved = (self.x, self.y)
        elif command == 'u':
            self.x, self.y = self.saved

    def feed(self, data):
        text = self.pending + self.decoder.decode(data)
        self.pending = ''
        i = 0
        while i < len(text):
            c = text[i]
            if c == '\x1b':
                if i + 1 >= len(text):
                    break
                if text[i + 1] == '[':
                    end = i + 2
                    while end < len(text) and not ('@' <= text[end] <= '~'):
                        end += 1
                    if end == len(text):
                        break
                    self._csi(text[i + 2:end], text[end])
                    i = end + 1
                    continue
                if text[i + 1] == ']':
                    ends = [(text.find(stop, i + 2), len(stop)) for stop in ('\x07', '\x1b\\')]
                    ends = [end for end in ends if end[0] >= 0]
                    if not ends:
                        break
                    end, length = min(ends)
                    i = end + length
                    continue
                i += 2
                continue
            if c == '\r':
                self.x = 0
            elif c == '\n':
                self._linefeed()
            elif c == '\b':
                self.x = max(0, self.x - 1)
            elif c == '\t':
                self.x = min(self.width - 1, ((self.x // 8) + 1) * 8)
            elif ord(c) >= 32 and c != '\x7f':
                if unicodedata.combining(c):
                    if self.x:
                        self.cells[self.y][min(self.x - 1, self.width - 1)] += c
                else:
                    width = 2 if unicodedata.east_asian_width(c) in ('W', 'F') else 1
                    if self.x + width > self.width:
                        self.x = 0
                        self._linefeed()
                    self.cells[self.y][self.x] = c
                    if width == 2:
                        self.cells[self.y][self.x + 1] = ''
                    self.x += width
            i += 1
        self.pending = text[i:]
