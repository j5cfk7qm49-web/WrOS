//! The progress screen after the unlock: the logo and a bar in the logo's own
//! style, filled from systemd's boot progress, until the login screen starts.
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use crate::geometry::to_u16;
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

/// Where the progress screen sits in the boot. The initrd's part (after the unlock, until the
/// switch to the installed system) fills the first [`INITRD_SHARE`] of the bar; the installed
/// system's own start-up fills the rest, from wherever the initrd's bar stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Initrd,
    System,
}

/// The share of the bar the initrd fills after the unlock (opening the disk, mounting it,
/// preparing the installed system).
pub const INITRD_SHARE: f64 = 0.3;

/// Where the initrd's screen leaves its last figure for the installed system's screen
/// (`/run` is carried over the switch).
pub const HANDOVER: &str = "/run/wros-boot/progress";

/// Made by the unlock screen once it has handed the password over: the initrd's progress
/// screen waits for it, then replaces the unlock screen in place.
pub const UNLOCKED: &str = "/run/wros-boot/unlocked";

/// Made by the installed system's progress screen once it has given the console back to the
/// login screen (`RuntimeDirectory=wros-boot-splash`); greetd waits for it.
pub const HANDED_OVER: &str = "/run/wros-boot-splash/handed-over";

/// The bar's figure for systemd's progress `p` in this stage. `start` is where the initrd's
/// bar stopped (read from [`HANDOVER`]; 0 when the initrd showed none). `first` is systemd's first
/// reading in this stage: by the time a screen starts, systemd has done much of its work (the
/// installed system's first reading was about 0.67 in the VM), so the stage's part of the bar is
/// counted from there and the bar moves on smoothly instead of leaping.
pub fn overall(stage: Stage, start: f64, first: f64, p: f64) -> f64 {
    let first = first.clamp(0.0, 1.0);
    let p = if first >= 1.0 {
        1.0
    } else {
        ((p.clamp(0.0, 1.0) - first) / (1.0 - first)).clamp(0.0, 1.0)
    };
    match stage {
        Stage::Initrd => p * INITRD_SHARE,
        Stage::System => {
            let start = start.clamp(0.0, 1.0);
            start + (1.0 - start) * p
        }
    }
}

/// The bar's three lines, `inner` columns wide: `filled` columns of `#` between `+ - |` lines.
pub fn bar(progress: f64, inner: usize) -> [String; 3] {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped progress fills at most the available columns, and NaN fills none"
    )]
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
    let bar_width = to_u16(inner + 2).min(area.width);
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
        let width = to_u16(CAPTION.len()).min(area.width);
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
    fn the_initrd_fills_the_first_part_and_the_system_carries_on_from_it() {
        assert_eq!(overall(Stage::Initrd, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(overall(Stage::Initrd, 0.0, 0.0, 1.0), INITRD_SHARE);
        assert_eq!(overall(Stage::Initrd, 0.9, 0.0, 0.5), INITRD_SHARE / 2.0);
        // The installed system's bar starts where the initrd's stopped, never behind it.
        let left = overall(Stage::Initrd, 0.0, 0.0, 0.8);
        assert_eq!(overall(Stage::System, left, 0.0, 0.0), left);
        let half = overall(Stage::System, left, 0.0, 0.5);
        assert!((half - (left + (1.0 - left) / 2.0)).abs() < 1e-9);
        assert_eq!(overall(Stage::System, left, 0.0, 1.0), 1.0);
        // No initrd screen (a text unlock, or an older initrd): the whole bar, as before.
        assert_eq!(overall(Stage::System, 0.0, 0.0, 0.25), 0.25);
        assert_eq!(overall(Stage::System, 7.0, 0.0, 0.25), 1.0);
        assert_eq!(overall(Stage::System, 0.0, 0.0, -1.0), 0.0);
    }

    #[test]
    fn each_stage_counts_from_systemds_first_reading_so_the_bar_never_leaps() {
        // The VM's installed system: 0.67 at the screen's first reading, 1.0 at the end.
        let left = 0.3;
        assert_eq!(overall(Stage::System, left, 0.67, 0.67), left);
        let mid = overall(Stage::System, left, 0.67, 0.835);
        assert!((mid - (left + (1.0 - left) / 2.0)).abs() < 1e-9, "{mid}");
        assert_eq!(overall(Stage::System, left, 0.67, 1.0), 1.0);
        // The initrd's part is counted the same way.
        assert_eq!(overall(Stage::Initrd, 0.0, 0.4, 0.4), 0.0);
        assert!((overall(Stage::Initrd, 0.0, 0.4, 0.7) - INITRD_SHARE / 2.0).abs() < 1e-9);
        // A screen that starts when systemd is already done shows a full bar.
        assert_eq!(overall(Stage::System, left, 1.0, 1.0), 1.0);
        // systemd's figure dipping below the first reading never takes the bar behind its start.
        assert_eq!(overall(Stage::System, left, 0.67, 0.5), left);
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
