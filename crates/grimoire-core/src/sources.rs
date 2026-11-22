//! Project-local CSL-JSON sources. Records retain unknown fields for round trips.

use anyhow::{Context, Result, bail};
use biblatex::{
    Bibliography, ChunksExt, Datetime, EntryType, PermissiveType, Person, RetrievalError,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

use crate::sync::{self, Fingerprint, SaveOutcome};

pub const FILENAME: &str = "sources.json";
const MAX_BYTES: usize = 4 * 1024 * 1024;

pub type Record = Map<String, Value>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sources(BTreeMap<String, Record>);

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub sources: Sources,
    seen: Option<Fingerprint>,
}

impl Snapshot {
    pub fn open(project: &Path) -> Result<Self> {
        let path = project.join(FILENAME);
        match read(&path) {
            Ok(text) => Ok(Self {
                sources: Sources::parse(&text).with_context(|| format!("reading {FILENAME}"))?,
                seen: Some(Fingerprint::of(&text)),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self {
                sources: Sources::default(),
                seen: None,
            }),
            Err(e) => Err(e).with_context(|| format!("reading {FILENAME}")),
        }
    }

    /// Save the entire edited list only if the loaded file is still current.
    pub fn save(&mut self, project: &Path) -> Result<()> {
        if !parked_sources(project)?.is_empty() {
            bail!("Project sources have a Drive conflict. Settle it before saving.");
        }
        self.save_version(project)
    }

    fn save_version(&mut self, project: &Path) -> Result<()> {
        let path = project.join(FILENAME);
        // Refuse aliases and non-regular files using the same check as open.
        match read(&path) {
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e).with_context(|| format!("checking {FILENAME}")),
        }
        let encoded = self.sources.to_json()?;
        match sync::save_guarded(&path, &encoded, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&encoded));
                Ok(())
            }
            SaveOutcome::Conflict { .. } => {
                bail!("Project sources changed on disk. Reopen them before saving.")
            }
        }
    }
}

pub fn parked_sources(project: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(project)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && is_parked_source(&path) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn is_parked_source(path: &Path) -> bool {
    if path.extension().is_none_or(|ext| ext != "json") {
        return false;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.strip_prefix("sources (from Drive, "))
        .and_then(|suffix| suffix.split_once(')'))
        .is_some_and(|(time, repeat)| {
            chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H-%M").is_ok()
                && (repeat.is_empty()
                    || repeat
                        .strip_prefix(' ')
                        .is_some_and(|n| n.parse::<usize>().is_ok()))
        })
}

/// Apply a merge only when the Drive copy and both source stores are readable.
/// The original disagreement stays available in the Trash after settlement.
pub fn merge_drive(project: &Path, parked: &Path, base: &Path) -> Result<()> {
    let change = DriveMerge::open(project, parked, base)?;
    if !change.conflicts().is_empty() {
        bail!(
            "Drive and this desk changed the same sources: {}.",
            change.conflicts().join(", ")
        );
    }
    change.apply(project, &BTreeMap::new())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DriveMerge {
    project: PathBuf,
    parked: PathBuf,
    incoming_fingerprint: Fingerprint,
    mine: Snapshot,
    theirs: Sources,
    base: Sources,
    conflicts: Vec<String>,
}

impl DriveMerge {
    pub fn open(project: &Path, parked: &Path, base: &Path) -> Result<Self> {
        if !parked_sources(project)?.iter().any(|p| p == parked) {
            bail!("The Drive copy is no longer available.");
        }
        let mine = Snapshot::open(project)?;
        let text = read(parked)?;
        let theirs = Sources::parse(&text)?;
        let base = Sources::parse(&read(base)?)?;
        let conflicts = Sources::merge(&base, &mine.sources, &theirs)
            .err()
            .unwrap_or_default();
        Ok(Self {
            project: project.to_owned(),
            parked: parked.to_owned(),
            incoming_fingerprint: Fingerprint::of(&text),
            mine,
            theirs,
            base,
            conflicts,
        })
    }

    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }

    pub fn record(&self, key: &str, use_drive: bool) -> Option<&Record> {
        (if use_drive {
            &self.theirs
        } else {
            &self.mine.sources
        })
        .get(key)
    }

    pub fn apply(&self, project: &Path, choose_drive: &BTreeMap<String, bool>) -> Result<()> {
        anyhow::ensure!(
            self.project == project,
            "Open the original project to settle its sources."
        );
        anyhow::ensure!(
            read(&self.parked).map(|text| Fingerprint::of(&text))? == self.incoming_fingerprint,
            "The Drive copy changed. Reopen the conflict before saving."
        );
        let mut mine = self.mine.clone();
        let mut theirs = self.theirs.clone();
        let mut base = self.base.clone();
        for key in &self.conflicts {
            let use_drive = *choose_drive
                .get(key)
                .context("Choose which version to keep for each conflicting source.")?;
            let preferred = self.record(key, use_drive);
            for sources in [&mut mine.sources, &mut theirs, &mut base] {
                if let Some(record) = preferred {
                    sources.0.insert(key.clone(), record.clone());
                } else {
                    sources.0.remove(key);
                }
            }
        }
        mine.sources = Sources::merge(&base, &mine.sources, &theirs)
            .map_err(|_| anyhow::anyhow!("The source conflict needs another review."))?;
        mine.save_version(project)?;
        crate::project::trash(project, &self.parked)?;
        Ok(())
    }
}

impl Sources {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_BYTES {
            bail!("Project sources exceed 4 MiB.");
        }
        let records: Vec<Record> = serde_json::from_str(text)?;
        let mut list = Self::default();
        for record in records {
            list.add(record)?;
        }
        Ok(list)
    }

    pub fn get(&self, key: &str) -> Option<&Record> {
        self.0.get(key)
    }

    pub fn suggested_key(&self, record: &Record) -> String {
        let author = record.get("author").and_then(|names| names.get(0));
        let label = author
            .and_then(|name| name["family"].as_str().or_else(|| name["literal"].as_str()))
            .or_else(|| record.get("title").and_then(Value::as_str))
            .unwrap_or_default();
        let mut stem: String = label
            .nfkd()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(36)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if stem.is_empty() {
            stem.push_str("source");
        }
        if let Some(year) = record
            .get("issued")
            .and_then(|date| date["date-parts"][0][0].as_i64())
        {
            stem.push_str(&year.to_string());
        }
        if self.get(&stem).is_none() {
            return stem;
        }
        (2..)
            .map(|i| format!("{stem}-{i}"))
            .find(|key| self.get(key).is_none())
            .unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Record)> {
        self.0.iter().map(|(key, record)| (key.as_str(), record))
    }

    pub fn add(&mut self, record: Record) -> Result<()> {
        let key = validate(&record)?;
        if self.0.contains_key(key) {
            bail!("A source already uses the key {key}.");
        }
        self.0.insert(key.to_owned(), record);
        Ok(())
    }

    pub fn update(&mut self, key: &str, record: Record) -> Result<()> {
        if validate(&record)? != key {
            bail!("A source key stays the same when editing the record.");
        }
        let Some(existing) = self.0.get_mut(key) else {
            bail!("No project source uses the key {key}.");
        };
        *existing = record;
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> Option<Record> {
        self.0.remove(key)
    }

    pub fn to_json(&self) -> Result<String> {
        let records: Vec<_> = self.0.values().collect();
        let mut encoded = serde_json::to_string_pretty(&records)?;
        encoded.push('\n');
        if encoded.len() > MAX_BYTES {
            bail!("Project sources exceed 4 MiB.");
        }
        Ok(encoded)
    }

    /// Merge independent source edits. The caller can ask the writer about
    /// conflicting keys while retaining all three original files.
    pub fn merge(base: &Self, mine: &Self, theirs: &Self) -> Result<Self, Vec<String>> {
        let keys: BTreeSet<_> = base
            .0
            .keys()
            .chain(mine.0.keys())
            .chain(theirs.0.keys())
            .collect();
        let mut merged = Self::default();
        let mut conflicts = Vec::new();
        for key in keys {
            let (base, mine, theirs) = (base.get(key), mine.get(key), theirs.get(key));
            let accepted = if mine == theirs || theirs == base {
                mine
            } else if mine == base {
                theirs
            } else {
                conflicts.push(key.clone());
                continue;
            };
            if let Some(record) = accepted {
                merged.0.insert(key.clone(), record.clone());
            }
        }
        if conflicts.is_empty() {
            Ok(merged)
        } else {
            Err(conflicts)
        }
    }
}

pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn validate(record: &Record) -> Result<&str> {
    let key = record
        .get("id")
        .and_then(Value::as_str)
        .context("A source needs a text ID.")?;
    if !valid_key(key) {
        bail!("Source keys use letters, digits, hyphens and underscores.");
    }
    if !record
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| !t.trim().is_empty())
    {
        bail!("Source {key} needs a type.");
    }
    Ok(key)
}

fn read(path: &Path) -> io::Result<String> {
    if !path.symlink_metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a regular file.",
        ));
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    if text.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Source files are limited to 4 MiB.",
        ));
    }
    Ok(text)
}

#[derive(Debug, Default)]
pub struct ImportPreview {
    pub records: Vec<Record>,
    pub notes: Vec<String>,
}

pub fn import_preview(path: &Path, existing: &Sources) -> Result<ImportPreview> {
    let text = read(path)?;
    let mut preview = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bib"))
    {
        import_bib(&text)?
    } else {
        ImportPreview {
            records: serde_json::from_str(&text).context("Read a CSL-JSON array of sources.")?,
            notes: Vec::new(),
        }
    };
    let mut staged = existing.clone();
    preview.records.retain_mut(|record| {
        let old = record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut key: String = old
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if key.is_empty() {
            key = "source".to_owned();
        }
        if staged.get(&key) == Some(record) {
            preview
                .notes
                .push(format!("{key} already matches a project source."));
            return false;
        }
        let stem = key.clone();
        for n in 2.. {
            if staged.get(&key).is_none() {
                break;
            }
            key = format!("{stem}-{n}");
        }
        if old != key {
            preview.notes.push(format!(
                "Use key {key} for {}.",
                if old.is_empty() {
                    "an unnamed source"
                } else {
                    &old
                }
            ));
            record.insert("id".into(), json!(key));
        }
        match staged.add(record.clone()) {
            Ok(()) => true,
            Err(e) => {
                preview.notes.push(format!("{key}: {e}"));
                false
            }
        }
    });
    Ok(preview)
}

fn import_bib(text: &str) -> Result<ImportPreview> {
    let bib = Bibliography::parse(text)?;
    let mut preview = ImportPreview::default();
    for entry in bib.iter() {
        let ty = match entry.entry_type {
            EntryType::Book | EntryType::Collection => "book",
            EntryType::InBook | EntryType::InCollection | EntryType::BookInBook => "chapter",
            EntryType::Article => "article-journal",
            EntryType::Online => "webpage",
            _ => {
                preview.notes.push(format!(
                    "{}: unsupported source type {}.",
                    entry.key, entry.entry_type
                ));
                continue;
            }
        };
        let mut record =
            Record::from_iter([("id".into(), json!(entry.key)), ("type".into(), json!(ty))]);
        for (name, chunks) in &entry.fields {
            let csl = match name.as_str() {
                "title" => "title",
                "journaltitle" | "journal" | "booktitle" => "container-title",
                "shorttitle" => "title-short",
                "publisher" => "publisher",
                "location" | "address" => "publisher-place",
                "volume" => "volume",
                "number" => "issue",
                "pages" => "page",
                "edition" => "edition",
                "doi" => "DOI",
                "isbn" => "ISBN",
                "issn" => "ISSN",
                "url" => "URL",
                "author" | "editor" => {
                    match entry.get_as::<Vec<Person>>(name) {
                        Ok(people) => {
                            record.insert(
                                name.clone(),
                                Value::Array(people.into_iter().map(person).collect()),
                            );
                        }
                        Err(e) => preview.notes.push(format!("{}: {name}: {e}.", entry.key)),
                    }
                    continue;
                }
                "year" | "month" | "day" | "date" | "urldate" => continue,
                _ => {
                    preview
                        .notes
                        .push(format!("{}: field {name} was left out.", entry.key));
                    continue;
                }
            };
            let value = chunks.format_verbatim();
            let value = if name == "pages" {
                value.replace("--", "–")
            } else {
                value
            };
            record.insert(csl.into(), json!(value));
        }
        for (label, csl, date) in [
            ("date", "issued", entry.date()),
            ("urldate", "accessed", entry.url_date()),
        ] {
            match date {
                Ok(PermissiveType::Typed(date)) => {
                    use biblatex::DateValue;
                    let parts = match date.value {
                        DateValue::At(date) => Some(vec![date_parts(date)]),
                        DateValue::Between(start, end) => {
                            Some(vec![date_parts(start), date_parts(end)])
                        }
                        DateValue::Before(_) | DateValue::After(_) => None,
                    };
                    if let Some(parts) = parts {
                        let mut value = json!({"date-parts": parts});
                        if date.uncertain || date.approximate {
                            value["circa"] = json!(true);
                        }
                        record.insert(csl.into(), value);
                    } else {
                        preview
                            .notes
                            .push(format!("{}: open-ended {label} was left out.", entry.key));
                    }
                }
                Ok(PermissiveType::Chunks(chunks)) => {
                    record.insert(csl.into(), json!({"literal": chunks.format_verbatim()}));
                }
                Err(RetrievalError::Missing(_)) => (),
                Err(e) => preview.notes.push(format!("{}: {label}: {e}.", entry.key)),
            }
        }
        preview.records.push(record);
    }
    Ok(preview)
}

fn date_parts(date: Datetime) -> Vec<i32> {
    let mut parts = vec![date.year];
    if let Some(month) = date.month {
        parts.push(i32::from(month) + 1);
        if let Some(day) = date.day {
            parts.push(i32::from(day) + 1);
        }
    }
    parts
}

fn person(person: Person) -> Value {
    let mut fields = Record::new();
    for (label, value) in [
        ("family", person.name),
        ("given", person.given_name),
        ("non-dropping-particle", person.prefix),
        ("suffix", person.suffix),
    ] {
        if !value.is_empty() {
            fields.insert(label.into(), json!(value));
        }
    }
    Value::Object(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_preserves_provenance_and_rejects_duplicate_keys() {
        let text = r#"[
            {"id":"z","type":"book","title":"Z","_wros":{"version":7,"edition":"e"}},
            {"id":"a","type":"webpage","title":"A","author":[{"family":"Yu"},{"family":"Su"}]}
        ]"#;
        let sources = Sources::parse(text).unwrap();
        let encoded = sources.to_json().unwrap();
        let records: Vec<Value> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(records[0]["id"], "a");
        assert_eq!(records[0]["author"][1]["family"], "Su");
        assert_eq!(records[1]["_wros"]["edition"], "e");
        assert_eq!(Sources::parse(&encoded).unwrap(), sources);
        assert!(Sources::parse(r#"[{"id":"a","type":"book"},{"id":"a","type":"book"}]"#).is_err());
        let record = json!({"author":[{"family":"García"}],"issued":{"date-parts":[[2013]]}});
        let mut entries = Sources::default();
        let record = record.as_object().unwrap();
        assert_eq!(entries.suggested_key(record), "garcia2013");
        entries
            .add(serde_json::from_value(json!({"id":"garcia2013","type":"book"})).unwrap())
            .unwrap();
        assert_eq!(entries.suggested_key(record), "garcia2013-2");
    }

    #[test]
    fn merges_independent_edits_and_identifies_conflicting_keys() {
        let base = Sources::parse(r#"[{"id":"a","type":"book","title":"A"}]"#).unwrap();
        let mine = Sources::parse(r#"[{"id":"a","type":"book","title":"New A"}]"#).unwrap();
        let theirs = Sources::parse(
            r#"[
            {"id":"a","type":"book","title":"A"},
            {"id":"b","type":"webpage","title":"B"}
        ]"#,
        )
        .unwrap();
        let merged = Sources::merge(&base, &mine, &theirs).unwrap();
        assert_eq!(merged.get("a").unwrap()["title"], "New A");
        assert!(merged.get("b").is_some());
        assert_eq!(
            Sources::merge(&base, &mine, &Sources::default()).unwrap_err(),
            ["a"]
        );
    }

    #[test]
    fn drive_merge_keeps_both_copies_until_explicit_choice_and_checks_revision() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let desk = root.join(FILENAME);
        let drive = root.join("sources (from Drive, 2026-11-22 10-30).json");
        let base = root.join("base.json");
        fs::write(&base, r#"[{"id":"a","type":"book","title":"Old"}]"#).unwrap();
        fs::write(&desk, r#"[{"id":"a","type":"book","title":"Desk"}]"#).unwrap();
        fs::write(
            &drive,
            r#"[{"id":"a","type":"book","title":"Drive"},{"id":"b","type":"book","title":"New"}]"#,
        )
        .unwrap();
        let disagreement = DriveMerge::open(root, &drive, &base).unwrap();
        assert_eq!(disagreement.conflicts(), ["a"]);
        assert!(disagreement.apply(root, &BTreeMap::new()).is_err());
        assert!(drive.exists());
        let choose_drive = BTreeMap::from([("a".to_owned(), true)]);
        fs::write(&desk, r#"[{"id":"a","type":"book","title":"Newer desk"}]"#).unwrap();
        assert!(disagreement.apply(root, &choose_drive).is_err());
        assert!(drive.exists());
        let disagreement = DriveMerge::open(root, &drive, &base).unwrap();
        disagreement.apply(root, &choose_drive).unwrap();
        let result = Snapshot::open(root).unwrap();
        assert_eq!(result.sources.get("a").unwrap()["title"], "Drive");
        assert!(result.sources.get("b").is_some());
        assert!(!drive.exists());
    }

    #[test]
    fn imports_bibtex_macros_and_creators_and_reports_unmapped_fields() {
        let bib = r#"
          @string{journal = "Folk Research"}
          @article{garcia2013,
            author = {Garc{\'i}a, Ana and Van Dijk, Sara},
            title = {{DNA} in Print}, journal = journal, year = {2013},
            volume = {128}, number = {1}, pages = {193--200},
            custom = {explanation}
          }
        "#;
        let preview = import_bib(bib).unwrap();
        let a = &preview.records[0];
        assert_eq!(a["author"][0]["family"], "García");
        assert_eq!(a["author"][1]["family"], "Van Dijk");
        assert_eq!(a["title"], "DNA in Print");
        assert_eq!(a["container-title"], "Folk Research");
        assert_eq!(a["issued"], json!({"date-parts":[[2013]]}));
        assert_eq!(a["page"], "193–200");
        assert!(preview.notes.iter().any(|s| s.contains("custom")));
    }
}
