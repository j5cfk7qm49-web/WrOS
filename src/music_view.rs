//! The clearing's Music view: what is playing, and rows to open the player, change the
//! source and sign in or out, so music never needs Settings for those.

#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::geometry::{to_u16, to_u16_f64};
use crate::music::{Source, State};
use crate::theme::Theme;
use crate::ui::truncate;

/// A row of the Music view, chosen with ↑/↓ and Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicRow {
    /// Open the F7 player.
    Player,
    /// Open the source picker.
    Source,
    /// Jellyfin or Plex with no server yet: open Settings' server form.
    SignIn,
    /// Jellyfin or Plex: forget the server and its token.
    SignOut,
}

/// The rows, with their labels, in order.
pub fn rows(app: &App) -> Vec<(MusicRow, String)> {
    let source = app.music.source;
    let label = source.label();
    let mut rows = vec![
        (MusicRow::Player, "Open player · F7".to_string()),
        (MusicRow::Source, format!("Source · {label}…")),
    ];
    if matches!(source, Source::Jellyfin | Source::Plex) {
        rows.push(if app.music.server_ready {
            (MusicRow::SignOut, format!("Sign out of {label}"))
        } else {
            (MusicRow::SignIn, format!("Sign in to {label}…"))
        });
    }
    rows
}

/// The view inside the pane's border: its title, its lines, and which line shows which
/// row (line index, row index) so a click can choose one.
pub fn view(
    app: &App,
    t: &Theme,
    width: u16,
    height: u16,
    focused: bool,
) -> (String, Vec<Line<'static>>, Vec<(u16, usize)>) {
    let title = format!("♪ {} · F7", app.music.source.label());
    let w = width as usize;
    let height = height as usize;
    let rows = rows(app);

    // What is playing: a headline, a second line (artist or a hint), and a progress bar.
    let dim = Style::default().fg(t.dim);
    let faint = Style::default().fg(t.border);
    type Lines = (Line<'static>, Option<Line<'static>>, Option<Line<'static>>);
    let (head, second, bar): Lines = match &app.music.state {
        State::NoToken if app.wros_music && !app.music.source.available(true) => (
            Line::from(Span::styled("not available yet", dim)),
            Some(Line::from(Span::styled("choose another source", faint))),
            None,
        ),
        State::NoToken => (Line::from(Span::styled("not set up", dim)), None, None),
        State::Offline => (
            Line::from(Span::styled(
                truncate(&format!("{} not running", app.music.source.label()), w),
                dim,
            )),
            None,
            None,
        ),
        State::Idle => (
            Line::from(Span::styled("nothing playing", dim)),
            Some(Line::from(Span::styled("space plays", faint))),
            None,
        ),
        State::Playing(tr) => {
            let glyph = if tr.playing { "▶" } else { "❚❚" };
            let head = Line::from(Span::styled(
                truncate(&format!("{glyph} {}", tr.title), w),
                Style::default().fg(if tr.playing { t.accent } else { t.dim }),
            ));
            let artist = (!tr.artist.is_empty())
                .then(|| Line::from(Span::styled(truncate(&tr.artist, w), dim)));
            (head, artist, Some(progress_line(app, tr, width, t)))
        }
    };

    // Room, from the top: the headline always; then the rows; then, while room is left,
    // the bar, the second line and a blank line before the rows.
    let mut room = height.saturating_sub(1);
    let shown_rows = rows.len().min(room);
    room -= shown_rows;
    let mut fits = |line: Option<Line<'static>>| {
        let line = line.filter(|_| room > 0);
        if line.is_some() {
            room -= 1;
        }
        line
    };
    let bar = fits(bar);
    let second = fits(second);
    let gap = room > 0;

    let mut lines = vec![head];
    lines.extend(second);
    lines.extend(bar);
    if gap {
        lines.push(Line::from(""));
    }
    let mut hits = Vec::new();
    for (i, (_, label)) in rows.iter().enumerate().take(shown_rows) {
        let on = i == app.music_row.min(rows.len() - 1);
        let mark = if on { "› " } else { "  " };
        let style = match (on, focused) {
            (true, true) => Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            (true, false) => Style::default().fg(t.text),
            (false, _) => Style::default().fg(t.dim),
        };
        hits.push((to_u16(lines.len()), i));
        lines.push(Line::from(Span::styled(
            truncate(&format!("{mark}{label}"), w),
            style,
        )));
    }
    (title, lines, hits)
}

/// The progress bar, its time, and the player's badges (repeat, shuffle, volume).
fn progress_line(app: &App, tr: &crate::music::Track, width: u16, t: &Theme) -> Line<'static> {
    let frac = if tr.duration > 0.0 {
        (tr.progress / tr.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let badges = app.music.modes.badges();
    let extra = if badges.is_empty() {
        0
    } else {
        badges.chars().count() + 1
    };
    let barw = width.saturating_sub(to_u16(7 + extra)).max(4);
    let filled = usize::from(to_u16_f64((frac * f64::from(barw)).round()));
    let barw = usize::from(barw);
    let mins = |s: f64| format!("{:.0}:{:02.0}", (s / 60.0).floor(), s % 60.0);
    // Different weights, not just different colours, so the bar still reads on a
    // monochrome terminal.
    Line::from(vec![
        Span::styled("━".repeat(filled.min(barw)), Style::default().fg(t.accent)),
        Span::styled(
            "─".repeat(barw.saturating_sub(filled)),
            Style::default().fg(t.border),
        ),
        Span::styled(
            format!(" {}", mins(tr.progress)),
            Style::default().fg(t.dim),
        ),
        Span::styled(
            if badges.is_empty() {
                String::new()
            } else {
                format!(" {badges}")
            },
            Style::default().fg(t.accent),
        ),
    ])
}
