use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver},
};
use std::time::Instant;

use anyhow::Context;
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
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use serde_json::Value;

use super::outline::{commands, draw_draft};
use super::{App, Theme};
use crate::app::overlays::catalogue_row;
use crate::catalogue::{self, Fetch, Hit, Search, SearchStatus, names};

mod lookup;
use lookup::Kind as LookupKind;

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
    Catalogue(Option<LookupKind>),
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
            Self::Catalogue(_) => "Search Open Library and Crossref · ISBN, DOI, title or author",
        }
    }
}

struct CataloguePrompt {
    search: Search,
    selected: usize,
    edition: usize,
    resolving: Option<Receiver<sources::Record>>,
}

impl CataloguePrompt {
    fn new(search: Search) -> Self {
        Self {
            search,
            selected: 0,
            edition: 0,
            resolving: None,
        }
    }

    fn hits(&self) -> (&[Hit], usize) {
        match self.search.editions() {
            Some((_, editions, _)) => (editions, self.edition),
            None => (self.search.hits(), self.selected),
        }
    }

    fn move_selection(&mut self, down: bool) {
        let (position, len) = match self.search.editions() {
            Some((_, editions, _)) => (&mut self.edition, editions.len()),
            None => (&mut self.selected, self.search.hits().len()),
        };
        let last = len.saturating_sub(1);
        *position = if down {
            position.saturating_add(1).min(last)
        } else {
            position.saturating_sub(1)
        };
        if down && *position == last {
            self.search.more_editions(Instant::now());
        }
    }

    fn tick(&mut self, now: Instant) {
        // Keep a selected Crossref result in view if Open Library replies later.
        let selected = self
            .search
            .hits()
            .get(self.selected)
            .filter(|_| self.selected > 0)
            .cloned();
        if self.search.tick(now) {
            let hits = self.search.hits();
            self.selected = selected
                .and_then(|hit| hits.iter().position(|row| row == &hit))
                .unwrap_or_else(|| self.selected.min(hits.len().saturating_sub(1)));
        }
    }
}

#[derive(Default)]
pub(super) struct ReadDesk {
    filter: Filter,
    filter_input: String,
    sort: usize,
    kind: usize,
    works: Vec<Work>,
    total_works: u64,
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
    catalogue: Option<CataloguePrompt>,
}

impl ReadDesk {
    pub(super) fn has_input(&self) -> bool {
        self.prompt.is_some()
    }

    pub(super) fn refresh(&mut self, store: &mut PagedBrain) -> anyhow::Result<()> {
        let (mut works, mut total) = store
            .library_works_with_count(&self.filter, self.offset, PAGE + 1)
            .map_err(anyhow::Error::msg)?;
        if works.is_empty() && self.offset > 0 {
            (works, total) = store
                .library_works_with_count(&self.filter, 0, PAGE + 1)
                .map_err(anyhow::Error::msg)?;
            self.offset = 0;
        }
        self.total_works = total;
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
        if !self.accepts_text() {
            return;
        }
        if let Some((_, input)) = &mut self.prompt {
            let text: String = text
                .chars()
                .filter(|c| !c.is_control())
                .take(4096)
                .collect();
            input.insert_str(&text);
            self.catalogue_edited(true);
        }
    }

    pub(super) fn edit_history(&mut self, redo: bool) {
        if !self.accepts_text() {
            return;
        }
        if let Some((_, editor)) = &mut self.prompt {
            let edited = if redo { editor.redo() } else { editor.undo() };
            if edited {
                self.catalogue_edited(false);
            }
        }
    }

    fn accepts_text(&self) -> bool {
        self.catalogue
            .as_ref()
            .is_none_or(|state| state.resolving.is_none() && state.search.editions().is_none())
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
        if let Some((Input::Catalogue(_), _)) = self.prompt
            && let Some(catalogue) = &mut self.catalogue
        {
            if catalogue.resolving.is_none() {
                catalogue.move_selection(down);
            }
        } else {
            self.move_selection(down);
        }
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

    fn open_catalogue(&mut self, shortcut: char) {
        let (kind, prefix) = match shortcut {
            'd' => (Some(LookupKind::Doi), "doi:"),
            'l' => (Some(LookupKind::Isbn), ""),
            _ => (None, ""),
        };
        let mut editor = Editor::from_text(prefix);
        editor.scene_end();
        self.prompt = Some((Input::Catalogue(kind), editor));
    }

    fn edit_prompt(&mut self, key: KeyCode) {
        let Some((_, editor)) = &mut self.prompt else {
            return;
        };
        let revision = editor.revision_stamp();
        match key {
            KeyCode::Char(c) if !c.is_control() => editor.insert(c),
            KeyCode::Backspace => editor.backspace(),
            KeyCode::Delete => editor.delete(),
            KeyCode::Left => editor.left(),
            KeyCode::Right => editor.right(),
            KeyCode::Home => editor.scene_start(),
            KeyCode::End => editor.scene_end(),
            _ => {}
        }
        if editor.revision_stamp() != revision {
            self.catalogue_edited(false);
        }
    }

    fn catalogue_edited(&mut self, pasted: bool) {
        if let Some((Input::Catalogue(kind), editor)) = &self.prompt
            && let Some(catalogue) = &mut self.catalogue
        {
            let now = Instant::now();
            catalogue.selected = 0;
            if kind.is_some() {
                // Exact shortcuts submit only on Enter, including after a paste.
                catalogue.search.edit("", now);
            } else {
                catalogue.search.edit(&editor.text(), now);
                if pasted {
                    catalogue.search.submit(now);
                }
            }
        }
    }

    fn close_catalogue(&mut self) {
        self.prompt = None;
        if let Some(catalogue) = &mut self.catalogue {
            catalogue.resolving = None;
            catalogue.selected = 0;
            catalogue.search.edit("", Instant::now());
        }
    }

    fn catalogue_key(
        &mut self,
        store: &mut PagedBrain,
        key: KeyCode,
        fetch: &Arc<dyn Fetch>,
        book: &Path,
        online: bool,
    ) -> anyhow::Result<Option<String>> {
        let Some((Input::Catalogue(kind), editor)) = &self.prompt else {
            return Ok(None);
        };
        let catalogue = self
            .catalogue
            .as_mut()
            .context("Catalogue search unavailable")?;
        let in_editions = catalogue.search.editions().is_some();
        let now = Instant::now();
        let (hits, selected) = catalogue.hits();
        match key {
            KeyCode::Left | KeyCode::Esc if in_editions => {
                catalogue.search.close_editions();
                catalogue.resolving = None;
            }
            KeyCode::Esc => self.close_catalogue(),
            _ if catalogue.resolving.is_some() => {}
            KeyCode::Up | KeyCode::Down => catalogue.move_selection(key == KeyCode::Down),
            KeyCode::Right
                if online
                    && !in_editions
                    && editor.cx == editor.lines[0].chars().count()
                    && hits.get(selected).is_some_and(|hit| hit.work.is_some()) =>
            {
                catalogue.edition = 0;
                catalogue.search.open_editions(selected, now);
            }
            KeyCode::Enter => {
                if let Some(hit) = hits.get(selected).cloned() {
                    if !online {
                        return self.keep_catalogue_pick(store, &hit.record).map(Some);
                    }
                    match CataloguePick::start(hit, fetch, book)? {
                        CataloguePick::Ready(record) => {
                            return self.keep_catalogue_pick(store, &record).map(Some);
                        }
                        CataloguePick::Resolving(receiver) => catalogue.resolving = Some(receiver),
                    }
                } else if in_editions {
                    catalogue.search.more_editions(now);
                } else {
                    let line = editor.text();
                    let line = match kind {
                        Some(kind) => kind.identifier(&line)?,
                        None => line,
                    };
                    catalogue.search.edit(&line, now);
                    catalogue.search.submit(now);
                }
            }
            _ if !in_editions => self.edit_prompt(key),
            _ => {}
        }
        Ok(None)
    }

    fn keep_catalogue_pick(
        &mut self,
        store: &mut PagedBrain,
        record: &sources::Record,
    ) -> anyhow::Result<String> {
        let key = keep_in_commonplace(store, record)?;
        let title = record
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let previous = self.open.replace((key.work, title.to_owned()));
        if let Err(error) = self.refresh(store) {
            self.open = previous;
            return Err(error);
        }
        if let Some(index) = self.works.iter().position(|work| work.id == key.work) {
            self.selected = index;
        }
        self.edition = self
            .editions
            .iter()
            .position(|(candidate, _)| *candidate == key)
            .unwrap_or(0);
        self.close_catalogue();
        Ok("Edition saved".into())
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
                    .name("Commonplace media copy".into())
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
            Input::Catalogue(_) => return Ok(None),
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

    fn key(
        &mut self,
        store: &mut PagedBrain,
        key: KeyEvent,
        fetch: &Arc<dyn Fetch>,
        book: &Path,
        online: bool,
    ) -> anyhow::Result<Option<String>> {
        if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            return Ok(None);
        }
        if matches!(self.prompt, Some((Input::Catalogue(_), _))) {
            return self.catalogue_key(store, key.code, fetch, book, online);
        }
        if self.prompt.is_some() {
            match key.code {
                KeyCode::Enter => return self.save_prompt(store),
                KeyCode::Esc => self.prompt = None,
                _ => self.edit_prompt(key.code),
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
                if let Some((id, _)) = &self.open {
                    self.prompt(Input::AddEdition(*id), "");
                }
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
        visible: bool,
    ) -> Option<anyhow::Result<String>> {
        if visible
            && matches!(self.prompt, Some((Input::Catalogue(_), _)))
            && let Some(catalogue) = &mut self.catalogue
        {
            catalogue.tick(Instant::now());
            if let Some(receiver) = &catalogue.resolving {
                match receiver.try_recv() {
                    Ok(record) => {
                        catalogue.resolving = None;
                        return Some(self.keep_catalogue_pick(store, &record));
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        catalogue.resolving = None;
                        return Some(Err(anyhow::anyhow!("The catalogue author lookup stopped.")));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
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

pub(in crate::app) enum CataloguePick {
    Ready(sources::Record),
    Resolving(Receiver<sources::Record>),
}

impl CataloguePick {
    pub(in crate::app) fn start(
        hit: Hit,
        fetch: &Arc<dyn Fetch>,
        book: &Path,
    ) -> anyhow::Result<Self> {
        let open_library = hit
            .record
            .get("x-wros-catalogue")
            .is_some_and(|data| data.get("openlibrary_work").is_some());
        if !open_library {
            return Ok(Self::Ready(hit.record));
        }
        let fetch = Arc::clone(fetch);
        let path = if cfg!(test) {
            book.join(".grimoire/catalogue-names.json")
        } else {
            crate::home().join(".cache/grimoire/catalogue-names.json")
        };
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("Catalogue author names".into())
            .spawn(move || {
                let record = resolve_open_library_names(hit, fetch.as_ref(), path);
                let _ = tx.send(record);
            })?;
        drop(worker);
        Ok(Self::Resolving(rx))
    }
}

fn resolve_open_library_names(mut hit: Hit, fetch: &dyn Fetch, path: PathBuf) -> sources::Record {
    // A picker can be closed while its worker is running. Serialize authority lookups
    // and cache writes so the next picker can reuse any names the first one resolved.
    static CACHE: Mutex<()> = Mutex::new(());
    let _cache_guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut names = names::Names::load(path);
    if let Some(authors) = hit.record.get_mut("author").and_then(Value::as_array_mut) {
        // The mapper drops missing author IDs and names independently.
        let matched = authors.len() == hit.authors.len();
        for (index, name) in authors.iter_mut().enumerate() {
            if let Some(written) = name["literal"].as_str() {
                *name = if matched {
                    names.resolve(fetch, &hit.authors[index], written)
                } else {
                    names::by_rule(written)
                };
            }
        }
    }
    if let Some(translators) = hit
        .record
        .get_mut("translator")
        .and_then(Value::as_array_mut)
    {
        for name in translators {
            if let Some(written) = name["literal"].as_str() {
                *name = names::by_rule(written);
            }
        }
    }
    hit.record
}

pub(crate) fn keep_in_commonplace(
    store: &mut PagedBrain,
    record: &sources::Record,
) -> anyhow::Result<EditionKey> {
    let edition = Edition::new(record.clone(), Utc::now().timestamp());
    let fingerprint = sources::library_fingerprint(record)?;
    let candidates = store
        .find_editions(edition.title(), 32)
        .map_err(anyhow::Error::msg)?;
    if let Some((key, _)) = candidates.iter().find(|(_, existing)| {
        sources::library_fingerprint(&existing.bibliography).is_ok_and(|found| found == fingerprint)
    }) {
        return Ok(*key);
    }

    let work = record.get("x-wros-catalogue").and_then(|catalogue| {
        candidates.into_iter().find_map(|(key, existing)| {
            let existing = existing.bibliography.get("x-wros-catalogue")?;
            let same = ["openlibrary_work", "doi"].into_iter().any(|field| {
                catalogue[field]
                    .as_str()
                    .is_some_and(|id| !id.is_empty() && existing[field].as_str() == Some(id))
            });
            same.then_some(key.work)
        })
    });
    match work {
        Some(work) => store.add_edition(work, edition),
        None => store.create_work(Place::default(), edition),
    }
    .map_err(anyhow::Error::msg)
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
    pub(in crate::app) fn cancel_read_catalogue(&mut self) {
        if matches!(self.commonplace.read.prompt, Some((Input::Catalogue(_), _))) {
            self.commonplace.read.close_catalogue();
        }
    }

    pub(super) fn on_read_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc && self.commonplace.read.can_open_menu() {
            self.open_menu();
            return;
        }
        if !self.commonplace.read.has_input()
            && self.commonplace.read.import.is_none()
            && self.commonplace.store.is_some()
            && key.modifiers.difference(KeyModifiers::SHIFT).is_empty()
            && let KeyCode::Char(shortcut @ ('@' | 'd' | 'l')) = key.code
        {
            if shortcut == '@' && !self.catalogue_on {
                self.msg = "Online search is off · Settings › Writing › Manuscript".into();
                return;
            }
            if self.commonplace.read.catalogue.is_none() {
                let search = if self.catalogue_on {
                    self.new_catalogue_search()
                } else {
                    // Only the explicitly submitted DOI and ISBN shortcuts get here.
                    Search::new(Arc::clone(&self.catalogue_fetch), true)
                };
                self.commonplace.read.catalogue = Some(CataloguePrompt::new(search));
            }
            self.commonplace.read.open_catalogue(shortcut);
            self.msg.clear();
            return;
        }
        if let Some(store) = &mut self.commonplace.store {
            self.msg = match self.commonplace.read.key(
                store,
                key,
                &self.catalogue_fetch,
                &self.project.root,
                self.catalogue_on,
            ) {
                Ok(notice) => notice.unwrap_or_default(),
                Err(error) => format!("read · {error}"),
            };
        }
    }
}

pub(crate) fn info(app: &App) -> String {
    let desk = &app.commonplace.read;
    let dot = crate::ui::info_separator(app.linux_console);
    if let Some((_, title)) = &desk.open {
        return if desk.editions.is_empty() {
            format!("read{dot}{title}")
        } else {
            format!(
                "read{dot}{title}{dot}edition {} of {}",
                desk.edition + 1,
                desk.editions.len()
            )
        };
    }
    let scope = desk.filter.source_type.map_or("All sources", Builtin::name);
    let count = desk.total_works;
    let works = if count == 1 { "work" } else { "works" };
    let sort = SORTS[desk.sort].1;
    let filter = if desk.filter_input.is_empty() {
        String::new()
    } else {
        format!("{dot}Filter: {}", desk.filter_input)
    };
    format!("read{dot}{count} {works}{dot}{scope}{filter}{dot}{sort}")
}

pub(crate) fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let desk = &mut app.commonplace.read;
    // The key hints use the bottom edge, leaving the shelf the same height.
    let panel = Block::new()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(theme.border));
    let inner = panel.inner(area);
    frame.render_widget(panel, area);
    if inner.is_empty() {
        return;
    }
    let hint = match (&desk.prompt, &desk.import, &desk.open) {
        (Some((input, _)), _, _) => input.hint(),
        (_, Some(_), _) => "Enter Add works · Esc Cancel",
        (_, _, Some(_)) => "↑↓ Edition · B Add edition · E Edit details · M Attach file · Esc Back",
        _ => concat!(
            "Enter Editions · @ Search · F Filter · S Sort · T Type · ",
            "D DOI · L ISBN · N New · I Import",
        ),
    };
    let mut controls = vec![Line::raw(hint)];
    if desk.can_open_menu() && (desk.more || desk.offset > 0) {
        let page = desk.offset / u32::from(PAGE) + 1;
        controls.push(Line::raw(format!("Page {page} · PgUp / PgDn")));
    }
    let controls = Paragraph::new(controls)
        .style(Style::default().fg(theme.dim))
        .wrap(Wrap { trim: true });
    let editing = u16::from(desk.prompt.is_some());
    let max_footer = inner.height.saturating_sub(editing + 1);
    let footer_height = controls
        .line_count(inner.width)
        .min(usize::from(max_footer)) as u16;
    let [entry, body, footer] = Layout::vertical([
        Constraint::Length(editing),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .areas(inner);
    frame.render_widget(controls, footer);
    if let Some((_, editor)) = &mut desk.prompt {
        draw_draft(frame, editor, entry, Style::default().fg(theme.text), true);
    }
    let items = if matches!(desk.prompt, Some((Input::Catalogue(_), _)))
        && let Some(catalogue) = &desk.catalogue
    {
        catalogue_lines(catalogue, body, theme, app.catalogue_on)
    } else if let Some((records, notes)) = &desk.import {
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
    } else if desk.open.is_some() {
        let mut rows = Vec::new();
        if desk.editions.is_empty() {
            rows.push(Line::raw("No editions yet. Press B to add one."));
        }
        for (number, (_, edition)) in desk
            .editions
            .iter()
            .enumerate()
            .skip(desk.edition.saturating_sub(4))
            .take(5)
        {
            rows.push(Line::styled(
                format!(
                    "{} {} · {}",
                    if number == desk.edition { "›" } else { " " },
                    edition.label,
                    edition.title()
                ),
                row_style(theme, number == desk.edition),
            ));
        }
        if let Some((_, edition)) = desk.editions.get(desk.edition) {
            for file in &edition.files {
                rows.push(Line::raw(format!(
                    "    {} · {} bytes",
                    file.name, file.size
                )));
            }
        }
        rows
    } else if desk.works.is_empty() {
        vec![Line::raw("No matching works.")]
    } else {
        let visible = usize::from(body.height.max(1));
        let first = desk.selected.saturating_add(1).saturating_sub(visible);
        desk.works
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(index, work)| {
                let mark = if index == desk.selected { "›" } else { " " };
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
                    row_style(theme, index == desk.selected),
                )
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(items).style(Style::default().fg(theme.text)),
        body,
    );
}

fn catalogue_lines(
    catalogue: &CataloguePrompt,
    area: Rect,
    theme: &Theme,
    online: bool,
) -> Vec<Line<'static>> {
    let setting = if online { "on" } else { "off" };
    let mut lines = vec![Line::raw(format!("Catalogue · online search {setting}"))];
    let (hits, selected) = catalogue.hits();
    let editions = catalogue.search.editions();
    let keys = if editions.is_some() {
        "↑↓ Choose · Enter add · ←/Esc back"
    } else if online {
        "↑↓ Choose · → editions · Enter search or add · Esc cancel"
    } else {
        "Enter look up or add · Esc cancel"
    };
    lines.push(Line::styled(keys, Style::default().fg(theme.dim)));
    if let Some((work, _, _)) = editions {
        let title = work
            .record
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        lines.push(Line::raw(format!("Editions · {title}")));
    }
    let status = if catalogue.resolving.is_some() {
        Some("Resolving author names…")
    } else if editions.is_some_and(|(_, _, loading)| loading) {
        Some("Loading editions…")
    } else {
        match catalogue.search.status() {
            SearchStatus::Searching => Some("Searching catalogues…"),
            SearchStatus::Offline => Some(catalogue::OFFLINE),
            SearchStatus::Error(error) => Some(error.as_str()),
            SearchStatus::Done if hits.is_empty() => Some("No matches found."),
            _ => None,
        }
    };
    if let Some(status) = status {
        lines.push(Line::raw(status.to_owned()));
    }
    let visible = usize::from(area.height).saturating_sub(lines.len()).max(1);
    let first = selected.saturating_add(1).saturating_sub(visible);
    let width = usize::from(area.width.saturating_sub(2));
    lines.extend(
        hits.iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(i, hit)| {
                let mark = if i == selected { "›" } else { " " };
                Line::styled(
                    format!("{mark} {}", catalogue_row(hit, width)),
                    row_style(theme, i == selected),
                )
            }),
    );
    lines
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
    fn catalogue_picks_join_their_work_and_reuse_editions() {
        let fixture = tempfile::tempdir().unwrap();
        let mut store = PagedBrain::open(fixture.path(), 1).unwrap();
        let record = |publisher: &str, year: u16, work: &str| {
            serde_json::json!({
                "type": "book",
                "title": "Crime and Punishment",
                "publisher": publisher,
                "issued": {"date-parts": [[year]]},
                "x-wros-catalogue": {"openlibrary_work": work},
            })
            .as_object()
            .unwrap()
            .clone()
        };
        let penguin = record("Penguin Books", 1998, "OL166894W");
        let vintage = record("Vintage", 1993, "OL166894W");
        let first = keep_in_commonplace(&mut store, &penguin).unwrap();
        let second = keep_in_commonplace(&mut store, &vintage).unwrap();
        assert_eq!(first.work, second.work);
        assert_ne!(first.edition, second.edition);
        assert_eq!(
            store.edition(second).unwrap().unwrap().bibliography,
            vintage
        );

        assert_eq!(keep_in_commonplace(&mut store, &penguin).unwrap(), first);
        let other = record("Vintage", 1998, "OL1W");
        let third = keep_in_commonplace(&mut store, &other).unwrap();
        assert_ne!(first.work, third.work);
    }

    #[test]
    fn rc28_read_catalog_preview_waits_for_the_current_input() {
        use crate::catalogue::{crossref, fetch::FixtureFetch};

        let fixture = tempfile::tempdir().unwrap();
        let mut store = PagedBrain::open(fixture.path(), 1).unwrap();
        let url = crossref::doi_url("10.1000/fixture");
        let fetch = Arc::new(FixtureFetch::new().reply(
            &url,
            200,
            r#"{"message":{"type":"book","title":["Catalog title"]}}"#,
        ));
        let handle: Arc<dyn Fetch> = fetch.clone();
        let mut library = ReadDesk {
            catalogue: Some(CataloguePrompt::new(Search::new(Arc::clone(&handle), true))),
            ..ReadDesk::default()
        };
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        library.open_catalogue('d');
        library.paste("10.1000/fixture");
        library
            .key(&mut store, enter, &handle, fixture.path(), true)
            .unwrap();
        let start = Instant::now();
        while library.catalogue.as_ref().unwrap().search.hits().is_empty() {
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            assert!(library.poll(&mut store, true).is_none());
            std::thread::yield_now();
        }
        assert_eq!(
            library.prompt.as_ref().unwrap().1.text(),
            "doi:10.1000/fixture"
        );

        library.close_catalogue();
        library.prompt(Input::New, "An unfinished title");
        assert!(library.poll(&mut store, true).is_none());

        assert!(library.import.is_none());
        assert_eq!(
            library.prompt.as_ref().unwrap().1.text(),
            "An unfinished title"
        );
        library.open_catalogue('d');
        library.paste("10.1000/fixture");
        library
            .key(&mut store, enter, &handle, fixture.path(), true)
            .unwrap();
        assert_eq!(library.catalogue.as_ref().unwrap().search.hits().len(), 1);
        assert_eq!(fetch.requests(), [url]);
    }
}
