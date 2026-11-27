#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::top::ranked_marks;
use super::{App, DeskView, Theme, fit_text, paint_band};
use crate::gauges::{self, Gauges, Item};
use crate::geometry::to_u16;
use crate::ui::{info_separator, thousands};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{StatefulWidget, Widget};
use std::time::{Instant, SystemTime};

pub(crate) struct InfoBar<'a> {
    text: String,
    items: Vec<(Item, String)>,
    gauges: Gauges,
    console: bool,
    theme: &'a Theme,
}

impl<'a> InfoBar<'a> {
    pub(crate) fn from_app(app: &App, theme: &'a Theme, now: Instant) -> Self {
        let light = app
            .deck
            .light_flash
            .is_some_and(|at| now.saturating_duration_since(at) < gauges::LIGHT_SHOWN);
        let gauges = app.deck.gauges;
        let console = app.linux_console;
        Self {
            text: info_section(app),
            items: gauges.items(false, console, light, &app.deck.clock),
            gauges,
            console,
            theme,
        }
    }
}

impl StatefulWidget for InfoBar<'_> {
    type State = Vec<(Rect, gauges::Widget)>;

    fn render(self, area: Rect, buf: &mut Buffer, hits: &mut Self::State) {
        if area.is_empty() {
            return;
        }
        let t = self.theme;
        let console = self.console;
        let width = usize::from(area.width);
        let text_width = Span::raw(self.text.as_str()).width();
        let reserved = text_width.min(width / 2);
        let items = gauges::fit(self.items, width.saturating_sub(reserved + 1));
        let gauge_width: usize = items.iter().map(|(_, s)| gauges::item_width(s)).sum();
        let left = fit_text(&self.text, width.saturating_sub(gauge_width + 1), console);
        Line::styled(format!(" {left}"), Style::default().fg(t.text)).render(area, buf);

        let mut x = area.right() - to_u16(gauge_width);
        let dim = Style::default().fg(t.dim);
        for (item, text) in items {
            let width = to_u16(gauges::item_width(&text) - 2);
            let rect = Rect::new(x + 1, area.y, width, 1);
            if let Some(widget) = item.widget() {
                hits.push((rect, widget));
            }
            x += width + 2;
            let line = match (item, self.gauges.wifi) {
                (Item::Wifi, Some(gauges::Wifi::Bars(n))) if !console => Line::from(
                    text.chars()
                        .enumerate()
                        .map(|(i, c)| {
                            let style = if i < usize::from(n) {
                                Style::default().fg(t.text)
                            } else {
                                dim
                            };
                            Span::styled(c.to_string(), style)
                        })
                        .collect::<Vec<_>>(),
                ),
                (Item::Battery, _) if self.gauges.low_battery() => {
                    Line::styled(text, Style::default().fg(t.warn))
                }
                _ => Line::styled(text, dim),
            };
            line.render(rect, buf);
        }
        paint_band(buf, area, t, console);
    }
}

fn info_section(app: &App) -> String {
    match app.desk_view {
        DeskView::Read => crate::app::read::info(app),
        DeskView::Outline => crate::app::outline::info(app).unwrap_or_default(),
        DeskView::Study => crate::app::study::info(app),
        DeskView::Write => {
            if let Some(selection) = app.editor.selected_text() {
                let words = grimoire_core::notes::count_words(&selection);
                return format!("{} words selected", thousands(words));
            }
            let Some(scene) = app.open.map(|i| &app.project.nodes[i]) else {
                return String::new();
            };
            // The editor's title already shows the chapter, the scene, its words and its goal, and
            // the outline shows the project: only what neither shows goes here (the owner's rule).
            let mut details = Vec::new();
            if !app.deck.state.current().outline || app.rect_tree.is_empty() {
                details.push(app.project.meta.title.clone());
            }
            details.extend(
                scene
                    .status
                    .iter()
                    .filter(|s| !s.trim().is_empty())
                    .cloned(),
            );
            if let Some(modified) = scene.stat.and_then(|stat| stat.modified) {
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                details.push(format!("edited {} min ago", age.as_secs() / 60));
            }
            details.join(info_separator(app.linux_console))
        }
    }
}

pub(crate) struct FocusWarning<'a> {
    text: String,
    theme: &'a Theme,
    console: bool,
}

impl Widget for FocusWarning<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let shown = fit_text(&self.text, usize::from(area.width), self.console);
        Line::styled(shown, Style::default().fg(self.theme.warn)).render(area, buf);
        paint_band(buf, area, self.theme, self.console);
    }
}

pub(crate) fn focus_warning<'a>(app: &App, t: &'a Theme) -> Option<FocusWarning<'a>> {
    let marks = ranked_marks(app, t, true);
    let warnings = marks
        .iter()
        .map(|(text, _, _)| text.as_str())
        .chain(app.deck.battery.countdown_warning());
    let text = warnings.collect::<Vec<_>>().join("  ");
    (!text.is_empty()).then_some(FocusWarning {
        text,
        theme: t,
        console: app.linux_console,
    })
}
