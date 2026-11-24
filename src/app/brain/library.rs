use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use chrono::Utc;
use grimoire_brain::bounded_store::PagedBrain;
use grimoire_brain::library::{
    Edition, EditionKey, Filter, SOURCE_TAGS, Sort, Work, bibliography_for_work,
};
use grimoire_brain::model::{Builtin, NodeId, Place};
use grimoire_core::editor::Editor;
use grimoire_core::sources;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::{App, Theme, commands, draw_draft};

mod lookup;
use lookup::{Kind as LookupKind, Lookup};

const PAGE: u16 = 32;
const SORTS: [(Sort, &str); 6] = [
    (Sort::Title, "Title"),
    (Sort::Author, "Author"),
    (Sort::Year, "Year"),
    (Sort::Type, "Type"),
    (Sort::Status, "Status"),
    (Sort::Added, "Recently added"),
];

#[derive(Clone, Copy)]
enum Input {
    Filter,
    New,
    AddEdition(NodeId),
    Edit(EditionKey),
    Attach(EditionKey),
    Import,
    Lookup(LookupKind),
}

impl Input {
    fn hint(self) -> &'static str {
        match self {
            Self::Filter => "Filter by title, author, status, DOI, ISBN, or #tag",
            Self::New => "Work title",
            Self::AddEdition(_) => "Edition label",
            Self::Edit(_) => "Field = value · title, author, year, publisher, ISBN, DOI, edition",
            Self::Attach(_) => "Path to a PDF, EPUB, or audio file",
            Self::Import => "Path to a .bib or CSL-JSON file",
            Self::Lookup(LookupKind::Doi) => "Crossref DOI · Enter to look up",
            Self::Lookup(LookupKind::Isbn) => "Open Library ISBN · Enter to look up",
        }
    }
}

#[derive(Default)]
pub(super) struct LibraryDesk {
    filter: Filter,
    filter_input: String,
    sort: usize,
    kind: usize,
    works: Vec<Work>,
    offset: u32,
    more: bool,
    selected: usize,
    open: Option<(NodeId, String)>,
    editions: Vec<(EditionKey, Edition)>,
    edition: usize,
    prompt: Option<(Input, Editor)>,
    import: Option<(VecDeque<sources::Record>, Vec<String>)>,
    attach: Option<Receiver<Result<grimoire_brain::library::Media, String>>>,
    attaching: Option<EditionKey>,
    lookup: Option<(Lookup, Receiver<Result<sources::Record, String>>)>,
    recent_lookups: VecDeque<(Lookup, sources::Record)>,
    last_lookup: Option<Instant>,
}

impl LibraryDesk {
    pub(super) fn has_input(&self) -> bool {
        self.prompt.is_some()
    }

    pub(super) fn refresh(&mut self, store: &mut PagedBrain) -> anyhow::Result<()> {
        let mut works = store
            .library_works(&self.filter, self.offset, PAGE + 1)
            .map_err(anyhow::Error::msg)?;
        self.more = works.len() > usize::from(PAGE);
        works.truncate(usize::from(PAGE));
        self.selected = self.selected.min(works.len().saturating_sub(1));
        self.works = works;
        if let Some((work, _)) = self.open {
            self.editions = store.work_editions(work).map_err(anyhow::Error::msg)?;
            self.edition = self.edition.min(self.editions.len().saturating_sub(1));
        }
        Ok(())
    }

    pub(super) fn paste(&mut self, text: &str) {
        if let Some((_, input)) = &mut self.prompt {
            let text: String = text
                .chars()
                .filter(|c| !c.is_control())
                .take(4096)
                .collect();
            input.insert_str(&text);
        }
    }

    pub(super) fn edit_history(&mut self, redo: bool) {
        if let Some((_, editor)) = &mut self.prompt {
            if redo {
                editor.redo();
            } else {
                editor.undo();
            }
        }
    }

    fn move_selection(&mut self, down: bool) {
        let (position, len) = if self.open.is_some() {
            (&mut self.edition, self.editions.len())
        } else {
            (&mut self.selected, self.works.len())
        };
        *position = if down {
            position.saturating_add(1).min(len.saturating_sub(1))
        } else {
            position.saturating_sub(1)
        };
    }

    pub(super) fn scroll(&mut self, down: bool) {
        self.move_selection(down);
    }

    fn page(&mut self, store: &mut PagedBrain, next: bool) -> anyhow::Result<()> {
        if next && self.more {
            self.offset = self.offset.saturating_add(u32::from(PAGE));
        } else if !next && self.offset > 0 {
            self.offset = self.offset.saturating_sub(u32::from(PAGE));
        } else {
            return Ok(());
        }
        self.selected = 0;
        self.refresh(store)
    }

    fn prompt(&mut self, input: Input, initial: &str) {
        self.prompt = Some((input, Editor::from_text(initial)));
    }

    fn save_prompt(&mut self, store: &mut PagedBrain) -> anyhow::Result<Option<String>> {
        let Some((input, editor)) = &self.prompt else {
            return Ok(None);
        };
        let text = editor.text();
        let value = text.trim();
        let mut notice = None;
        match *input {
            Input::Filter => {
                let tag = value
                    .strip_prefix('#')
                    .map(|tag| commands::find_tag(store, tag))
                    .transpose()?;
                self.filter.tag = tag;
                self.filter.text = if tag.is_some() {
                    String::new()
                } else {
                    value.into()
                };
                self.filter_input = value.into();
                self.offset = 0;
                self.selected = 0;
            }
            Input::New => {
                anyhow::ensure!(!value.is_empty(), "Enter a work title");
                let kind = self.filter.source_type.unwrap_or(Builtin::Book);
                let record = bibliography_for_work(kind, value, &[]);
                let key = store
                    .create_work(
                        Place::default(),
                        Edition::new(record, Utc::now().timestamp()),
                    )
                    .map_err(anyhow::Error::msg)?;
                self.open = Some((key.work, value.to_owned()));
                self.edition = 0;
                notice = Some("Work saved".into());
            }
            Input::AddEdition(work) => {
                anyhow::ensure!(!value.is_empty(), "Enter an edition label");
                let mut record = if let Some((_, edition)) = self.editions.get(self.edition) {
                    edition.bibliography.clone()
                } else {
                    let title = self.open.as_ref().map_or("", |(_, title)| title);
                    let kind = self
                        .works
                        .iter()
                        .find(|row| row.id == work)
                        .map_or(Builtin::Book, |row| row.source_type);
                    let details = store.details(work).map_err(anyhow::Error::msg)?;
                    bibliography_for_work(kind, title, &details.fields)
                };
                record.insert("edition".into(), serde_json::Value::String(value.into()));
                store
                    .add_edition(work, Edition::new(record, Utc::now().timestamp()))
                    .map_err(anyhow::Error::msg)?;
                self.edition = self.editions.len();
                notice = Some("Edition saved".into());
            }
            Input::Edit(key) => {
                let mut edition = store
                    .edition(key)
                    .map_err(anyhow::Error::msg)?
                    .ok_or_else(|| anyhow::anyhow!("This edition is no longer available"))?;
                update_metadata(&mut edition, value)?;
                store
                    .save_edition(key, edition)
                    .map_err(anyhow::Error::msg)?;
                notice = Some("Edition details saved".into());
            }
            Input::Attach(key) => {
                anyhow::ensure!(self.attach.is_none(), "A media file is already being added");
                let path = Path::new(value);
                anyhow::ensure!(path.is_absolute(), "Enter the full path");
                let (result, receiver) = mpsc::sync_channel(1);
                let source = path.to_path_buf();
                let library = store
                    .library_root()
                    .map_err(anyhow::Error::msg)?
                    .to_path_buf();
                std::thread::Builder::new()
                    .name("Library media copy".into())
                    .spawn(move || {
                        let done = grimoire_brain::library::import_media(&library, &source)
                            .map_err(|error| error.to_string());
                        let _ = result.send(done);
                    })?;
                self.attach = Some(receiver);
                self.attaching = Some(key);
                notice = Some("Adding media in the background".into());
            }
            Input::Import => {
                let mut preview =
                    sources::import_preview(Path::new(value), &sources::Sources::default())?;
                let (records, notes) = (&mut preview.records, &mut preview.notes);
                records.retain(|record| {
                    if Edition::new(record.clone(), 0).is_valid() {
                        return true;
                    }
                    let label = record
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Source");
                    notes.push(format!("{label} needs a title and valid details."));
                    false
                });
                anyhow::ensure!(!preview.records.is_empty(), "No sources to add");
                self.import = Some((preview.records.into(), preview.notes));
            }
            Input::Lookup(kind) => {
                let query = Lookup::new(kind, value)?;
                if let Some((_, record)) = self.recent_lookups.iter().find(|(key, _)| key == &query)
                {
                    self.import = Some((vec![record.clone()].into(), Vec::new()));
                } else {
                    anyhow::ensure!(self.lookup.is_none(), "A catalog lookup is already running");
                    anyhow::ensure!(
                        self.last_lookup
                            .is_none_or(|last| last.elapsed() >= Duration::from_secs(1)),
                        "Wait a moment before another lookup"
                    );
                    let (result, receiver) = mpsc::sync_channel(1);
                    let request = query.clone();
                    std::thread::Builder::new()
                        .name("Library catalog lookup".into())
                        .spawn(move || {
                            let _ = result.send(request.fetch().map_err(|error| error.to_string()));
                        })?;
                    self.lookup = Some((query, receiver));
                    self.last_lookup = Some(Instant::now());
                    notice = Some("Looking up the edition".into());
                }
            }
        }
        self.prompt = None;
        self.refresh(store)?;
        Ok(notice)
    }

    fn accept_import(&mut self, store: &mut PagedBrain) -> anyhow::Result<Option<String>> {
        let Some((records, _)) = &mut self.import else {
            return Ok(None);
        };
        let mut added = 0;
        while let Some(record) = records.front() {
            let source = Edition::new(record.clone(), Utc::now().timestamp());
            let fingerprint = sources::library_fingerprint(record)?;
            let same = store
                .find_editions(source.title(), 32)
                .map_err(anyhow::Error::msg)?
                .into_iter()
                .any(|(_, existing)| {
                    sources::library_fingerprint(&existing.bibliography)
                        .is_ok_and(|found| found == fingerprint)
                });
            if !same {
                store
                    .create_work(Place::default(), source)
                    .map_err(anyhow::Error::msg)?;
                added += 1;
            }
            records.pop_front();
        }
        self.import = None;
        self.refresh(store)?;
        Ok(Some(format!("{added} works added")))
    }

    fn key(&mut self, store: &mut PagedBrain, key: KeyEvent) -> anyhow::Result<Option<String>> {
        if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            return Ok(None);
        }
        if let Some((_, editor)) = &mut self.prompt {
            match key.code {
                KeyCode::Enter => return self.save_prompt(store),
                KeyCode::Esc => self.prompt = None,
                KeyCode::Char(c) if !c.is_control() => editor.insert(c),
                KeyCode::Backspace => editor.backspace(),
                KeyCode::Delete => editor.delete(),
                KeyCode::Left => editor.left(),
                KeyCode::Right => editor.right(),
                KeyCode::Home => editor.scene_start(),
                KeyCode::End => editor.scene_end(),
                _ => {}
            }
            return Ok(None);
        }
        if self.import.is_some() {
            match key.code {
                KeyCode::Esc => self.import = None,
                KeyCode::Enter => return self.accept_import(store),
                _ => {}
            }
            return Ok(None);
        }
        match key.code {
            KeyCode::Up => self.move_selection(false),
            KeyCode::Down => self.move_selection(true),
            KeyCode::Home => {
                if self.open.is_some() {
                    self.edition = 0;
                } else {
                    self.selected = 0;
                }
            }
            KeyCode::End => {
                if self.open.is_some() {
                    self.edition = self.editions.len().saturating_sub(1);
                } else {
                    self.selected = self.works.len().saturating_sub(1);
                }
            }
            KeyCode::Left | KeyCode::Esc if self.open.is_some() => {
                self.open = None;
                self.editions.clear();
            }
            KeyCode::Enter | KeyCode::Right if self.open.is_none() => {
                if let Some(row) = self.works.get(self.selected) {
                    self.open = Some((row.id, row.title.clone()));
                    self.edition = 0;
                    self.refresh(store)?;
                }
            }
            KeyCode::Char('b') if self.open.is_some() => {
                self.prompt(Input::AddEdition(self.open.as_ref().unwrap().0), "")
            }
            KeyCode::Char('m' | 'e') if self.open.is_some() => {
                if let Some((key_id, _)) = self.editions.get(self.edition) {
                    self.prompt(
                        if key.code == KeyCode::Char('m') {
                            Input::Attach(*key_id)
                        } else {
                            Input::Edit(*key_id)
                        },
                        "",
                    );
                }
            }
            KeyCode::Char('n') => self.prompt(Input::New, ""),
            KeyCode::Char('i') => self.prompt(Input::Import, ""),
            KeyCode::Char('d') => self.prompt(Input::Lookup(LookupKind::Doi), ""),
            KeyCode::Char('l') => self.prompt(Input::Lookup(LookupKind::Isbn), ""),
            KeyCode::Char('f' | '/') if self.open.is_none() => {
                let text = self.filter_input.clone();
                self.prompt(Input::Filter, &text);
            }
            KeyCode::Char('s') if self.open.is_none() => {
                self.sort = (self.sort + 1) % SORTS.len();
                self.filter.sort = SORTS[self.sort].0;
                self.offset = 0;
                self.selected = 0;
                self.refresh(store)?;
            }
            KeyCode::Char('t') if self.open.is_none() => {
                self.kind = (self.kind + 1) % (SOURCE_TAGS.len() + 1);
                self.filter.source_type = self.kind.checked_sub(1).map(|i| SOURCE_TAGS[i]);
                self.offset = 0;
                self.selected = 0;
                self.refresh(store)?;
            }
            KeyCode::PageDown | KeyCode::PageUp if self.open.is_none() => {
                self.page(store, key.code == KeyCode::PageDown)?;
            }
            _ => {}
        }
        Ok(None)
    }

    fn can_open_menu(&self) -> bool {
        self.prompt.is_none() && self.import.is_none() && self.open.is_none()
    }

    pub(super) fn poll(
        &mut self,
        store: &mut PagedBrain,
        show_preview: bool,
    ) -> Option<anyhow::Result<String>> {
        if let Some((_, receiver)) = &self.lookup {
            match receiver.try_recv() {
                Ok(result) => {
                    let (query, _) = self.lookup.take()?;
                    return Some(result.map_err(anyhow::Error::msg).map(|record| {
                        if self.recent_lookups.len() >= 16 {
                            self.recent_lookups.pop_front();
                        }
                        self.recent_lookups.push_back((query, record.clone()));
                        if show_preview && self.can_open_menu() {
                            self.import = Some((vec![record].into(), Vec::new()));
                            "Catalog match ready in Library".into()
                        } else {
                            "Catalog match cached. Enter its identifier again to preview it.".into()
                        }
                    }));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.lookup = None;
                    return Some(Err(anyhow::anyhow!("The catalog lookup stopped")));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let result = match self.attach.as_ref()?.try_recv() {
            Ok(result) => result.map_err(anyhow::Error::msg),
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!("The media copy stopped")),
        };
        self.attach = None;
        let key = self.attaching.take()?;
        Some(result.and_then(|media| {
            store.record_media(key, media).map_err(anyhow::Error::msg)?;
            self.refresh(store)?;
            Ok("Media saved".into())
        }))
    }
}

fn update_metadata(edition: &mut Edition, input: &str) -> anyhow::Result<()> {
    use serde_json::{Value, json};
    let (field, value) = input
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("Enter a field name, =, and its value"))?;
    let value = value.trim();
    let record = &mut edition.bibliography;
    let field = field.trim().to_ascii_lowercase();
    let (key, value) = match field.as_str() {
        "title" => {
            anyhow::ensure!(!value.is_empty(), "Enter a title");
            ("title", json!(value))
        }
        "author" => {
            let names: Vec<_> = value
                .split(';')
                .filter_map(|name| {
                    let name = name.trim();
                    if name.is_empty() {
                        return None;
                    }
                    Some(if let Some((family, given)) = name.split_once(',') {
                        json!({"family": family.trim(), "given": given.trim()})
                    } else {
                        json!({"literal": name})
                    })
                })
                .collect();
            (
                "author",
                if names.is_empty() {
                    Value::Null
                } else {
                    json!(names)
                },
            )
        }
        "year" => (
            "issued",
            if value.is_empty() {
                Value::Null
            } else {
                let year: i32 = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Enter a year as a number"))?;
                anyhow::ensure!(
                    (1..=9999).contains(&year),
                    "Enter a year between 1 and 9999"
                );
                json!({"date-parts": [[year]]})
            },
        ),
        "isbn" => ("ISBN", json!(value)),
        "doi" => ("DOI", json!(value)),
        "publisher" => ("publisher", json!(value)),
        "edition" => {
            edition.label = if value.is_empty() {
                "Edition".into()
            } else {
                value.into()
            };
            ("edition", json!(value))
        }
        _ => anyhow::bail!("Edit title, author, year, publisher, ISBN, DOI, or edition"),
    };
    if value.is_null() || value.as_str() == Some("") {
        record.remove(key);
    } else {
        record.insert(key.into(), value);
    }
    Ok(())
}

impl App {
    pub(super) fn on_library_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc && self.brain.library.can_open_menu() {
            self.open_menu();
            return;
        }
        if let Some(store) = &mut self.brain.store {
            self.msg = match self.brain.library.key(store, key) {
                Ok(notice) => notice.unwrap_or_default(),
                Err(error) => format!("Library · {error}"),
            };
        }
    }
}

pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let library = &mut app.brain.library;
    let heading = match &library.open {
        Some((_, title)) => format!(" Library · {title} "),
        None => format!(
            " Library · {} · {} ",
            library
                .filter
                .source_type
                .map_or("All sources", Builtin::name),
            SORTS[library.sort].1
        ),
    };
    let panel = Block::bordered()
        .title(heading)
        .border_style(Style::default().fg(theme.border));
    let inner = panel.inner(area);
    frame.render_widget(panel, area);
    if inner.is_empty() {
        return;
    }
    let hint = match (&library.prompt, &library.import, &library.open) {
        (Some((input, _)), _, _) => input.hint(),
        (_, Some(_), _) => "Enter Add works · Esc Cancel",
        (_, _, Some(_)) => "↑↓ Edition · B Add edition · E Edit details · M Attach file · Esc Back",
        _ => "Enter Editions · F Filter · S Sort · T Type · D DOI · L ISBN · N New · I Import",
    };
    let mut controls = vec![Line::raw(hint)];
    if library.can_open_menu() {
        if !library.filter_input.is_empty() {
            controls.push(Line::raw(format!("Filter: {}", library.filter_input)));
        }
        if library.more || library.offset > 0 {
            let page = library.offset / u32::from(PAGE) + 1;
            controls.push(Line::raw(format!("Page {page} · PgUp / PgDn")));
        }
    }
    let controls = Paragraph::new(controls)
        .style(Style::default().fg(theme.dim))
        .wrap(Wrap { trim: true });
    let editing = u16::from(library.prompt.is_some());
    let max_header = inner.height.saturating_sub(editing + 1);
    let header_height = controls
        .line_count(inner.width)
        .min(usize::from(max_header)) as u16;
    let [header, entry, body] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(editing),
        Constraint::Min(1),
    ])
    .areas(inner);
    frame.render_widget(controls, header);
    if let Some((_, editor)) = &mut library.prompt {
        draw_draft(frame, editor, entry, Style::default().fg(theme.text), true);
    }
    let items = if let Some((records, notes)) = &library.import {
        std::iter::once(Line::raw(format!("{} works ready to add", records.len())))
            .chain(notes.iter().take(5).map(|note| Line::raw(note.clone())))
            .chain(records.iter().take(12).map(|record| {
                Line::raw(
                    record
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("(Untitled)")
                        .to_owned(),
                )
            }))
            .collect()
    } else if library.open.is_some() {
        let mut rows = Vec::new();
        if library.editions.is_empty() {
            rows.push(Line::raw("No editions yet. Press B to add one."));
        }
        for (number, (_, edition)) in library
            .editions
            .iter()
            .enumerate()
            .skip(library.edition.saturating_sub(4))
            .take(5)
        {
            rows.push(Line::styled(
                format!(
                    "{} {} · {}",
                    if number == library.edition {
                        "›"
                    } else {
                        " "
                    },
                    edition.label,
                    edition.title()
                ),
                row_style(theme, number == library.edition),
            ));
        }
        if let Some((_, edition)) = library.editions.get(library.edition) {
            for file in &edition.files {
                rows.push(Line::raw(format!(
                    "    {} · {} bytes",
                    file.name, file.size
                )));
            }
        }
        rows
    } else if library.works.is_empty() {
        vec![Line::raw("No matching works.")]
    } else {
        let visible = usize::from(body.height.max(1));
        let first = library.selected.saturating_add(1).saturating_sub(visible);
        library
            .works
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(index, work)| {
                let mark = if index == library.selected {
                    "›"
                } else {
                    " "
                };
                let status = if work.status.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", work.status)
                };
                let year = work
                    .year
                    .map_or_else(String::new, |year| format!(" · {year}"));
                Line::styled(
                    format!(
                        "{mark} {} · {}{year} · {}{status}",
                        work.title,
                        work.author,
                        work.source_type.name()
                    ),
                    row_style(theme, index == library.selected),
                )
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(items).style(Style::default().fg(theme.text)),
        body,
    );
}

fn row_style(theme: &Theme, selected: bool) -> Style {
    if selected {
        Style::default().fg(theme.accent).bg(theme.sel)
    } else {
        Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc28_library_catalog_preview_waits_for_the_current_input() {
        let fixture = tempfile::tempdir().unwrap();
        let mut store = PagedBrain::open(fixture.path(), 1).unwrap();
        let mut library = LibraryDesk::default();
        library.prompt(Input::New, "An unfinished title");
        let catalog = Lookup::new(LookupKind::Doi, "10.1000/fixture").unwrap();
        let (send, receive) = mpsc::channel();
        let record = serde_json::json!({"type":"book", "title":"Catalog title"});
        send.send(Ok(record.as_object().unwrap().clone())).unwrap();
        library.lookup = Some((catalog.clone(), receive));

        library.poll(&mut store, true).unwrap().unwrap();
        assert!(library.import.is_none());
        assert_eq!(
            library.prompt.as_ref().unwrap().1.text(),
            "An unfinished title"
        );
        assert_eq!(library.recent_lookups[0].0, catalog);
        library.prompt(Input::Lookup(LookupKind::Doi), "10.1000/fixture");
        library.save_prompt(&mut store).unwrap();
        assert!(library.import.is_some());
        assert!(library.lookup.is_none(), "use the cached reply");
    }
}
