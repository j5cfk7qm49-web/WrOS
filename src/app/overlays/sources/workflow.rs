use super::state::{
    CatalogueSearch, CatalogueState, Citation, EditionChange, Page, SourceDialog, Target, form,
    missing_keys,
};
use crate::app::{App, HistoryTimeline, Overlay, outline::OutlineReferenceTarget};
use anyhow::Result;
use grimoire_brain::{library::EditionKey, model::NodeId};
use grimoire_core::{citations, project::Area, sources, sync::Fingerprint, timeline::Entry};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

impl App {
    fn sources_dialog(&mut self, page: Page) {
        let book = self.project.root.clone();
        let (snapshot, notice) = if matches!(page, Page::OutlinePick(_)) {
            (None, String::new())
        } else {
            match sources::Snapshot::open(&book) {
                Ok(snapshot) => (Some(snapshot), String::new()),
                Err(e) => (None, format!("{e:#}")),
            }
        };
        let catalogue = matches!(
            page,
            Page::Pick { .. }
                | Page::OutlinePick(_)
                | Page::Form {
                    original_key: None,
                    ..
                }
        )
        .then(|| {
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

    pub(in crate::app::overlays) fn preview_sources_restore(&mut self, text: &str) {
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

    pub(crate) fn open_outline_edition_picker(&mut self, target: OutlineReferenceTarget) {
        self.sources_dialog(Page::OutlinePick(target));
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
        let Ok(Some(store)) = self.commonplace_store_if_present() else {
            return false;
        };
        let mut changes = std::collections::VecDeque::new();
        for (key, record, edition) in linked {
            if let Ok(Some(current)) = store.edition(edition)
                && let Ok(Some(proposed)) =
                    sources::proposed_library_update(record, &current.bibliography)
            {
                changes.push_back(EditionChange {
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
        self.sources_dialog(Page::EditionChanges {
            changes,
            export,
            scroll: 0,
        });
        true
    }

    pub(super) fn accept_library_change(
        &mut self,
        book: &Path,
        change: &EditionChange,
    ) -> Result<()> {
        anyhow::ensure!(self.project.root == book, "The open project changed");
        let mut sources_file = sources::Snapshot::open(book)?;
        anyhow::ensure!(
            sources_file.sources.get(&change.project_key) == Some(&change.before),
            "The project source changed. Open the review again."
        );
        let current = self
            .commonplace_store_if_present()?
            .ok_or_else(|| anyhow::anyhow!("Commonplace is unavailable"))?
            .edition(change.edition)
            .map_err(anyhow::Error::msg)?
            .ok_or_else(|| anyhow::anyhow!("The edition disappeared from Commonplace"))?;
        anyhow::ensure!(
            sources::proposed_library_update(&change.before, &current.bibliography)?.as_ref()
                == Some(&change.proposed),
            "The Commonplace edition changed. Open the review again."
        );
        sources_file
            .sources
            .update(&change.project_key, change.proposed.clone())?;
        sources_file.save(book)?;
        self.sources_saved();
        Ok(())
    }

    pub(super) fn citation_target(&self, target: &Target, book: &Path) -> Result<usize> {
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

    pub(super) fn insert_citation(
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
                        anyhow::anyhow!("The Commonplace record has no accepted details")
                    })?;
                    let current = self
                        .commonplace_store_if_present()?
                        .ok_or_else(|| anyhow::anyhow!("Commonplace is unavailable"))?
                        .edition(pick.edition)
                        .map_err(anyhow::Error::msg)?
                        .ok_or_else(|| anyhow::anyhow!("The Commonplace edition is unavailable"))?;
                    anyhow::ensure!(
                        sources::library_fingerprint(&current.bibliography)?
                            == accepted.fingerprint,
                        "The Commonplace edition changed. Open the picker again."
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
                        "Enter a page (194), a range (12–14) or a list (54, 56)."
                    );
                    let label = if page.contains([',', '-', '–', '—']) {
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

    pub(super) fn sources_saved(&mut self) {
        self.deck.shell.backup.sync.saved(Instant::now());
        if let Some(timeline) = &self.timeline {
            timeline.record(Entry::Save);
        }
        self.msg = "Project sources saved.".into();
    }
}
