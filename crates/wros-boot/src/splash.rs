//! The progress screen after the unlock: the logo and a bar in the logo's own
//! style, filled from systemd's boot progress, until the login screen starts.

use crate::logo::Logo;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use wros_shell::grimoire_theme::Theme;

/// Columns inside the bar when the logo is too small to set its width.
const BAR: usize = 32;

/// systemd's `Progress`, as `systemctl show --property=Progress --value` prints it (`0.734`),
/// or as `busctl get-property` does (`d 0.734`).
pub fn parse_progress(text: &str) -> Option<f64> {
    let text = text.trim();
    let text = text.strip_prefix("Progress=").unwrap_or(text);
    let text = text.strip_prefix("d ").unwrap_or(text);
    let value: f64 = text.trim().parse().ok()?;
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

/// The bar's three lines, `inner` columns wide: `filled` columns of `#` between `+ - |` lines.
pub fn bar(progress: f64, inner: usize) -> [String; 3] {
    let filled = ((progress.clamp(0.0, 1.0) * inner as f64).round() as usize).min(inner);
    let rule = format!("+{}+", "-".repeat(inner));
    [
        rule.clone(),
        format!("|{}{}|", "#".repeat(filled), " ".repeat(inner - filled)),
        rule,
    ]
}

/// Columns inside the bar: exactly the logo's width less the two ends, so bar and logo share one
/// centre (a bar one column narrower than the logo sits half a column off).
pub fn inner_width(logo_width: u16, area_width: u16) -> usize {
    if logo_width >= 12 {
        usize::from(logo_width) - 2
    } else {
        BAR.min(usize::from(area_width).saturating_sub(2))
    }
}

pub fn draw(frame: &mut Frame, theme: &Theme, progress: f64) {
    let area = frame.area();
    let border = Style::default().fg(theme.border);
    let fill = Style::default().fg(theme.accent);
    let logo_area = Rect {
        height: area.height.saturating_sub(6),
        ..area
    };
    let inner = inner_width(Logo::width_for(logo_area), area.width);
    let [top, middle, bottom] = bar(progress, inner);
    let middle_line = Line::from(vec![
        Span::styled("|", border),
        Span::styled(
            middle
                .trim_start_matches('|')
                .trim_end_matches('|')
                .to_string(),
            fill,
        ),
        Span::styled("|", border),
    ]);
    let bar_lines = vec![
        Line::styled(top, border),
        middle_line,
        Line::styled(bottom, border),
    ];
    // Three bar rows, a blank row, and the caption.
    const BELOW: u16 = 5;
    let logo_height = Logo::height_for(logo_area);
    let used = logo_height + 1 + BELOW;
    let [_, logo, _, below, _] = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(used) / 2),
        Constraint::Length(logo_height),
        Constraint::Length(1),
        Constraint::Length(BELOW),
        Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(Logo { theme }, logo);
    // The bar starts in the logo's own first column, worked out the way the logo works it out.
    // (Paragraph's centring rounds the other way on even widths: half a column off.)
    let bar_width = (inner as u16 + 2).min(area.width);
    let bar_area = Rect {
        x: area.x + area.width.saturating_sub(bar_width) / 2,
        width: bar_width,
        height: 3.min(below.height),
        ..below
    };
    frame.render_widget(Paragraph::new(bar_lines), bar_area);
    if below.height == BELOW {
        // The caption is centred the same way, so all three share one centre.
        const CAPTION: &str = "Starting WrOS";
        let width = (CAPTION.len() as u16).min(area.width);
        let caption = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: below.y + 4,
            width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::styled(CAPTION, Style::default().fg(theme.dim))),
            caption,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use wros_shell::grimoire_theme::default_theme;

    fn screen(progress: f64, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let theme = default_theme();
        terminal.draw(|f| draw(f, &theme, progress)).unwrap();
        let buf = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn systemds_progress_is_read_and_kept_in_range() {
        assert_eq!(parse_progress("0.734\n"), Some(0.734));
        assert_eq!(parse_progress("Progress=0.5"), Some(0.5));
        assert_eq!(parse_progress("1"), Some(1.0));
        assert_eq!(parse_progress("d 0.734000\n"), Some(0.734));
        assert_eq!(parse_progress("d 1.2"), Some(1.0));
        assert_eq!(parse_progress("d -1"), Some(0.0));
        assert_eq!(parse_progress("d nan"), None);
        assert_eq!(parse_progress("s \"x\""), None);
        assert_eq!(parse_progress(""), None);
    }

    #[test]
    fn the_bar_fills_with_the_progress() {
        assert_eq!(bar(0.0, 33)[1], format!("|{}|", " ".repeat(33)));
        assert_eq!(
            bar(0.5, 32)[1],
            format!("|{}{}|", "#".repeat(16), " ".repeat(16))
        );
        assert_eq!(bar(1.0, 33)[1], format!("|{}|", "#".repeat(33)));
        assert_eq!(bar(0.0, 33)[0], format!("+{}+", "-".repeat(33)));
    }

    /// The first and last columns drawn on a row, if any.
    fn ends(row: &str) -> Option<(usize, usize)> {
        let first = row.find(|c: char| c != ' ')?;
        Some((first, row.trim_end().chars().count() - 1))
    }

    #[test]
    fn the_bar_is_exactly_under_the_logo() {
        // The full logo, the half-size one, and screens of odd and even widths.
        for (width, height) in [(80, 25), (81, 25), (170, 48), (171, 48), (40, 18), (41, 18)] {
            let text = screen(0.5, width, height);
            let rows: Vec<&str> = text.lines().collect();
            let logo = ends(rows.iter().find(|r| r.contains('+')).unwrap()).unwrap();
            let bar_bottom = rows
                .iter()
                .rposition(|r| r.trim_start().starts_with("+-"))
                .unwrap();
            assert_eq!(
                ends(rows[bar_bottom]),
                Some(logo),
                "{width}x{height}: bar and logo share both edges\n{text}"
            );
            assert_eq!(
                ends(rows[bar_bottom - 1]),
                Some(logo),
                "{width}x{height}\n{text}"
            );
            // The caption (odd width, like the logo) shares the same centre column.
            let caption = ends(rows.iter().find(|r| r.contains("Starting WrOS")).unwrap()).unwrap();
            assert_eq!(
                caption.0 + caption.1,
                logo.0 + logo.1,
                "{width}x{height}: caption centre\n{text}"
            );
        }
    }

    #[test]
    fn the_progress_screen_at_console_sizes() {
        insta::assert_snapshot!("splash_half_80x25", screen(0.5, 80, 25));
        let full = screen(1.0, 170, 48);
        assert!(full.contains("+----------------+----------------+"));
        assert!(full.contains(&format!("|{}|", "#".repeat(33))));
        assert!(full.contains("Starting WrOS"));
        assert!(screen(0.3, 40, 12).contains("Starting WrOS"));
    }
}
