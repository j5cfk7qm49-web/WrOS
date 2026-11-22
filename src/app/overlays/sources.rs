//! The project bibliography and the keyboard citation picker.

use super::*;
use grimoire_core::sync::Fingerprint;
use grimoire_core::{citations, sources};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceDialog {
    book: PathBuf,
    snapshot: Option<sources::Snapshot>,
    page: Page,
    query: String,
    selected: usize,
    notice: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Page {
    List,
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
        let (snapshot, notice) = match sources::Snapshot::open(&book) {
            Ok(snapshot) => (Some(snapshot), String::new()),
            Err(e) => (None, format!("{e:#}")),
        };
        self.overlay = Overlay::ProjectSources(Box::new(SourceDialog {
            book,
            snapshot,
            page,
            query: String::new(),
            selected: 0,
            notice,
        }));
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

    fn insert_citation(
        &mut self,
        target: &Target,
        literal: bool,
        picked: &[Citation],
        from: &SourceDialog,
    ) -> Result<()> {
        let i = self
            .open
            .filter(|&i| {
                self.focus.is_editor()
                    && self.views.active == target.pane
                    && self.project.root == from.book
                    && self.project.nodes[i].path == target.document
                    && (self.editor.cy, self.editor.cx) == target.position
                    && !self.editor.has_selection()
                    && Fingerprint::of(&self.editor.text()) == target.revision
            })
            .ok_or_else(|| {
                anyhow::anyhow!("The writing cursor or text changed. Open the picker again.")
            })?;
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
            let on_disk = sources::Snapshot::open(&from.book)?;
            let previous = &from
                .snapshot
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("The project source list is unavailable."))?
                .sources;
            let mut items = Vec::new();
            for citation in picked {
                anyhow::ensure!(
                    on_disk.sources.get(&citation.key).is_some()
                        && on_disk.sources.get(&citation.key) == previous.get(&citation.key),
                    "The project source changed. Open the picker again."
                );
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
        if key == Key::Esc {
            return;
        }
        let mut pending = None;
        let current = dialog
            .matches()
            .get(dialog.selected)
            .map(|(key, _)| (*key).to_owned());
        match &mut dialog.page {
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
                    if let Some(key) = current {
                        *chosen = Some(Citation {
                            key,
                            page: String::new(),
                            suppress: false,
                        });
                        *field = 0;
                    }
                }
                (None, _) => update_search(&mut dialog.query, &mut dialog.selected, key),
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
        if let Some((target, citations)) = pending {
            match self.insert_citation(&target, false, &citations, &dialog) {
                Ok(()) => {
                    self.msg = "Citation inserted.".into();
                    return;
                }
                Err(e) => dialog.notice = format!("{e:#}"),
            }
        }
        dialog.selected = dialog
            .selected
            .min(dialog.matches().len().saturating_sub(1));
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
    fn matches(&self) -> Vec<(&str, &sources::Record)> {
        let query = self.query.trim().to_lowercase();
        self.snapshot
            .iter()
            .flat_map(|s| s.sources.iter())
            .filter(|(key, source)| {
                key.to_lowercase().contains(&query)
                    || text(source, "title").to_lowercase().contains(&query)
                    || field_value(Some(source), "author")
                        .to_lowercase()
                        .contains(&query)
            })
            .collect()
    }

    pub fn paste(&mut self, input: &str) {
        let target = match &mut self.page {
            Page::List | Page::Pick { chosen: None, .. } => Some(&mut self.query),
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

pub(super) fn draw(f: &mut Frame, dialog: &SourceDialog, area: Rect, t: &Theme) {
    let r = centred(area, 68.min(area.width), 20.min(area.height));
    f.render_widget(Clear, r);
    let title = match &dialog.page {
        Page::List => "Project sources",
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
        Page::List | Page::Pick { chosen: None, .. } => {
            lines.push(Line::styled(format!("Find [{}]", dialog.query), active));
            if let Page::Pick { cluster, .. } = &dialog.page
                && !cluster.is_empty()
            {
                lines.push(Line::from(format!("{} in this citation", cluster.len())));
            }
            let matches = dialog.matches();
            let room = inner.height.saturating_sub(6).max(1) as usize;
            let first = (dialog.selected + 1).saturating_sub(room);
            for (i, (key, record)) in matches.iter().enumerate().skip(first).take(room) {
                let label = format!(
                    "{key} · {} · {}",
                    field_value(Some(record), "author"),
                    text(record, "title")
                );
                lines.push(Line::styled(
                    format!(
                        "{} {}",
                        if i == dialog.selected { "›" } else { " " },
                        truncate(&label, inner.width.saturating_sub(3) as usize)
                    ),
                    if i == dialog.selected {
                        active
                    } else {
                        Style::default()
                    },
                ));
            }
            if matches.is_empty() {
                lines.push(Line::from(
                    "No sources match. Add or import sources from Tools › Notes.",
                ));
            }
            if matches!(dialog.page, Page::List) {
                "↑/↓ choose   Enter edit   Delete remove   Esc close"
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
            "Tab field   Space check   Enter confirm   Esc cancel"
        }
        Page::Form {
            original_key,
            kind,
            fields,
            field,
        } => {
            let type_name = original_key
                .as_ref()
                .and_then(|key| dialog.snapshot.as_ref()?.sources.get(key))
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
