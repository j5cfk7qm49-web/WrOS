//! Exact glyph templates for VM screenshot OCR, rendered by the production backend.
use ratatui::{
    backend::Backend,
    buffer::Cell,
    style::{Color, Modifier},
};
use std::io::{self, Write};
use wros_graphics::TextSize;

fn main() -> io::Result<()> {
    let size = if std::env::args_os().any(|arg| arg == "--writing") {
        TextSize::Writing
    } else {
        TextSize::Startup
    };
    let glyphs: Vec<_> = (32..=126)
        .map(char::from)
        .chain("▸●›→↑↓←✓✗…•·—–│─┌┐└┘├┤┬┴┼".chars())
        .collect();
    let rows = glyphs.len().div_ceil(16);
    let mut pixels = size.raster(16, rows as u16 * 3);
    let mut cells = vec![];
    for (style, modifier) in [Modifier::empty(), Modifier::BOLD, Modifier::ITALIC]
        .into_iter()
        .enumerate()
    {
        for (i, glyph) in glyphs.iter().enumerate() {
            let mut cell = Cell::new(" ");
            cell.set_char(*glyph)
                .set_fg(Color::White)
                .set_bg(Color::Black);
            cell.modifier = modifier;
            cells.push(((i % 16) as u16, (style * rows + i / 16) as u16, cell));
        }
    }
    pixels
        .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
        .unwrap();
    pixels.flush().unwrap();
    let mut out = io::stdout().lock();
    write!(
        out,
        "P6\n# cells {} {}\n# codepoints {}\n{} {}\n255\n",
        pixels.get_pixmap_width() / 16,
        pixels.get_pixmap_height() / (rows * 3),
        glyphs
            .iter()
            .map(|&c| (c as u32).to_string())
            .collect::<Vec<_>>()
            .join(" "),
        pixels.get_pixmap_width(),
        pixels.get_pixmap_height()
    )?;
    out.write_all(pixels.get_pixmap_data())
}
