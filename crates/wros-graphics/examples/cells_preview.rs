//! Paint frames of cells (the JSON `src/shots.rs` writes) with the native
//! session's renderer, one binary PPM per frame, into a folder.
//! `cargo run -p wros-graphics --example cells_preview -- frames.json out/`
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use ratatui::{
    backend::Backend,
    buffer::Cell,
    style::{Color, Modifier},
};
use std::io::Write;
use wros_graphics::TextSize;

fn colour(v: &serde_json::Value) -> Color {
    v.as_str()
        .and_then(|s| u32::from_str_radix(s.trim_start_matches('#'), 16).ok())
        .map_or(Color::Reset, |n| {
            let [_, r, g, b] = n.to_be_bytes();
            Color::Rgb(r, g, b)
        })
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, input, out, ..] = args.as_slice() else {
        return Err(std::io::Error::other(
            "usage: cells_preview FRAMES.json OUTPUT_DIR",
        ));
    };
    let invalid = std::io::Error::other;
    let frames: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(input)?)?;
    let out = std::path::Path::new(out);
    std::fs::create_dir_all(out)?;
    for frame in frames
        .as_array()
        .ok_or_else(|| invalid("expected a list of frames"))?
    {
        let (w, h) = (
            frame["w"]
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| invalid("expected a u16 frame width"))?,
            frame["h"]
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| invalid("expected a u16 frame height"))?,
        );
        let mut pixels = TextSize::Writing.raster(w, h);
        let mut cells = vec![];
        for (y, row) in frame["rows"]
            .as_array()
            .ok_or_else(|| invalid("expected frame rows"))?
            .iter()
            .enumerate()
        {
            let y = u16::try_from(y).map_err(std::io::Error::other)?;
            for (x, c) in row
                .as_array()
                .ok_or_else(|| invalid("expected a row of cells"))?
                .iter()
                .enumerate()
            {
                let mut cell = Cell::default();
                cell.set_symbol(c[0].as_str().unwrap_or(" "));
                // Reset reaches the screen as the native session paints it.
                let fg = match colour(&c[1]) {
                    Color::Reset => Color::Rgb(221, 221, 221),
                    other => other,
                };
                let bg = match colour(&c[2]) {
                    Color::Reset => Color::Rgb(0, 0, 0),
                    other => other,
                };
                cell.set_fg(fg).set_bg(bg);
                let style = c[3].as_u64().unwrap_or(0);
                let mut m = Modifier::empty();
                if style & 1 != 0 {
                    m |= Modifier::BOLD;
                }
                if style & 2 != 0 {
                    m |= Modifier::ITALIC;
                }
                if style & 4 != 0 {
                    m |= Modifier::UNDERLINED;
                }
                cell.modifier = m;
                cells.push((u16::try_from(x).map_err(std::io::Error::other)?, y, cell));
            }
        }
        pixels
            .draw(cells.iter().map(|(x, y, c)| (*x, *y, c)))
            .unwrap();
        let name = frame["name"].as_str().unwrap_or("frame");
        let mut file = std::fs::File::create(out.join(format!("{name}.ppm")))?;
        let (pw, ph) = (pixels.get_pixmap_width(), pixels.get_pixmap_height());
        write!(file, "P6\n{pw} {ph}\n255\n")?;
        file.write_all(pixels.get_pixmap_data())?;
    }
    Ok(())
}
