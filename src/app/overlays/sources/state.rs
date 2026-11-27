use crate::app::outline::OutlineReferenceTarget;
use crate::catalogue::{self, Hit};
use anyhow::Result;
use grimoire_brain::{library::EditionKey, model::NodeId};
use grimoire_core::{
    citations,
    documents::Pane,
    project::{Area, Kind, Project},
    sources,
    sync::Fingerprint,
};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceDialog {
    pub(super) book: PathBuf,
    pub(super) snapshot: Option<sources::Snapshot>,
    pub(super) page: Page,
    pub(super) query: String,
    pub(super) selected: usize,
    pub(super) notice: String,
    pub(super) library: Vec<EditionPick>,
    pub(super) catalogue: Option<CatalogueSearch>,
    pub(super) catalogue_view: Option<usize>,
    pub(super) staged: BTreeSet<String>,
    pub(super) editing_citation: Option<Box<Page>>,
}

// Modal navigation can clone an overlay. Keep its in-flight search and author reply together.
#[derive(Clone)]
pub(super) struct CatalogueSearch(pub(super) Rc<RefCell<CatalogueState>>);

pub(super) struct CatalogueState {
    pub(super) search: catalogue::Search,
    pub(super) resolving: Option<mpsc::Receiver<sources::Record>>,
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
pub(super) enum Page {
    List,
    OutlinePick(OutlineReferenceTarget),
    EditionChanges {
        changes: std::collections::VecDeque<EditionChange>,
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
        catalogue_record: Option<sources::Record>,
        kind: String,
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
pub(super) struct Target {
    pub(super) document: PathBuf,
    pub(super) pane: Pane,
    pub(super) position: (usize, usize),
    pub(super) revision: Fingerprint,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Citation {
    pub(super) key: String,
    pub(super) page: String,
    pub(super) suppress: bool,
    pub(super) library: Option<Box<EditionPick>>,
    pub(super) edited: Option<Box<sources::Record>>,
    pub(super) works_cited: String,
}

impl Citation {
    pub(super) fn new(key: String, record: &sources::Record) -> Self {
        Self {
            key,
            page: String::new(),
            suppress: false,
            library: None,
            edited: None,
            works_cited: source_line(record),
        }
    }

    pub(super) fn from_library(pick: EditionPick) -> Self {
        let mut cite = Self::new(pick.key.clone(), &pick.record);
        cite.library = Some(Box::new(pick));
        cite
    }

    pub(super) fn record<'a>(
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
pub(super) struct EditionPick {
    pub(super) key: String,
    pub(super) edition: EditionKey,
    pub(super) record: sources::Record,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditionChange {
    pub(super) project_key: String,
    pub(super) edition: EditionKey,
    pub(super) before: sources::Record,
    pub(super) proposed: sources::Record,
}

pub(super) const TYPES: [(&str, &str); 8] = [
    ("Book", "book"),
    ("Chapter", "chapter"),
    ("Journal article", "article-journal"),
    ("Web page", "webpage"),
    ("Conference paper", "paper-conference"),
    ("Report", "report"),
    ("Thesis", "thesis"),
    ("Other", "document"),
];
pub(super) const FIELDS: [(&str, &str); 17] = [
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
    ("Translator (Family, Given; …)", "translator"),
    ("Place", "publisher-place"),
    ("ISBN", "ISBN"),
    ("Accessed (YYYY, YYYY-MM or YYYY-MM-DD)", "accessed"),
];
pub(super) const FORM_SEARCH: usize = 0;
pub(super) const FORM_TYPE: usize = 1;
pub(super) const FORM_FIELD_START: usize = 2;

impl SourceDialog {
    pub(super) fn picker_catalogue(&self) -> Option<&CatalogueSearch> {
        self.catalogue.as_ref().filter(|_| {
            matches!(
                self.page,
                Page::Pick { chosen: None, .. }
                    | Page::OutlinePick(_)
                    | Page::Form {
                        original_key: None,
                        field: FORM_SEARCH,
                        ..
                    }
            )
        })
    }

    pub(super) fn resolving_catalogue(&self) -> bool {
        self.catalogue
            .as_ref()
            .is_some_and(|catalogue| catalogue.0.borrow().resolving.is_some())
    }

    pub(super) fn result_count(&self) -> usize {
        self.matches().len()
            + self
                .picker_catalogue()
                .map_or(0, |catalogue| catalogue.0.borrow().search.hits().len())
    }

    pub(super) fn catalogue_hit(&self, index: usize) -> Option<Hit> {
        self.picker_catalogue()?
            .0
            .borrow()
            .search
            .hits()
            .get(index)
            .cloned()
    }

    pub(super) fn matches(&self) -> Vec<(&str, &sources::Record)> {
        if matches!(self.page, Page::Form { .. }) {
            return Vec::new();
        }
        let query = self.query.trim().to_lowercase();
        self.snapshot
            .iter()
            .filter(|_| !matches!(self.page, Page::OutlinePick(_)))
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
}

pub(super) fn library_identity(edition: EditionKey) -> sources::LibraryIdentity {
    sources::LibraryIdentity {
        work_id: edition.work.external_id(),
        edition_id: NodeId::Note(edition.edition).external_id(),
    }
}

pub(super) fn library_pick(
    project: Option<&sources::Sources>,
    edition: EditionKey,
    identity: sources::LibraryIdentity,
    bibliography: &sources::Record,
) -> Result<EditionPick> {
    let base = format!(
        "library-{:x}-{:x}",
        edition.edition.device, edition.edition.clock
    );
    let occupied = |key: &str| project.is_some_and(|sources| sources.get(key).is_some());
    let key = if !occupied(&base) {
        base
    } else {
        let mut suffix = 2usize;
        loop {
            let key = format!("{base}-{suffix}");
            if !occupied(&key) {
                break key;
            }
            suffix += 1;
        }
    };
    let record = sources::accepted_library_record(&key, identity, bibliography)?;
    Ok(EditionPick {
        key,
        edition,
        record,
    })
}

pub(super) fn source_line(record: &sources::Record) -> String {
    citations::mla::works_cited_line(record).unwrap_or_else(|_| text(record, "title").to_owned())
}

pub(super) fn text<'a>(record: &'a sources::Record, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn used_keys(project: &Project) -> std::collections::BTreeSet<String> {
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

pub(super) fn missing_keys(project: &Project, after: &sources::Sources) -> Vec<String> {
    used_keys(project)
        .into_iter()
        .filter(|key| after.get(key).is_none())
        .collect()
}

pub(super) fn field_value(record: Option<&sources::Record>, key: &str) -> String {
    let Some(record) = record else {
        return String::new();
    };
    match key {
        "author" | "editor" | "translator" => record
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
        "issued" | "accessed" => {
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

pub(super) fn form(existing: Option<(&str, &sources::Record)>) -> Page {
    let (key, record) = existing.map_or(("", None), |(key, record)| (key, Some(record)));
    Page::Form {
        original_key: (!key.is_empty()).then(|| key.to_owned()),
        catalogue_record: record.filter(|_| key.is_empty()).cloned(),
        kind: record
            .map_or(TYPES[0].1, |record| text(record, "type"))
            .to_owned(),
        fields: FIELDS
            .iter()
            .map(|(_, key)| field_value(record, key))
            .collect(),
        field: if record.is_some() {
            FORM_TYPE
        } else {
            FORM_SEARCH
        },
    }
}

pub(super) fn form_record(
    prior: Option<&sources::Record>,
    kind: &str,
    fields: &[String],
) -> Result<sources::Record> {
    let mut record = prior.cloned().unwrap_or_default();
    record.insert("type".into(), json!(kind));
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
            "author" | "editor" | "translator" => Value::Array(
                field
                    .split(';')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(|name| {
                        name.split_once(',').map_or_else(
                            || json!({"family": name}),
                            |(family, given)| {
                                json!({"family": family.trim(), "given": given.trim()})
                            },
                        )
                    })
                    .collect(),
            ),
            "issued" | "accessed" => {
                let date = field
                    .split('-')
                    .map(str::parse::<i32>)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                anyhow::ensure!(
                    (1..=3).contains(&date.len())
                        && date[0] > 0
                        && date.get(1).is_none_or(|m| (1..=12).contains(m))
                        && date.get(2).is_none_or(|&day| {
                            chrono::NaiveDate::from_ymd_opt(date[0], date[1] as u32, day as u32)
                                .is_some()
                        }),
                    "Enter a date in YYYY, YYYY-MM or YYYY-MM-DD form."
                );
                json!({"date-parts": [date]})
            }
            _ => json!(field),
        };
        record.insert((*key).into(), value);
    }
    Ok(record)
}
