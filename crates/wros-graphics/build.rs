//! Turn GNU Unifont's published `.hex` file and WrOS's one-cell drawings into
//! the table `src/unifont.rs` draws from.
//!
//! Output (`$OUT_DIR/unifont.bin`, little endian):
//! - `b"WUF1"`, glyph count `u32`
//! - per glyph, sorted by code point: `u32` code point (bit 31 set when the
//!   glyph is two cells wide), `u32` offset into the bitmaps
//! - the bitmaps: 16 rows of one byte (one cell) or two bytes (two cells),
//!   most significant bit leftmost.
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::{collections::BTreeMap, env, fs, io::Read, path::PathBuf};

const HEX: &str = "../../assets/fonts/unifont-18.0.01.hex.gz";
const NARROW: &str = "../../assets/fonts/wros-narrow.txt";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={HEX}");
    println!("cargo:rerun-if-changed={NARROW}");
    let mut text = String::new();
    flate2::read::GzDecoder::new(fs::File::open(HEX)?).read_to_string(&mut text)?;
    let mut glyphs: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let (cp, bits) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("hex line {}: no colon", n + 1));
        let cp = u32::from_str_radix(cp, 16)?;
        assert!(
            bits.len() == 32 || bits.len() == 64,
            "hex line {}: {} digits",
            n + 1,
            bits.len()
        );
        glyphs.insert(cp, decode(bits)?);
    }
    // Symbols Unifont draws two cells wide where the layout gives one: scaled
    // down evenly, so they keep their proportions. (A letter of a script keeps
    // its height instead; the renderer squeezes it.)
    for (cp, rows) in glyphs.iter_mut() {
        let one_cell = char::from_u32(*cp)
            .and_then(unicode_width::UnicodeWidthChar::width)
            .is_some_and(|w| w == 1);
        if rows.len() == 32 && one_cell && SYMBOLS.contains(cp) {
            *rows = one_cell_from(rows);
        }
    }
    // WrOS's own drawings, made by hand for the symbols the interface uses.
    let narrow = fs::read_to_string(NARROW)?;
    for (cp, rows) in narrow_drawings(&narrow)? {
        glyphs.insert(cp, rows);
    }
    // The view tiles (rc31): two cells each, a 14x14 square frame with Unifont's own letter (or
    // the power symbol) centred inside. Each tile is drawn twice, from E120 and from E130: the
    // renderer fills the E130 copy's square (the current view) and draws its lines in black.
    let symbols = [('W', 1), ('r', 1), ('O', 1), ('S', 1), ('\u{E114}', 0)];
    for (n, (symbol, lift)) in symbols.into_iter().enumerate() {
        let tile = 2 * u32::try_from(n)?;
        let rows = glyphs
            .get(&(symbol as u32))
            .unwrap_or_else(|| panic!("tile symbol {symbol:?} is missing"))
            .clone();
        assert_eq!(
            rows.len(),
            16,
            "tile symbol {symbol:?} is not one cell wide"
        );
        let (left, right) = tile_halves(&rows, lift);
        for base in [0xE120, 0xE130] {
            glyphs.insert(base + tile, left.clone());
            glyphs.insert(base + tile + 1, right.clone());
        }
    }

    let mut index = Vec::with_capacity(glyphs.len() * 8);
    let mut blob = Vec::new();
    for (cp, rows) in &glyphs {
        let wide = if rows.len() == 32 { 1 << 31 } else { 0 };
        index.extend_from_slice(&(cp | wide).to_le_bytes());
        index.extend_from_slice(&u32::try_from(blob.len())?.to_le_bytes());
        blob.extend_from_slice(rows);
    }
    let mut out = b"WUF1".to_vec();
    out.extend_from_slice(&u32::try_from(glyphs.len())?.to_le_bytes());
    out.extend(index);
    out.extend(blob);
    let path = PathBuf::from(env::var("OUT_DIR")?).join("unifont.bin");
    fs::write(path, out)?;
    Ok(())
}

/// A view tile's two cells: the square's lines on columns 1 and 14 and rows 1 and 14 of the
/// 16-pixel slot, and the eight-pixel `symbol` on columns 4-11, raised `lift` rows. The symbol
/// must keep a pixel clear of the lines.
fn tile_halves(symbol: &[u8], lift: usize) -> (Vec<u8>, Vec<u8>) {
    let mut slot = [0u16; 16];
    for (y, &bits) in symbol.iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let to = y
            .checked_sub(lift)
            .filter(|to| (3..=12).contains(to))
            .unwrap_or_else(|| panic!("tile symbol row {y} lands outside rows 3-12"));
        slot[to] = u16::from(bits) << 4;
    }
    for (y, row) in slot.iter_mut().enumerate() {
        if y == 1 || y == 14 {
            *row |= 0x7FFE;
        } else if (2..=13).contains(&y) {
            *row |= 0x4002;
        }
    }
    (
        slot.iter().map(|r| r.to_be_bytes()[0]).collect(),
        slot.iter().map(|r| r.to_be_bytes()[1]).collect(),
    )
}

/// General Punctuation through Miscellaneous Symbols and Arrows: arrows,
/// technical symbols, shapes, dingbats.
const SYMBOLS: std::ops::RangeInclusive<u32> = 0x2000..=0x2BFF;

type Grid = Vec<Vec<bool>>;

/// Unifont's own one-cell symbols use seven of the eight columns.
const FIT_WIDTH: usize = 7;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "dimensions and checked pixel indices in a 16 by 16 glyph fit in usize"
)]
fn pixel_index(n: f32) -> usize {
    n as usize
}

/// A 16x16 drawing fitted into an 8x16 cell at the original's own height and
/// proportions: scaled evenly by only as much as it needs to be seven pixels
/// wide.
///
/// Scaling pixel art loses detail, so several pixel alignments and several
/// rules are tried, and the one closest to the original (both blurred as an
/// eye blurs small type) that keeps its separate strokes and enclosed holes
/// wins. Strokes are carried by the drawing's centre line (thinned, scaled,
/// redrawn a pixel wide), so a one-pixel outline survives; solid areas by
/// how much of each new pixel the original covers.
fn one_cell_from(rows: &[u8]) -> Vec<u8> {
    let orig: Grid = (0..16)
        .map(|y| {
            let v = u16::from_be_bytes([rows[y * 2], rows[y * 2 + 1]]);
            (0..16).map(|x| v & (0x8000 >> x) != 0).collect()
        })
        .collect();
    let Some((x0, x1, y0, y1)) = ink_box(&orig) else {
        return vec![0; 16];
    };
    let (iw, ih) = (x1 - x0 + 1, y1 - y0 + 1);
    let s = (FIT_WIDTH as f32 / iw as f32).min(1.0);
    let (wc, hc) = (
        pixel_index((iw as f32 * s).round().max(1.0)) + 1,
        pixel_index((ih as f32 * s).round().max(1.0)) + 1,
    );
    let want = shape(&orig);
    let skeleton = thin(&orig);
    let mut best: Option<(f32, Grid)> = None;
    for ox in [0.0, 1.0 / 3.0, 2.0 / 3.0] {
        for oy in [0.0, 1.0 / 3.0, 2.0 / 3.0] {
            // How much of each new pixel's source area is ink.
            let reference: Vec<Vec<f32>> = (0..hc)
                .map(|y| {
                    (0..wc)
                        .map(|x| {
                            let mut ink = 0;
                            for sy in 0..4 {
                                for sx in 0..4 {
                                    let fx =
                                        x0 as f32 + (x as f32 + (sx as f32 + 0.5) / 4.0 - ox) / s;
                                    let fy =
                                        y0 as f32 + (y as f32 + (sy as f32 + 0.5) / 4.0 - oy) / s;
                                    let (ix, iy) = (fx.floor(), fy.floor());
                                    if (0.0..16.0).contains(&ix)
                                        && (0.0..16.0).contains(&iy)
                                        && orig[pixel_index(iy)][pixel_index(ix)]
                                    {
                                        ink += 1;
                                    }
                                }
                            }
                            ink as f32 / 16.0
                        })
                        .collect()
                })
                .collect();
            let mut line: Grid = vec![vec![false; wc]; hc];
            for (y, row) in skeleton.iter().enumerate() {
                for (x, &on) in row.iter().enumerate() {
                    if !on {
                        continue;
                    }
                    let fx = ((x as f32 + 0.5 - x0 as f32) * s + ox).floor();
                    let fy = ((y as f32 + 0.5 - y0 as f32) * s + oy).floor();
                    if (0.0..wc as f32).contains(&fx) && (0.0..hc as f32).contains(&fy) {
                        line[pixel_index(fy)][pixel_index(fx)] = true;
                    }
                }
            }
            let target = blur(&reference);
            for threshold in [0.3, 0.5, 0.7, 2.0] {
                for strokes in [true, false] {
                    let c: Grid = (0..hc)
                        .map(|y| {
                            (0..wc)
                                .map(|x| strokes && line[y][x] || reference[y][x] >= threshold)
                                .collect()
                        })
                        .collect();
                    let Some((cx0, cx1, cy0, cy1)) = ink_box(&c) else {
                        continue;
                    };
                    if cx1 - cx0 + 1 > FIT_WIDTH || cy1 - cy0 + 1 > 16 {
                        continue;
                    }
                    let got = shape(&c);
                    let b = blur(&to_f32(&c));
                    let diff: f32 = (0..hc)
                        .flat_map(|y| (0..wc).map(move |x| (y, x)))
                        .map(|(y, x)| (target[y][x] - b[y][x]).powi(2))
                        .sum();
                    let lost = got.0.abs_diff(want.0) + got.1.abs_diff(want.1);
                    let score = diff + 3.0 * lost as f32;
                    if best.as_ref().is_none_or(|(old, _)| score < *old) {
                        best = Some((score, c));
                    }
                }
            }
        }
    }
    let Some((_, c)) = best else {
        return vec![0; 16];
    };
    let Some((cx0, cx1, cy0, cy1)) = ink_box(&c) else {
        return vec![0; 16];
    };
    let (w, h) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
    // Columns 1-7, like Unifont's own one-cell symbols, at the original's height.
    let left = 1 + (FIT_WIDTH - w) / 2;
    let centre = (y0 + y1) as f32 / 2.0;
    let top = pixel_index(
        (centre - (h as f32 - 1.0) / 2.0)
            .round()
            .clamp(0.0, (16 - h) as f32),
    );
    let mut cell = vec![0u8; 16];
    for y in 0..h {
        for x in 0..w {
            if c[cy0 + y][cx0 + x] {
                cell[top + y] |= 0x80 >> (left + x);
            }
        }
    }
    cell
}

/// Columns and rows that hold ink: (first x, last x, first y, last y).
fn ink_box(g: &Grid) -> Option<(usize, usize, usize, usize)> {
    let ys: Vec<usize> = (0..g.len()).filter(|&y| g[y].iter().any(|&b| b)).collect();
    let xs: Vec<usize> = (0..g[0].len())
        .filter(|&x| g.iter().any(|r| r[x]))
        .collect();
    Some((*xs.first()?, *xs.last()?, *ys.first()?, *ys.last()?))
}

/// Zhang-Suen thinning: each stroke down to its one-pixel centre line.
fn thin(g: &Grid) -> Grid {
    let mut g = g.clone();
    loop {
        let mut changed = false;
        for step in 0..2 {
            let at = |g: &Grid, y: i32, x: i32| {
                let (Ok(y), Ok(x)) = (usize::try_from(y), usize::try_from(x)) else {
                    return false;
                };
                g.get(y)
                    .and_then(|row| row.get(x))
                    .copied()
                    .unwrap_or(false)
            };
            let mut kill = vec![];
            for (row, pixels) in g.iter().enumerate() {
                let y = i32::try_from(row).unwrap_or(i32::MAX);
                for (column, &on) in pixels.iter().enumerate() {
                    if !on {
                        continue;
                    }
                    let x = i32::try_from(column).unwrap_or(i32::MAX);
                    let p = [
                        at(&g, y - 1, x),
                        at(&g, y - 1, x + 1),
                        at(&g, y, x + 1),
                        at(&g, y + 1, x + 1),
                        at(&g, y + 1, x),
                        at(&g, y + 1, x - 1),
                        at(&g, y, x - 1),
                        at(&g, y - 1, x - 1),
                    ];
                    let b = p.iter().filter(|&&v| v).count();
                    let a = (0..8).filter(|&i| !p[i] && p[(i + 1) % 8]).count();
                    if !(2..=6).contains(&b) || a != 1 {
                        continue;
                    }
                    let keep = if step == 0 {
                        p[0] && p[2] && p[4] || p[2] && p[4] && p[6]
                    } else {
                        p[0] && p[2] && p[6] || p[0] && p[4] && p[6]
                    };
                    if !keep {
                        kill.push((row, column));
                    }
                }
            }
            changed |= !kill.is_empty();
            for (y, x) in kill {
                g[y][x] = false;
            }
        }
        if !changed {
            return g;
        }
    }
}

fn to_f32(g: &Grid) -> Vec<Vec<f32>> {
    g.iter()
        .map(|r| r.iter().map(|&b| f32::from(u8::from(b))).collect())
        .collect()
}

/// Two passes of a 3x3 binomial filter.
fn blur(grid: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let k = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];
    let mut g = grid.to_vec();
    for _ in 0..2 {
        let mut out = vec![vec![0.0f32; grid[0].len()]; grid.len()];
        for (y, row) in out.iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                let (mut total, mut weight) = (0.0, 0.0);
                for (dy, weights) in (-1..=1).zip(k) {
                    let Some(source) = y.checked_add_signed(dy).and_then(|yy| g.get(yy)) else {
                        continue;
                    };
                    for (dx, c) in (-1..=1).zip(weights) {
                        if let Some(&value) = x.checked_add_signed(dx).and_then(|xx| source.get(xx))
                        {
                            total += c * value;
                            weight += c;
                        }
                    }
                }
                *pixel = total / weight;
            }
        }
        g = out;
    }
    g
}

/// (separate strokes, enclosed holes), 4-connected.
fn shape(g: &Grid) -> (usize, usize) {
    let (h, w) = (g.len(), g[0].len());
    let regions = |value: bool| -> (usize, usize) {
        let mut seen = vec![vec![false; w]; h];
        let (mut total, mut inner) = (0, 0);
        for y in 0..h {
            for x in 0..w {
                if seen[y][x] || g[y][x] != value {
                    continue;
                }
                total += 1;
                let mut edge = false;
                let mut stack = vec![(y, x)];
                seen[y][x] = true;
                while let Some((cy, cx)) = stack.pop() {
                    edge |= cy == 0 || cy == h - 1 || cx == 0 || cx == w - 1;
                    let around = [
                        (cy + 1, cx),
                        (cy.wrapping_sub(1), cx),
                        (cy, cx + 1),
                        (cy, cx.wrapping_sub(1)),
                    ];
                    for (ny, nx) in around {
                        if ny < h && nx < w && !seen[ny][nx] && g[ny][nx] == value {
                            seen[ny][nx] = true;
                            stack.push((ny, nx));
                        }
                    }
                }
                inner += usize::from(!edge);
            }
        }
        (total, inner)
    };
    (regions(true).0, regions(false).1)
}

fn decode(bits: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    (0..bits.len() / 2)
        .map(|i| u8::from_str_radix(&bits[i * 2..i * 2 + 2], 16))
        .collect()
}

/// `U+XXXX` headers, each followed by 16 rows of 8 `#`/`.` pixels.
fn narrow_drawings(text: &str) -> Result<Vec<(u32, Vec<u8>)>, std::num::ParseIntError> {
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
        let cp = u32::from_str_radix(&hex, 16)?;
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
    Ok(out)
}
