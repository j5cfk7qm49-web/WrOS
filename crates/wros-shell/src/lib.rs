//! Shared presentation only. No input reader, processes, filesystem, editor,
//! credential storage or authentication implementation belongs in this crate.
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const HISTORY_LIMIT: usize = 16;
const ROUTE_LIMIT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection<R> {
    pub route: R,
    pub index: usize,
}

/// Caller-owned, versioned presentation preferences; never authentication state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences<R> {
    pub version: u32,
    pub selections: Vec<Selection<R>>,
}

#[derive(Debug, Clone)]
pub struct Navigation<R> {
    current: R,
    selected: usize,
    history: Vec<(R, usize)>,
    selections: Vec<Selection<R>>,
}

impl<R: Copy + Eq> Navigation<R> {
    pub fn new(home: R) -> Self {
        Self {
            current: home,
            selected: 0,
            history: vec![],
            selections: vec![],
        }
    }
    pub fn restore(&mut self, preferences: Preferences<R>) -> Result<(), &'static str> {
        if preferences.version != 1 || preferences.selections.len() > ROUTE_LIMIT {
            return Err("unsupported or oversized shell preferences; original preserved");
        }
        for (at, selection) in preferences.selections.iter().enumerate() {
            if selection.index > 10_000
                || preferences.selections[..at]
                    .iter()
                    .any(|s| s.route == selection.route)
            {
                return Err("invalid shell selections; original preserved");
            }
        }
        self.selections = preferences.selections;
        self.selected = self.remembered(self.current);
        Ok(())
    }
    pub fn preferences(&self) -> Preferences<R> {
        let mut copy = self.clone();
        copy.remember();
        Preferences {
            version: 1,
            selections: copy.selections,
        }
    }
    pub fn current(&self) -> R {
        self.current
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    fn remembered(&self, route: R) -> usize {
        self.selections
            .iter()
            .find(|s| s.route == route)
            .map_or(0, |s| s.index)
    }
    fn remember(&mut self) {
        if let Some(s) = self.selections.iter_mut().find(|s| s.route == self.current) {
            s.index = self.selected;
        } else {
            if self.selections.len() == ROUTE_LIMIT {
                self.selections.remove(0);
            }
            self.selections.push(Selection {
                route: self.current,
                index: self.selected,
            });
        }
    }
    pub fn reset(&mut self, home: R) {
        self.remember();
        self.history.clear();
        self.current = home;
        self.selected = self.remembered(home);
    }
    pub fn open(&mut self, route: R) {
        if route == self.current {
            return;
        }
        self.remember();
        if self.history.len() == HISTORY_LIMIT {
            self.history.remove(1);
        }
        self.history.push((self.current, self.selected));
        self.current = route;
        self.selected = self.remembered(route);
    }
    pub fn back(&mut self) -> bool {
        let Some((route, selection)) = self.history.pop() else {
            return false;
        };
        self.remember();
        self.current = route;
        self.selected = selection;
        true
    }
    pub fn select(&mut self, index: usize, count: usize) {
        self.selected = index.min(count.saturating_sub(1));
    }
    pub fn move_by(&mut self, amount: isize, count: usize) {
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = self
            .selected
            .min(count - 1)
            .saturating_add_signed(amount)
            .min(count - 1);
    }
}

#[derive(Debug, Default)]
pub struct Notices(VecDeque<String>);
impl Notices {
    /// Status only. Authentication secrets must never enter notifications.
    pub fn push(&mut self, text: &str) {
        if self.0.len() == 16 {
            self.0.pop_front();
        }
        self.0.push_back(
            text.chars()
                .take(512)
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect(),
        );
    }
    pub fn latest(&self) -> Option<&str> {
        self.0.back().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub text: Color,
    pub accent: Color,
    pub dim: Color,
    pub border: Color,
}
pub struct Menu<'a> {
    pub title: &'a str,
    pub rows: &'a [String],
    pub selected: usize,
    pub notice: &'a str,
    pub hint: &'a str,
}

/// Shared clipped menu with a visible selection and fixed help/status area.
pub fn render_menu(frame: &mut Frame, area: Rect, menu: Menu<'_>, theme: Theme) {
    let border = Block::bordered()
        .title(menu.title)
        .border_style(Style::default().fg(theme.border));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let [body, status, help] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    let rows = menu
        .rows
        .iter()
        .map(|row| ListItem::new(row.as_str()))
        .collect::<Vec<_>>();
    let mut selected = ListState::default().with_selected(
        (!rows.is_empty()).then_some(menu.selected.min(rows.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(
        List::new(rows)
            .style(Style::default().fg(theme.text))
            .highlight_style(Style::default().fg(theme.accent))
            .highlight_symbol("▸ "),
        body,
        &mut selected,
    );
    frame.render_widget(
        Paragraph::new(menu.notice).style(Style::default().fg(theme.dim)),
        status,
    );
    frame.render_widget(
        Paragraph::new(menu.hint).style(Style::default().fg(theme.accent)),
        help,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn back_restores_parent_selection_and_reopening_remembers_child() {
        let mut nav = Navigation::new(0u8);
        nav.select(3, 4);
        nav.open(1);
        nav.select(7, 10);
        assert!(nav.back());
        assert_eq!((nav.current(), nav.selected()), (0, 3));
        nav.open(1);
        assert_eq!(nav.selected(), 7);
        nav.move_by(1, 2);
        assert_eq!(nav.selected(), 1);
    }
    #[test]
    fn history_is_bounded_but_retains_the_entry_route() {
        let mut nav = Navigation::new(0u8);
        for route in 1..100 {
            nav.open(route);
        }
        let mut count = 0;
        while nav.back() {
            count += 1;
        }
        assert_eq!(count, HISTORY_LIMIT);
        assert_eq!(nav.current(), 0);
        nav.move_by(-1, 0);
        assert_eq!(nav.selected(), 0);
    }
    #[test]
    fn invalid_preferences_do_not_replace_the_previous_state() {
        let mut nav = Navigation::new(0u8);
        nav.select(2, 3);
        let before = nav.preferences();
        assert!(
            nav.restore(Preferences {
                version: 2,
                selections: vec![]
            })
            .is_err()
        );
        assert_eq!(nav.preferences(), before);
        assert!(
            nav.restore(Preferences {
                version: 1,
                selections: vec![
                    Selection { route: 0, index: 1 },
                    Selection { route: 0, index: 2 },
                ]
            })
            .is_err()
        );
        let text = serde_json::to_string(&before).unwrap();
        nav.restore(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(nav.selected(), 2);
    }
    #[test]
    fn notices_bound_untrusted_control_text_and_keep_utf8() {
        let mut notices = Notices::default();
        for _ in 0..100 {
            notices.push("old");
        }
        notices.push(&format!("é\x1b\n{}", "λ".repeat(600)));
        assert_eq!(notices.0.len(), 16);
        let last = notices.latest().unwrap();
        assert_eq!(last.chars().count(), 512);
        assert!(last.starts_with("é  "));
        assert!(!last.chars().any(char::is_control));
    }
    #[test]
    fn very_narrow_menus_clip_without_panicking() {
        use ratatui::{Terminal, backend::TestBackend};
        for (width, height) in [(1, 1), (20, 5), (80, 24)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let rows = (0..40)
                .map(|n| format!("{n} · λ system setting"))
                .collect::<Vec<_>>();
            terminal
                .draw(|frame| {
                    render_menu(
                        frame,
                        frame.area(),
                        Menu {
                            title: "WrOS settings",
                            rows: &rows,
                            selected: 39,
                            notice: "offline",
                            hint: "Esc back",
                        },
                        Theme {
                            text: Color::White,
                            accent: Color::Yellow,
                            dim: Color::Gray,
                            border: Color::Blue,
                        },
                    )
                })
                .unwrap();
        }
    }
}
