//! The Today outline on the writing desk.

use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use grimoire_brain::bounded_store::{OutlineRow, PagedBrain};
use grimoire_brain::model::{NodeId, Place};
use grimoire_core::editor::Editor;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{App, Overlay};
use crate::theme::Theme;

const PAGE_SIZE: u16 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeskView {
    #[default]
    Write,
    Library,
    Brain,
    Review,
}

impl DeskView {
    pub const ALL: [Self; 4] = [Self::Write, Self::Library, Self::Brain, Self::Review];

    fn label(self) -> &'static str {
        match self {
            Self::Write => "Write",
            Self::Library => "Library",
            Self::Brain => "Brain",
            Self::Review => "Review",
        }
    }

    pub fn from_shortcut(c: char) -> Option<Self> {
        let index = c.to_digit(10)?.checked_sub(1)?;
        Self::ALL.get(index as usize).copied()
    }
}

#[derive(Default)]
pub struct BrainDesk {
    store: Option<PagedBrain>,
    outline: Option<Outline>,
    paths: Option<(PathBuf, PathBuf)>,
    note_hits: Vec<(Rect, usize)>,
    pub(crate) view_hits: Vec<(Rect, DeskView)>,
}

struct Outline {
    date: NaiveDate,
    location: Location,
    ancestors: Vec<Location>,
    rows: Vec<OutlineRow>,
    has_more: bool,
    draft: Option<Draft>,
}

#[derive(Clone)]
struct Location {
    parent: Option<NodeId>,
    title: String,
    offset: u32,
    selected: usize,
}

struct Draft {
    node: Option<NodeId>,
    editor: Editor,
}

impl Location {
    fn new(parent: Option<NodeId>, title: String) -> Self {
        Self {
            parent,
            title,
            offset: 0,
            selected: 0,
        }
    }
}

impl BrainDesk {
    #[cfg(test)]
    pub(crate) fn set_paths(&mut self, library: PathBuf, cache: PathBuf) {
        self.paths = Some((library, cache));
    }

    fn open_today(&mut self, date: NaiveDate) -> anyhow::Result<()> {
        if self
            .outline
            .as_ref()
            .is_some_and(|outline| outline.date == date || outline.draft.is_some())
        {
            return Ok(());
        }
        if self.store.is_none() {
            let (library, cache) = self.paths.clone().unwrap_or_else(|| {
                let home = crate::home();
                let cache = std::env::var_os("XDG_CACHE_HOME")
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .unwrap_or_else(|| home.join(".cache"));
                (home.join("Documents/Brain"), cache.join("grimoire/brain"))
            });
            self.store =
                Some(PagedBrain::open_library(&library, &cache).map_err(anyhow::Error::msg)?);
        }
        let store = self.store.as_mut().expect("Brain store was opened");
        let today = store.ensure_today(date).map_err(anyhow::Error::msg)?;
        let location = Location::new(Some(today), "Today".into());
        let (rows, has_more) = page(store, &location)?;
        self.outline = Some(Outline {
            date,
            location,
            ancestors: Vec::new(),
            rows,
            has_more,
            draft: None,
        });
        Ok(())
    }

    fn paste(&mut self, text: &str) {
        let Some(draft) = self
            .outline
            .as_mut()
            .and_then(|outline| outline.draft.as_mut())
        else {
            return;
        };
        let text: String = text
            .replace("\r\n", "\n")
            .chars()
            .filter_map(|c| match c {
                '\n' | '\r' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        draft.editor.insert_str(&text);
    }

    fn save(&mut self) -> anyhow::Result<bool> {
        let (Some(store), Some(outline)) = (&mut self.store, &mut self.outline) else {
            return Ok(false);
        };
        let Some(draft) = &outline.draft else {
            return Ok(false);
        };
        let body = draft.editor.text();
        if draft.node.is_none() && body.trim().is_empty() {
            outline.draft = None;
            return Ok(false);
        }
        let node = match draft.node {
            Some(node) => {
                store.set_note(node, &body).map_err(anyhow::Error::msg)?;
                node
            }
            None => store
                .create_note_with_text(
                    Place {
                        parent: outline.location.parent,
                        after: outline.selected().map(|row| row.id),
                    },
                    &body,
                )
                .map_err(anyhow::Error::msg)?,
        };
        outline.draft = None;
        let location = outline.location.clone();
        outline.load(store, location)?;
        if let Some(index) = outline.rows.iter().position(|row| row.id == node) {
            outline.location.selected = index;
        } else if outline.has_more {
            outline.step(store, true)?;
        }
        Ok(true)
    }

    fn browse(&mut self, code: KeyCode) -> anyhow::Result<()> {
        let (Some(store), Some(outline)) = (&mut self.store, &mut self.outline) else {
            return Ok(());
        };
        match code {
            KeyCode::Char('p') | KeyCode::Char('t') => {
                let (date, location) = if code == KeyCode::Char('p') {
                    (outline.date, Location::new(None, "Pages".into()))
                } else {
                    let date = Local::now().date_naive();
                    let today = store.ensure_today(date).map_err(anyhow::Error::msg)?;
                    (date, Location::new(Some(today), "Today".into()))
                };
                outline.load(store, location)?;
                outline.date = date;
                outline.ancestors.clear();
            }
            KeyCode::Char('n') => {
                outline.draft = Some(Draft {
                    node: None,
                    editor: Editor::from_text(""),
                });
            }
            KeyCode::Enter => {
                if let Some(row) = outline.selected() {
                    let mut editor = Editor::from_text(&row.body);
                    editor.scene_end();
                    outline.draft = Some(Draft {
                        node: Some(row.id),
                        editor,
                    });
                }
            }
            KeyCode::Up => outline.step(store, false)?,
            KeyCode::Down => outline.step(store, true)?,
            KeyCode::Right => {
                if let Some(row) = outline.selected() {
                    let previous = outline.location.clone();
                    let next = Location::new(Some(row.id), note_title(&row.body).to_owned());
                    outline.load(store, next)?;
                    outline.ancestors.push(previous);
                }
            }
            KeyCode::Left => {
                if let Some(previous) = outline.ancestors.last().cloned() {
                    outline.load(store, previous)?;
                    outline.ancestors.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn click(&mut self, x: u16, y: u16) {
        let Some(outline) = &mut self.outline else {
            return;
        };
        if outline.draft.is_none()
            && let Some((_, index)) = self
                .note_hits
                .iter()
                .find(|(rect, _)| super::hit(*rect, x, y))
        {
            outline.location.selected = *index;
        }
    }

    pub(super) fn scroll(&mut self, down: bool) -> anyhow::Result<()> {
        if self
            .outline
            .as_ref()
            .is_some_and(|outline| outline.draft.is_none())
        {
            self.browse(if down { KeyCode::Down } else { KeyCode::Up })?;
        }
        Ok(())
    }
}

impl Outline {
    fn selected(&self) -> Option<&OutlineRow> {
        self.rows.get(self.location.selected)
    }

    fn load(&mut self, store: &mut PagedBrain, mut location: Location) -> anyhow::Result<()> {
        let (rows, has_more) = page(store, &location)?;
        location.selected = location.selected.min(rows.len().saturating_sub(1));
        self.location = location;
        self.rows = rows;
        self.has_more = has_more;
        Ok(())
    }

    fn step(&mut self, store: &mut PagedBrain, down: bool) -> anyhow::Result<()> {
        let mut next = self.location.clone();
        if down {
            if next.selected + 1 < self.rows.len() {
                self.location.selected += 1;
            } else if self.has_more {
                next.offset = next.offset.saturating_add(u32::from(PAGE_SIZE));
                next.selected = 0;
                self.load(store, next)?;
            }
        } else if next.selected > 0 {
            self.location.selected -= 1;
        } else if next.offset > 0 {
            next.offset = next.offset.saturating_sub(u32::from(PAGE_SIZE));
            next.selected = usize::from(PAGE_SIZE - 1);
            self.load(store, next)?;
        }
        Ok(())
    }
}

fn page(store: &mut PagedBrain, location: &Location) -> anyhow::Result<(Vec<OutlineRow>, bool)> {
    let mut rows = if location.parent.is_some() {
        store.children(location.parent, location.offset, PAGE_SIZE + 1)
    } else {
        store.named_pages(location.offset, PAGE_SIZE + 1)
    }
    .map_err(anyhow::Error::msg)?;
    let has_more = rows.len() > usize::from(PAGE_SIZE);
    rows.truncate(usize::from(PAGE_SIZE));
    Ok((rows, has_more))
}

fn note_title(body: &str) -> &str {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled note")
}

impl App {
    pub fn switch_desk_view(&mut self, view: DeskView) {
        match view {
            DeskView::Library => {
                self.msg = "Library is planned for RC26".into();
                return;
            }
            DeskView::Review => {
                self.msg = "Review is planned for RC27".into();
                return;
            }
            DeskView::Write | DeskView::Brain => {}
        }
        if view == self.desk_view {
            return;
        }
        if view == DeskView::Brain
            && let Err(error) = self.brain.open_today(Local::now().date_naive())
        {
            self.msg = format!("Could not open Brain · {error}");
            return;
        }
        self.tree_drag = None;
        self.desk_view = view;
        self.msg.clear();
    }

    pub fn paste_brain(&mut self, text: &str) {
        self.brain.paste(text);
    }

    pub(super) fn scroll_brain(&mut self, down: bool) {
        if let Err(error) = self.brain.scroll(down) {
            self.msg = format!("Could not open Brain notes · {error}");
        }
    }

    pub fn save_brain(&mut self) -> bool {
        match self.brain.save() {
            Ok(saved) => {
                if saved {
                    self.msg = "Brain note saved".into();
                }
                true
            }
            Err(error) => {
                self.msg = format!("Could not save Brain note · {error}");
                false
            }
        }
    }

    pub fn on_brain_key(&mut self, key: KeyEvent) {
        let draft = self
            .brain
            .outline
            .as_mut()
            .and_then(|outline| outline.draft.as_mut());
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('s') => {
                    self.save_brain();
                }
                KeyCode::Char('v') if draft.is_some() => self.paste_brain(&self.clipboard_text()),
                KeyCode::Char('z') => {
                    if let Some(draft) = draft {
                        draft.editor.undo();
                    }
                }
                KeyCode::Char('y') => {
                    if let Some(draft) = draft {
                        draft.editor.redo();
                    }
                }
                _ => {}
            }
            return;
        }
        if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            return;
        }
        if let Some(draft) = draft {
            let editor = &mut draft.editor;
            match key.code {
                KeyCode::Enter => {
                    self.save_brain();
                }
                KeyCode::Esc => {
                    self.brain
                        .outline
                        .as_mut()
                        .expect("Open Brain outline")
                        .draft = None;
                    self.msg.clear();
                }
                KeyCode::Char(c) if !c.is_control() => editor.insert(c),
                KeyCode::Backspace => editor.backspace(),
                KeyCode::Delete => editor.delete(),
                KeyCode::Left => editor.left(),
                KeyCode::Right => editor.right(),
                KeyCode::Home => editor.place(editor.cy, 0),
                KeyCode::End => editor.place(editor.cy, editor.lines[editor.cy].chars().count()),
                KeyCode::Up | KeyCode::Down => {
                    let rows = editor.layout(usize::from(self.screen.0.saturating_sub(2)).max(1));
                    if key.code == KeyCode::Up {
                        editor.up(&rows);
                    } else {
                        editor.down(&rows);
                    }
                }
                _ => {}
            }
        } else if key.code == KeyCode::Esc {
            self.open_menu();
        } else if let Err(error) = self.brain.browse(key.code) {
            self.msg = format!("Could not open Brain notes · {error}");
        }
    }
}

pub(crate) fn draw_view_bar(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    use ratatui::style::Modifier;
    app.brain.view_hits.clear();
    if area.is_empty() {
        return;
    }
    let mut x = area.x;
    for view in DeskView::ALL {
        let width = view.label().len() as u16 + 2;
        if x + width > area.right() {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let style = if view == app.desk_view {
            Style::default()
                .fg(theme.accent)
                .bg(theme.sel)
                .add_modifier(Modifier::BOLD)
        } else if matches!(view, DeskView::Write | DeskView::Brain) {
            Style::default().fg(theme.text)
        } else {
            Style::default().fg(theme.dim)
        };
        f.render_widget(Line::styled(format!(" {} ", view.label()), style), rect);
        app.brain.view_hits.push((rect, view));
        x += width;
        if x < area.right() && view != DeskView::Review {
            f.render_widget(
                Line::styled("│", Style::default().fg(theme.border)),
                Rect::new(x, area.y, 1, 1),
            );
            x += 1;
        }
    }
}

pub(crate) fn draw(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    app.brain.note_hits.clear();
    let Some(outline) = &mut app.brain.outline else {
        return;
    };
    let [title, body, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.dim);
    let accent = Style::default().fg(theme.accent);
    let root = outline.ancestors.first().unwrap_or(&outline.location);
    let heading = if root.parent.is_none() {
        " Brain · Pages".into()
    } else {
        format!(" Brain · Today · {}", outline.date.format("%Y-%m-%d"))
    };
    let path = if outline.ancestors.is_empty() {
        String::new()
    } else {
        format!("  ›  {}", outline.location.title)
    };
    f.render_widget(
        Line::from(vec![
            Span::styled(heading, accent),
            Span::styled(path, text),
        ]),
        title,
    );
    if let Some(draft) = &mut outline.draft {
        draw_draft(
            f,
            &mut draft.editor,
            body,
            text,
            app.overlay == Overlay::None,
        );
        f.render_widget(Line::styled(" Enter Save   Esc Cancel", dim), keys);
    } else {
        let inner = Block::new()
            .padding(ratatui::widgets::Padding::horizontal(1))
            .inner(body);
        if outline.rows.is_empty() {
            let empty = if outline.location.parent.is_none() {
                "No pages yet."
            } else {
                "No notes here yet."
            };
            f.render_widget(Line::styled(empty, dim), inner);
        } else {
            let height = usize::from(inner.height);
            let first = outline
                .location
                .selected
                .saturating_sub(height.saturating_sub(1));
            for (index, row) in outline.rows.iter().enumerate().skip(first).take(height) {
                let y = inner.y + (index - first) as u16;
                let line = Rect::new(inner.x, y, inner.width, 1);
                app.brain.note_hits.push((line, index));
                let (marker, style) = if index == outline.location.selected {
                    ("› ", accent.bg(theme.sel))
                } else {
                    ("  ", text)
                };
                let bullet = if row.has_children { "▸ " } else { "• " };
                f.render_widget(
                    Line::styled(format!("{marker}{bullet}{}", note_title(&row.body)), style),
                    line,
                );
            }
        }
        let create = if outline.location.parent.is_none() {
            "N New page"
        } else {
            "N New note"
        };
        f.render_widget(
            Line::styled(
                format!(" {create}   Enter Edit   ←↑↓→ Navigate   P Pages   T Today"),
                dim,
            ),
            keys,
        );
    }
}

fn draw_draft(f: &mut Frame, editor: &mut Editor, area: Rect, style: Style, focused: bool) {
    let area = Block::new()
        .padding(ratatui::widgets::Padding::horizontal(1))
        .inner(area);
    if area.is_empty() {
        return;
    }
    let rows = editor.layout(usize::from(area.width));
    let (row, col) = editor.cursor_vis(&rows);
    let height = usize::from(area.height);
    if row < editor.scroll {
        editor.scroll = row;
    } else if row >= editor.scroll + height {
        editor.scroll = row + 1 - height;
    }
    let lines: Vec<_> = rows
        .iter()
        .skip(editor.scroll)
        .take(height)
        .map(|row| {
            let text: String = editor.lines[row.line]
                .chars()
                .skip(row.start)
                .take(row.end - row.start)
                .collect();
            Line::styled(text, style)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
    if focused {
        f.set_cursor_position((
            area.x + (col.min(usize::from(area.width - 1))) as u16,
            area.y + (row - editor.scroll) as u16,
        ));
    }
}
