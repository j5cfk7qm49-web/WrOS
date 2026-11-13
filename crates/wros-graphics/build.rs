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

/// General Punctuation through Miscellaneous Symbols and Arrows: arrows,
/// technical symbols, shapes, dingbats.
const SYMBOLS: std::ops::RangeInclusive<u32> = 0x2000..=0x2BFF;

type Grid = Vec<Vec<bool>>;

/// Unifont's own one-cell symbols use seven of the eight columns.
const FIT_WIDTH: usize = 7;

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
        (iw as f32 * s).round().max(1.0) as usize + 1,
        (ih as f32 * s).round().max(1.0) as usize + 1,
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
                                        && orig[iy as usize][ix as usize]
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
                    if fx >= 0.0 && fy >= 0.0 && (fx as usize) < wc && (fy as usize) < hc {
                        line[fy as usize][fx as usize] = true;
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
    let (cx0, cx1, cy0, cy1) = ink_box(&c).expect("a chosen drawing has ink");
    let (w, h) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
    // Columns 1-7, like Unifont's own one-cell symbols, at the original's height.
    let left = 1 + (FIT_WIDTH - w) / 2;
    let centre = (y0 + y1) as f32 / 2.0;
    let top = (centre - (h as f32 - 1.0) / 2.0)
        .round()
        .clamp(0.0, (16 - h) as f32) as usize;
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
    let (h, w) = (g.len() as i32, g[0].len() as i32);
    loop {
        let mut changed = false;
        for step in 0..2 {
            let at = |g: &Grid, y: i32, x: i32| {
                (0..h).contains(&y) && (0..w).contains(&x) && g[y as usize][x as usize]
            };
            let mut kill = vec![];
            for y in 0..h {
                for x in 0..w {
                    if !g[y as usize][x as usize] {
                        continue;
                    }
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
                        kill.push((y as usize, x as usize));
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
    let (h, w) = (grid.len() as i32, grid[0].len() as i32);
    let k = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];
    let mut g = grid.to_vec();
    for _ in 0..2 {
        let mut out = vec![vec![0.0f32; w as usize]; h as usize];
        for y in 0..h {
            for x in 0..w {
                let (mut total, mut weight) = (0.0, 0.0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (yy, xx) = (y + dy, x + dx);
                        if (0..h).contains(&yy) && (0..w).contains(&xx) {
                            let c = k[(dy + 1) as usize][(dx + 1) as usize];
                            total += c * g[yy as usize][xx as usize];
                            weight += c;
                        }
                    }
                }
                out[y as usize][x as usize] = total / weight;
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
