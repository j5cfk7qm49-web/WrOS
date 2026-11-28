#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::{App, BarHit, DeskView, Theme, Title, bar_band};
use crate::geometry::to_u16;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};

// The native renderer's two-cell tiles (wros-graphics `unifont::TILES`, `TILES_LIT` and
// `TILE_POWER`). Keep the same code points available to TestBackend in builds without
// wros-graphics.
const TILES: [[char; 2]; 4] = [
    ['\u{E120}', '\u{E121}'],
    ['\u{E122}', '\u{E123}'],
    ['\u{E124}', '\u{E125}'],
    ['\u{E126}', '\u{E127}'],
];
const TILES_LIT: [[char; 2]; 4] = [
    ['\u{E130}', '\u{E131}'],
    ['\u{E132}', '\u{E133}'],
    ['\u{E134}', '\u{E135}'],
    ['\u{E136}', '\u{E137}'],
];
const TILE_POWER: [char; 2] = ['\u{E128}', '\u{E129}'];
const TILE_POWER_LIT: [char; 2] = ['\u{E138}', '\u{E139}'];

pub(super) struct ViewTiles<'a> {
    current: DeskView,
    selected: Option<DeskView>,
    study_due: bool,
    power_lit: bool,
    glyphs: bool,
    console: bool,
    theme: &'a Theme,
}

impl<'a> ViewTiles<'a> {
    pub(super) fn from_app(
        app: &App,
        theme: &'a Theme,
        selected: Option<DeskView>,
        lit: Option<Title>,
    ) -> Self {
        Self {
            current: app.desk_view,
            selected,
            study_due: app.commonplace.study.all_due > 0,
            power_lit: lit == Some(Title::Power),
            glyphs: app.tile_glyphs,
            console: app.linux_console,
            theme,
        }
    }

    pub(super) fn areas(&self, area: Rect) -> (Rect, Rect) {
        let power_width = (if self.glyphs { 2 } else { 5 }).min(area.width);
        let space = area.width - power_width;
        let shown = Self::view_count(space);
        // Leading space, two cells per tile, then a divider if it fits.
        let views_width = (1 + to_u16(shown) * 2 + 1).min(space);
        (
            Rect::new(area.x, area.y, views_width, 1),
            Rect::new(area.right() - power_width, area.y, power_width, 1),
        )
    }

    fn view_count(width: u16) -> usize {
        usize::from(width.saturating_sub(1) / 2).min(DeskView::ALL.len())
    }
}

impl StatefulWidget for ViewTiles<'_> {
    type State = Vec<(Rect, BarHit)>;

    fn render(self, area: Rect, buf: &mut Buffer, hits: &mut Self::State) {
        let (views, power_area) = self.areas(area);
        let t = self.theme;
        // On the band, give the tiles a different ground so their lines are visible.
        let base = if bar_band(t, self.console).is_some() {
            Style::default().bg(t.sel)
        } else {
            Style::default()
        };
        let mut spans = vec![Span::raw(" ")];
        let mut x = views.x.saturating_add(1).min(views.right());
        let shown = Self::view_count(views.width);
        // Each tile is two cells: in the native renderer a square around the letter (rc31), in text
        // the letter and a space. The current view is filled; cards due turn the S red.
        for (n, view) in DeskView::ALL.into_iter().take(shown).enumerate() {
            let width = 2;
            let current = view == self.current;
            let due = view == DeskView::Study && self.study_due;
            let ink = if due { t.warn } else { t.dim };
            let selected = self.selected == Some(view);
            if self.glyphs {
                let (pair, style) = if current {
                    (TILES_LIT[n], base.fg(t.accent))
                } else {
                    (TILES[n], base.fg(ink))
                };
                let style = if selected {
                    style.add_modifier(Modifier::UNDERLINED)
                } else {
                    style
                };
                spans.push(Span::styled(String::from_iter(pair), style));
            } else {
                let ground = if current {
                    Style::default().fg(Color::Black).bg(t.accent)
                } else {
                    base.fg(ink)
                };
                let letter = if selected {
                    ground.add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                } else {
                    ground
                };
                let initial = view.label().chars().next().unwrap_or(' ');
                spans.push(Span::styled(initial.to_string(), letter));
                spans.push(Span::styled(" ", if current { ground } else { base }));
            }
            hits.push((Rect::new(x, views.y, width, 1), BarHit::Tile(view)));
            x += width;
        }
        if views.right() > x {
            spans.push(Span::styled("│", base.fg(t.border)));
        }
        Paragraph::new(Line::from(spans)).render(views, buf);

        let look: wros_shell::Theme = t.into();
        let power_style = if self.power_lit {
            wros_shell::look::highlight(look).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text)
        };
        let power = if self.glyphs {
            // Lit, the power tile fills its square with the highlight, as the current view's does.
            let (pair, style) = if self.power_lit {
                (TILE_POWER_LIT, Style::default().fg(t.accent))
            } else {
                (TILE_POWER, Style::default().fg(t.text))
            };
            vec![Span::styled(String::from_iter(pair), style)]
        } else {
            vec![
                Span::styled("Po", power_style),
                Span::styled("w", power_style.add_modifier(Modifier::UNDERLINED)),
                Span::styled("er", power_style),
            ]
        };
        Paragraph::new(Line::from(power)).render(power_area, buf);
    }
}
