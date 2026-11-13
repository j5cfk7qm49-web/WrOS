//! Turn GNU Unifont's published `.hex` file and WrOS's one-cell drawings into
//! the table `src/unifont.rs` draws from.
//!
//! Output (`$OUT_DIR/unifont.bin`, little endian):
//! - `b"WUF1"`, glyph count `u32`
//! - per glyph, sorted by code point: `u32` code point (bit 31 set when the
//!   glyph is two cells wide), `u32` offset into the bitmaps
//! - the bitmaps: 16 rows of one byte (one cell) or two bytes (two cells),
//!   most significant bit leftmost.
use std::{collections::BTreeMap, env, fs, io::Read, path::PathBuf};

const HEX: &str = "../../assets/fonts/unifont-18.0.01.hex.gz";
const NARROW: &str = "../../assets/fonts/wros-narrow.txt";

fn main() {
    println!("cargo:rerun-if-changed={HEX}");
    println!("cargo:rerun-if-changed={NARROW}");
    let mut text = String::new();
    flate2::read::GzDecoder::new(fs::File::open(HEX).expect("Unifont hex file"))
        .read_to_string(&mut text)
        .expect("Unifont hex file is gzip text");
    let mut glyphs: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let (cp, bits) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("hex line {}: no colon", n + 1));
        let cp = u32::from_str_radix(cp, 16).expect("hex code point");
        assert!(
            bits.len() == 32 || bits.len() == 64,
            "hex line {}: {} digits",
            n + 1,
            bits.len()
        );
        glyphs.insert(cp, decode(bits));
    }
    let narrow = fs::read_to_string(NARROW).expect("narrow drawings");
    for (cp, rows) in narrow_drawings(&narrow) {
        glyphs.insert(cp, rows);
    }

    let mut index = Vec::with_capacity(glyphs.len() * 8);
    let mut blob = Vec::new();
    for (cp, rows) in &glyphs {
        let wide = if rows.len() == 32 { 1 << 31 } else { 0 };
        index.extend_from_slice(&(cp | wide).to_le_bytes());
        index.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        blob.extend_from_slice(rows);
    }
    let mut out = b"WUF1".to_vec();
    out.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());
    out.extend(index);
    out.extend(blob);
    let path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("unifont.bin");
    fs::write(path, out).expect("write unifont.bin");
}

fn decode(bits: &str) -> Vec<u8> {
    (0..bits.len() / 2)
        .map(|i| u8::from_str_radix(&bits[i * 2..i * 2 + 2], 16).expect("hex digits"))
        .collect()
}

/// `U+XXXX` headers, each followed by 16 rows of 8 `#`/`.` pixels.
fn narrow_drawings(text: &str) -> Vec<(u32, Vec<u8>)> {
    let mut out = vec![];
    let mut lines = text.lines().map(str::trim_end);
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("U+") else {
            assert!(
                line.is_empty() || line.starts_with('#'),
                "narrow drawings: stray line {line:?}"
            );
            continue;
        };
        let hex: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
        let cp = u32::from_str_radix(&hex, 16).expect("narrow drawing code point");
        let rows: Vec<u8> = (0..16)
            .map(|r| {
                let row = lines
                    .next()
                    .unwrap_or_else(|| panic!("U+{hex}: row {r} missing"));
                assert!(
                    row.len() == 8 && row.chars().all(|c| c == '#' || c == '.'),
                    "U+{hex}: row {r} is {row:?}"
                );
                row.chars()
                    .fold(0u8, |acc, c| (acc << 1) | u8::from(c == '#'))
            })
            .collect();
        out.push((cp, rows));
    }
    out
}
