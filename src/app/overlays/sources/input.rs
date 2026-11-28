use super::state::{
    Citation, FIELDS, FORM_FIELD_START, FORM_SEARCH, FORM_TYPE, Page, SourceDialog, TYPES, form,
    form_record, library_identity, missing_keys, source_line, text, used_keys,
};
use crate::app::{App, Key, Overlay, read::keep_in_commonplace, timeline::CHECKPOINT_WAIT};
use anyhow::{Context, Result};
use grimoire_core::{citations, sources, timeline::Entry};
use serde_json::Value;
use std::path::Path;
use std::time::Instant;

impl App {
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

    fn on_catalogue_editions_key(&mut self, dialog: &mut SourceDialog, key: Key) -> bool {
        if matches!(key, Key::Left | Key::Esc) {
            dialog.close_catalogue_editions();
            dialog.notice.clear();
            return false;
        }
        let (Some(catalogue), Some(selected)) = (&dialog.catalogue, &mut dialog.catalogue_view)
        else {
            return false;
        };
        let mut catalogue = catalogue.0.borrow_mut();
        if catalogue.resolving.is_some() {
            return false;
        }
        let Some((_, editions, _)) = catalogue.search.editions() else {
            return false;
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
                    return self.on_catalogue_pick(dialog, hit);
                } else {
                    catalogue.search.more_editions(Instant::now());
                }
            }
            _ => (),
        }
        false
    }

    pub(in crate::app::overlays) fn on_project_sources_key(&mut self, key: Key) {
        let Overlay::ProjectSources(mut dialog) =
            std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        if dialog.catalogue_view.is_some() {
            if !self.on_catalogue_editions_key(&mut dialog, key) {
                self.overlay = Overlay::ProjectSources(dialog);
            }
            return;
        }
        if key == Key::Esc {
            if let Some(page) = dialog.editing_citation.take() {
                dialog.page = *page;
                dialog.notice.clear();
                self.overlay = Overlay::ProjectSources(dialog);
                return;
            }
            if matches!(dialog.page, Page::EditionChanges { export: true, .. }) {
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
                        if self.on_catalogue_pick(&mut dialog, hit) {
                            return;
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
            Page::OutlinePick(target) => match key {
                Key::Char('@') if dialog.query.is_empty() => {
                    if let Err(error) = self.write_outline_reference(target, None) {
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
                        match self.write_outline_reference(
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
            Page::EditionChanges {
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
                                        CHECKPOINT_WAIT
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
                        dialog.notice =
                            "Enter a page (194), a range (12–14) or a list (54, 56).".into();
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
                catalogue_record,
                kind,
                fields,
                field,
            } => match key {
                Key::Up | Key::Down | Key::Char(_) | Key::Backspace if *field == FORM_SEARCH => {
                    update_search(&mut dialog.query, &mut dialog.selected, key);
                }
                Key::Tab | Key::Down => {
                    *field = if *field == FIELDS.len() + 1 {
                        usize::from(original_key.is_some())
                    } else {
                        *field + 1
                    };
                }
                Key::BackTab | Key::Up => {
                    *field = if *field == usize::from(original_key.is_some()) {
                        FIELDS.len() + 1
                    } else {
                        *field - 1
                    };
                }
                Key::Left | Key::Right if *field == FORM_TYPE && original_key.is_none() => {
                    let index = TYPES.iter().position(|(_, csl)| *csl == kind.as_str());
                    let next = if key == Key::Left {
                        (index.unwrap_or(0) + TYPES.len() - 1) % TYPES.len()
                    } else {
                        (index.unwrap_or(TYPES.len() - 1) + 1) % TYPES.len()
                    };
                    *kind = TYPES[next].1.to_owned();
                }
                Key::Enter => {
                    if let Some(Page::Pick {
                        chosen: Some(cite), ..
                    }) = dialog.editing_citation.as_deref_mut()
                    {
                        let prior = cite.record(dialog.snapshot.as_ref());
                        let result = form_record(prior, kind, fields);
                        match result {
                            Ok(record) => {
                                cite.works_cited = source_line(&record);
                                cite.edited = Some(Box::new(record));
                                if let Some(page) = dialog.editing_citation.take() {
                                    dialog.page = *page;
                                    dialog.notice.clear();
                                }
                            }
                            Err(error) => dialog.notice = format!("{error:#}"),
                        }
                        self.overlay = Overlay::ProjectSources(dialog);
                        return;
                    }
                    let result = (|| -> Result<()> {
                        let mut next = dialog
                            .snapshot
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("The source file cannot be read."))?;
                        let prior = original_key
                            .as_deref()
                            .and_then(|key| next.sources.get(key))
                            .or(catalogue_record.as_ref());
                        let mut record = form_record(prior, kind, fields)?;
                        if let Some(key) = original_key {
                            next.sources.update(key, record)?;
                        } else {
                            let key = record
                                .get("id")
                                .and_then(Value::as_str)
                                .map_or_else(|| next.sources.suggested_key(&record), str::to_owned);
                            record.insert("id".into(), Value::String(key.clone()));
                            next.sources.add(record)?;
                            if catalogue_record.is_some()
                                && let Ok(Some(store)) = self.commonplace_store_if_present()
                            {
                                let record = next
                                    .sources
                                    .get(&key)
                                    .context("New bibliography record unavailable")?;
                                let edition = keep_in_commonplace(store, record)?;
                                let accepted = sources::accepted_library_record(
                                    &key,
                                    library_identity(edition),
                                    record,
                                )?;
                                next.sources.update(&key, accepted)?;
                            }
                        }
                        next.save(&dialog.book)
                    })();
                    match result {
                        Ok(()) => {
                            self.sources_saved();
                            return;
                        }
                        Err(e) => dialog.notice = format!("{e:#}"),
                    }
                }
                _ if *field >= FORM_FIELD_START
                    && !(*field == FORM_FIELD_START && original_key.is_some()) =>
                {
                    edit(&mut fields[*field - FORM_FIELD_START], key, 2048);
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
            && let Page::Pick {
                chosen: Some(cite), ..
            } = &dialog.page
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
}

impl SourceDialog {
    fn paste(&mut self, input: &str) {
        let target = match &mut self.page {
            Page::List
            | Page::OutlinePick(_)
            | Page::Pick { chosen: None, .. }
            | Page::Form {
                original_key: None,
                field: FORM_SEARCH,
                ..
            } => Some(&mut self.query),
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
            } if *field >= FORM_FIELD_START
                && !(*field == FORM_FIELD_START && original_key.is_some()) =>
            {
                Some(&mut fields[*field - FORM_FIELD_START])
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
