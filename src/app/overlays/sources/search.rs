use super::state::{
    Citation, EditionPick, Page, SourceDialog, form, library_identity, library_pick, text,
};
use crate::app::{
    App, Overlay,
    read::{CataloguePick, keep_in_commonplace},
};
use crate::catalogue::Hit;
use anyhow::{Context, Result};
use grimoire_core::sources;
use serde_json::Value;
use std::sync::mpsc;
use std::time::Instant;

impl App {
    pub(super) fn refresh_citation_library(&mut self, dialog: &mut SourceDialog) {
        let outline = matches!(dialog.page, Page::OutlinePick(_));
        if !outline && !matches!(dialog.page, Page::Pick { .. }) {
            return;
        }
        dialog.library.clear();
        let project = dialog.snapshot.as_ref().map(|snapshot| &snapshot.sources);
        if !outline && project.is_none() {
            return;
        }
        let Ok(Some(store)) = self.commonplace_store_if_present() else {
            return;
        };
        let query = dialog.query.trim();
        let Ok(editions) = store.find_editions(query, 48) else {
            return;
        };
        let linked: std::collections::BTreeSet<_> = project
            .into_iter()
            .filter(|_| !outline)
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
        match result {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => dialog.notice = format!("{error:#}"),
        }
        self.overlay = Overlay::ProjectSources(dialog);
    }

    pub(super) fn on_catalogue_pick(&mut self, dialog: &mut SourceDialog, hit: Hit) -> bool {
        let result =
            CataloguePick::start(hit, &self.catalogue_fetch, &dialog.book).and_then(|pick| {
                match pick {
                    CataloguePick::Ready(record) => self.finish_catalogue_pick(dialog, record),
                    CataloguePick::Resolving(receiver) => {
                        let catalogue = dialog
                            .catalogue
                            .as_ref()
                            .context("Catalogue search unavailable")?;
                        catalogue.0.borrow_mut().resolving = Some(receiver);
                        dialog.notice.clear();
                        Ok(false)
                    }
                }
            });
        match result {
            Ok(close) => close,
            Err(error) => {
                dialog.notice = format!("{error:#}");
                false
            }
        }
    }

    // An Outline link completes the picker immediately. Citation and form picks advance a step.
    fn finish_catalogue_pick(
        &mut self,
        dialog: &mut SourceDialog,
        mut record: sources::Record,
    ) -> Result<bool> {
        if let Page::OutlinePick(target) = &dialog.page {
            let store = self
                .commonplace_store_if_present()?
                .ok_or_else(|| anyhow::anyhow!("Commonplace is unavailable"))?;
            let edition = keep_in_commonplace(store, &record)?;
            self.write_outline_reference(
                target,
                Some((edition, text(&record, "title").to_owned())),
            )?;
            return Ok(true);
        }
        if matches!(
            dialog.page,
            Page::Form {
                original_key: None,
                ..
            }
        ) {
            dialog.close_catalogue_editions();
            dialog.page = form(Some(("", &record)));
            dialog.query.clear();
            dialog.selected = 0;
            if let Some(catalogue) = &dialog.catalogue {
                catalogue.0.borrow_mut().search.edit("", Instant::now());
            }
            dialog.notice.clear();
            return Ok(false);
        }
        let Page::Pick { target, .. } = &dialog.page else {
            return Ok(false);
        };
        let library = if let Ok(Some(store)) = self.commonplace_store_if_present() {
            let edition = keep_in_commonplace(store, &record)?;
            let metadata = store
                .edition(edition)
                .map_err(anyhow::Error::msg)?
                .ok_or_else(|| anyhow::anyhow!("The Commonplace edition is unavailable"))?;
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
                EditionPick {
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
        Ok(false)
    }
}

impl SourceDialog {
    pub(super) fn close_catalogue_editions(&mut self) {
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
        let local_count = match self.page {
            Page::Pick { chosen: None, .. } | Page::OutlinePick(_) => self.matches().len(),
            Page::Form {
                original_key: None, ..
            } => 0,
            _ => return None,
        };
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
}
