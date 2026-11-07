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

/// Columns inside the bar.
const BAR: usize = 32;

/// systemd's `Progress` as `busctl get-property` prints it: `d 0.734`.
pub fn parse_progress(text: &str) -> Option<f64> {
    let value: f64 = text.trim().strip_prefix("d ")?.trim().parse().ok()?;
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

/// The bar's three lines, `filled` columns of `#` between `+ - |` lines.
pub fn bar(progress: f64) -> [String; 3] {
    let filled = ((progress.clamp(0.0, 1.0) * BAR as f64).round() as usize).min(BAR);
    let rule = format!("+{}+", "-".repeat(BAR));
    [
        rule.clone(),
        format!("|{}{}|", "#".repeat(filled), " ".repeat(BAR - filled)),
        rule,
    ]
}

pub fn draw(frame: &mut Frame, theme: &Theme, progress: f64) {
    let area = frame.area();
    let border = Style::default().fg(theme.border);
    let fill = Style::default().fg(theme.accent);
    let [top, middle, bottom] = bar(progress);
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
    let lines = vec![
        Line::styled(top, border),
        middle_line,
        Line::styled(bottom, border),
        Line::from(""),
        Line::styled("Starting WrOS", Style::default().fg(theme.dim)),
    ];
    let logo_height = Logo::height_for(Rect {
        height: area.height.saturating_sub(lines.len() as u16 + 1),
        ..area
    });
    let used = logo_height + 1 + lines.len() as u16;
    let [_, logo, _, below, _] = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(used) / 2),
        Constraint::Length(logo_height),
        Constraint::Length(1),
        Constraint::Length(lines.len() as u16),
        Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(Logo { theme }, logo);
    frame.render_widget(Paragraph::new(lines).centered(), below);
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
        assert_eq!(parse_progress("d 0.734000\n"), Some(0.734));
        assert_eq!(parse_progress("d 1.2"), Some(1.0));
        assert_eq!(parse_progress("d -1"), Some(0.0));
        assert_eq!(parse_progress("d nan"), None);
        assert_eq!(parse_progress("s \"x\""), None);
        assert_eq!(parse_progress(""), None);
    }

    #[test]
    fn the_bar_fills_with_the_progress() {
        assert_eq!(bar(0.0)[1], format!("|{}|", " ".repeat(32)));
        assert_eq!(
            bar(0.5)[1],
            format!("|{}{}|", "#".repeat(16), " ".repeat(16))
        );
        assert_eq!(bar(1.0)[1], format!("|{}|", "#".repeat(32)));
        assert_eq!(bar(0.0)[0], format!("+{}+", "-".repeat(32)));
    }

    #[test]
    fn the_progress_screen_at_console_sizes() {
        insta::assert_snapshot!("splash_half_80x25", screen(0.5, 80, 25));
        let full = screen(1.0, 170, 48);
        assert!(full.contains("+----------------+----------------+"));
        assert!(full.contains(&format!("|{}|", "#".repeat(32))));
        assert!(full.contains("Starting WrOS"));
        assert!(screen(0.3, 40, 12).contains("Starting WrOS"));
    }
}
