#!/usr/bin/env python3
"""Draw the boot screen's pictures (#89) into os/plymouth/wros/.

Plymouth can only draw text with an extra plugin that may not be in the boot image,
so the logo and the disk-unlock prompt are pictures. The logo is a placeholder
wordmark until WrOS has a real one: replace logo.png (any size) to change it.

    python3 tools/make-bootscreen.py --font /usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf

Needs Pillow. The font is DejaVu Sans (free licence); only rendered pictures of it ship.
"""
import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'os/plymouth/wros'
CREAM, DIM, AMBER = (234, 223, 200, 255), (154, 143, 122, 255), (217, 164, 65, 255)


def text(font, size, words, colour):
    face = ImageFont.truetype(font, size)
    left, top, right, bottom = face.getbbox(words)
    image = Image.new('RGBA', (right - left + 4, bottom - top + 4), (0, 0, 0, 0))
    ImageDraw.Draw(image).text((2 - left, 2 - top), words, font=face, fill=colour)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font', required=True)
    font = parser.parse_args().font
    OUT.mkdir(parents=True, exist_ok=True)

    name, line = text(font, 84, 'WrOS', CREAM), text(font, 22, "Writer's OS", DIM)
    logo = Image.new('RGBA', (max(name.width, line.width), name.height + 14 + line.height), (0, 0, 0, 0))
    logo.alpha_composite(name, ((logo.width - name.width) // 2, 0))
    logo.alpha_composite(line, ((logo.width - line.width) // 2, name.height + 14))
    logo.save(OUT / 'logo.png', optimize=True)

    box = Image.new('RGBA', (320, 14), (0, 0, 0, 0))
    ImageDraw.Draw(box).rectangle((0, 0, 319, 13), outline=(90, 82, 70, 255))
    box.save(OUT / 'box.png', optimize=True)
    Image.new('RGBA', (4, 10), AMBER).save(OUT / 'bar.png', optimize=True)

    text(font, 22, 'Type your disk password, then press Enter', CREAM).save(OUT / 'prompt.png', optimize=True)
    bullet = Image.new('RGBA', (12, 12), (0, 0, 0, 0))
    ImageDraw.Draw(bullet).ellipse((1, 1, 10, 10), fill=CREAM)
    bullet.save(OUT / 'bullet.png', optimize=True)
    print('Drew', ', '.join(sorted(p.name for p in OUT.glob('*.png'))))


if __name__ == '__main__':
    main()
