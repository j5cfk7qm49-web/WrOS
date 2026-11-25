//! The project bibliography and the keyboard citation picker.

use super::*;
use crate::app::brain::BrainReferenceTarget;
use crate::app::brain::library::keep_in_library;
use crate::catalogue::{Hit, SearchStatus, names};
use grimoire_brain::library::EditionKey;
use grimoire_brain::model::NodeId;
use grimoire_core::sync::Fingerprint;
use grimoire_core::{citations, sources};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceDialog {
    book: PathBuf,
    snapshot: Option<sources::Snapshot>,
    page: Page,
    query: String,
    selected: usize,
    notice: String,
    library: Vec<LibraryPick>,
    catalogue: Option<CatalogueSearch>,
    catalogue_view: Option<usize>,
    staged: BTreeSet<String>,
    editing_citation: Option<Box<Page>>,
}

// Modal navigation can clone an overlay. Keep its in-flight search and author reply together.
#[derive(Clone)]
struct CatalogueSearch(Rc<RefCell<CatalogueState>>);

struct CatalogueState {
    search: catalogue::Search,
    resolving: Option<mpsc::Receiver<sources::Record>>,
}

impl fmt::Debug for CatalogueSearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CatalogueSearch").finish_non_exhaustive()
    }
}

impl PartialEq for CatalogueSearch {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Page {
    List,
    BrainPick(BrainReferenceTarget),
    LibraryChanges {
        changes: std::collections::VecDeque<LibraryChange>,
        export: bool,
        scroll: usize,
    },
    Remove(String),
    Restore {
        version: sources::Sources,
        missing: Vec<String>,
        scroll: usize,
    },
    Drive {
        merge: Box<sources::DriveMerge>,
        choices: std::collections::BTreeMap<String, bool>,
        index: usize,
        scroll: usize,
    },
    Pick {
        target: Target,
        chosen: Option<Citation>,
        cluster: Vec<Citation>,
        field: usize,
    },
    Form {
        original_key: Option<String>,
        kind: usize,
        fields: Vec<String>,
        field: usize,
    },
    Import {
        path: String,
        preview: Option<(sources::Sources, Vec<String>)>,
        scroll: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Target {
    document: PathBuf,
    pane: Pane,
    position: (usize, usize),
    revision: Fingerprint,
}

#[derive(Debug, Clone, PartialEq)]
struct Citation {
    key: String,
    page: String,
    suppress: bool,
    library: Option<Box<LibraryPick>>,
    edited: Option<Box<sources::Record>>,
    works_cited: String,
}

impl Citation {
    fn new(key: String, record: &sources::Record) -> Self {
        Self {
            key,
            page: String::new(),
            suppress: false,
            library: None,
            edited: None,
            works_cited: source_line(record),
        }
    }

    fn from_library(pick: LibraryPick) -> Self {
        let mut cite = Self::new(pick.key.clone(), &pick.record);
        cite.library = Some(Box::new(pick));
        cite
    }

    fn record<'a>(
        &'a self,
        snapshot: Option<&'a sources::Snapshot>,
    ) -> Option<&'a sources::Record> {
        self.edited
            .as_deref()
            .or_else(|| self.library.as_ref().map(|pick| &pick.record))
            .or_else(|| snapshot?.sources.get(&self.key))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct LibraryPick {
    key: String,
    edition: EditionKey,
    record: sources::Record,
}

#[derive(Debug, Clone, PartialEq)]
struct LibraryChange {
    project_key: String,
    edition: EditionKey,
    before: sources::Record,
    proposed: sources::Record,
}

const TYPES: [(&str, &str); 4] = [
    ("Book", "book"),
    ("Chapter", "chapter"),
    ("Journal article", "article-journal"),
    ("Web page", "webpage"),
];
const FIELDS: [(&str, &str); 13] = [
    ("Key (leave blank to suggest one)", "id"),
    ("Title", "title"),
    ("Authors (Family, Given; …)", "author"),
    ("Editors (Family, Given; …)", "editor"),
    ("Date (YYYY, YYYY-MM or YYYY-MM-DD)", "issued"),
    ("Journal or book", "container-title"),
    ("Publisher", "publisher"),
    ("Volume", "volume"),
    ("Issue", "issue"),
    ("Pages", "page"),
    ("Edition", "edition"),
    ("DOI", "DOI"),
    ("URL", "URL"),
];

impl App {
    fn sources_dialog(&mut self, page: Page) {
        let book = self.project.root.clone();
        let (snapshot, notice) = if matches!(page, Page::BrainPick(_)) {
            (None, String::new())
        } else {
            match sources::Snapshot::open(&book) {
                Ok(snapshot) => (Some(snapshot), String::new()),
                Err(e) => (None, format!("{e:#}")),
            }
        };
        let catalogue = matches!(page, Page::Pick { .. } | Page::BrainPick(_)).then(|| {
            CatalogueSearch(Rc::new(RefCell::new(CatalogueState {
                search: self.new_catalogue_search(),
                resolving: None,
            })))
        });
        let mut dialog = Box::new(SourceDialog {
            book,
            snapshot,
            page,
            query: String::new(),
            selected: 0,
            notice,
            library: Vec::new(),
            catalogue,
            catalogue_view: None,
            staged: BTreeSet::new(),
            editing_citation: None,
        });
        self.refresh_citation_library(&mut dialog);
        self.overlay = Overlay::ProjectSources(dialog);
    }

    pub fn open_project_sources(&mut self) {
        self.sources_dialog(Page::List);
    }

    pub fn add_project_source(&mut self) {
        self.sources_dialog(form(None));
    }

    pub fn import_project_sources(&mut self) {
        self.sources_dialog(Page::Import {
            path: String::new(),
            preview: None,
            scroll: 0,
        });
    }

    pub fn open_sources_timeline(&mut self) {
        self.commit_saves();
        let Some(timeline) = &self.timeline else {
            self.msg = "Turn on the project timeline to see source versions.".into();
            return;
        };
        let path = self.project.root.join(sources::FILENAME);
        let loading = HistoryTimeline::Reading(timeline.scene_versions(path.clone()).into());
        self.overlay = Overlay::History {
            scene: path,
            title: "Project sources".into(),
            versions: Vec::new(),
            timeline: loading,
            sel: 0,
            scroll: 0,
        };
    }

    pub(super) fn preview_sources_restore(&mut self, text: &str) {
        match sources::Sources::parse(text) {
            Ok(version) => {
                self.flush();
                let missing = missing_keys(&self.project, &version);
                self.sources_dialog(Page::Restore {
                    version,
                    missing,
                    scroll: 0,
                });
            }
            Err(e) => self.msg = format!("The saved source file cannot be restored: {e}"),
        }
    }

    pub fn open_sources_conflict(&mut self) {
        let root = &self.project.root;
        let result = (|| -> Result<sources::DriveMerge> {
            let pending = self
                .deck
                .shell
                .backup
                .sync
                .pending
                .iter()
                .find(|p| p.path == sources::FILENAME && root.join(&p.parked).is_file())
                .ok_or_else(|| {
                    anyhow::anyhow!("Run Drive sync to find the parked project source version.")
                })?;
            let base = pending.merge_base.as_ref().ok_or_else(|| {
                anyhow::anyhow!("The common source version is missing. Both copies were kept.")
            })?;
            sources::DriveMerge::open(root, &root.join(&pending.parked), Path::new(base))
        })();
        match result {
            Ok(merge) => self.sources_dialog(Page::Drive {
                merge: Box::new(merge),
                choices: Default::default(),
                index: 0,
                scroll: 0,
            }),
            Err(e) => self.msg = format!("{e:#}"),
        }
    }

    pub fn maybe_open_citation_picker(&mut self) -> bool {
        let Some(node) = self.open.map(|id| &self.project.nodes[id]) else {
            return false;
        };
        if self.overlay != Overlay::None
            || !self.focus.is_editor()
            || self.editor.has_selection()
            || node.read_only
            || !matches!(node.area, Area::Manuscript | Area::Notes | Area::Research)
        {
            return false;
        }
        let col = self.editor.cx;
        let row = self.editor.cy;
        let Some(line) = self.editor.lines.get(row) else {
            return false;
        };
        if col > 0
            && line
                .chars()
                .nth(col - 1)
                .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '@'))
        {
            return false;
        }
        let target = Target {
            document: node.path.clone(),
            pane: self.views.active,
            position: (row, col),
            revision: Fingerprint::of(&self.editor.text()),
        };
        self.sources_dialog(Page::Pick {
            target,
            chosen: None,
            cluster: Vec::new(),
            field: 0,
        });
        true
    }

    pub(crate) fn open_brain_library_picker(&mut self, target: BrainReferenceTarget) {
        self.sources_dialog(Page::BrainPick(target));
    }

    pub(crate) fn paste_project_sources(&mut self, text: &str) {
        let Overlay::ProjectSources(mut dialog) =
            std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        if dialog.catalogue_view.is_some() || dialog.resolving_catalogue() {
            self.overlay = Overlay::ProjectSources(dialog);
            return;
        }
        let before = dialog.query.clone();
        dialog.paste(text);
        if before != dialog.query {
            self.refresh_citation_library(&mut dialog);
        }
        if let Some(catalogue) = dialog.picker_catalogue() {
            let mut catalogue = catalogue.0.borrow_mut();
            let now = Instant::now();
            catalogue.search.edit(&dialog.query, now);
            catalogue.search.submit(now);
        }
        self.overlay = Overlay::ProjectSources(dialog);
    }

    fn refresh_citation_library(&mut self, dialog: &mut SourceDialog) {
        let brain = matches!(dialog.page, Page::BrainPick(_));
        if !brain && !matches!(dialog.page, Page::Pick { .. }) {
            return;
        }
        dialog.library.clear();
        let project = dialog.snapshot.as_ref().map(|snapshot| &snapshot.sources);
        if !brain && project.is_none() {
            return;
        }
        let Ok(Some(store)) = self.library_store_if_present() else {
            return;
        };
        let query = dialog.query.trim();
        let Ok(editions) = store.find_editions(query, 48) else {
            return;
        };
        let linked: std::collections::BTreeSet<_> = project
            .into_iter()
            .filter(|_| !brain)
            .flat_map(sources::Sources::iter)
            .filter_map(|(_, record)| sources::library_provenance(record).map(|link| link.identity))
            .collect();
        for (edition, metadata) in editions {
            let identity = library_identity(edition);
            if linked.contains(&identity) {
                continue;
            }
            if let Ok(pick) = library_pick(project, edition, identity, &metadata.bibliography) {
                dialog.library.push(pick);
            }
        }
    }

    pub(in crate::app) fn tick_source_catalogue(&mut self) {
        let Overlay::ProjectSources(dialog) = &mut self.overlay else {
            return;
        };
        let Some(result) = dialog.tick_catalogue(Instant::now()) else {
            return;
        };
        let Overlay::ProjectSources(mut dialog) =
            std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            unreachable!("the source picker was just polled");
        };
        let result = result.and_then(|record| self.finish_catalogue_pick(&mut dialog, record));
        if let Err(error) = result {
            dialog.notice = format!("{error:#}");
        }
        self.overlay = Overlay::ProjectSources(dialog);
    }

    fn start_catalogue_pick(&mut self, dialog: &mut SourceDialog, hit: Hit) -> Result<()> {
        let open_library = hit
            .record
            .get("x-wros-catalogue")
            .is_some_and(|data| data.get("openlibrary_work").is_some());
        if !open_library {
            return self.finish_catalogue_pick(dialog, hit.record);
        }
        let fetch = Arc::clone(&self.catalogue_fetch);
        let path = catalogue_names_path(&dialog.book);
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("Catalogue author names".into())
            .spawn(move || {
                let record = resolve_open_library_names(hit, fetch.as_ref(), path);
                let _ = tx.send(record);
            })?;
        drop(worker);
        let catalogue = dialog.catalogue.as_ref().expect("a catalogue pick");
        catalogue.0.borrow_mut().resolving = Some(rx);
        dialog.notice.clear();
        Ok(())
    }

    fn finish_catalogue_pick(
        &mut self,
        dialog: &mut SourceDialog,
        mut record: sources::Record,
    ) -> Result<()> {
        let Page::Pick { target, .. } = &dialog.page else {
            return Ok(());
        };
        let library = if let Ok(Some(store)) = self.library_store_if_present() {
            let edition = keep_in_library(store, &record)?;
            let metadata = store
                .edition(edition)
                .map_err(anyhow::Error::msg)?
                .ok_or_else(|| anyhow::anyhow!("The Library edition is unavailable"))?;
            Some((edition, metadata.bibliography))
        } else {
            None
        };
        // Keep the edition even when the original place to cite has disappeared.
        self.citation_target(target, &dialog.book)?;
        let project = &mut dialog
            .snapshot
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("The project source list is unavailable."))?
            .sources;
        let cite = if let Some((edition, bibliography)) = library {
            let identity = library_identity(edition);
            let linked = project.iter().find(|(_, record)| {
                sources::library_provenance(record).is_some_and(|link| link.identity == identity)
            });
            let pick = if let Some((key, record)) = linked {
                LibraryPick {
                    key: key.to_owned(),
                    edition,
                    record: record.clone(),
                }
            } else {
                library_pick(Some(project), edition, identity, &bibliography)?
            };
            Citation::from_library(pick)
        } else {
            let key = project.suggested_key(&record);
            record.insert("id".into(), Value::String(key.clone()));
            let cite = Citation::new(key.clone(), &record);
            project.add(record)?;
            dialog.staged.insert(key);
            cite
        };
        dialog.close_catalogue_editions();
        if let Page::Pick { chosen, field, .. } = &mut dialog.page {
            *chosen = Some(cite);
            *field = 0;
        }
        dialog.notice.clear();
        Ok(())
    }

    fn on_catalogue_editions_key(&mut self, dialog: &mut SourceDialog, key: Key) {
        if matches!(key, Key::Left | Key::Esc) {
            dialog.close_catalogue_editions();
            dialog.notice.clear();
            return;
        }
        let mut catalogue = dialog
            .catalogue
            .as_ref()
            .expect("the editions view has a search")
            .0
            .borrow_mut();
        if catalogue.resolving.is_some() {
            return;
        }
        let selected = dialog
            .catalogue_view
            .as_mut()
            .expect("the editions view is open");
        let Some((_, editions, _)) = catalogue.search.editions() else {
            return;
        };
        match key {
            Key::Up => *selected = selected.saturating_sub(1),
            Key::Down => {
                let last = editions.len().saturating_sub(1);
                *selected = (*selected + 1).min(last);
                if *selected == last {
                    catalogue.search.more_editions(Instant::now());
                }
            }
            Key::Enter => {
                if let Some(hit) = editions.get(*selected).cloned() {
                    drop(catalogue);
                    if let Err(error) = self.start_catalogue_pick(dialog, hit) {
                        dialog.notice = format!("{error:#}");
                    }
                } else {
                    catalogue.search.more_editions(Instant::now());
                }
            }
            _ => (),
        }
    }

    pub(crate) fn review_library_changes(&mut self, export: bool) -> bool {
        let Ok(sources) = sources::Snapshot::open(&self.project.root) else {
            return false;
        };
        let linked: Vec<_> = sources
            .sources
            .iter()
            .filter_map(|(key, record)| {
                let link = sources::library_provenance(record)?;
                let work = link.identity.work_id.parse().ok()?;
                let NodeId::Note(edition) = link.identity.edition_id.parse().ok()? else {
                    return None;
                };
                Some((key, record, EditionKey { work, edition }))
            })
            .collect();
        if linked.is_empty() {
            return false;
        }
        let Ok(Some(store)) = self.library_store_if_present() else {
            return false;
        };
        let mut changes = std::collections::VecDeque::new();
        for (key, record, edition) in linked {
            if let Ok(Some(current)) = store.edition(edition)
                && let Ok(Some(proposed)) =
                    sources::proposed_library_update(record, &current.bibliography)
            {
                changes.push_back(LibraryChange {
                    project_key: key.into(),
                    edition,
                    before: record.clone(),
                    proposed,
                });
            }
        }
        if changes.is_empty() {
            return false;
        }
        self.sources_dialog(Page::LibraryChanges {
            changes,
            export,
            scroll: 0,
        });
        true
    }

    fn accept_library_change(&mut self, book: &Path, change: &LibraryChange) -> Result<()> {
        anyhow::ensure!(self.project.root == book, "The open project changed");
        let mut sources_file = sources::Snapshot::open(book)?;
        anyhow::ensure!(
            sources_file.sources.get(&change.project_key) == Some(&change.before),
            "The project source changed. Open the review again."
        );
        let current = self
            .library_store_if_present()?
            .ok_or_else(|| anyhow::anyhow!("The Library is unavailable"))?
            .edition(change.edition)
            .map_err(anyhow::Error::msg)?
            .ok_or_else(|| anyhow::anyhow!("The edition disappeared from Library"))?;
        anyhow::ensure!(
            sources::proposed_library_update(&change.before, &current.bibliography)?.as_ref()
                == Some(&change.proposed),
            "The Library edition changed. Open the review again."
        );
        sources_file
            .sources
            .update(&change.project_key, change.proposed.clone())?;
        sources_file.save(book)?;
        self.sources_saved();
        Ok(())
    }

    fn citation_target(&self, target: &Target, book: &Path) -> Result<usize> {
        self.open
            .filter(|&i| {
                self.focus.is_editor()
                    && self.views.active == target.pane
                    && self.project.root == book
                    && self.project.nodes[i].path == target.document
                    && (self.editor.cy, self.editor.cx) == target.position
                    && !self.editor.has_selection()
                    && Fingerprint::of(&self.editor.text()) == target.revision
            })
            .ok_or_else(|| {
                anyhow::anyhow!("The writing cursor or text changed. Open the picker again.")
            })
    }

    fn insert_citation(
        &mut self,
        target: &Target,
        literal: bool,
        picked: &[Citation],
        from: &SourceDialog,
    ) -> Result<()> {
        let i = self.citation_target(target, &from.book)?;
        self.ensure_document_claims()?;
        self.check_document(target.pane)?;
        let node = &self.project.nodes[i];
        anyhow::ensure!(
            !node.read_only && node.seen.is_some() && Fingerprint::read(&node.path) == node.seen,
            "The source changed on disk. Reopen it before citing."
        );
        let text = if literal {
            "@".to_owned()
        } else {
            let mut on_disk = sources::Snapshot::open(&from.book)?;
            let previous = &from
                .snapshot
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("The project source list is unavailable."))?
                .sources;
            let mut items = Vec::new();
            let mut changes = BTreeMap::new();
            for citation in picked {
                let existing = on_disk.sources.get(&citation.key);
                let record = if let Some(pick) = &citation.library {
                    let accepted = sources::library_provenance(&pick.record).ok_or_else(|| {
                        anyhow::anyhow!("The Library record has no accepted details")
                    })?;
                    let current = self
                        .library_store_if_present()?
                        .ok_or_else(|| anyhow::anyhow!("The Library is unavailable"))?
                        .edition(pick.edition)
                        .map_err(anyhow::Error::msg)?
                        .ok_or_else(|| anyhow::anyhow!("The Library edition is unavailable"))?;
                    anyhow::ensure!(
                        sources::library_fingerprint(&current.bibliography)?
                            == accepted.fingerprint,
                        "The Library edition changed. Open the picker again."
                    );
                    anyhow::ensure!(
                        existing.map_or_else(
                            || previous.get(&citation.key).is_none(),
                            |record| record == &pick.record,
                        ),
                        "A project source now uses that key. Open the picker again."
                    );
                    &pick.record
                } else {
                    let record = previous
                        .get(&citation.key)
                        .ok_or_else(|| anyhow::anyhow!("The project source is unavailable."))?;
                    anyhow::ensure!(
                        (from.staged.contains(&citation.key) && existing.is_none())
                            || existing == Some(record),
                        "The project source changed. Open the picker again."
                    );
                    record
                };
                if let Some(edited) = citation.edited.as_deref() {
                    if existing != Some(edited) {
                        changes.insert(citation.key.as_str(), edited);
                    }
                } else if existing.is_none() {
                    changes.entry(citation.key.as_str()).or_insert(record);
                }
                let page = citation.page.trim();
                let mut item = format!(
                    "{}@{}",
                    if citation.suppress { "-" } else { "" },
                    citation.key
                );
                if !page.is_empty() {
                    anyhow::ensure!(
                        citations::valid_page(page),
                        "Enter a page or range, for example 194 or 12–14."
                    );
                    let label = if page.contains(['-', '–', '—']) {
                        "pp."
                    } else {
                        "p."
                    };
                    item.push_str(&format!(", {label} {page}"));
                }
                items.push(item);
            }
            if !changes.is_empty() {
                for (key, record) in changes {
                    if on_disk.sources.get(key).is_some() {
                        on_disk.sources.update(key, record.clone())?;
                    } else {
                        on_disk.sources.add(record.clone())?;
                    }
                }
                on_disk.save(&from.book)?;
                self.sources_saved();
            }
            format!("[{}]", items.join("; "))
        };
        self.editor
            .insert_str_as(&text, grimoire_core::measure::Origin::Programmatic);
        self.flush();
        Ok(())
    }

    pub(super) fn on_project_sources_key(&mut self, key: Key) {
        let Overlay::ProjectSources(mut dialog) =
            std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        if dialog.catalogue_view.is_some() {
            self.on_catalogue_editions_key(&mut dialog, key);
            self.overlay = Overlay::ProjectSources(dialog);
            return;
        }
        if key == Key::Esc {
            if let Some(page) = dialog.editing_citation.take() {
                dialog.page = *page;
                dialog.notice.clear();
                self.overlay = Overlay::ProjectSources(dialog);
                return;
            }
            if matches!(dialog.page, Page::LibraryChanges { export: true, .. }) {
                self.open_export_ready();
            }
            return;
        }
        if dialog.resolving_catalogue() {
            self.overlay = Overlay::ProjectSources(dialog);
            return;
        }
        let mut pending = None;
        let mut edit_citation = false;
        let query_before = dialog.query.clone();
        let matches = dialog.matches();
        let local_count = matches.len();
        let current = matches
            .get(dialog.selected)
            .map(|(key, _)| (*key).to_owned());
        let choosing_local =
            key == Key::Enter && matches!(dialog.page, Page::Pick { chosen: None, .. });
        let local_cite = if choosing_local {
            matches.get(dialog.selected).map(|&(key, record)| {
                if let Some(pick) = dialog.library.iter().find(|pick| pick.key == key) {
                    Citation::from_library(pick.clone())
                } else {
                    Citation::new(key.into(), record)
                }
            })
        } else {
            None
        };
        if dialog.picker_catalogue().is_some() && current.is_none() {
            let index = dialog.selected.saturating_sub(local_count);
            match key {
                Key::Enter => {
                    if let Some(hit) = dialog.catalogue_hit(index) {
                        if let Err(error) = self.start_catalogue_pick(&mut dialog, hit) {
                            dialog.notice = format!("{error:#}");
                        }
                    } else if let Some(catalogue) = dialog.picker_catalogue() {
                        catalogue.0.borrow_mut().search.submit(Instant::now());
                    }
                    self.overlay = Overlay::ProjectSources(dialog);
                    return;
                }
                Key::Right => {
                    let opened = dialog.picker_catalogue().is_some_and(|catalogue| {
                        let mut catalogue = catalogue.0.borrow_mut();
                        catalogue.search.open_editions(index, Instant::now());
                        catalogue.search.editions().is_some()
                    });
                    if opened {
                        dialog.catalogue_view = Some(0);
                    }
                    self.overlay = Overlay::ProjectSources(dialog);
                    return;
                }
                _ => (),
            }
        }
        match &mut dialog.page {
            Page::BrainPick(target) => match key {
                Key::Char('@') if dialog.query.is_empty() => {
                    if let Err(error) = self.write_brain_reference(target, None) {
                        self.msg = format!("{error:#}");
                    }
                    return;
                }
                Key::Enter => {
                    if let Some(pick) = dialog
                        .library
                        .iter()
                        .find(|pick| current.as_ref() == Some(&pick.key))
                    {
                        match self.write_brain_reference(
                            target,
                            Some((pick.edition, text(&pick.record, "title").to_owned())),
                        ) {
                            Ok(()) => return,
                            Err(error) => dialog.notice = format!("{error:#}"),
                        }
                    }
                }
                _ => update_search(&mut dialog.query, &mut dialog.selected, key),
            },
            Page::LibraryChanges {
                changes,
                export,
                scroll,
            } => {
                match key {
                    Key::Up => *scroll = scroll.saturating_sub(1),
                    Key::Down => *scroll = scroll.saturating_add(1),
                    Key::Char('k') => {
                        changes.pop_front();
                        *scroll = 0;
                    }
                    Key::Enter => {
                        if let Some(change) = changes.front() {
                            match self.accept_library_change(&dialog.book, change) {
                                Ok(()) => {
                                    changes.pop_front();
                                    *scroll = 0;
                                    dialog.notice.clear();
                                }
                                Err(error) => dialog.notice = format!("{error:#}"),
                            }
                        }
                    }
                    _ => {}
                }
                if changes.is_empty() {
                    if *export {
                        self.open_export_ready();
                    }
                    return;
                }
            }
            Page::List => match key {
                Key::Delete => {
                    if let Some(key) = current {
                        dialog.page = Page::Remove(key);
                    }
                }
                Key::Enter => {
                    if let Some(key) = current
                        && let Some(record) =
                            dialog.snapshot.as_ref().and_then(|s| s.sources.get(&key))
                    {
                        dialog.page = form(Some((&key, record)));
                    }
                }
                _ => update_search(&mut dialog.query, &mut dialog.selected, key),
            },
            Page::Remove(key_name) if key == Key::Enter => {
                let key_name = key_name.clone();
                self.flush();
                if used_keys(&self.project).contains(&key_name) {
                    dialog.notice = format!(
                        "{key_name} is cited. Remove its citations before deleting the source."
                    );
                } else {
                    let result = (|| -> Result<()> {
                        let mut snapshot = dialog
                            .snapshot
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("The source list cannot be read."))?;
                        snapshot.sources.remove(&key_name);
                        snapshot.save(&dialog.book)
                    })();
                    match result {
                        Ok(()) => {
                            self.sources_saved();
                            return;
                        }
                        Err(e) => dialog.notice = format!("{e:#}"),
                    }
                }
            }
            Page::Remove(_) => (),
            Page::Restore {
                version,
                missing,
                scroll,
            } => match key {
                Key::Enter => {
                    self.flush();
                    let now = missing_keys(&self.project, version);
                    if now != *missing {
                        *missing = now;
                        dialog.notice =
                            "The affected citations changed. Review the list again.".into();
                    } else {
                        let result = (|| -> Result<()> {
                            let mut snapshot = dialog.snapshot.clone().ok_or_else(|| {
                                anyhow::anyhow!("The current sources cannot be read.")
                            })?;
                            if let Some(timeline) = &self.timeline {
                                anyhow::ensure!(
                                    timeline.record_now(
                                        Entry::BeforeRestoring("Project sources".into()),
                                        super::super::timeline::CHECKPOINT_WAIT
                                    ),
                                    "The current sources could not be saved in the timeline."
                                );
                            }
                            snapshot.sources = version.clone();
                            snapshot.save(&dialog.book)
                        })();
                        match result {
                            Ok(()) => {
                                self.sources_saved();
                                return;
                            }
                            Err(e) => dialog.notice = format!("{e:#}"),
                        }
                    }
                }
                Key::Up => *scroll = scroll.saturating_sub(1),
                Key::Down => *scroll = scroll.saturating_add(1),
                _ => (),
            },
            Page::Drive {
                merge,
                choices,
                index,
                scroll,
            } => match key {
                Key::Left | Key::Right => {
                    if let Some(id) = merge.conflicts().get(*index) {
                        choices.insert(id.clone(), key == Key::Right);
                        *index = (*index + 1).min(merge.conflicts().len());
                        *scroll = 0;
                    }
                }
                Key::BackTab => {
                    *index = index.saturating_sub(1);
                    *scroll = 0;
                }
                Key::Up => *scroll = scroll.saturating_sub(1),
                Key::Down => *scroll = scroll.saturating_add(1),
                Key::Enter if *index == merge.conflicts().len() => {
                    match merge.apply(&self.project.root, choices) {
                        Ok(()) => {
                            self.sources_saved();
                            return;
                        }
                        Err(e) => dialog.notice = format!("{e:#}"),
                    }
                }
                _ => (),
            },
            Page::Pick {
                target,
                chosen,
                cluster,
                field,
            } => match (chosen.as_mut(), key) {
                (None, Key::Char('@')) if dialog.query.is_empty() && cluster.is_empty() => {
                    let target = target.clone();
                    let result = self.insert_citation(&target, true, &[], &dialog);
                    if let Err(e) = result {
                        self.msg = format!("{e:#}");
                    }
                    return;
                }
                (None, Key::Enter) => {
                    if let Some(cite) = local_cite {
                        *chosen = Some(cite);
                        *field = 0;
                        dialog.notice.clear();
                    }
                }
                (None, _) => update_search(&mut dialog.query, &mut dialog.selected, key),
                (Some(_), Key::Char('e')) => edit_citation = true,
                (Some(_), Key::Tab) => *field = (*field + 1) % 3,
                (Some(_), Key::BackTab) => *field = (*field + 2) % 3,
                (Some(cite), Key::Char(' ')) if *field == 1 => cite.suppress = !cite.suppress,
                (Some(cite), Key::Enter) => {
                    if !cite.page.trim().is_empty() && !citations::valid_page(cite.page.trim()) {
                        dialog.notice = "Enter a page or range, for example 194 or 12–14.".into();
                    } else if *field == 2 {
                        cluster.push(cite.clone());
                        *chosen = None;
                        dialog.query.clear();
                        dialog.selected = 0;
                    } else {
                        let mut all = cluster.clone();
                        all.push(cite.clone());
                        pending = Some((target.clone(), all));
                    }
                }
                (Some(cite), _) if *field == 0 => edit(&mut cite.page, key, 64),
                _ => (),
            },
            Page::Form {
                original_key,
                kind,
                fields,
                field,
            } => match key {
                Key::Tab | Key::Down => *field = (*field + 1) % (FIELDS.len() + 1),
                Key::BackTab | Key::Up => *field = (*field + FIELDS.len()) % (FIELDS.len() + 1),
                Key::Left if *field == 0 && original_key.is_none() => {
                    *kind = (*kind + TYPES.len() - 1) % TYPES.len()
                }
                Key::Right if *field == 0 && original_key.is_none() => {
                    *kind = (*kind + 1) % TYPES.len()
                }
                Key::Enter => {
                    if let Some(Page::Pick { chosen: Some(cite), .. }) =
                        dialog.editing_citation.as_deref_mut()
                    {
                        let prior = cite.record(dialog.snapshot.as_ref());
                        let result = form_record(prior, *kind, fields);
                        match result {
                            Ok(record) => {
                                cite.works_cited = source_line(&record);
                                cite.edited = Some(Box::new(record));
                                dialog.page =
                                    *dialog.editing_citation.take().expect("citation edit");
                                dialog.notice.clear();
                            }
                            Err(error) => dialog.notice = format!("{error:#}"),
                        }
                        self.overlay = Overlay::ProjectSources(dialog);
                        return;
                    }
                    let result = dialog
                        .snapshot
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("The source file cannot be read."))
                        .and_then(|snapshot| {
                            let mut next = snapshot.clone();
                            let prior = original_key
                                .as_deref()
                                .and_then(|key| next.sources.get(key));
                            let mut record = form_record(prior, *kind, fields)?;
                            if let Some(key) = original_key {
                                next.sources.update(key, record)?;
                            } else {
                                if !record.contains_key("id") {
                                    record.insert(
                                        "id".into(),
                                        json!(next.sources.suggested_key(&record)),
                                    );
                                }
                                next.sources.add(record)?;
                            }
                            next.save(&dialog.book)
                        });
                    match result {
                        Ok(()) => {
                            self.sources_saved();
                            return;
                        }
                        Err(e) => dialog.notice = format!("{e:#}"),
                    }
                }
                _ if *field > 0 && !(*field == 1 && original_key.is_some()) => {
                    edit(&mut fields[*field - 1], key, 2048)
                }
                _ => (),
            },
            Page::Import {
                path,
                preview,
                scroll,
            } => match key {
                Key::Up => *scroll = scroll.saturating_sub(1),
                Key::Down => *scroll = scroll.saturating_add(1),
                Key::Enter => {
                    let result = (|| -> Result<bool> {
                        let snapshot = dialog
                            .snapshot
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("The source list cannot be read."))?;
                        if let Some((accepted, _)) = preview {
                            let mut next = snapshot.clone();
                            next.sources = accepted.clone();
                            next.save(&dialog.book)?;
                            return Ok(true);
                        }
                        let imported =
                            sources::import_preview(Path::new(path.trim()), &snapshot.sources)?;
                        let mut accepted = snapshot.sources.clone();
                        let mut summary = Vec::new();
                        for record in imported.records {
                            summary.push(format!(
                                "Add {} · {}",
                                text(&record, "id"),
                                text(&record, "title")
                            ));
                            accepted.add(record)?;
                        }
                        summary.extend(imported.notes);
                        if summary.is_empty() {
                            summary.push("No sources were found.".into());
                        }
                        *preview = Some((accepted, summary));
                        *scroll = 0;
                        Ok(false)
                    })();
                    match result {
                        Ok(true) => {
                            self.sources_saved();
                            return;
                        }
                        Ok(false) => (),
                        Err(e) => dialog.notice = format!("{e:#}"),
                    }
                }
                _ if preview.is_none() => edit(path, key, 2048),
                _ => (),
            },
        }
        if edit_citation
            && let Page::Pick { chosen: Some(cite), .. } = &dialog.page
            && let Some(record) = cite.record(dialog.snapshot.as_ref())
        {
            let page = form(Some((&cite.key, record)));
            dialog.editing_citation = Some(Box::new(std::mem::replace(&mut dialog.page, page)));
            dialog.notice.clear();
        }
        if let Some((target, citations)) = pending {
            match self.insert_citation(&target, false, &citations, &dialog) {
                Ok(()) => {
                    self.msg = "Citation inserted.".into();
                    return;
                }
                Err(e) => dialog.notice = format!("{e:#}"),
            }
        }
        if query_before != dialog.query {
            self.refresh_citation_library(&mut dialog);
            if let Some(catalogue) = dialog.picker_catalogue() {
                catalogue
                    .0
                    .borrow_mut()
                    .search
                    .edit(&dialog.query, Instant::now());
            }
        }
        dialog.selected = dialog.selected.min(dialog.result_count().saturating_sub(1));
        self.overlay = Overlay::ProjectSources(dialog);
    }

    fn sources_saved(&mut self) {
        self.deck.shell.backup.sync.saved(Instant::now());
        if let Some(timeline) = &self.timeline {
            timeline.record(Entry::Save);
        }
        self.msg = "Project sources saved.".into();
    }
}

impl SourceDialog {
    fn picker_catalogue(&self) -> Option<&CatalogueSearch> {
        // The Brain's picker receives its catalogue rows and keys in the Library task.
        self.catalogue
            .as_ref()
            .filter(|_| matches!(self.page, Page::Pick { chosen: None, .. }))
    }

    fn resolving_catalogue(&self) -> bool {
        self.catalogue
            .as_ref()
            .is_some_and(|catalogue| catalogue.0.borrow().resolving.is_some())
    }

    fn result_count(&self) -> usize {
        self.matches().len()
            + self
                .picker_catalogue()
                .map_or(0, |catalogue| catalogue.0.borrow().search.hits().len())
    }

    fn catalogue_hit(&self, index: usize) -> Option<Hit> {
        self.picker_catalogue()?
            .0
            .borrow()
            .search
            .hits()
            .get(index)
            .cloned()
    }

    fn close_catalogue_editions(&mut self) {
        if self.catalogue_view.take().is_some()
            && let Some(catalogue) = &self.catalogue
        {
            let mut catalogue = catalogue.0.borrow_mut();
            catalogue.resolving = None;
            catalogue.search.close_editions();
        }
    }

    fn tick_catalogue(&mut self, now: Instant) -> Option<Result<sources::Record>> {
        let catalogue = self.catalogue.as_ref()?;
        if matches!(self.page, Page::BrainPick(_)) {
            catalogue.0.borrow_mut().search.tick(now);
            return None;
        }
        if !matches!(self.page, Page::Pick { chosen: None, .. }) {
            return None;
        }
        let local_count = self.matches().len();
        let mut catalogue = catalogue.0.borrow_mut();
        // Open Library can arrive after Crossref. Keep the selected result in view when
        // it moves down to make room for books.
        let selected_hit = self
            .selected
            .checked_sub(local_count)
            .filter(|_| self.selected > 0)
            .and_then(|index| catalogue.search.hits().get(index))
            .cloned();
        if catalogue.search.tick(now) && self.catalogue_view.is_none() {
            let hits = catalogue.search.hits();
            if let Some(hit) = selected_hit
                && let Some(index) = hits.iter().position(|row| row == &hit)
            {
                self.selected = local_count + index;
            } else {
                self.selected = self
                    .selected
                    .min((local_count + hits.len()).saturating_sub(1));
            }
        }
        let result = match catalogue.resolving.as_ref()?.try_recv() {
            Ok(record) => Ok(record),
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("The catalogue author lookup stopped."))
            }
        };
        catalogue.resolving = None;
        Some(result)
    }

    fn matches(&self) -> Vec<(&str, &sources::Record)> {
        let query = self.query.trim().to_lowercase();
        self.snapshot
            .iter()
            .filter(|_| !matches!(self.page, Page::BrainPick(_)))
            .flat_map(|s| s.sources.iter())
            .filter(|(key, source)| {
                key.to_lowercase().contains(&query)
                    || text(source, "title").to_lowercase().contains(&query)
                    || field_value(Some(source), "author")
                        .to_lowercase()
                        .contains(&query)
            })
            .chain(
                self.library
                    .iter()
                    .map(|pick| (pick.key.as_str(), &pick.record)),
            )
            .collect()
    }

    pub fn paste(&mut self, input: &str) {
        let target = match &mut self.page {
            Page::List | Page::BrainPick(_) | Page::Pick { chosen: None, .. } => {
                Some(&mut self.query)
            }
            Page::Pick {
                chosen: Some(cite),
                field: 0,
                ..
            } => Some(&mut cite.page),
            Page::Import {
                path,
                preview: None,
                ..
            } => Some(path),
            Page::Form {
                original_key,
                fields,
                field,
                ..
            } if *field > 0 && !(*field == 1 && original_key.is_some()) => {
                Some(&mut fields[*field - 1])
            }
            _ => None,
        };
        if let Some(target) = target {
            target.extend(
                input
                    .chars()
                    .take(2048usize.saturating_sub(target.chars().count()))
                    .filter(|c| !c.is_control()),
            );
            self.selected = 0;
        }
    }
}

fn library_identity(edition: EditionKey) -> sources::LibraryIdentity {
    sources::LibraryIdentity {
        work_id: edition.work.external_id(),
        edition_id: NodeId::Note(edition.edition).external_id(),
    }
}

fn library_pick(
    project: Option<&sources::Sources>,
    edition: EditionKey,
    identity: sources::LibraryIdentity,
    bibliography: &sources::Record,
) -> Result<LibraryPick> {
    let base = format!(
        "library-{:x}-{:x}",
        edition.edition.device, edition.edition.clock
    );
    let occupied = |key: &str| project.is_some_and(|sources| sources.get(key).is_some());
    let key = if !occupied(&base) {
        base
    } else {
        (2..)
            .map(|index| format!("{base}-{index}"))
            .find(|key| !occupied(key))
            .expect("an available citation key")
    };
    let record = sources::accepted_library_record(&key, identity, bibliography)?;
    Ok(LibraryPick {
        key,
        edition,
        record,
    })
}

fn catalogue_names_path(book: &Path) -> PathBuf {
    if cfg!(test) {
        book.join(".grimoire/catalogue-names.json")
    } else {
        crate::home().join(".cache/grimoire/catalogue-names.json")
    }
}

fn resolve_open_library_names(
    mut hit: Hit,
    fetch: &dyn catalogue::Fetch,
    path: PathBuf,
) -> sources::Record {
    // A picker can be closed while its worker is running. Serialize authority lookups
    // and cache writes so the next picker can reuse any names the first one resolved.
    static CACHE: Mutex<()> = Mutex::new(());
    let _cache_guard = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
    if let Some(translators) = hit.record.get_mut("translator").and_then(Value::as_array_mut) {
        for name in translators {
            if let Some(written) = name["literal"].as_str() {
                *name = names::by_rule(written);
            }
        }
    }
    hit.record
}

fn source_line(record: &sources::Record) -> String {
    citations::mla::works_cited_line(record).unwrap_or_else(|_| text(record, "title").to_owned())
}

fn update_search(query: &mut String, selected: &mut usize, key: Key) {
    match key {
        Key::Up => *selected = selected.saturating_sub(1),
        Key::Down => *selected = selected.saturating_add(1),
        Key::Char(_) | Key::Backspace => {
            edit(query, key, 128);
            *selected = 0;
        }
        _ => (),
    }
}

fn edit(text: &mut String, key: Key, limit: usize) {
    match key {
        Key::Char(c) if !c.is_control() && text.chars().count() < limit => text.push(c),
        Key::Backspace => {
            text.pop();
        }
        _ => (),
    }
}

fn text<'a>(record: &'a sources::Record, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn used_keys(project: &Project) -> std::collections::BTreeSet<String> {
    project
        .nodes
        .iter()
        .filter(|node| node.kind == Kind::Scene && node.area != Area::Trash)
        .flat_map(|node| citations::markers(&node.body))
        .filter_map(|citation| citation.items.ok())
        .flatten()
        .map(|cite| cite.key.to_owned())
        .collect()
}

fn missing_keys(project: &Project, after: &sources::Sources) -> Vec<String> {
    used_keys(project)
        .into_iter()
        .filter(|key| after.get(key).is_none())
        .collect()
}

fn field_value(record: Option<&sources::Record>, key: &str) -> String {
    let Some(record) = record else {
        return String::new();
    };
    match key {
        "author" | "editor" => record
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|name| {
                let family = name["family"]
                    .as_str()
                    .or_else(|| name["literal"].as_str())
                    .unwrap_or_default();
                name["given"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| family.to_owned(), |given| format!("{family}, {given}"))
            })
            .collect::<Vec<_>>()
            .join("; "),
        "issued" => {
            if let Some(date) = record.get(key).and_then(|date| date["literal"].as_str()) {
                return date.to_owned();
            }
            record
                .get(key)
                .and_then(|date| date["date-parts"][0].as_array())
                .map_or_else(String::new, |parts| {
                    parts
                        .iter()
                        .enumerate()
                        .filter_map(|(i, item)| {
                            item.as_i64().map(|n| {
                                if i == 0 {
                                    n.to_string()
                                } else {
                                    format!("{n:02}")
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("-")
                })
        }
        _ => text(record, key).to_owned(),
    }
}

fn form(existing: Option<(&str, &sources::Record)>) -> Page {
    let record = existing.map(|(_, record)| record);
    Page::Form {
        original_key: existing.map(|(key, _)| key.to_owned()),
        kind: record
            .and_then(|record| {
                TYPES
                    .iter()
                    .position(|(_, csl)| *csl == text(record, "type"))
            })
            .unwrap_or(0),
        fields: FIELDS
            .iter()
            .map(|(_, key)| field_value(record, key))
            .collect(),
        field: 0,
    }
}

fn form_record(
    prior: Option<&sources::Record>,
    kind: usize,
    fields: &[String],
) -> Result<sources::Record> {
    let mut record = prior.cloned().unwrap_or_default();
    if prior.is_none() {
        record.insert("type".into(), json!(TYPES[kind].1));
    }
    anyhow::ensure!(!fields[1].trim().is_empty(), "Enter the title.");
    for ((_, key), field) in FIELDS.iter().zip(fields) {
        let field = field.trim();
        if prior.is_some() && field == field_value(prior, key) {
            continue;
        }
        if field.is_empty() {
            record.remove(*key);
            continue;
        }
        let value = match *key {
            "author" | "editor" => Value::Array(field.split(';').map(str::trim).filter(|n| !n.is_empty()).map(|name| {
                name.split_once(',').map_or_else(|| json!({"family": name}), |(family, given)| json!({"family": family.trim(), "given": given.trim()}))
            }).collect()),
            "issued" => {
                let date = field.split('-').map(str::parse::<i32>).collect::<std::result::Result<Vec<_>, _>>()?;
                anyhow::ensure!((1..=3).contains(&date.len()) && date[0] > 0
                    && date.get(1).is_none_or(|m| (1..=12).contains(m))
                    && date.get(2).is_none_or(|d| chrono::NaiveDate::from_ymd_opt(date[0], date[1] as u32, *d as u32).is_some()),
                    "Enter a date in YYYY, YYYY-MM or YYYY-MM-DD form.");
                json!({"date-parts": [date]})
            }
            _ => json!(field),
        };
        record.insert((*key).into(), value);
    }
    Ok(record)
}

fn catalogue_row(hit: &Hit, width: usize) -> String {
    let title = text(&hit.record, "title");
    let author = field_value(Some(&hit.record), "author");
    let publisher = text(&hit.record, "publisher");
    let issued = field_value(Some(&hit.record), "issued");
    let year = issued.split('-').next().unwrap_or_default();
    let isbn = text(&hit.record, "ISBN");

    let mut end = String::new();
    if !isbn.is_empty() {
        end.push_str(" · ISBN");
    } else {
        if matches!(text(&hit.record, "type"), "article" | "article-journal") {
            end.push_str(" · article");
        }
        if !text(&hit.record, "DOI").is_empty() {
            end.push_str(" · DOI");
        }
    }
    let edition_count = hit
        .editions
        .filter(|&count| count > 1)
        .map_or_else(String::new, |count| format!(" · {count} editions →"));
    let details = [title, &author, publisher, year]
        .iter()
        .map(|part| part.chars().count())
        .sum::<usize>();
    if !isbn.is_empty()
        && details + end.chars().count() + edition_count.chars().count() + isbn.len() + 9
            <= width
    {
        end.push(' ');
        end.push_str(isbn);
    }
    end.push_str(&edition_count);
    if !publisher.is_empty() || !year.is_empty() {
        let room = width
            .saturating_sub(end.chars().count() + year.chars().count() + 22)
            .max(1);
        let publisher = truncate(publisher, room);
        let details = format!("{publisher} {year}");
        end.insert_str(0, &format!(" · {}", details.trim()));
    }
    let room = width.saturating_sub(end.chars().count()).max(1);
    let head = if author.is_empty() || room < 9 {
        truncate(title, room)
    } else {
        let title = truncate(title, room - 6);
        let author = truncate(&author, room - title.chars().count() - 3);
        format!("{title} · {author}")
    };
    format!("{head}{end}")
}

fn picker_row(label: &str, selected: bool, width: usize, active: Style) -> Line<'static> {
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let label = truncate(&label, width);
    let marker = if selected { "›" } else { " " };
    Line::styled(
        format!("{marker} {label}"),
        if selected { active } else { Style::default() },
    )
}

fn catalogue_status(catalogue: &CatalogueState) -> Option<&str> {
    if catalogue.resolving.is_some() {
        return Some("Resolving author names…");
    }
    match catalogue.search.status() {
        SearchStatus::Offline => Some(catalogue::OFFLINE),
        SearchStatus::Error(error) => Some(error),
        _ => None,
    }
}

fn catalogue_header(catalogue: &CatalogueState) -> Line<'static> {
    let online = if catalogue.search.status() == &SearchStatus::Off {
        "off"
    } else {
        "on"
    };
    Line::from(format!("Catalogue · online search {online}"))
}

fn draw_editions(
    dialog: &SourceDialog,
    selected: usize,
    inner: Rect,
    active: Style,
) -> Vec<Line<'static>> {
    let Some(catalogue) = &dialog.catalogue else {
        return Vec::new();
    };
    let catalogue = catalogue.0.borrow();
    let Some((work, editions, loading)) = catalogue.search.editions() else {
        return Vec::new();
    };
    let width = usize::from(inner.width.saturating_sub(2));
    let mut lines = vec![
        catalogue_header(&catalogue),
        Line::styled(
            truncate(&format!("Editions · {}", text(&work.record, "title")), width),
            active,
        ),
    ];
    let status = catalogue_status(&catalogue).or(loading.then_some("Loading editions…"));
    let room = usize::from(inner.height)
        .saturating_sub(4 + status.is_some() as usize)
        .max(1);
    let first = (selected + 1).saturating_sub(room);
    for (i, hit) in editions.iter().enumerate().skip(first).take(room) {
        let label = catalogue_row(hit, width);
        lines.push(picker_row(&label, i == selected, width, active));
    }
    if let Some(status) = status {
        lines.push(Line::from(truncate(status, width)));
    } else if editions.is_empty() {
        lines.push(Line::from("No editions found."));
    }
    lines
}

pub(super) fn draw(f: &mut Frame, dialog: &SourceDialog, area: Rect, t: &Theme) {
    let width = if matches!(dialog.page, Page::Pick { .. }) {
        100
    } else {
        68
    };
    let r = centred(area, width.min(area.width), 20.min(area.height));
    f.render_widget(Clear, r);
    let title = match &dialog.page {
        Page::List => "Project sources",
        Page::BrainPick(_) => "Link a Library edition",
        Page::LibraryChanges { .. } => "Details changed. Update?",
        Page::Remove(_) => "Delete source",
        Page::Restore { .. } => "Restore project sources",
        Page::Drive { .. } => "Resolve source conflict",
        Page::Pick { .. } => "Cite a source",
        Page::Form {
            original_key: Some(_),
            ..
        } => "Edit source",
        Page::Form { .. } => "Add a source",
        Page::Import { .. } => "Import sources",
    };
    let block = pane_block(title, true, t);
    let inner = block.inner(r);
    f.render_widget(block, r);
    let active = Style::default().fg(t.accent);
    let mut lines = Vec::new();
    let help = match &dialog.page {
        Page::LibraryChanges {
            changes, scroll, ..
        } => {
            if let Some(change) = changes.front() {
                lines.push(Line::styled(
                    format!("{} · {} to review", change.project_key, changes.len()),
                    active,
                ));
                let fields: std::collections::BTreeSet<_> = change
                    .before
                    .keys()
                    .chain(change.proposed.keys())
                    .filter(|key| {
                        !key.starts_with("x-wros")
                            && change.before.get(*key) != change.proposed.get(*key)
                    })
                    .collect();
                let width = usize::from(inner.width);
                let mut preview = Vec::new();
                for field in fields {
                    preview.push(format!(
                        "{field} · {}",
                        field_value(Some(&change.before), field)
                    ));
                    preview.push(format!(
                        "  → {}",
                        field_value(Some(&change.proposed), field)
                    ));
                }
                if preview.is_empty() {
                    preview.push("Your local details stay as they are.".into());
                }
                let room = usize::from(inner.height.saturating_sub(5).max(1));
                let scroll = (*scroll).min(preview.len().saturating_sub(room));
                lines.extend(
                    preview
                        .iter()
                        .skip(scroll)
                        .take(room)
                        .map(|line| Line::from(truncate(line, width))),
                );
            }
            "Enter update   K keep accepted   ↑/↓ scroll   Esc keep all"
        }
        Page::Remove(key) => {
            lines.push(Line::from(format!(
                "Remove {key} from this project's sources?"
            )));
            "Enter delete   Esc cancel"
        }
        Page::Restore {
            missing, scroll, ..
        } => {
            if missing.is_empty() {
                lines.push(Line::from(
                    "Every citation in this project has a source in this version.",
                ));
            } else {
                lines.push(Line::from(
                    "This version leaves these citation keys without a source:",
                ));
                let room = inner.height.saturating_sub(5).max(1) as usize;
                let start = (*scroll).min(missing.len().saturating_sub(room));
                lines.extend(
                    missing
                        .iter()
                        .skip(start)
                        .take(room)
                        .map(|id| Line::from(id.as_str())),
                );
            }
            "↑/↓ preview   Enter restore   Esc cancel"
        }
        Page::Drive {
            merge,
            choices,
            index,
            scroll,
        } => {
            if let Some(id) = merge.conflicts().get(*index) {
                lines.push(Line::from(format!(
                    "Source {id} · {} of {}",
                    index + 1,
                    merge.conflicts().len()
                )));
                let mut preview = Vec::new();
                for (label, drive) in [("This desk", false), ("Drive", true)] {
                    preview.push(format!(
                        "{label}{}",
                        if choices.get(id) == Some(&drive) {
                            " [selected]"
                        } else {
                            ""
                        }
                    ));
                    let json = merge
                        .record(id, drive)
                        .and_then(|record| serde_json::to_string_pretty(record).ok())
                        .unwrap_or_else(|| "(deleted)".into());
                    preview.extend(json.lines().map(str::to_owned));
                }
                let room = inner.height.saturating_sub(5).max(1) as usize;
                let start = (*scroll).min(preview.len().saturating_sub(room));
                lines.extend(
                    preview
                        .iter()
                        .skip(start)
                        .take(room)
                        .map(|l| Line::from(truncate(l, inner.width as usize))),
                );
                "← keep desk   → keep Drive   ↑/↓ scroll   Esc cancel"
            } else {
                lines.push(Line::from(format!(
                    "{} conflicting sources reviewed.",
                    choices.len()
                )));
                "Enter save   Shift-Tab back   Esc cancel"
            }
        }
        Page::Pick { chosen: None, .. } if dialog.catalogue_view.is_some() => {
            lines = draw_editions(dialog, dialog.catalogue_view.unwrap_or(0), inner, active);
            "↑/↓ choose   Enter select   ←/Esc back"
        }
        Page::List | Page::BrainPick(_) | Page::Pick { chosen: None, .. } => {
            let catalogue = dialog
                .picker_catalogue()
                .map(|catalogue| catalogue.0.borrow());
            let width = usize::from(inner.width.saturating_sub(2));
            lines.push(Line::styled(format!("Find [{}]", dialog.query), active));
            if let Page::Pick { cluster, .. } = &dialog.page
                && !cluster.is_empty()
            {
                lines.push(Line::from(format!("{} in this citation", cluster.len())));
            }
            let matches = dialog.matches();
            let status = catalogue.as_deref().and_then(catalogue_status);
            let room = if catalogue.is_some() {
                usize::from(inner.height)
                    .saturating_sub(lines.len() + status.is_some() as usize + 3)
                    .max(1)
            } else {
                usize::from(inner.height.saturating_sub(6).max(1))
            };
            let first = (dialog.selected + 1).saturating_sub(room);
            let mut visible_local = 0;
            for (i, (key, record)) in matches.iter().enumerate().skip(first).take(room) {
                let label = format!(
                    "{key} · {} · {}",
                    field_value(Some(record), "author"),
                    text(record, "title")
                );
                lines.push(picker_row(&label, i == dialog.selected, width, active));
                visible_local += 1;
            }
            if matches.is_empty()
                && catalogue
                    .as_ref()
                    .is_none_or(|catalogue| catalogue.search.status() == &SearchStatus::Off)
            {
                lines.push(Line::from(
                    "No sources match. Add or import sources from Tools › Notes.",
                ));
            }
            if let Some(catalogue) = catalogue.as_deref() {
                lines.push(catalogue_header(catalogue));
                if let Some(status) = status {
                    lines.push(Line::from(truncate(status, width)));
                }
                for (i, hit) in catalogue
                    .search
                    .hits()
                    .iter()
                    .enumerate()
                    .skip(first.saturating_sub(matches.len()))
                    .take(room - visible_local)
                {
                    lines.push(picker_row(
                        &catalogue_row(hit, width),
                        i + matches.len() == dialog.selected,
                        width,
                        active,
                    ));
                }
            }
            if matches!(dialog.page, Page::List) {
                "↑/↓ choose   Enter edit   Delete remove   Esc close"
            } else if catalogue
                .as_ref()
                .is_some_and(|catalogue| catalogue.search.status() != &SearchStatus::Off)
            {
                "↑/↓ choose   → editions   Enter select   Esc cancel"
            } else {
                "↑/↓ choose   Enter select   Esc cancel"
            }
        }
        Page::Pick {
            chosen: Some(cite),
            field,
            ..
        } => {
            lines.push(Line::from(cite.key.as_str()));
            lines.push(Line::from(truncate(
                &cite.works_cited,
                usize::from(inner.width),
            )));
            for (i, label) in [
                format!("Page [{}]", cite.page),
                format!(
                    "[{}] Author named in my sentence",
                    if cite.suppress { "x" } else { " " }
                ),
                "Add another source".into(),
            ]
            .into_iter()
            .enumerate()
            {
                lines.push(Line::styled(
                    label,
                    if i == *field {
                        active
                    } else {
                        Style::default()
                    },
                ));
            }
            "Tab field   Space check   E edit   Enter confirm   Esc cancel"
        }
        Page::Form {
            original_key,
            kind,
            fields,
            field,
        } => {
            let type_name = original_key
                .as_ref()
                .and_then(|key| {
                    if let Some(Page::Pick { chosen: Some(cite), .. }) =
                        dialog.editing_citation.as_deref()
                    {
                        cite.record(dialog.snapshot.as_ref())
                    } else {
                        dialog.snapshot.as_ref()?.sources.get(key)
                    }
                })
                .map_or(TYPES[*kind].0, |record| text(record, "type"));
            lines.push(Line::styled(
                format!("Type [{type_name}]"),
                if *field == 0 {
                    active
                } else {
                    Style::default()
                },
            ));
            let room = inner.height.saturating_sub(5).max(1) as usize;
            for (i, ((label, _), entry)) in FIELDS
                .iter()
                .zip(fields)
                .enumerate()
                .skip(field.saturating_sub(room))
                .take(room)
            {
                let row = format!("{label} [{entry}]");
                lines.push(Line::styled(
                    truncate(&row, inner.width as usize),
                    if i + 1 == *field {
                        active
                    } else {
                        Style::default()
                    },
                ));
            }
            "←/→ type   Tab field   Enter save   Esc cancel"
        }
        Page::Import {
            path,
            preview,
            scroll,
        } => {
            if let Some((_, notes)) = preview {
                let room = inner.height.saturating_sub(4).max(1) as usize;
                let first = (*scroll).min(notes.len().saturating_sub(room));
                lines.extend(
                    notes
                        .iter()
                        .skip(first)
                        .take(room)
                        .map(|note| Line::from(truncate(note, inner.width as usize))),
                );
                "↑/↓ preview   Enter import   Esc cancel"
            } else {
                lines.push(Line::styled(
                    format!(".bib or CSL-JSON path [{path}]"),
                    active,
                ));
                "Enter preview   Esc cancel"
            }
        }
    };
    if !dialog.notice.is_empty() {
        lines.push(Line::styled(
            truncate(&dialog.notice, inner.width as usize),
            Style::default().fg(t.warn),
        ));
    }
    lines.push(hint_line(help, t));
    f.render_widget(Paragraph::new(lines), inner);
}
