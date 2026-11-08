//! The WrOS logo, the owner's design (2026-11-07): W, r, O and S in four boxes,
//! drawn with the very characters the owner typed. They are plain ASCII, so every
//! console font has them and the logo looks the same on the console before the
//! disk is unlocked as in a terminal later. The letters take the theme's accent
//! and the box its border. Where the full logo doesn't fit, a half-size copy does.

use crate::grimoire_theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// The logo as the owner typed it: `+ - |` are the boxes, `#` the letters.
pub const SKETCH: [&str; 17] = [
    "+----------------+----------------+",
    "| ###        ### |                |",
    "|  ##        ##  |                |",
    "|  ##   ##   ##  |  ###  #####    |",
    "|  ##  ####  ##  |   ## ###  ##   |",
    "|  ## ##  ## ##  |   ####         |",
    "|  ####    ####  |   ##           |",
    "| ###        ### |  ######        |",
    "+----------------+----------------+",
    "|     ######     |   ##########   |",
    "|   ###    ###   |  ##        ##  |",
    "|  ##        ##  |  ##            |",
    "|  ##        ##  |   ########     |",
    "|  ##        ##  |          ###   |",
    "|   ###    ###   |  ##       ##   |",
    "|     ######     |   #########    |",
    "+----------------+----------------+",
];

/// Columns and rows of the full logo.
pub const FULL: (u16, u16) = (35, 17);

/// The half-size logo: every two columns and two rows of the sketch become one,
/// a `#` wherever either had one.
pub fn compact() -> Vec<String> {
    let grid: Vec<Vec<char>> = SKETCH.iter().map(|row| row.chars().collect()).collect();
    let bars: Vec<usize> = (0..grid[0].len()).filter(|&c| grid[0][c] == '+').collect();
    let borders: Vec<usize> = (0..grid.len()).filter(|&r| grid[r][0] == '+').collect();
    let mut out = Vec::new();
    for (i, &border) in borders.iter().enumerate() {
        let mut line = String::new();
        for (j, &bar) in bars.iter().enumerate() {
            line.push('+');
            if let Some(&next) = bars.get(j + 1) {
                line.push_str(&"-".repeat((next - bar - 1) / 2));
            }
        }
        out.push(line);
        let Some(&next_border) = borders.get(i + 1) else {
            break;
        };
        let rows: Vec<usize> = (border + 1..next_border).collect();
        for pair in rows.chunks(2) {
            let mut line = String::from("|");
            for window in bars.windows(2) {
                for k in 0..(window[1] - window[0] - 1) / 2 {
                    let cols = [window[0] + 1 + 2 * k, window[0] + 2 + 2 * k];
                    let hash = pair
                        .iter()
                        .any(|&r| cols.iter().any(|&c| grid[r].get(c) == Some(&'#')));
                    line.push(if hash { '#' } else { ' ' });
                }
                line.push('|');
            }
            out.push(line);
        }
    }
    out
}

/// The logo, as large as fits, centred in its area.
pub struct Logo<'a> {
    pub theme: &'a Theme,
}

impl Logo<'_> {
    /// The rows the logo will take in an area this size.
    pub fn height_for(area: Rect) -> u16 {
        let compact = compact();
        if area.width >= FULL.0 && area.height >= FULL.1 {
            FULL.1
        } else if area.width >= compact[0].chars().count() as u16
            && area.height >= compact.len() as u16
        {
            compact.len() as u16
        } else {
            1
        }
    }
}

impl Widget for Logo<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<String> = match Self::height_for(area) {
            h if h == FULL.1 => SKETCH.iter().map(|l| l.to_string()).collect(),
            1 => vec!["WrOS".to_string()],
            _ => compact(),
        };
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
        let left = area.x + area.width.saturating_sub(width) / 2;
        let top = area.y + area.height.saturating_sub(lines.len() as u16) / 2;
        for (dy, line) in lines.iter().enumerate() {
            for (dx, ch) in line.chars().enumerate() {
                let (x, y) = (left + dx as u16, top + dy as u16);
                if ch == ' ' || x >= area.right() || y >= area.bottom() {
                    continue;
                }
                let colour: Color = match ch {
                    '+' | '-' | '|' => self.theme.border,
                    _ => self.theme.accent,
                };
                buf[(x, y)]
                    .set_char(ch)
                    .set_style(Style::default().fg(colour));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grimoire_theme::default_theme;

    fn drawn_at(width: u16, height: u16) -> (Buffer, Theme) {
        let theme = default_theme();
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        Logo { theme: &theme }.render(area, &mut buf);
        (buf, theme)
    }

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn the_full_logo_is_the_owners_sketch_exactly() {
        let (buf, theme) = drawn_at(35, 17);
        assert_eq!(rows(&buf), SKETCH.map(str::to_string));
        assert_eq!(buf[(2, 1)].fg, theme.accent, "a letter takes the accent");
        assert_eq!(
            buf[(0, 0)].fg,
            theme.border,
            "the box takes the border colour"
        );
    }

    #[test]
    fn the_compact_logo_keeps_every_letter_at_half_size() {
        let small = compact();
        assert_eq!(small.len(), 11, "{small:#?}");
        assert!(small.iter().all(|l| l.chars().count() == 19), "{small:#?}");
        assert_eq!(small[0], "+--------+--------+");
        assert_eq!(small[5], "+--------+--------+");
        assert_eq!(small[10], "+--------+--------+");
        for band in [&small[1..5], &small[6..10]] {
            for half in [1..9, 10..18] {
                assert!(
                    band.iter().any(|l| l
                        .chars()
                        .skip(half.start)
                        .take(half.len())
                        .any(|c| c == '#')),
                    "{small:#?}"
                );
            }
        }
    }

    #[test]
    fn the_logo_shrinks_to_fit_and_never_draws_outside() {
        let (buf, _) = drawn_at(80, 14);
        assert!(rows(&buf).iter().any(|r| r.contains("+--------+--------+")));
        let (buf, _) = drawn_at(10, 3);
        assert!(rows(&buf).iter().any(|r| r.contains("WrOS")));
    }

    #[test]
    fn console_colours_are_named_ansi_colours_and_the_saved_theme_stays_rgb() {
        use crate::grimoire_theme::{ROLES, presets, to_config};
        for original in presets() {
            let saved = to_config(&original);
            let console = original.for_console();
            for i in 0..ROLES.len() {
                assert!(!matches!(
                    console.role(i),
                    Color::Rgb(..) | Color::Indexed(_)
                ));
            }
            assert!(!matches!(console.accent, Color::Black | Color::Reset));
            assert!(!matches!(console.border, Color::Black | Color::Reset));
            assert!(!matches!(console.text, Color::Black | Color::Reset));
            assert_eq!(to_config(&original), saved);
            assert_eq!(console.for_console(), console);
        }
    }
}
