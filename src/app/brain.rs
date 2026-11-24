//! The Today outline on the writing desk.

use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use grimoire_brain::bounded_store::{OutlineRow, PagedBrain};
use grimoire_brain::library::EditionKey;
use grimoire_brain::model::{NodeId, Place};
use grimoire_brain::review::looks_like_card;
use grimoire_brain::schema::SavedQuery;
use grimoire_core::editor::Editor;
use grimoire_core::sync::Fingerprint;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{App, Overlay};
use crate::theme::Theme;

mod commands;
pub(crate) mod library;
pub(crate) mod review;
use commands::Command;

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
    library: library::LibraryDesk,
    review: review::ReviewDesk,
    outline: Option<Outline>,
    other_outline: Option<Outline>,
    active_side: bool,
    pub(crate) side_visible: bool,
    pub(crate) side_area: Rect,
    pub(super) backup_observed: Option<(String, std::time::Instant)>,
    pub(super) backup_attempted: Option<std::time::Instant>,
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
    prompt: Option<Prompt>,
    details: Option<Vec<String>>,
    details_scroll: usize,
}

#[derive(Clone)]
struct Location {
    parent: Option<NodeId>,
    title: String,
    offset: u32,
    selected: usize,
    query: Option<SavedQuery>,
    create_tag: Option<NodeId>,
}

struct Prompt {
    command: Command,
    editor: Editor,
}
struct Draft {
    node: Option<NodeId>,
    editor: Editor,
    work_links: Vec<(String, NodeId)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrainReferenceTarget {
    node: Option<NodeId>,
    side: bool,
    position: (usize, usize),
    revision: Fingerprint,
}

impl Location {
    fn new(parent: Option<NodeId>, title: String) -> Self {
        Self {
            parent,
            title,
            offset: 0,
            selected: 0,
            query: None,
            create_tag: None,
        }
    }
}

impl BrainDesk {
    pub(super) fn library_path(&self) -> PathBuf {
        self.paths
            .as_ref()
            .map(|(library, _)| library.clone())
            .unwrap_or_else(|| crate::home().join("Documents/Brain"))
    }

    pub(super) fn backed_up_version(&self) -> Option<String> {
        self.store.as_ref().and_then(PagedBrain::version_id)
    }

    pub(super) fn next_copy_error(&self) -> Option<String> {
        self.store.as_ref().and_then(PagedBrain::readable_error)
    }

    pub(crate) fn select_side(&mut self, active: bool) {
        if self.active_side != active {
            std::mem::swap(&mut self.outline, &mut self.other_outline);
            self.active_side = active;
        }
    }

    pub(super) fn close_project_pane(&mut self) {
        self.select_side(false);
        self.other_outline = None;
        self.side_visible = false;
        self.side_area = Rect::default();
    }

    #[cfg(test)]
    pub(crate) fn set_paths(&mut self, library: PathBuf, cache: PathBuf) {
        self.paths = Some((library, cache));
    }

    fn open_store(&mut self) -> anyhow::Result<&mut PagedBrain> {
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
        Ok(self.store.as_mut().expect("Brain store was opened"))
    }

    fn refresh_review(&mut self) -> anyhow::Result<()> {
        self.open_store()?;
        self.review
            .refresh(self.store.as_mut().expect("Brain store was opened"))
    }

    fn refresh_library(&mut self) -> anyhow::Result<()> {
        self.open_store()?;
        self.library
            .refresh(self.store.as_mut().expect("Brain store was opened"))
    }

    fn open_today(&mut self, date: NaiveDate) -> anyhow::Result<()> {
        if self
            .outline
            .as_ref()
            .is_some_and(|outline| outline.date == date || outline.draft.is_some())
        {
            return Ok(());
        }
        let store = self.open_store()?;
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
            prompt: None,
            details: None,
            details_scroll: 0,
        });
        Ok(())
    }

    fn paste(&mut self, text: &str) {
        let Some(outline) = self.outline.as_mut() else {
            return;
        };
        let editor = if let Some(prompt) = &mut outline.prompt {
            &mut prompt.editor
        } else if let Some(draft) = &mut outline.draft {
            &mut draft.editor
        } else {
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
        editor.insert_str(&text);
    }

    pub(super) fn edit_history(&mut self, redo: bool) {
        if let Some(draft) = self
            .outline
            .as_mut()
            .and_then(|outline| outline.draft.as_mut())
        {
            if redo {
                draft.editor.redo();
            } else {
                draft.editor.undo();
            }
        }
    }

    fn save(&mut self) -> anyhow::Result<bool> {
        let (Some(store), Some(outline)) = (&mut self.store, &mut self.outline) else {
            return Ok(false);
        };
        let Some(draft) = &outline.draft else {
            return Ok(false);
        };
        let body = draft.editor.text();
        let links = draft
            .work_links
            .iter()
            .filter_map(|(destination, work)| body.contains(destination).then_some(*work))
            .collect();
        let affects_review = looks_like_card(&body)
            || looks_like_card(&outline.location.title)
            || draft.node.is_some_and(|node| {
                outline
                    .rows
                    .iter()
                    .any(|row| row.id == node && looks_like_card(&row.body))
            });
        if draft.node.is_none() && body.trim().is_empty() {
            outline.draft = None;
            return Ok(false);
        }
        let node = match draft.node {
            Some(node) => {
                store
                    .set_note_with_links(node, &body, links)
                    .map_err(anyhow::Error::msg)?;
                node
            }
            None => store
                .create_tagged_note_with_links(
                    Place {
                        parent: outline.location.parent,
                        after: outline.selected().map(|row| row.id),
                    },
                    &body,
                    outline.location.create_tag,
                    links,
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
        if affects_review {
            // A busy cache can retry the count on the next view change.
            let _ = self.review.refresh(store);
        }
        Ok(true)
    }

    fn browse(&mut self, code: KeyCode, shift: bool) -> anyhow::Result<()> {
        let (Some(store), Some(outline)) = (&mut self.store, &mut self.outline) else {
            return Ok(());
        };
        let selected = outline.selected().map(|row| row.id);
        let command = match code {
            KeyCode::Char('/') => Some(Command::Find),
            KeyCode::Char('@') => selected.map(Command::Link),
            KeyCode::Char('#') => selected.map(Command::Tag),
            KeyCode::Char('f') => selected.map(Command::Field),
            KeyCode::Char('g') => Some(Command::Schema),
            KeyCode::Char('s') => selected.map(Command::SaveSearch),
            _ => None,
        };
        if let Some(command) = command {
            outline.prompt = Some(Prompt {
                command,
                editor: Editor::from_text(""),
            });
            return Ok(());
        }
        if outline.details.is_some() && matches!(code, KeyCode::Up | KeyCode::Down) {
            if code == KeyCode::Up {
                outline.details_scroll = outline.details_scroll.saturating_sub(1);
            } else if let Some(details) = &outline.details {
                outline.details_scroll =
                    (outline.details_scroll + 1).min(details.len().saturating_sub(1));
            }
            return Ok(());
        }
        match code {
            KeyCode::Char('i') => {
                if outline.details.take().is_none()
                    && let Some(node) = selected
                {
                    outline.details = Some(commands::describe(store, node)?);
                    outline.details_scroll = 0;
                }
            }
            KeyCode::Char('b') if selected.is_some() => {
                outline.show_query(store, "Backlinks", SavedQuery::LinksTo(selected.unwrap()))?;
            }
            KeyCode::Char('o') if selected.is_some() => {
                outline.show_query(
                    store,
                    "Linked notes",
                    SavedQuery::LinksFrom(selected.unwrap()),
                )?;
            }
            KeyCode::Char('r') if selected.is_some() => {
                if let Some(query) = store
                    .details(selected.unwrap())
                    .map_err(anyhow::Error::msg)?
                    .query
                {
                    outline.show_query(store, "Saved search", query)?;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                outline.move_selection(store, code)?;
            }
            KeyCode::Up | KeyCode::Down if shift => {
                outline.move_selection(store, code)?;
            }
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
                if outline.location.query.is_some() && outline.location.create_tag.is_none() {
                    anyhow::bail!("Open a page to add a note");
                }
                outline.details = None;
                outline.draft = Some(Draft {
                    node: None,
                    editor: Editor::from_text(""),
                    work_links: Vec::new(),
                });
            }
            KeyCode::Enter => {
                if let Some(row) = outline.selected() {
                    let mut editor = Editor::from_text(&row.body);
                    editor.scene_end();
                    outline.draft = Some(Draft {
                        node: Some(row.id),
                        editor,
                        work_links: Vec::new(),
                    });
                    outline.details = None;
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

    fn prompt_key(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
        let (Some(store), Some(outline)) = (&mut self.store, &mut self.outline) else {
            return Ok(false);
        };
        let Some(prompt) = &mut outline.prompt else {
            return Ok(false);
        };
        if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            return Ok(true);
        }
        match key.code {
            KeyCode::Esc => outline.prompt = None,
            KeyCode::Enter => {
                let input = prompt.editor.text();
                let command = prompt.command;
                let selected = outline.selected().map(|row| row.id);
                if let Some(query) = command.apply(store, &input, selected)? {
                    outline.show_query(store, "Search results", query)?;
                } else if outline.details.is_some()
                    && let Some(node) = selected
                {
                    outline.details = Some(commands::describe(store, node)?);
                }
                outline.prompt = None;
            }
            KeyCode::Left => prompt.editor.left(),
            KeyCode::Right => prompt.editor.right(),
            KeyCode::Home => prompt.editor.scene_start(),
            KeyCode::End => prompt.editor.scene_end(),
            KeyCode::Backspace => prompt.editor.backspace(),
            KeyCode::Delete => prompt.editor.delete(),
            KeyCode::Char(c) if !c.is_control() => prompt.editor.insert(c),
            _ => {}
        }
        Ok(true)
    }

    pub(super) fn click(&mut self, x: u16, y: u16) {
        let Some(outline) = &mut self.outline else {
            return;
        };
        if outline.draft.is_none()
            && outline.details.is_none()
            && outline.prompt.is_none()
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
            .is_some_and(|outline| outline.draft.is_none() && outline.prompt.is_none())
        {
            self.browse(if down { KeyCode::Down } else { KeyCode::Up }, false)?;
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
        self.details = None;
        Ok(())
    }

    fn show_query(
        &mut self,
        store: &mut PagedBrain,
        title: &str,
        query: SavedQuery,
    ) -> anyhow::Result<()> {
        let mut next = Location::new(None, title.into());
        next.query = Some(query);
        let previous = self.location.clone();
        self.load(store, next)?;
        self.ancestors.push(previous);
        Ok(())
    }

    fn move_selection(&mut self, store: &mut PagedBrain, code: KeyCode) -> anyhow::Result<()> {
        let Some(node) = self.selected().map(|row| row.id) else {
            return Ok(());
        };
        if self.location.query.is_some() {
            anyhow::bail!("Open the note's page to change its place");
        }
        let parent = self.location.parent;
        let current = self.location.offset + self.location.selected as u32;
        let sibling = |store: &mut PagedBrain, offset| -> anyhow::Result<Option<NodeId>> {
            let page = if parent.is_some() {
                store.children(parent, offset, 1)
            } else {
                store.named_pages(offset, 1)
            };
            Ok(page.map_err(anyhow::Error::msg)?.first().map(|row| row.id))
        };
        let (moving, place, next) = match code {
            KeyCode::Tab if current > 0 => {
                let Some(previous) = sibling(store, current - 1)? else {
                    return Ok(());
                };
                (
                    node,
                    Place {
                        parent: Some(previous),
                        after: None,
                    },
                    None,
                )
            }
            KeyCode::BackTab => {
                let Some(parent) = parent else {
                    return Ok(());
                };
                let Some(above) = self.ancestors.last().cloned() else {
                    return Ok(());
                };
                if above.query.is_some() {
                    return Ok(());
                }
                (
                    node,
                    Place {
                        parent: above.parent,
                        after: Some(parent),
                    },
                    Some(above),
                )
            }
            KeyCode::Up if current > 0 => {
                let Some(previous) = sibling(store, current - 1)? else {
                    return Ok(());
                };
                (
                    previous,
                    Place {
                        parent,
                        after: Some(node),
                    },
                    None,
                )
            }
            KeyCode::Down => {
                let Some(following) = sibling(store, current + 1)? else {
                    return Ok(());
                };
                (
                    node,
                    Place {
                        parent,
                        after: Some(following),
                    },
                    None,
                )
            }
            _ => return Ok(()),
        };
        store.move_note(moving, place).map_err(anyhow::Error::msg)?;
        let changing_parent = next.is_some();
        let mut location = next.unwrap_or_else(|| self.location.clone());
        if code == KeyCode::Up && current < location.offset + 1 {
            location.offset = location.offset.saturating_sub(u32::from(PAGE_SIZE));
        } else if code == KeyCode::Down && self.location.selected + 1 == usize::from(PAGE_SIZE) {
            location.offset += u32::from(PAGE_SIZE);
        }
        self.load(store, location)?;
        if changing_parent {
            self.ancestors.pop();
        }
        if let Some(index) = self.rows.iter().position(|row| row.id == node) {
            self.location.selected = index;
        }
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
    let mut rows = if let Some(query) = &location.query {
        store.search(query, location.offset, PAGE_SIZE + 1)
    } else if location.parent.is_some() {
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
    pub fn brain_accepts_input(&self) -> bool {
        self.desk_view != DeskView::Write || self.brain.active_side
    }

    pub fn toggle_brain_beside(&mut self) {
        if self.desk_view == DeskView::Write && self.brain.side_visible {
            self.brain.select_side(false);
            self.brain.side_visible = false;
            return;
        }
        if let Err(error) = self.open_brain_beside() {
            self.msg = format!("Could not open project notes · {error}");
        }
    }

    fn open_brain_beside(&mut self) -> anyhow::Result<()> {
        self.brain.select_side(false);
        let date = Local::now().date_naive();
        self.brain.open_today(date)?;
        let store = self.brain.store.as_mut().expect("Open Brain store");
        let tag = if let Some(tag) = &self.project.meta.brain_tag {
            tag.parse::<NodeId>().map_err(anyhow::Error::msg)?
        } else {
            let title = format!("Project — {}", self.project.meta.title);
            let tag = store
                .create_note_with_text(Place::default(), &title)
                .map_err(anyhow::Error::msg)?;
            let key = tag.external_id();
            grimoire_core::submission::save_brain_tag(&self.project.root, &key)?;
            self.project.meta.brain_tag = Some(key);
            tag
        };
        if self.brain.other_outline.is_none() {
            let parent = if store.note(tag).map_err(anyhow::Error::msg)?.is_some() {
                tag
            } else {
                store.ensure_today(date).map_err(anyhow::Error::msg)?
            };
            let mut location = Location::new(Some(parent), "Project notes".into());
            location.query = Some(SavedQuery::Tagged(tag));
            location.create_tag = Some(tag);
            let (rows, has_more) = page(store, &location)?;
            self.brain.other_outline = Some(Outline {
                date,
                location,
                rows,
                has_more,
                ancestors: Vec::new(),
                draft: None,
                prompt: None,
                details: None,
                details_scroll: 0,
            });
        }
        self.desk_view = DeskView::Write;
        self.brain.side_visible = true;
        self.brain.select_side(true);
        self.focus_mode = false;
        self.msg.clear();
        Ok(())
    }

    pub fn switch_desk_view(&mut self, view: DeskView) {
        self.brain.select_side(false);
        if view == self.desk_view {
            return;
        }
        if view == DeskView::Brain
            && let Err(error) = self.brain.open_today(Local::now().date_naive())
        {
            self.msg = format!("Could not open Brain · {error}");
            return;
        }
        if view == DeskView::Library
            && let Err(error) = self.brain.refresh_library()
        {
            self.msg = format!("Could not open Library · {error}");
            return;
        }
        if view != DeskView::Write
            && let Err(error) = self.brain.refresh_review()
        {
            self.msg = format!("Could not open Review cards · {error}");
            if view == DeskView::Review {
                return;
            }
        }
        self.tree_drag = None;
        self.desk_view = view;
        self.msg.clear();
    }

    pub(crate) fn library_store_if_present(&mut self) -> anyhow::Result<Option<&mut PagedBrain>> {
        if self.brain.store.is_some() || self.brain.library_path().join("store").is_dir() {
            return self.brain.open_store().map(Some);
        }
        Ok(None)
    }

    pub(crate) fn brain_reference_target(&self) -> Option<BrainReferenceTarget> {
        let draft = self.brain.outline.as_ref()?.draft.as_ref()?;
        let editor = &draft.editor;
        if editor.has_selection() {
            return None;
        }
        if let Some(previous) = editor
            .cx
            .checked_sub(1)
            .and_then(|index| editor.lines.get(editor.cy)?.chars().nth(index))
            && (previous.is_alphanumeric() || matches!(previous, '_' | '@'))
        {
            return None;
        }
        Some(BrainReferenceTarget {
            node: draft.node,
            side: self.brain.active_side,
            position: (editor.cy, editor.cx),
            revision: Fingerprint::of(&editor.text()),
        })
    }

    pub(crate) fn write_brain_reference(
        &mut self,
        target: &BrainReferenceTarget,
        picked: Option<(EditionKey, String)>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.brain_reference_target().as_ref() == Some(target),
            "The note changed. Open the Library picker again."
        );
        let draft = self
            .brain
            .outline
            .as_mut()
            .and_then(|outline| outline.draft.as_mut())
            .ok_or_else(|| anyhow::anyhow!("The note draft is unavailable"))?;
        if let Some((edition, title)) = picked {
            let marker = format!(
                "brain:{}#{}",
                edition.work.external_id(),
                NodeId::Note(edition.edition).external_id()
            );
            let mut label = String::new();
            for c in title.chars().filter(|c| !c.is_control()).take(256) {
                if matches!(c, '[' | ']' | '\\') {
                    label.push('\\');
                }
                label.push(c);
            }
            draft.editor.insert_str(&format!("[{label}]({marker})"));
            draft
                .work_links
                .push((format!("]({marker})"), edition.work));
        } else {
            draft.editor.insert('@');
        }
        Ok(())
    }

    pub(super) fn tick_library(&mut self) {
        if let Some(store) = &mut self.brain.store
            && let Some(result) = self.brain.library.poll(store)
        {
            self.msg = result.unwrap_or_else(|error| format!("Library · {error}"));
        }
    }

    pub fn paste_brain(&mut self, text: &str) {
        match self.desk_view {
            DeskView::Library => self.brain.library.paste(text),
            DeskView::Review => self.brain.review.paste(text),
            _ => self.brain.paste(text),
        }
    }

    pub(super) fn undo_brain_input(&mut self, redo: bool) {
        match self.desk_view {
            DeskView::Library => self.brain.library.edit_history(redo),
            DeskView::Review => self.brain.review.edit_history(redo),
            _ => self.brain.edit_history(redo),
        }
    }

    pub(super) fn scroll_brain(&mut self, down: bool) {
        match self.desk_view {
            DeskView::Library => self.brain.library.scroll(down),
            DeskView::Review => self.brain.review.scroll(down),
            _ => {
                if let Err(error) = self.brain.scroll(down) {
                    self.msg = format!("Could not open Brain notes · {error}");
                }
            }
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

    pub(super) fn save_brain_panes(&mut self) -> bool {
        if !self.save_brain() {
            return false;
        }
        let side = self.brain.active_side;
        self.brain.select_side(!side);
        let saved = self.save_brain();
        self.brain.select_side(side);
        saved
    }

    pub fn on_brain_key(&mut self, key: KeyEvent) {
        if self.desk_view == DeskView::Library {
            self.on_library_key(key);
            return;
        }
        if self.desk_view == DeskView::Review {
            self.on_review_card_key(key);
            return;
        }
        if self
            .brain
            .outline
            .as_ref()
            .is_some_and(|outline| outline.prompt.is_some())
        {
            if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('v') {
                self.paste_brain(&self.clipboard_text());
            } else if let Err(error) = self.brain.prompt_key(key) {
                self.msg = error.to_string();
            } else {
                self.msg.clear();
            }
            return;
        }
        if key.code == KeyCode::Char('@')
            && key.modifiers.difference(KeyModifiers::SHIFT).is_empty()
            && let Some(target) = self.brain_reference_target()
        {
            self.open_brain_library_picker(target);
            return;
        }
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
                KeyCode::Char('z' | 'y') => self.brain.edit_history(key.code == KeyCode::Char('y')),
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
            if self
                .brain
                .outline
                .as_mut()
                .is_none_or(|outline| outline.details.take().is_none())
            {
                if self.brain.active_side {
                    self.brain.select_side(false);
                } else {
                    self.open_menu();
                }
            }
        } else if let Err(error) = self
            .brain
            .browse(key.code, key.modifiers.contains(KeyModifiers::SHIFT))
        {
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
        let label = if view == DeskView::Review {
            format!("Review {}", app.brain.review.all_due)
        } else {
            view.label().into()
        };
        let width = label.len() as u16 + 2;
        if x + width > area.right() {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let style = if view == app.desk_view {
            Style::default()
                .fg(theme.accent)
                .bg(theme.sel)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        f.render_widget(Line::styled(format!(" {label} "), style), rect);
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

pub(crate) fn draw(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme, side: bool) {
    app.brain.note_hits.clear();
    let focused = side == app.brain.active_side;
    let outline = if focused {
        &mut app.brain.outline
    } else {
        &mut app.brain.other_outline
    };
    let Some(outline) = outline else {
        return;
    };
    let [title, body, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(area);
    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.dim);
    let accent = Style::default().fg(theme.accent);
    let root = outline.ancestors.first().unwrap_or(&outline.location);
    let heading = if outline.location.query.is_some() {
        format!(" Brain · {}", outline.location.title)
    } else if root.parent.is_none() {
        " Brain · Pages".into()
    } else {
        format!(" Brain · Today · {}", outline.date.format("%Y-%m-%d"))
    };
    let path = if outline.ancestors.is_empty() || outline.location.query.is_some() {
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
    if let Some(prompt) = &mut outline.prompt {
        let [hint, input] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(body);
        f.render_widget(Line::styled(prompt.command.hint(), dim), hint);
        draw_draft(
            f,
            &mut prompt.editor,
            input,
            text,
            focused && app.overlay == Overlay::None,
        );
        f.render_widget(Line::styled(" Enter Apply   Esc Cancel", dim), keys);
    } else if let Some(draft) = &mut outline.draft {
        draw_draft(
            f,
            &mut draft.editor,
            body,
            text,
            focused && app.overlay == Overlay::None,
        );
        f.render_widget(Line::styled(" Enter Save   Esc Cancel", dim), keys);
    } else if let Some(lines) = &outline.details {
        f.render_widget(
            Paragraph::new(
                lines
                    .iter()
                    .skip(outline.details_scroll)
                    .take(usize::from(body.height))
                    .map(|line| Line::styled(line.as_str(), text))
                    .collect::<Vec<_>>(),
            ),
            body,
        );
        f.render_widget(
            Line::styled(
                " ↑↓ Scroll   # Tag   F Field   G Schema   O Links   B Backlinks   Esc Back",
                dim,
            ),
            keys,
        );
    } else {
        let inner = Block::new()
            .padding(ratatui::widgets::Padding::horizontal(1))
            .inner(body);
        if outline.rows.is_empty() {
            let empty = if outline.location.query.is_some() {
                "No matching notes."
            } else if outline.location.parent.is_none() {
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
            Paragraph::new(vec![
                Line::styled(
                    format!(" {create}   Enter Edit   ←↑↓→ Navigate   P Pages   T Today"),
                    dim,
                ),
                Line::styled(
                    " / Search   S Save search   R Run   I Details   # Tag   @ Link",
                    dim,
                ),
            ]),
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
