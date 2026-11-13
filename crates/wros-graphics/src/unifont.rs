//! GNU Unifont, drawn from its own bitmaps: every pixel is the font's pixel,
//! so text is as crisp as the Linux console's Fixed 8×16 (which Debian builds
//! from Unifont). WrOS adds one-cell drawings where Unifont's is two cells and
//! the interface gives one (`assets/fonts/wros-narrow.txt`).
//!
//! A cell never paints outside the cells it was given: a glyph wider than its
//! cells is squeezed into them, never spilled onto a neighbour.
use ratatui::{
    buffer::Cell,
    style::{Color, Modifier},
};
use soft_ratatui::{RasterBackend, RgbPixmap};
use unicode_width::UnicodeWidthStr;

/// One cell in pixels.
pub const CELL_WIDTH: usize = 8;
pub const CELL_HEIGHT: usize = 16;

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
    u32::from_le_bytes(DATA[4..8].try_into().unwrap()) as usize
}

fn entry(i: usize) -> (u32, bool, usize) {
    let at = 8 + i * 8;
    let cp = u32::from_le_bytes(DATA[at..at + 4].try_into().unwrap());
    let offset = u32::from_le_bytes(DATA[at + 4..at + 8].try_into().unwrap()) as usize;
    (cp & 0x7fff_ffff, cp & (1 << 31) != 0, offset)
}

/// Unifont's drawing of `c` (or WrOS's one-cell drawing), if it has one.
pub fn glyph(c: char) -> Option<Glyph> {
    let n = count();
    let (mut lo, mut hi) = (0, n);
    let target = c as u32;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let (cp, wide, offset) = entry(mid);
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
    c.map(|v| ((v as u32 * 77 + 127) / 255) as u8)
}

/// Is pixel (x, y) of a `span`-pixel-wide cell set for `symbol`, before
/// styling? Base glyph plus any combining marks over it; a glyph wider than
/// the span is squeezed (pairs of columns merged), a missing one is a box.
fn ink(symbol: &str, span: usize) -> impl Fn(usize, usize) -> bool + '_ {
    let mut chars = symbol.chars();
    let base = chars.next().unwrap_or(' ');
    let marks: Vec<Glyph> = chars.filter_map(glyph).collect();
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
        let span =
            (char_width * cells).min(pixmap.width().saturating_sub(xik as usize * char_width));
        let (left, top) = (xik as usize * char_width, yik as usize * char_height);
        let ink = ink(symbol, span);
        let bold = m.contains(Modifier::BOLD);
        let italic = m.contains(Modifier::ITALIC);
        for y in 0..char_height {
            // Italic leans everything above the middle of a lowercase letter one pixel right.
            let lean = usize::from(italic && y < 9);
            for x in 0..span {
                let src = x.wrapping_sub(lean);
                let on = src < span
                    && y < CELL_HEIGHT
                    && (ink(src, y)
                        // Bold is the console's overstrike: each pixel and the one to its left.
                        || bold && src > 0 && ink(src - 1, y));
                let on = on
                    || m.contains(Modifier::UNDERLINED) && y == char_height - 1
                    || m.contains(Modifier::CROSSED_OUT) && y == char_height / 2;
                pixmap.put_pixel(left + x, top + y, if on { fg } else { bg });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

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
        unicode_width::UnicodeWidthChar::width(c)
            .unwrap_or(1)
            .max(1) as u16
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

    #[test]
    fn wros_one_cell_drawings_replace_unifonts_wide_ones() {
        for c in "⌖⏯ⓘ★⚠✎✦✧✱✲✶✺✿❀❚❡❦❧".chars() {
            assert!(!glyph(c).unwrap().wide, "{c} is still two cells");
        }
    }

    #[test]
    fn a_wide_glyph_never_paints_outside_its_cells() {
        // ☆ is two cells in Unifont and one in the layout, with no WrOS drawing.
        assert!(glyph('☆').unwrap().wide);
        let with = render("a☆b", Modifier::BOLD | Modifier::ITALIC);
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
