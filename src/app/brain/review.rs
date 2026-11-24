use chrono::Local;
use grimoire_brain::bounded_store::{DueCards, PagedBrain};
use grimoire_brain::schema::SavedQuery;
use grimoire_core::editor::Editor;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::{App, Theme, commands, draw_draft};

#[derive(Default)]
pub(super) struct ReviewDesk {
    filter: Option<(String, SavedQuery)>,
    due: DueCards,
    pub(super) all_due: u64,
    revealed: bool,
    input: Option<Editor>,
    scroll: u16,
}

impl ReviewDesk {
    pub(super) fn refresh(&mut self, store: &mut PagedBrain) -> anyhow::Result<()> {
        let now = Local::now().timestamp();
        let all = store.due_cards(now, None).map_err(anyhow::Error::msg)?;
        let current = match &self.filter {
            Some((_, query)) => store
                .due_cards(now, Some(query))
                .map_err(anyhow::Error::msg)?,
            None => all.clone(),
        };
        if self.due.next != current.next {
            self.revealed = false;
            self.scroll = 0;
        }
        self.all_due = all.count;
        self.due = current;
        Ok(())
    }

    pub(super) fn paste(&mut self, text: &str) {
        if let Some(input) = &mut self.input {
            let text: String = text.chars().filter(|c| !c.is_control()).collect();
            input.insert_str(&text);
        }
    }

    pub(super) fn edit_history(&mut self, redo: bool) {
        if let Some(input) = &mut self.input {
            if redo {
                input.redo();
            } else {
                input.undo();
            }
        }
    }

    pub(super) fn scroll(&mut self, down: bool) {
        self.scroll = if down {
            self.scroll.saturating_add(1)
        } else {
            self.scroll.saturating_sub(1)
        };
    }

    fn set_filter(&mut self, store: &mut PagedBrain) -> anyhow::Result<()> {
        let Some(input) = &self.input else {
            return Ok(());
        };
        let value = input.text();
        let value = value.trim();
        let next = if value.is_empty() {
            None
        } else if let Some(tag) = value.strip_prefix('#') {
            Some((
                value.to_owned(),
                SavedQuery::Tagged(commands::find_tag(store, tag)?),
            ))
        } else {
            let note = commands::find_note(store, value)?;
            let query = store
                .details(note)
                .map_err(anyhow::Error::msg)?
                .query
                .ok_or_else(|| anyhow::anyhow!("That note does not have a saved search"))?;
            Some((value.to_owned(), query))
        };
        let previous = std::mem::replace(&mut self.filter, next);
        if let Err(error) = self.refresh(store) {
            self.filter = previous;
            return Err(error);
        }
        self.input = None;
        self.revealed = false;
        self.scroll = 0;
        Ok(())
    }

    fn key(&mut self, store: &mut PagedBrain, key: KeyEvent) -> anyhow::Result<Option<String>> {
        if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            return Ok(None);
        }
        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Enter => self.set_filter(store)?,
                KeyCode::Esc => self.input = None,
                KeyCode::Char(c) if !c.is_control() => input.insert(c),
                KeyCode::Left => input.left(),
                KeyCode::Right => input.right(),
                KeyCode::Backspace => input.backspace(),
                KeyCode::Delete => input.delete(),
                _ => {}
            }
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('r') => self.refresh(store)?,
            KeyCode::Char('f') => {
                self.input = Some(Editor::from_text(
                    self.filter.as_ref().map_or("", |(label, _)| label),
                ));
            }
            KeyCode::Char(' ') | KeyCode::Enter if self.due.next.is_some() => {
                self.revealed = true;
                self.scroll = 0;
            }
            KeyCode::Up => self.scroll(false),
            KeyCode::Down => self.scroll(true),
            KeyCode::Char(c @ '0'..='5') if key.kind == KeyEventKind::Press && self.revealed => {
                if let Some(card) = &self.due.next {
                    let grade = c as u8 - b'0';
                    store
                        .grade_card(card.key, grade, Local::now().fixed_offset())
                        .map_err(anyhow::Error::msg)?;
                    self.revealed = false;
                    self.scroll = 0;
                    self.refresh(store)?;
                    return Ok(Some(format!("Grade {grade} saved")));
                }
            }
            _ => {}
        }
        Ok(None)
    }
}

impl App {
    pub(super) fn on_review_card_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc && self.brain.review.input.is_none() {
            self.open_menu();
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('v') {
            self.paste_brain(&self.clipboard_text());
            return;
        }
        if let Some(store) = &mut self.brain.store {
            self.msg = match self.brain.review.key(store, key) {
                Ok(message) => message.unwrap_or_default(),
                Err(error) => format!("Review cards · {error}"),
            };
        }
    }
}

pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let desk = &mut app.brain.review;
    let filter = desk.filter.as_ref().map_or("All cards", |(label, _)| label);
    let panel = Block::bordered()
        .title(format!(
            " Review cards · {filter} · {} due ",
            desk.due.count
        ))
        .border_style(Style::default().fg(theme.border));
    let inner = panel.inner(area);
    frame.render_widget(panel, area);
    if inner.is_empty() {
        return;
    }
    let [controls, entry, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(desk.input.is_some())),
        Constraint::Min(1),
    ])
    .areas(inner);
    let message = if desk.input.is_some() {
        "Enter a #tag or saved search title · empty for all"
    } else if desk.revealed {
        "0–5 Grade · ↑↓ Scroll · F Filter"
    } else if desk.due.next.is_some() {
        "Enter Reveal · F Filter"
    } else {
        "F Filter · R Refresh"
    };
    frame.render_widget(
        Paragraph::new(message).style(Style::default().fg(theme.dim)),
        controls,
    );
    if let Some(input) = &mut desk.input {
        draw_draft(frame, input, entry, Style::default().fg(theme.text), true);
    }
    let text = match &desk.due.next {
        Some(card) if desk.revealed => format!("{}\n\n{}", card.prompt, card.answer),
        Some(card) => card.prompt.clone(),
        None => "No cards due.".into(),
    };
    let width = usize::from(body.width.max(1));
    let content = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme.text));
    let max_scroll = content
        .line_count(width as u16)
        .saturating_sub(usize::from(body.height));
    desk.scroll = desk
        .scroll
        .min(max_scroll.min(usize::from(u16::MAX)) as u16);
    frame.render_widget(content.scroll((desk.scroll, 0)), body);
}
