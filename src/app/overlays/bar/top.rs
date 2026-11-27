#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::tiles::ViewTiles;
use super::{App, BarHit, Overlay, Theme, Title, paint_band};
use crate::geometry::to_u16;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, StatefulWidget, Widget};

/// The view tiles, titles, marks and Power, with hit rectangles for the mouse.
pub(crate) struct TopBar<'a> {
    tiles: ViewTiles<'a>,
    lit: Option<Title>,
    marks: Vec<(String, Style, u8)>,
    console: bool,
    theme: &'a Theme,
}

impl<'a> TopBar<'a> {
    pub(crate) fn from_app(app: &App, theme: &'a Theme, show_marks: bool) -> Self {
        let (lit, tile) = match &app.overlay {
            Overlay::Bar {
                tile: Some(view), ..
            } => (None, Some(*view)),
            Overlay::Bar { title, .. } => (Some(*title), None),
            Overlay::Palette {
                under_bar: true, ..
            } => (Some(app.bar_last), None),
            _ => (None, None),
        };
        Self {
            tiles: ViewTiles::from_app(app, theme, tile, lit),
            lit,
            marks: if show_marks {
                ranked_marks(app, theme, false)
            } else {
                Vec::new()
            },
            console: app.linux_console,
            theme,
        }
    }
}

impl StatefulWidget for TopBar<'_> {
    type State = Vec<(Rect, BarHit)>;

    fn render(self, area: Rect, buf: &mut Buffer, hits: &mut Self::State) {
        if area.is_empty() {
            return;
        }
        Clear.render(area, buf);
        hits.retain(|(_, h)| !matches!(h, BarHit::Tile(_) | BarHit::Title(_)));
        let (views, power) = self.tiles.areas(area);
        self.tiles.render(area, buf, hits);

        let look: wros_shell::Theme = self.theme.into();
        let mut x = views.right();
        let titles = Rect::new(x, area.y, power.x - x, 1);
        let titles_end = render_status_marks(buf, titles, self.marks, self.console);
        let mut spans = Vec::new();
        for title in Title::ALL
            .into_iter()
            .take_while(|&title| title != Title::Power)
        {
            let w = to_u16(title.label().chars().count()).saturating_add(2);
            if w > titles_end - x {
                break;
            }
            hits.push((Rect::new(x, area.y, w, 1), BarHit::Title(title)));
            x += w;
            spans.extend(wros_shell::look::underlined_title(
                title.label(),
                0,
                self.lit == Some(title),
                look,
            ));
        }
        Paragraph::new(Line::from(spans)).render(Rect::new(titles.x, area.y, x - titles.x, 1), buf);
        hits.push((power, BarHit::Title(Title::Power)));
        paint_band(buf, area, self.theme, self.console);
    }
}

/// How much a mark matters when the line is short: words not saved most,
/// then conflicts, a failed backup, words waiting, and the rest.
const UNSAVED: u8 = 4;
const CONFLICT: u8 = 3;
const TROUBLE: u8 = 2;
const PENDING: u8 = 1;
const CALM: u8 = 0;

pub(super) fn ranked_marks(app: &App, t: &Theme, warnings_only: bool) -> Vec<(String, Style, u8)> {
    let console = app.linux_console;
    let mut out = vec![];
    let conflicts = app.conflicts().len();
    if conflicts > 0 {
        out.push((
            if console {
                format!(
                    "{conflicts} conflict{}",
                    if conflicts == 1 { "" } else { "s" }
                )
            } else {
                format!("⚠ {conflicts}")
            },
            Style::default().fg(t.warn),
            CONFLICT,
        ));
    }
    // Saved is a steady dim tick; saving, waiting and not saved say so.
    let dirty = app.project.dirty_count();
    let save = match &app.save_state {
        crate::app::SaveState::Failed(_) if dirty > 0 => (
            if console {
                format!("{dirty} not saved")
            } else {
                format!("● {dirty}")
            },
            Style::default().fg(t.warn),
            UNSAVED,
        ),
        _ if warnings_only => return out,
        crate::app::SaveState::Waiting(_) if dirty > 0 => (
            if console {
                format!("{dirty} waiting")
            } else {
                format!("◌ {dirty}")
            },
            Style::default().fg(t.sun),
            PENDING,
        ),
        _ if dirty > 0 => (
            if console {
                "saving".into()
            } else {
                "○".into()
            },
            Style::default().fg(t.dim),
            CALM,
        ),
        _ => (
            if console {
                "saved".into()
            } else {
                "✓".into()
            },
            Style::default().fg(t.dim),
            CALM,
        ),
    };
    out.push(save);
    if warnings_only {
        return out;
    }
    if let Some(mark) = app.backup_mark() {
        use crate::app::BackupMark;
        let (text, style, rank) = match mark {
            BackupMark::Done => (
                if console { "backed up" } else { "⇅" }.into(),
                Style::default().fg(t.dim),
                CALM,
            ),
            BackupMark::Running => (
                if console {
                    "backing up".into()
                } else {
                    "⇅…".into()
                },
                Style::default().fg(t.accent),
                CALM,
            ),
            BackupMark::Waiting => (
                if console {
                    "backup waiting".into()
                } else {
                    "⇅◌".into()
                },
                Style::default().fg(t.sun),
                CALM,
            ),
            BackupMark::Failed => (
                if console {
                    "backup failed".into()
                } else {
                    "⇅!".into()
                },
                Style::default().fg(t.warn),
                TROUBLE,
            ),
        };
        out.push((text, style, rank));
    }
    out
}

/// Draw the marks between the logo and Power. Return where the titles must stop.
fn render_status_marks(
    buf: &mut Buffer,
    line: Rect,
    mut marks: Vec<(String, Style, u8)>,
    console: bool,
) -> u16 {
    let mut gap = "  ";
    while !marks.is_empty() {
        let width: usize = marks
            .iter()
            .map(|(s, _, _)| s.chars().count() + gap.len())
            .sum::<usize>()
            + 1;
        if width <= usize::from(line.width) {
            let width = to_u16(width);
            let mut spans = vec![];
            for (text, style, _) in marks {
                spans.push(Span::raw(gap));
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw(" "));
            let area = Rect::new(line.right() - width, line.y, width, 1);
            Paragraph::new(Line::from(spans)).render(area, buf);
            return area.x.saturating_sub(2).max(line.x);
        }
        let least = marks.iter().map(|(_, _, rank)| *rank).min().unwrap_or(CALM);
        if least >= CONFLICT && gap == "  " {
            // When the titles and other marks are gone, shorten the warnings first.
            gap = " ";
            for (text, _, rank) in &mut marks {
                *text = match (*rank == UNSAVED, console) {
                    (true, true) => "unsaved",
                    (false, true) => "conflict",
                    (true, false) => "●",
                    (false, false) => "⚠",
                }
                .into();
            }
        } else if let Some(at) = marks.iter().rposition(|(_, _, rank)| *rank == least) {
            marks.remove(at);
        }
    }
    line.right().saturating_sub(1).max(line.x)
}
