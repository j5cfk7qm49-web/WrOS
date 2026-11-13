//! Print the shipped one-cell drawings of the given characters, one per line,
//! as `U+XXXX:` and 16 rows of `#`/`.` (8 or 16 wide), for preview sheets.
//! `cargo run -p wros-graphics --example unifont_dump -- "☆✔"`
use wros_graphics::unifont::{CELL_HEIGHT, glyph};

fn main() {
    for c in std::env::args().skip(1).flat_map(|a| a.chars().collect::<Vec<_>>()) {
        let Some(g) = glyph(c) else {
            println!("U+{:04X}:missing", c as u32);
            continue;
        };
        let rows: Vec<String> = (0..CELL_HEIGHT)
            .map(|y| (0..g.width()).map(|x| if g.pixel(x, y) { '#' } else { '.' }).collect())
            .collect();
        println!("U+{:04X}:{}", c as u32, rows.join(","));
    }
}
