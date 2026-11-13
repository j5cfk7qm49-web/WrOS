//! Paint frames of cells (the JSON `src/shots.rs` writes) with the native
//! session's renderer, one binary PPM per frame, into a folder.
//! `cargo run -p wros-graphics --example cells_preview -- frames.json out/`
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
            Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
        })
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let frames: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&args[1])?).expect("frames JSON");
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out)?;
    for frame in frames.as_array().expect("a list of frames") {
        let (w, h) = (frame["w"].as_u64().unwrap() as u16, frame["h"].as_u64().unwrap() as u16);
        let mut pixels = TextSize::Writing.raster(w, h);
        let mut cells = vec![];
        for (y, row) in frame["rows"].as_array().unwrap().iter().enumerate() {
            for (x, c) in row.as_array().unwrap().iter().enumerate() {
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
                cells.push((x as u16, y as u16, cell));
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
