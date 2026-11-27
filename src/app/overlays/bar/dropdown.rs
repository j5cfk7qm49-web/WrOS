#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::panel::{PanelBackground, light_panel_row, panel_inner};
use super::{App, BarHit, BarRow, Frame, MAX_ROWS, Overlay, Theme};
use crate::geometry::to_u16;
use crate::ui::truncate;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};

/// The open menu, under its title, and a side group beside it.
pub(in crate::app::overlays) fn draw_dropdowns(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Bar {
        title,
        open: true,
        sel,
        side,
        ..
    } = &app.overlay
    else {
        return;
    };
    let anchor = app
        .bar_hits
        .borrow()
        .iter()
        .find(|(_, h)| *h == BarHit::Title(*title))
        .map_or(Rect::new(area.x, area.y, 1, 1), |(r, _)| *r);
    let rows = app.bar_rows(*title);
    let mut menu = Drawn::default();
    f.render_stateful_widget(
        MenuList::new(
            &rows,
            (anchor.x, anchor.y + 1),
            *sel,
            side.is_none(),
            app.linux_console,
            t,
        ),
        area,
        &mut menu,
    );
    let mut hits = app.bar_hits.borrow_mut();
    hits.retain(|(_, h)| matches!(h, BarHit::Tile(_) | BarHit::Title(_)));
    for (i, r) in &menu.rows {
        hits.push((*r, BarHit::Row(*i)));
    }
    if let Some((group, at)) = side {
        let side_rows = app.side_rows(*group);
        let row_y = menu
            .rows
            .iter()
            .find(|(i, _)| *i == *sel)
            .map_or(menu.area.y, |(_, r)| r.y);
        let w = list_width(&side_rows);
        let x = if menu.area.right() + w <= area.right() {
            menu.area.right()
        } else {
            menu.area.x.saturating_sub(w)
        };
        let mut drawn = Drawn::default();
        f.render_stateful_widget(
            MenuList::new(&side_rows, (x, row_y), *at, true, app.linux_console, t),
            area,
            &mut drawn,
        );
        hits.push((drawn.area, BarHit::Cover));
        for (i, r) in drawn.rows {
            hits.push((r, BarHit::SideRow(i)));
        }
    }
}

#[derive(Default)]
struct Drawn {
    area: Rect,
    rows: Vec<(usize, Rect)>,
}

fn list_width(rows: &[BarRow]) -> u16 {
    let label = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let key = rows
        .iter()
        .map(|r| r.key.chars().count())
        .max()
        .unwrap_or(0);
    // Side padding, marker, label, a gap, the key, a space, side padding.
    to_u16(1 + 2 + label + 3 + key + 1 + 1)
}

/// One dropdown at an anchor, clamped to the available area and scrolled to the selection.
struct MenuList<'a> {
    rows: &'a [BarRow],
    anchor: (u16, u16),
    sel: usize,
    lit: bool,
    console: bool,
    theme: &'a Theme,
}

impl<'a> MenuList<'a> {
    fn new(
        rows: &'a [BarRow],
        anchor: (u16, u16),
        sel: usize,
        lit: bool,
        console: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            rows,
            anchor,
            sel,
            lit,
            console,
            theme,
        }
    }
}

impl StatefulWidget for MenuList<'_> {
    type State = Drawn;

    fn render(self, area: Rect, buf: &mut Buffer, drawn: &mut Self::State) {
        let Self {
            rows,
            anchor: (x, y),
            sel,
            lit,
            console: linux_console,
            theme: t,
        } = self;
        let w = list_width(rows).min(area.width);
        // The status line stays visible under a menu.
        let y = y.min(area.bottom().saturating_sub(2)).max(area.y);
        let room = usize::from(area.bottom().saturating_sub(1).saturating_sub(y));
        let shown = rows.len().min(MAX_ROWS).min(room);
        let h = to_u16(shown);
        let x = x.min(area.right().saturating_sub(w));
        let rect = Rect::new(x, y, w, h);
        let background = PanelBackground::new(area, linux_console, t);
        let inner = panel_inner(rect);
        let band = background.band;
        background.render(rect, buf);
        let start = (sel + 1).saturating_sub(shown);
        drawn.area = rect;
        drawn.rows.clear();
        for (line, (i, row)) in rows.iter().enumerate().skip(start).take(shown).enumerate() {
            let on = i == sel;
            let base = if row.dim.is_some() {
                Style::default().fg(t.dim)
            } else {
                Style::default().fg(t.text)
            };
            let r = Rect::new(inner.x, inner.y + to_u16(line), inner.width, 1);
            let style = match (on, lit) {
                (true, true) => light_panel_row(buf, rect, r.y, linux_console, t),
                (true, false) => base.bg(t.sel),
                _ => base.bg(band),
            };
            let label_w = usize::from(inner.width).saturating_sub(row.key.chars().count() + 5);
            let text = format!(
                "{}{:<label_w$}  {} ",
                if on && lit {
                    wros_shell::look::MARKER
                } else {
                    "  "
                },
                truncate(&row.label, label_w),
                row.key,
            );
            Paragraph::new(Line::styled(text, style)).render(r, buf);
            drawn.rows.push((i, r));
        }
    }
}
