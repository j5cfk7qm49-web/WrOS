#!/usr/bin/env python3
"""Paint rc30 owner previews from the real interface's cells. Requires Pillow.

    GRIMOIRE_PREVIEW=/tmp/preview.json cargo test --locked owner_preview -- --ignored
    python3 tools/unifont-preview.py /tmp/preview.json /tmp/preview
"""

import argparse
from functools import cache
import gzip
from html import escape
import json
from math import floor
from pathlib import Path
import re
from struct import Struct
import unicodedata

from PIL import Image, ImageColor

FONTS = Path(__file__).resolve().parents[1] / "assets/fonts"
CELL_WIDTH, CELL_HEIGHT = 8, 16
LAPTOP_COLUMNS, LAPTOP_WIDTH = 170, 1366
FIT_WIDTH = 7

# The four preview presets' terminal backgrounds from tools/screenshots.py.
BACKGROUNDS = {
    "Grimoire": "#0d0d10",
    "Gruvbox Dark": "#282828",
    "Nord": "#2e3440",
    "Rainbow": "#15121f",
}
ANSI_NAMES = (
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "gray",
    "darkgray", "lightred", "lightgreen", "lightyellow", "lightblue",
    "lightmagenta", "lightcyan", "white",
)
ANSI = (
    (0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0),
    (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
    (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0),
    (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
)
BOLD, ITALIC, UNDERLINE, REVERSED, DIM, HIDDEN, CROSSED_OUT = (1 << i for i in range(7))


def colour(value, reset):
    """Map colors as in wros-graphics/src/lib.rs."""
    if value is None:
        return reset
    if isinstance(value, str):
        if value.startswith("#"):
            return ImageColor.getrgb(value)
        value = ANSI_NAMES.index(value.lower())
    if value < 16:
        return ANSI[value]
    if value <= 231:
        cube = (0, 95, 135, 175, 215, 255)
        n = value - 16
        return cube[n // 36], cube[n // 6 % 6], cube[n % 6]
    gray = 8 + 10 * (value - 232)
    return gray, gray, gray


def load_glyphs():
    with gzip.open(FONTS / "unifont-18.0.01.hex.gz", "rt", encoding="ascii") as source:
        glyphs = {
            chr(int(code, 16)): bytes.fromhex(bits)
            for code, bits in (line.split(":", 1) for line in source)
        }
    with (FONTS / "wros-narrow.txt").open(encoding="utf-8") as source:
        for line in source:
            if line.startswith("U+"):
                code = re.match(r"U\+([0-9A-Fa-f]+)", line)[1]
                glyphs[chr(int(code, 16))] = bytes(
                    int(next(source).rstrip().replace(".", "0").replace("#", "1"), 2)
                    for _ in range(CELL_HEIGHT)
                )
    # The view tiles (rc31), as build.rs makes them: a 14x14 square in two cells around Unifont's
    # letter, raised a row, or the power symbol. E130-E139 are the lit copies.
    for n, (symbol, lift) in enumerate([("W", 1), ("r", 1), ("O", 1), ("S", 1), ("\ue114", 0)]):
        slot = [0] * CELL_HEIGHT
        for y, bits in enumerate(glyphs[symbol]):
            if bits:
                slot[y - lift] = bits << 4
        for y in range(CELL_HEIGHT):
            slot[y] |= 0x7FFE if y in (1, 14) else 0x4002 if 2 <= y <= 13 else 0
        for base in (0xE120, 0xE130):
            glyphs[chr(base + 2 * n)] = bytes(row >> 8 for row in slot)
            glyphs[chr(base + 2 * n + 1)] = bytes(row & 0xFF for row in slot)
    return glyphs


# build.rs uses f32. Round intermediate values the same way so close scores
# choose the same alignment when fitting a two-cell symbol.
FLOAT = Struct("=f")


def f32(value):
    return FLOAT.unpack(FLOAT.pack(value))[0]


def ink_box(grid):
    ys = [y for y, row in enumerate(grid) if any(row)]
    if not ys:
        return None
    xs = [x for x, column in enumerate(zip(*grid)) if any(column)]
    return xs[0], xs[-1], ys[0], ys[-1]


def thin(grid):
    """Zhang-Suen thinning, as in build.rs."""
    grid = [row[:] for row in grid]
    w, h = len(grid[0]), len(grid)
    neighbours = ((0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1))
    while True:
        changed = False
        for groups in (((0, 2, 4), (2, 4, 6)), ((0, 2, 6), (0, 4, 6))):
            remove = []
            for y, row in enumerate(grid):
                for x, on in enumerate(row):
                    if not on:
                        continue
                    ring = [
                        0 <= x + dx < w and 0 <= y + dy < h and grid[y + dy][x + dx]
                        for dx, dy in neighbours
                    ]
                    if not 2 <= sum(ring) <= 6:
                        continue
                    if sum(not ring[i] and ring[(i + 1) % 8] for i in range(8)) != 1:
                        continue
                    if not any(all(ring[i] for i in group) for group in groups):
                        remove.append((x, y))
            for x, y in remove:
                grid[y][x] = False
            changed |= bool(remove)
        if not changed:
            return grid


def shape(grid):
    """Count separate strokes and enclosed holes with 4-connected regions."""
    w, h = len(grid[0]), len(grid)
    strokes = holes = 0
    seen = set()
    for y, row in enumerate(grid):
        for x, on in enumerate(row):
            if (x, y) in seen:
                continue
            stack = [(x, y)]
            seen.add((x, y))
            edge = False
            while stack:
                cx, cy = stack.pop()
                edge |= cx in (0, w - 1) or cy in (0, h - 1)
                for nx, ny in ((cx, cy + 1), (cx, cy - 1), (cx + 1, cy), (cx - 1, cy)):
                    if (0 <= nx < w and 0 <= ny < h and (nx, ny) not in seen
                            and grid[ny][nx] == on):
                        seen.add((nx, ny))
                        stack.append((nx, ny))
            strokes += on
            holes += not on and not edge
    return strokes, holes


def blur(grid):
    """Two passes of build.rs's 3×3 binomial filter."""
    w, h = len(grid[0]), len(grid)
    for _ in range(2):
        blurred = [[0.0] * w for _ in range(h)]
        for y in range(h):
            for x in range(w):
                total = weight = 0
                for yy in range(max(y - 1, 0), min(y + 2, h)):
                    for xx in range(max(x - 1, 0), min(x + 2, w)):
                        k = (2 if yy == y else 1) * (2 if xx == x else 1)
                        total = f32(total + f32(k * grid[yy][xx]))
                        weight += k
                blurred[y][x] = f32(total / weight)
        grid = blurred
    return grid


def sample_axis(start, scale, offset, size):
    return [
        [floor(f32(start + f32(f32(i + (sub + 0.5) / 4 - offset) / scale)))
         for sub in range(4)]
        for i in range(size)
    ]


@cache
def fit_one_cell(bitmap):
    """Fit like build.rs, scoring coverage and thinned strokes at nine alignments."""
    original = [
        [bool(int.from_bytes(bitmap[y:y + 2], "big") & (0x8000 >> x)) for x in range(16)]
        for y in range(0, len(bitmap), 2)
    ]
    bounds = ink_box(original)
    if bounds is None:
        return bytes(CELL_HEIGHT)
    x0, x1, y0, y1 = bounds
    iw, ih = x1 - x0 + 1, y1 - y0 + 1
    scale = f32(min(FIT_WIDTH / iw, 1))
    w = max(1, floor(f32(iw * scale) + 0.5)) + 1
    h = max(1, floor(f32(ih * scale) + 0.5)) + 1
    wanted = shape(original)
    skeleton = thin(original)
    best_score, best = float("inf"), None
    offsets = (0, f32(1 / 3), f32(2 / 3))
    for ox in offsets:
        samples_x = sample_axis(x0, scale, ox, w)
        for oy in offsets:
            samples_y = sample_axis(y0, scale, oy, h)
            reference = [
                [sum(0 <= iy < 16 and 0 <= ix < 16 and original[iy][ix]
                     for iy in sy for ix in sx) / 16 for sx in samples_x]
                for sy in samples_y
            ]
            line = [[False] * w for _ in range(h)]
            for y, row in enumerate(skeleton):
                for x, on in enumerate(row):
                    if on:
                        fx = floor(f32(f32((x + 0.5 - x0) * scale) + ox))
                        fy = floor(f32(f32((y + 0.5 - y0) * scale) + oy))
                        if 0 <= fx < w and 0 <= fy < h:
                            line[fy][fx] = True
            target = blur(reference)
            for threshold in (0.3, 0.5, 0.7, 2.0):
                for strokes in (True, False):
                    candidate = [
                        [strokes and line[y][x] or reference[y][x] >= threshold
                         for x in range(w)]
                        for y in range(h)
                    ]
                    bounds = ink_box(candidate)
                    if bounds is None:
                        continue
                    cx0, cx1, cy0, cy1 = bounds
                    if cx1 - cx0 + 1 > FIT_WIDTH or cy1 - cy0 + 1 > CELL_HEIGHT:
                        continue
                    got = shape(candidate)
                    blurred = blur(candidate)
                    diff = 0.0
                    for y in range(h):
                        for x in range(w):
                            delta = f32(target[y][x] - blurred[y][x])
                            diff = f32(diff + f32(delta * delta))
                    lost = abs(got[0] - wanted[0]) + abs(got[1] - wanted[1])
                    score = f32(diff + 3 * lost)
                    if score < best_score:
                        best_score, best = score, (candidate, bounds)
    rows = bytearray(CELL_HEIGHT)
    if best is not None:
        chosen, (cx0, cx1, cy0, cy1) = best
        left = 1 + (FIT_WIDTH - (cx1 - cx0 + 1)) // 2
        top = floor((y0 + y1 - (cy1 - cy0)) / 2 + 0.5)
        top = max(0, min(top, CELL_HEIGHT - (cy1 - cy0 + 1)))
        for y in range(cy0, cy1 + 1):
            for x in range(cx0, cx1 + 1):
                if chosen[y][x]:
                    rows[top + y - cy0] |= 0x80 >> (left + x - cx0)
    return bytes(rows)


def char_width(char):
    category = unicodedata.category(char)
    if category in ("Mn", "Me", "Cf", "Cc"):
        return 0
    return 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1


def visible_mark(char):
    """Exclude selectors, joiners and other format characters from zero-width marks."""
    code = ord(char)
    invisible = (
        (0x034F, 0x034F), (0x115F, 0x1160), (0x17B4, 0x17B5), (0x180B, 0x180F),
        (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x206F), (0x3164, 0x3164),
        (0xFE00, 0xFE0F), (0xFEFF, 0xFEFF), (0xFFA0, 0xFFA0),
        (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A), (0xE0000, 0xE0FFF),
    )
    return not any(start <= code <= end for start, end in invisible)


def glyph_rows(char, bitmap, span, width):
    # Fitting is lazy. Hand-drawn replacements and tile letters are already narrow.
    if len(bitmap) == 32 and 0x2000 <= ord(char) <= 0x2BFF and width(char) == 1:
        bitmap = fit_one_cell(bitmap)
    stride = len(bitmap) // CELL_HEIGHT
    rows = [int.from_bytes(bitmap[y:y + stride], "big") for y in range(0, len(bitmap), stride)]
    if stride * 8 > span:
        return [
            sum(bool(bits & (0xC000 >> (2 * x))) << (7 - x) for x in range(8))
            for bits in rows
        ]
    return [bits << (span - stride * 8) for bits in rows]


def glyph_mask(symbol, span, style, glyphs, width):
    base = symbol[0] if symbol else " "
    if base in glyphs:
        rows = glyph_rows(base, glyphs[base], span, width)
    else:
        # The missing-character box sits one pixel in from each side.
        sides = (1 << (span - 2)) | 2
        edge = (1 << (span - 1)) - 2
        rows = [0, 0, edge] + [sides] * 10 + [edge, 0, 0]
    for mark in symbol[1:]:
        if mark in glyphs and width(mark) == 0 and visible_mark(mark):
            rows = [a | b for a, b in zip(rows, glyph_rows(mark, glyphs[mark], span, width))]
    for y, bits in enumerate(rows):
        if style & BOLD:
            # unifont.rs tests the source pixel and its left neighbour.
            bits |= bits >> 1
        if style & ITALIC and y < 9:
            bits >>= 1
        if (style & UNDERLINE and y == CELL_HEIGHT - 1
                or style & CROSSED_OUT and y == CELL_HEIGHT // 2):
            bits = (1 << span) - 1
        rows[y] = bits
    if not any(rows):
        return None
    mask = Image.new("L", (span, CELL_HEIGHT))
    mask.putdata([255 * bool(bits & (1 << x)) for bits in rows for x in reversed(range(span))])
    return mask


def paint(frame, glyphs):
    grid_width = frame["w"] * CELL_WIDTH
    # DRM leaves six black pixels past the 170-cell grid at the laptop's resolution.
    image_width = LAPTOP_WIDTH if frame["w"] == LAPTOP_COLUMNS else grid_width
    image = Image.new("RGB", (image_width, frame["h"] * CELL_HEIGHT))
    reset_bg = (0, 0, 0) if frame["console"] else ImageColor.getrgb(BACKGROUNDS[frame["theme"]])
    widths = frame.get("widths", {})

    def width(symbol):
        if symbol in widths:
            return widths[symbol]
        return max((char_width(char) for char in symbol), default=0)

    @cache
    def mask(symbol, span, style):
        return glyph_mask(symbol, span, style, glyphs, width)

    for y, row in enumerate(frame["rows"]):
        x = 0
        while x < len(row):
            symbol, fg, bg, style = row[x]
            fg, bg = colour(fg, (221, 221, 221)), colour(bg, reset_bg)
            if style & REVERSED:
                fg, bg = bg, fg
            if style & DIM:
                fg, bg = (tuple((v * 77 + 127) // 255 for v in c) for c in (fg, bg))
            cells = 2 if width(symbol) >= 2 else 1
            left, top = x * CELL_WIDTH, y * CELL_HEIGHT
            span = min(cells * CELL_WIDTH, grid_width - left)
            box = (left, top, left + span, top + CELL_HEIGHT)
            image.paste(bg, box)
            lit = len(symbol) == 1 and 0xE130 <= ord(symbol) <= 0xE139
            if lit:
                # A lit tile (wros-graphics unifont.rs): its square filled with the ink, its lines black.
                side = (1, 8) if ord(symbol) % 2 == 0 else (0, 7)
                image.paste(fg, (left + side[0], top + 1, left + side[1], top + 15))
            if (fg != bg or lit) and not style & HIDDEN:
                ink = mask(symbol, span, style & (BOLD | ITALIC | UNDERLINE | CROSSED_OUT))
                if ink is not None:
                    image.paste((0, 0, 0) if lit else fg, box, ink)
            # As in repair_wide, the leading cell paints over the continuation cell.
            x += cells
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("frames", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    frames = json.loads(args.frames.read_text(encoding="utf-8"))
    args.output.mkdir(parents=True, exist_ok=True)
    glyphs = load_glyphs()
    figures = []
    for frame in frames:
        name, theme = frame["name"], frame["theme"]
        slug = re.sub(r"[^a-z0-9]+", "-", f"{name}-{theme}".lower()).strip("-")
        variant = "-console" if frame["console"] else ""
        filename = f"{slug}{variant}.png"
        paint(frame, glyphs).save(args.output / filename)
        caption = f"{name} · {theme}" + (" (console)" if variant else "")
        figures.append(
            f'<figure><a href="{filename}"><img src="{filename}" alt="{escape(caption)}"></a>'
            f'<figcaption>{escape(caption)}</figcaption></figure>'
        )
    (args.output / "index.html").write_text(
        "<!doctype html>\n<meta charset=utf-8>\n"
        '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
        "<title>WrOS rc30 owner preview</title>\n"
        "<style>\n"
        "body { background: #18181a; color: #eee; font: 16px system-ui; margin: 1.5rem; }\n"
        "main { display: grid; gap: 1rem;\n"
        "  grid-template-columns: repeat(auto-fit, minmax(min(100%, 24rem), 1fr)); }\n"
        "figure { margin: 0; } img { display: block; width: 100%; height: auto; }\n"
        "figcaption { padding-top: .35rem; }\n"
        "</style>\n<h1>WrOS rc30 owner preview</h1>\n<main>\n"
        + "\n".join(figures) + "\n</main>\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
