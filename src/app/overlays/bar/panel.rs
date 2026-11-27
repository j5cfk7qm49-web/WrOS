#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::{MAX_ROWS, Theme, bar_band};
use crate::geometry::to_u16;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, StatefulWidget, Widget};

/// A quick control above a gauge on the info bar.
pub(in crate::app::overlays) struct Panel<'a> {
    rows: Vec<Line<'static>>,
    anchor: Rect,
    sel: usize,
    console: bool,
    theme: &'a Theme,
}

impl<'a> Panel<'a> {
    pub(in crate::app::overlays) fn new(
        rows: Vec<Line<'static>>,
        anchor: Rect,
        sel: usize,
        console: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            rows,
            anchor,
            sel,
            console,
            theme,
        }
    }
}

impl StatefulWidget for Panel<'_> {
    type State = (Rect, usize);

    fn render(self, area: Rect, buf: &mut Buffer, hit: &mut Self::State) {
        let Self {
            rows,
            anchor,
            sel,
            console,
            theme: t,
        } = self;
        let sel = sel.min(rows.len() - 1);
        let right = anchor.right().min(area.right());
        let width = rows.iter().map(Line::width).max().unwrap_or(0) + 5;
        let width = to_u16(width).min(right.saturating_sub(area.x));
        let room = usize::from(anchor.y.saturating_sub(area.y));
        let shown = rows.len().min(MAX_ROWS).min(room);
        let start = (sel + 1).saturating_sub(shown);
        let h = to_u16(shown);
        let panel = Rect::new(right - width, anchor.y - h, width, h);
        *hit = (panel, start);
        let background = PanelBackground::new(area, console, t);
        let inner = panel_inner(panel);
        let band = background.band;
        background.render(panel, buf);
        for (offset, mut line) in rows.into_iter().skip(start).take(shown).enumerate() {
            let at = Rect::new(inner.x, inner.y + to_u16(offset), inner.width, 1);
            let marker = if start + offset == sel {
                let style = light_panel_row(buf, panel, at.y, console, t);
                line.style = style;
                for span in &mut line.spans {
                    span.style = span.style.patch(style);
                }
                if console {
                    "> "
                } else {
                    wros_shell::look::MARKER
                }
            } else {
                line.style = Style::default().fg(t.text).bg(band);
                "  "
            };
            line.spans.insert(0, Span::raw(marker));
            Paragraph::new(line).render(at, buf);
        }
    }
}

/// The Platinum band and shadow shared by dropdowns and widget panels.
pub(super) struct PanelBackground<'a> {
    clip: Rect,
    console: bool,
    theme: &'a Theme,
    pub(super) band: Color,
}

impl<'a> PanelBackground<'a> {
    pub(super) fn new(clip: Rect, console: bool, theme: &'a Theme) -> Self {
        Self {
            clip,
            console,
            theme,
            band: bar_band(theme, console).unwrap_or(theme.sel),
        }
    }
}

pub(super) fn panel_inner(rect: Rect) -> Rect {
    Rect::new(
        rect.x.saturating_add(1),
        rect.y,
        rect.width.saturating_sub(2),
        rect.height,
    )
}

impl Widget for PanelBackground<'_> {
    fn render(self, rect: Rect, buf: &mut Buffer) {
        Clear.render(rect, buf);
        let t = self.theme;
        let linux_console = self.console;
        let band = self.band;
        for cy in rect.top()..rect.bottom() {
            for cx in rect.left()..rect.right() {
                let cell = &mut buf[(cx, cy)];
                cell.set_fg(t.text).set_bg(band);
                if linux_console && (cx == rect.left() || cx + 1 == rect.right()) {
                    cell.set_symbol("│").set_fg(t.border);
                }
            }
        }
        if !rect.is_empty() {
            let clip = self.clip.intersection(buf.area);
            for cy in rect.y.saturating_add(1)..rect.bottom() {
                if rect.right() < clip.right() && cy >= clip.top() && cy < clip.bottom() {
                    shadow_cell(&mut buf[(rect.right(), cy)], t.dim);
                }
            }
            if rect.bottom() < clip.bottom() {
                for cx in
                    rect.x.saturating_add(1)..=rect.right().min(clip.right().saturating_sub(1))
                {
                    if cx >= clip.left() {
                        shadow_cell(&mut buf[(cx, rect.bottom())], t.dim);
                    }
                }
            }
        }
    }
}

pub(super) fn light_panel_row(
    buf: &mut Buffer,
    panel: Rect,
    y: u16,
    linux_console: bool,
    t: &Theme,
) -> Style {
    if !linux_console {
        for x in panel.left()..panel.right() {
            buf[(x, y)].set_fg(Color::Black).set_bg(t.accent);
        }
    }
    Style::default().fg(Color::Black).bg(t.accent)
}

fn shadow_cell(cell: &mut ratatui::buffer::Cell, dim: ratatui::style::Color) {
    cell.set_fg(dim);
    if let ratatui::style::Color::Rgb(r, g, b) = cell.bg {
        cell.set_bg(ratatui::style::Color::Rgb(r / 2, g / 2, b / 2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_preserves_symbols_and_darkens_only_rgb_backgrounds() {
        use ratatui::style::Color;

        let mut cell = ratatui::buffer::Cell::default();
        cell.set_symbol("X").set_bg(Color::Rgb(101, 50, 9));
        shadow_cell(&mut cell, Color::DarkGray);
        assert_eq!(cell.symbol(), "X");
        assert_eq!(cell.fg, Color::DarkGray);
        assert_eq!(cell.bg, Color::Rgb(50, 25, 4));
        cell.set_bg(Color::Blue);
        shadow_cell(&mut cell, Color::Gray);
        assert_eq!(cell.symbol(), "X");
        assert_eq!(cell.fg, Color::Gray);
        assert_eq!(cell.bg, Color::Blue);
    }
}
