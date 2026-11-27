#![warn(clippy::print_stdout, clippy::print_stderr)]

//! Shared presentation only. No input reader, processes, filesystem, editor,
//! credential storage or authentication implementation belongs in this crate.
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub mod clock;
pub mod grimoire_theme;
pub mod logo;
pub mod look;
pub mod paths;

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

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
    /// Move `amount` options up or down, passing over descriptions.
    pub fn move_among(&mut self, amount: isize, kinds: &[RowKind], count: usize) {
        let mut at = settle(self.selected, kinds, count);
        for _ in 0..amount.unsigned_abs() {
            let next = if amount < 0 {
                (0..at).rev().find(|&i| choosable(kinds, i))
            } else {
                (at + 1..count).find(|&i| choosable(kinds, i))
            };
            match next {
                Some(i) => at = i,
                None => break,
            }
        }
        self.selected = at;
    }
    /// Off a description onto the nearest option, preferring the one below.
    pub fn settle(&mut self, kinds: &[RowKind], count: usize) {
        self.selected = settle(self.selected, kinds, count);
    }
}

/// What a menu row is, so an option never looks like the text describing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Does something when chosen.
    Option,
    /// Opens another screen; drawn with a trailing ›.
    Opens,
    /// Describes or reports; dimmed, indented, never selected.
    Info,
}

fn choosable(kinds: &[RowKind], index: usize) -> bool {
    kinds.get(index) != Some(&RowKind::Info)
}

/// The option at or after `index`, else the nearest before it. A menu with
/// no options keeps `index` (clamped), so its rows can still be read.
pub fn settle(index: usize, kinds: &[RowKind], count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let index = index.min(count - 1);
    (index..count)
        .find(|&i| choosable(kinds, i))
        .or_else(|| (0..index).rev().find(|&i| choosable(kinds, i)))
        .unwrap_or(index)
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
    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub text: Color,
    pub accent: Color,
    pub dim: Color,
    pub border: Color,
    /// The shaded highlight row's background.
    pub sel: Color,
    pub warn: Color,
}
/// Project the exact Grimoire palette onto the common system widgets.
impl From<&grimoire_theme::Theme> for Theme {
    fn from(theme: &grimoire_theme::Theme) -> Self {
        Self {
            text: theme.text,
            accent: theme.accent,
            dim: theme.dim,
            border: theme.border,
            sel: theme.sel,
            warn: theme.warn,
        }
    }
}
impl Default for Theme {
    fn default() -> Self {
        (&grimoire_theme::default_theme()).into()
    }
}

pub struct Menu<'a> {
    pub title: &'a str,
    pub rows: &'a [String],
    /// One per row; rows past its end are options.
    pub kinds: &'a [RowKind],
    pub selected: usize,
    pub notice: &'a str,
    pub hint: &'a str,
}

/// Shared clipped menu with a visible selection and fixed help/status area.
pub fn render_menu(frame: &mut Frame, area: Rect, menu: Menu<'_>, theme: Theme) {
    render_menu_at(frame, area, menu, theme);
}

/// [`render_menu`], returning the list's area and the first row shown in it,
/// so a click can be matched to a row.
pub fn render_menu_at(
    frame: &mut Frame,
    area: Rect,
    menu: Menu<'_>,
    theme: Theme,
) -> (Rect, usize) {
    // The notice wraps into rows the list leaves empty, never into the list's own.
    let inner_height = area.height.saturating_sub(2);
    let spare = usize::from(inner_height.saturating_sub(2)).saturating_sub(menu.rows.len());
    let body = render_panel_lines(
        frame,
        area,
        menu.title,
        menu.notice,
        menu.hint,
        theme,
        1 + spare,
    );
    let rows = menu
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| match menu.kinds.get(i) {
            Some(RowKind::Info) => {
                ListItem::new(format!("  {row}")).style(Style::default().fg(theme.dim))
            }
            Some(RowKind::Opens) => ListItem::new(format!("{row} ›")),
            _ => ListItem::new(row.as_str()),
        })
        .collect::<Vec<_>>();
    let mut selected = ListState::default().with_selected(
        (!rows.is_empty()).then_some(menu.selected.min(rows.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(
        List::new(rows)
            .style(Style::default().fg(theme.text))
            .highlight_style(look::highlight(theme))
            .highlight_symbol(look::MARKER),
        body,
        &mut selected,
    );
    (body, selected.offset())
}

/// Common system-panel geometry: the pane border with its bold title, then
/// fixed status and hint lines under the body. Forms and progress use it as
/// Settings and login do. Returns the clipped body; callers own their content
/// and input state.
pub fn render_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    notice: &str,
    hint: &str,
    theme: Theme,
) -> Rect {
    render_panel_lines(frame, area, title, notice, hint, theme, 1)
}

/// [`render_panel`] with up to `max_notice` lines for a wrapped notice. Callers whose content has
/// a fixed height pass 1 (the notice is cut at the box's edge, as before).
#[warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn render_panel_lines(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    notice: &str,
    hint: &str,
    theme: Theme,
    max_notice: usize,
) -> Rect {
    let border = look::pane_block(title, true, theme);
    let inner = border.inner(area);
    frame.render_widget(border, area);
    // The notice wraps inside the box rather than setting its width (a long one used to stretch
    // the box nearly across the screen after it opened).
    let mut notice_lines = notice_lines(notice, inner.width);
    if notice_lines.len() > max_notice.max(1) {
        // No room to wrap: one line, cut at the edge as it always was.
        notice_lines = vec![notice.to_string()];
    }
    let [body, status, help] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(to_u16(notice_lines.len())),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(
            notice_lines
                .into_iter()
                .map(ratatui::text::Line::from)
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(theme.dim)),
        status,
    );
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(look::hint_spans(hint, theme))),
        help,
    );
    body
}

/// Most lines a panel's notice takes before the rest is cut with an ellipsis.
pub const NOTICE_LINES: usize = 3;

/// `notice` word-wrapped to `width` columns, one line at least (an empty notice keeps its line, so
/// the box doesn't jump when one arrives) and [`NOTICE_LINES`] at most.
pub fn notice_lines(notice: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in notice.split_whitespace() {
        let mut word = word.to_string();
        // A word longer than the line is broken across lines.
        while word.chars().count() > width {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            lines.push(head);
        }
        let need = line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
        if need > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    if lines.len() > NOTICE_LINES {
        lines.truncate(NOTICE_LINES);
        let last = &mut lines[NOTICE_LINES - 1];
        let keep = width.saturating_sub(1).min(last.chars().count());
        *last = last.chars().take(keep).collect::<String>() + "…";
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_notice_wraps_at_words_and_keeps_at_least_one_line() {
        assert_eq!(notice_lines("", 20), vec![String::new()]);
        assert_eq!(notice_lines("short", 20), vec!["short"]);
        // Four lines' worth: the third is cut with an ellipsis.
        assert_eq!(
            notice_lines("one two three four five six", 9),
            vec!["one two", "three", "four fiv…"]
        );
        assert_eq!(notice_lines("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

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
    fn the_cursor_moves_only_among_options_and_never_rests_on_a_description() {
        use RowKind::{Info, Opens, Option as Act};
        let kinds = [Info, Act, Info, Info, Opens, Info];
        let mut nav = Navigation::new(0u8);
        nav.settle(&kinds, 6);
        assert_eq!(nav.selected(), 1);
        nav.move_among(1, &kinds, 6);
        assert_eq!(nav.selected(), 4);
        nav.move_among(1, &kinds, 6);
        assert_eq!(nav.selected(), 4);
        nav.move_among(-10, &kinds, 6);
        assert_eq!(nav.selected(), 1);
        nav.select(5, 6);
        nav.settle(&kinds, 6);
        assert_eq!(nav.selected(), 4);
        // Rows without a kind are options, as in menus that never describe.
        nav.select(0, 3);
        nav.move_among(2, &[], 3);
        assert_eq!(nav.selected(), 2);
        // A menu of descriptions only keeps its place rather than vanishing.
        nav.select(1, 2);
        nav.move_among(1, &[Info, Info], 2);
        assert_eq!(nav.selected(), 1);
    }
    #[test]
    fn descriptions_are_dim_and_indented_and_options_that_open_screens_are_marked() {
        let theme = Theme::default();
        assert_ne!(theme.dim, theme.text);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        let rows = ["Cancel".to_string(), "Details".into(), "Explains it".into()];
        terminal
            .draw(|frame| {
                render_menu(
                    frame,
                    frame.area(),
                    Menu {
                        title: "T",
                        rows: &rows,
                        kinds: &[RowKind::Option, RowKind::Opens, RowKind::Info],
                        selected: 0,
                        notice: "",
                        hint: "",
                    },
                    theme,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let line = |y| (1..40).map(|x| buffer[(x, y)].symbol()).collect::<String>();
        assert!(line(1).starts_with("▸ Cancel "), "{}", line(1));
        assert!(line(2).starts_with("  Details ›"), "{}", line(2));
        assert!(line(3).starts_with("    Explains it"), "{}", line(3));
        assert_eq!(buffer[(5, 3)].fg, theme.dim);
        assert_eq!(buffer[(3, 2)].fg, theme.text);
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
    fn render_menu_uses_the_shaded_marker_row() {
        use ratatui::{Terminal, backend::TestBackend};
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        let rows = vec!["Writing".to_string(), "System".to_string()];
        terminal
            .draw(|frame| {
                render_menu(
                    frame,
                    frame.area(),
                    Menu {
                        title: "Settings",
                        rows: &rows,
                        kinds: &[RowKind::Opens, RowKind::Opens],
                        selected: 1,
                        notice: "",
                        hint: "↵ open  esc back",
                    },
                    theme,
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let line = |y: u16| (0..40).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(line(2).starts_with("│▸ System ›"), "{:?}", line(2));
        assert_eq!(buf[(3, 2)].bg, theme.sel);
        assert_eq!(buf[(3, 2)].fg, theme.accent);
        assert!(
            buf[(2, 0)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD),
            "bold title"
        );
        let hint_key = (0..40).find(|x| buf[(*x, 8)].symbol() == "↵").unwrap();
        assert!(
            buf[(hint_key, 8)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD),
            "lit key"
        );
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
                            kinds: &[],
                            selected: 39,
                            notice: "offline",
                            hint: "Esc back",
                        },
                        Theme {
                            text: Color::White,
                            accent: Color::Yellow,
                            dim: Color::Gray,
                            border: Color::Blue,
                            sel: Color::DarkGray,
                            warn: Color::Red,
                        },
                    )
                })
                .unwrap();
        }
    }
}
