//! The login form, ported from Lemurs (<https://github.com/coastalwhite/lemurs>,
//! MIT, Copyright (c) 2021 Gijs Burghoorn; see LICENSE-LEMURS): its layout, its
//! session switcher and its input fields, drawn in the WrOS theme.
//!
//! greetd still authenticates and starts the session; this only collects what
//! the writer types. Changes from Lemurs: typed text sits in a fixed-capacity,
//! zeroized buffer and removed characters are wiped; the password field shows a
//! constant mark instead of one per character; Up and Down in the account field
//! pick a local account; there are no power commands and no configuration file.
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use zeroize::{Zeroize, Zeroizing};

use crate::{MAX_INPUT, Session, display_text};

/// What the password field shows, whatever was typed.
pub const HIDDEN: &str = "[hidden]";
/// Lemurs' default field width.
const FIELD_WIDTH: u16 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Switcher,
    Username,
    Password,
}

/// One line of typed text with a cursor. From Lemurs' `InputFieldWidget`.
pub struct Field {
    // Fixed capacity: typed secrets are never reallocated, so no copy is left behind.
    content: Zeroizing<String>,
    /// Cursor position in characters.
    cursor: usize,
    secret: bool,
}

impl Field {
    fn new(secret: bool) -> Self {
        Self {
            content: Zeroizing::new(String::with_capacity(MAX_INPUT)),
            cursor: 0,
            secret,
        }
    }

    fn byte_offset(&self, chars: usize) -> usize {
        self.content
            .char_indices()
            .nth(chars)
            .map_or(self.content.len(), |(i, _)| i)
    }

    fn chars(&self) -> usize {
        self.content.chars().count()
    }

    pub fn insert(&mut self, c: char) {
        if c.is_control() || self.content.len() + c.len_utf8() > MAX_INPUT {
            return;
        }
        let at = self.byte_offset(self.cursor);
        self.content.insert(at, c);
        self.cursor += 1;
    }

    /// Removes the character before the cursor and wipes the bytes it vacated.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.remove_at_cursor();
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars() {
            self.remove_at_cursor();
        }
    }

    fn remove_at_cursor(&mut self) {
        let start = self.byte_offset(self.cursor);
        let end = self.byte_offset(self.cursor + 1);
        let len = self.content.len();
        // Shift the tail left in place, then wipe the freed bytes before truncating.
        unsafe {
            let bytes = self.content.as_bytes_mut();
            bytes.copy_within(end..len, start);
            bytes[len - (end - start)..].zeroize();
        }
        self.content.truncate(len - (end - start));
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars();
    }

    pub fn clear(&mut self) {
        self.content.zeroize();
        self.cursor = 0;
    }

    pub fn set(&mut self, text: &str) {
        self.clear();
        for c in text.chars().take(64) {
            self.insert(c);
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// What the field shows in `width` cells, and where its cursor goes. Echoed
    /// text scrolls so the cursor stays visible. A secret shows a constant mark
    /// and no cursor, so neither the text nor its length reaches the screen.
    fn shown(&self, width: usize, focused: bool) -> (String, Option<usize>) {
        if self.secret {
            let mark = if focused || !self.is_empty() {
                HIDDEN
            } else {
                ""
            };
            return (mark.into(), None);
        }
        let width = width.max(1);
        let scroll = (self.cursor + 1).saturating_sub(width);
        let text = display_text(&self.content)
            .chars()
            .skip(scroll)
            .take(width)
            .collect();
        (text, focused.then_some(self.cursor - scroll))
    }
}

impl Drop for Field {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Lemurs' environment switcher, choosing the writing session.
pub struct Switcher {
    items: Vec<Session>,
    selected: usize,
}

impl Switcher {
    pub fn left(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.selected = (self.selected + 1).min(self.items.len() - 1);
    }

    pub fn current(&self) -> Session {
        self.items[self.selected]
    }

    /// `◂ previous   current   next ▸`, as Lemurs draws it, neighbours dimmed.
    fn line(&self, focused: bool, theme: wros_shell::Theme) -> Line<'static> {
        let mover = Style::default().fg(if focused { theme.accent } else { theme.dim });
        let neighbour = Style::default().fg(theme.dim);
        let current = if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let mut spans = vec![];
        match self.selected.checked_sub(1).map(|i| self.items[i]) {
            Some(prev) => spans.extend([
                Span::styled("◂ ", mover),
                Span::styled(prev.title(), neighbour),
                Span::raw("   "),
            ]),
            None => spans.push(Span::raw("  ")),
        }
        spans.push(Span::styled(self.current().title(), current));
        if let Some(next) = self.items.get(self.selected + 1) {
            spans.extend([
                Span::raw("   "),
                Span::styled(next.title(), neighbour),
                Span::styled(" ▸", mover),
            ]);
        }
        Line::from(spans)
    }
}

/// What a key asks the login to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Enter in the password field: sign in.
    Submit,
}

/// The whole form: session switcher, account field and password field.
pub struct Form {
    pub focus: Focus,
    /// None when only the console session can start.
    pub switcher: Option<Switcher>,
    pub username: Field,
    pub password: Field,
    accounts: Vec<String>,
    account: Option<usize>,
}

impl Form {
    pub fn new(accounts: Vec<String>, graphical: bool) -> Self {
        Self {
            focus: Focus::Username,
            switcher: graphical.then(|| Switcher {
                items: vec![Session::Usual, Session::Console],
                selected: 0,
            }),
            username: Field::new(false),
            password: Field::new(true),
            accounts,
            account: None,
        }
    }

    pub fn session(&self) -> Session {
        self.switcher
            .as_ref()
            .map_or(Session::Usual, Switcher::current)
    }

    /// Back to an empty form, as after a rejected login or a cancel.
    pub fn reset(&mut self) {
        self.username.clear();
        self.password.clear();
        self.account = None;
        self.focus = Focus::Username;
    }

    fn next(&mut self) {
        self.focus = match self.focus {
            Focus::Switcher => Focus::Username,
            Focus::Username | Focus::Password => Focus::Password,
        };
    }

    fn prev(&mut self) {
        self.focus = match self.focus {
            Focus::Password => Focus::Username,
            Focus::Username | Focus::Switcher if self.switcher.is_some() => Focus::Switcher,
            other => other,
        };
    }

    fn pick_account(&mut self, step: isize) {
        if self.accounts.is_empty() {
            return;
        }
        let len = self.accounts.len() as isize;
        let index = match self.account {
            None if step < 0 => len - 1,
            None => 0,
            Some(i) => (i as isize + step).rem_euclid(len),
        } as usize;
        self.account = Some(index);
        self.username.set(&self.accounts[index]);
    }

    pub fn key(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Tab => self.next(),
            KeyCode::BackTab => self.prev(),
            KeyCode::Enter => match self.focus {
                Focus::Password => return Action::Submit,
                _ => self.next(),
            },
            _ => match self.focus {
                Focus::Switcher => match key.code {
                    KeyCode::Left => self.switcher.as_mut().map_or((), Switcher::left),
                    KeyCode::Right => self.switcher.as_mut().map_or((), Switcher::right),
                    KeyCode::Down => self.next(),
                    _ => {}
                },
                Focus::Username | Focus::Password => {
                    let username = self.focus == Focus::Username;
                    let field = if username {
                        &mut self.username
                    } else {
                        &mut self.password
                    };
                    match key.code {
                        KeyCode::Up if username => self.pick_account(-1),
                        KeyCode::Down if username => self.pick_account(1),
                        KeyCode::Up => self.prev(),
                        KeyCode::Down => {}
                        KeyCode::Left => field.left(),
                        KeyCode::Right => field.right(),
                        KeyCode::Home => field.home(),
                        KeyCode::End => field.end(),
                        KeyCode::Backspace => field.backspace(),
                        KeyCode::Delete => field.delete(),
                        KeyCode::Char('u') if ctrl => field.clear(),
                        KeyCode::Char(c)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            field.insert(c)
                        }
                        _ => {}
                    }
                }
            },
        }
        Action::None
    }

    /// Lemurs' layout inside the WrOS panel: switcher, then the two fields,
    /// each at most 48 cells wide and centred.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: wros_shell::Theme) {
        let [_, switcher, _, username, _, password, _] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(area);
        if let Some(choice) = &self.switcher {
            frame.render_widget(
                Paragraph::new(choice.line(self.focus == Focus::Switcher, theme)).centered(),
                switcher,
            );
        }
        for (field, rect, title, focus) in [
            (&self.username, username, " Local account ", Focus::Username),
            (&self.password, password, " Password ", Focus::Password),
        ] {
            let width = rect.width.min(FIELD_WIDTH);
            let rect = Rect {
                x: rect.x + (rect.width - width) / 2,
                width,
                ..rect
            };
            let focused = self.focus == focus;
            let block =
                Block::bordered()
                    .title(title)
                    .border_style(Style::default().fg(if focused {
                        theme.accent
                    } else {
                        theme.border
                    }));
            let inner = block.inner(rect);
            let (text, cursor) = field.shown(inner.width as usize, focused);
            let style = Style::default().fg(if field.secret { theme.dim } else { theme.text });
            frame.render_widget(Paragraph::new(text).style(style).block(block), rect);
            if let Some(column) = cursor
                && inner.width > 0
                && inner.height > 0
            {
                frame.set_cursor_position(Position::new(inner.x + column as u16, inner.y));
            }
        }
    }
}
