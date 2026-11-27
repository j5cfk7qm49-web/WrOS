//! GNU Unifont, drawn from its own bitmaps: every pixel is the font's pixel,
//! so text is as crisp as the Linux console's Fixed 8×16 (which Debian builds
//! from Unifont). WrOS adds one-cell drawings where Unifont's is two cells and
//! the interface gives one (`assets/fonts/wros-narrow.txt`).
//!
//! A cell never paints outside the cells it was given: a glyph wider than its
//! cells is squeezed into them, never spilled onto a neighbour.
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use ratatui::{
    buffer::Cell,
    style::{Color, Modifier},
};
use soft_ratatui::{RasterBackend, RgbPixmap};
use unicode_width::UnicodeWidthStr;

/// One cell in pixels.
pub const CELL_WIDTH: usize = 8;
pub const CELL_HEIGHT: usize = 16;

/// The view tiles, two cells each, in W, r, O, S order: a square frame around the letter, in
/// the cell's ink.
pub const TILES: [[char; 2]; 4] = [
    ['\u{E120}', '\u{E121}'],
    ['\u{E122}', '\u{E123}'],
    ['\u{E124}', '\u{E125}'],
    ['\u{E126}', '\u{E127}'],
];
/// The same tiles lit, for the current view: the square is filled with the cell's ink and its
/// frame and letter are black; outside the square stays the cell's ground.
pub const TILES_LIT: [[char; 2]; 4] = [
    ['\u{E130}', '\u{E131}'],
    ['\u{E132}', '\u{E133}'],
    ['\u{E134}', '\u{E135}'],
    ['\u{E136}', '\u{E137}'],
];
/// The power tile, in the same frame, and lit (its menu open).
pub const TILE_POWER: [char; 2] = ['\u{E128}', '\u{E129}'];
pub const TILE_POWER_LIT: [char; 2] = ['\u{E138}', '\u{E139}'];

/// For a lit tile's cell: whether it is the tile's left cell.
fn lit_tile_side(symbol: &str) -> Option<bool> {
    let mut chars = symbol.chars();
    let c = chars.next()? as u32;
    (chars.next().is_none() && (0xE130..=0xE139).contains(&c)).then_some(c.is_multiple_of(2))
}

/// Is (x, y) inside a lit tile's square, in its left or right cell?
fn in_tile_square(left: bool, x: usize, y: usize) -> bool {
    let columns = if left { 1..8 } else { 0..7 };
    columns.contains(&x) && (1..=14).contains(&y)
}

static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unifont.bin"));

/// A glyph's bitmap: 16 rows, 8 or 16 pixels each.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub wide: bool,
    rows: &'static [u8],
}

impl Glyph {
    pub fn width(&self) -> usize {
        if self.wide { 16 } else { 8 }
    }

    pub fn pixel(&self, x: usize, y: usize) -> bool {
        if y >= CELL_HEIGHT || x >= self.width() {
            return false;
        }
        let per_row = self.width() / 8;
        let byte = self.rows[y * per_row + x / 8];
        byte & (0x80 >> (x % 8)) != 0
    }
}

fn count() -> usize {
    DATA.get(4..)
        .and_then(|tail| tail.first_chunk())
        .map_or(0, |bytes| u32::from_le_bytes(*bytes) as usize)
}

fn entry(i: usize) -> Option<(u32, bool, usize)> {
    let at = 8 + i * 8;
    let cp = u32::from_le_bytes(*DATA.get(at..)?.first_chunk()?);
    let offset = u32::from_le_bytes(*DATA.get(at + 4..)?.first_chunk()?) as usize;
    Some((cp & 0x7fff_ffff, cp & (1 << 31) != 0, offset))
}

/// Unifont's drawing of `c` (or WrOS's one-cell drawing), if it has one.
pub fn glyph(c: char) -> Option<Glyph> {
    let n = count();
    let (mut lo, mut hi) = (0, n);
    let target = c as u32;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let (cp, wide, offset) = entry(mid)?;
        match cp.cmp(&target) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                let start = 8 + n * 8 + offset;
                let len = if wide { 32 } else { 16 };
                return Some(Glyph {
                    wide,
                    rows: &DATA[start..start + len],
                });
            }
        }
    }
    None
}

/// The raster backend `SoftBackend<Unifont>` draws cells with.
#[derive(Default)]
pub struct Unifont;

fn rgb(colour: Color, foreground: bool) -> [u8; 3] {
    match colour {
        Color::Rgb(r, g, b) => [r, g, b],
        // Cells are normalised to RGB before drawing; Reset can still reach a
        // cleared buffer.
        _ if foreground => [221, 221, 221],
        _ => [0, 0, 0],
    }
}

fn dim(c: [u8; 3]) -> [u8; 3] {
    // soft_ratatui's own dimming, so DIM looks as it did.
    c.map(|v| u8::try_from((u32::from(v) * 77 + 127) / 255).unwrap_or(77))
}

/// Is pixel (x, y) of a `span`-pixel-wide cell set for `symbol`, before
/// styling? Base glyph plus any combining marks over it; a glyph wider than
/// the span is squeezed (pairs of columns merged), a missing one is a box.
fn ink(symbol: &str, span: usize) -> impl Fn(usize, usize) -> bool + '_ {
    let mut chars = symbol.chars();
    let base = chars.next().unwrap_or(' ');
    // Only marks drawn over their letter: never a selector, a joiner or the
    // rest of an emoji sequence, which Unifont would stack on top as boxes.
    let marks: Vec<Glyph> = chars.filter(|&c| overlays(c)).filter_map(glyph).collect();
    let main = glyph(base);
    move |x, y| {
        let from = |g: &Glyph| {
            if g.width() > span {
                g.pixel(x * 2, y) || g.pixel(x * 2 + 1, y)
            } else {
                g.pixel(x, y)
            }
        };
        let base_ink = match &main {
            Some(g) => from(g),
            // The missing-character box: an outline inside the cell(s).
            None => {
                let (l, r) = (1, span.saturating_sub(2));
                let side = (x == l || x == r) && (2..=13).contains(&y);
                let edge = (y == 2 || y == 13) && (l..=r).contains(&x);
                side || edge
            }
        };
        base_ink || marks.iter().any(from)
    }
}

/// A character after the first in a cell that belongs drawn over it: a
/// zero-width combining mark, but not one of the invisible format characters.
fn overlays(c: char) -> bool {
    let invisible = matches!(c,
        '\u{034F}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}'
        | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
        | '\u{FFA0}' | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0000}'..='\u{E0FFF}');
    !invisible && unicode_width::UnicodeWidthChar::width(c) == Some(0)
}

impl RasterBackend for Unifont {
    fn draw_cell(
        &mut self,
        xik: u16,
        yik: u16,
        cell: &Cell,
        always_redraw: &mut rustc_hash::FxHashSet<(u16, u16)>,
        blinking_fast: bool,
        blinking_slow: bool,
        char_width: usize,
        char_height: usize,
        pixmap: &mut RgbPixmap,
    ) {
        let mut fg = rgb(cell.fg, true);
        let mut bg = rgb(cell.bg, false);
        let m = cell.modifier;
        if m.contains(Modifier::REVERSED) {
            (fg, bg) = (bg, fg);
        }
        if m.contains(Modifier::DIM) {
            (fg, bg) = (dim(fg), dim(bg));
        }
        if m.contains(Modifier::HIDDEN) {
            fg = bg;
        }
        if m.contains(Modifier::SLOW_BLINK) {
            always_redraw.insert((xik, yik));
            if blinking_slow {
                fg = bg;
            }
        }
        if m.contains(Modifier::RAPID_BLINK) {
            always_redraw.insert((xik, yik));
            if blinking_fast {
                fg = bg;
            }
        }
        let symbol = cell.symbol();
        let cells = symbol.width().clamp(1, 2);
        let (left, top) = (
            usize::from(xik) * char_width,
            usize::from(yik) * char_height,
        );
        let span = (char_width * cells).min(pixmap.width().saturating_sub(left));
        let ink = ink(symbol, span);
        let lit = lit_tile_side(symbol);
        let bold = m.contains(Modifier::BOLD);
        let italic = m.contains(Modifier::ITALIC);
        for y in 0..char_height {
            // Italic leans everything above the middle of a lowercase letter one pixel right.
            let lean = usize::from(italic && y < 9);
            for x in 0..span {
                let src = x.wrapping_sub(lean);
                let glyph = src < span
                    && y < CELL_HEIGHT
                    && (ink(src, y)
                        // Bold is the console's overstrike: each pixel and the one to its left.
                        || bold && src > 0 && ink(src - 1, y));
                let line = m.contains(Modifier::UNDERLINED) && y == char_height - 1
                    || m.contains(Modifier::CROSSED_OUT) && y == char_height / 2;
                let colour = match lit {
                    // A lit tile: black lines on a square of the ink colour.
                    Some(_) if glyph => [0, 0, 0],
                    Some(side) if in_tile_square(side, x, y) => fg,
                    _ if glyph || line => fg,
                    _ => bg,
                };
                pixmap.put_pixel(left + x, top + y, colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::to_u16;
    use ratatui::style::Color;

    fn inked(symbol: &str, span: usize) -> Vec<bool> {
        let at = ink(symbol, span);
        (0..CELL_HEIGHT)
            .flat_map(|y| (0..span).map(move |x| (x, y)))
            .map(|(x, y)| at(x, y))
            .collect()
    }

    #[test]
    fn selectors_and_joiners_add_nothing_but_accents_still_sit_on_their_letter() {
        // Unifont draws U+FE0F, U+FE0E and U+200D as labelled boxes.
        assert_eq!(inked("\u{2764}\u{FE0F}", 8), inked("\u{2764}", 8));
        assert_eq!(inked("\u{2764}\u{FE0E}", 8), inked("\u{2764}", 8));
        assert_eq!(
            inked("\u{1F469}\u{200D}\u{1F4BB}", 16),
            inked("\u{1F469}", 16)
        );
        assert_ne!(
            inked("e\u{301}", 8),
            inked("e", 8),
            "a combining accent is drawn"
        );
    }

    fn render(text: &str, modifier: Modifier) -> soft_ratatui::SoftBackend<Unifont> {
        let mut pixels = crate::TextSize::Writing.raster(4, 1);
        let mut cells = vec![];
        let mut x = 0;
        for c in text.chars() {
            let mut cell = Cell::new(" ");
            cell.set_char(c)
                .set_fg(Color::Rgb(255, 255, 255))
                .set_bg(Color::Rgb(0, 0, 0));
            cell.modifier = modifier;
            cells.push((x, 0, cell));
            x += cell_width(c);
        }
        ratatui::backend::Backend::draw(&mut pixels, cells.iter().map(|(x, y, c)| (*x, *y, c)))
            .unwrap();
        pixels
    }

    fn cell_width(c: char) -> u16 {
        to_u16(
            unicode_width::UnicodeWidthChar::width(c)
                .unwrap_or(1)
                .max(1),
        )
    }

    fn cell_pixels(p: &soft_ratatui::SoftBackend<Unifont>, col: usize) -> Vec<[u8; 3]> {
        let mut out = vec![];
        for y in 0..CELL_HEIGHT {
            for x in 0..CELL_WIDTH {
                out.push(p.rgb_pixmap.get_pixel(col * CELL_WIDTH + x, y));
            }
        }
        out
    }

    #[test]
    fn letters_are_unifonts_own_pixels() {
        let a = glyph('A').unwrap();
        assert!(!a.wide);
        let p = render("A", Modifier::empty());
        for y in 0..CELL_HEIGHT {
            for x in 0..CELL_WIDTH {
                let lit = p.rgb_pixmap.get_pixel(x, y) == [255, 255, 255];
                assert_eq!(lit, a.pixel(x, y), "pixel {x},{y}");
            }
        }
    }

    /// A tile's two cells as one 16-pixel-wide row per pixel row.
    fn slot(pair: [char; 2]) -> Vec<u16> {
        let (l, r) = (glyph(pair[0]).unwrap(), glyph(pair[1]).unwrap());
        assert!(!l.wide && !r.wide, "{pair:?} must be two one-cell glyphs");
        (0..CELL_HEIGHT)
            .map(|y| u16::from(l.rows[y]) << 8 | u16::from(r.rows[y]))
            .collect()
    }

    /// The square's lines on each pixel row of a tile.
    fn frame(y: usize) -> u16 {
        match y {
            1 | 14 => 0x7FFE,
            2..=13 => 0x4002,
            _ => 0,
        }
    }

    #[test]
    fn tiles_are_14_pixel_squares_with_the_symbol_a_pixel_clear() {
        let all = TILES
            .into_iter()
            .chain(TILES_LIT)
            .chain([TILE_POWER, TILE_POWER_LIT]);
        for pair in all {
            for (y, row) in slot(pair).into_iter().enumerate() {
                assert_eq!(row & frame(y), frame(y), "{pair:?}: the frame on row {y}");
                // The symbol stays on columns 4-11 and rows 3-12: a pixel clear of the lines.
                let symbol = row & !frame(y);
                assert_eq!(symbol & !0x0FF0, 0, "{pair:?}: row {y} strays");
                if !(3..=12).contains(&y) {
                    assert_eq!(symbol, 0, "{pair:?}: row {y} must be clear");
                }
            }
        }
        // Unifont's own letters, raised a row so they clear the bottom line as they clear the top.
        for (n, letter) in ['W', 'r', 'O', 'S'].into_iter().enumerate() {
            let original = glyph(letter).unwrap();
            assert!(!original.wide);
            assert_ne!(original.rows[13], 0, "{letter:?} sits on row 13");
            assert_eq!(slot(TILES[n]), slot(TILES_LIT[n]));
            let rows = slot(TILES[n]);
            for y in 2..14 {
                assert_eq!(
                    ((rows[y - 1] & !frame(y - 1)) >> 4).to_le_bytes()[0],
                    original.rows[y],
                    "{letter:?}, row {y}"
                );
            }
        }
        let power = glyph('\u{E114}').unwrap();
        assert_eq!(slot(TILE_POWER), slot(TILE_POWER_LIT));
        let rows = slot(TILE_POWER);
        for (y, row) in rows.iter().enumerate().take(14).skip(2) {
            assert_eq!(
                ((row & !frame(y)) >> 4).to_le_bytes()[0],
                power.rows[y],
                "power row {y}"
            );
        }
    }

    #[test]
    fn a_lit_tile_fills_its_square_and_draws_its_lines_black() {
        let (accent, ground) = ([136, 192, 208], [46, 52, 64]);
        let mut pixels = crate::TextSize::Writing.raster(4, 1);
        let cells: Vec<Cell> = TILES_LIT[0]
            .into_iter()
            .chain(TILES[0])
            .map(|c| {
                let mut cell = Cell::new(" ");
                cell.set_char(c)
                    .set_fg(Color::Rgb(accent[0], accent[1], accent[2]))
                    .set_bg(Color::Rgb(ground[0], ground[1], ground[2]));
                cell
            })
            .collect();
        ratatui::backend::Backend::draw(
            &mut pixels,
            cells.iter().enumerate().map(|(x, c)| (to_u16(x), 0, c)),
        )
        .unwrap();
        let rows = slot(TILES[0]);
        for (y, row) in rows.iter().enumerate() {
            for x in 0..16 {
                let line = row & (0x8000 >> x) != 0;
                let square = (1..=14).contains(&x) && (1..=14).contains(&y);
                let want = if line {
                    [0, 0, 0]
                } else if square {
                    accent
                } else {
                    ground
                };
                assert_eq!(pixels.rgb_pixmap.get_pixel(x, y), want, "lit {x},{y}");
                let plain = if line { accent } else { ground };
                assert_eq!(
                    pixels.rgb_pixmap.get_pixel(16 + x, y),
                    plain,
                    "plain {x},{y}"
                );
            }
        }
    }

    #[test]
    fn wros_one_cell_drawings_replace_unifonts_wide_ones() {
        for c in "⌖⏯ⓘ★⚠✎✦✧✱✲✶✺✿❀❚❡❦❧".chars() {
            assert!(!glyph(c).unwrap().wide, "{c} is still two cells");
        }
    }

    #[test]
    fn a_wide_symbol_is_scaled_evenly_into_one_cell() {
        // Unifont draws ☆ two cells wide; the layout gives it one. Halving it in
        // both directions keeps a star's proportions: no taller than it is wide.
        let star = glyph('☆').unwrap();
        assert!(!star.wide);
        let rows: Vec<usize> = (0..CELL_HEIGHT)
            .filter(|&y| (0..8).any(|x| star.pixel(x, y)))
            .collect();
        let cols: Vec<usize> = (0..8)
            .filter(|&x| (0..CELL_HEIGHT).any(|y| star.pixel(x, y)))
            .collect();
        let (h, w) = (rows.len(), cols.len());
        assert!(h <= 8 && h <= w + 1, "{w}x{h}");
        // Its outline survives: an enclosed hole, as in Unifont's drawing.
        assert!((0..CELL_HEIGHT).any(|y| {
            (1..7).any(|x| !star.pixel(x, y) && star.pixel(x - 1, y) && star.pixel(x + 1, y))
        }));
    }

    #[test]
    fn a_wide_glyph_never_paints_outside_its_cells() {
        // क is a letter Unifont draws two cells wide and the layout gives one;
        // a letter keeps its height, squeezed into its cell.
        assert!(glyph('क').unwrap().wide);
        let with = render("aकb", Modifier::BOLD | Modifier::ITALIC);
        let without = render("a b", Modifier::BOLD | Modifier::ITALIC);
        assert_eq!(cell_pixels(&with, 0), cell_pixels(&without, 0));
        assert_eq!(cell_pixels(&with, 2), cell_pixels(&without, 2));
        assert!(
            cell_pixels(&with, 1).contains(&[255, 255, 255]),
            "squeezed, not blank"
        );
    }

    #[test]
    fn bold_and_italic_differ_from_plain() {
        let plain = cell_pixels(&render("m", Modifier::empty()), 0);
        assert_ne!(plain, cell_pixels(&render("m", Modifier::BOLD), 0));
        assert_ne!(plain, cell_pixels(&render("m", Modifier::ITALIC), 0));
    }

    #[test]
    fn a_character_unifont_lacks_shows_a_box() {
        assert!(glyph('\u{10FFFD}').is_none());
        let p = render("\u{10FFFD}", Modifier::empty());
        assert!(cell_pixels(&p, 0).contains(&[255, 255, 255]));
    }
}
