//! Accepted Library provenance for independent project bibliography records.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::Record;

const PROVENANCE_KEY: &str = "x-wros-library";
const VERSION: u16 = 1;
const CSL_FIELDS: &[&str] = &[
    "type",
    "title",
    "title-short",
    "author",
    "editor",
    "translator",
    "compiler",
    "issued",
    "original-date",
    "publisher",
    "publisher-place",
    "container-title",
    "edition",
    "volume",
    "issue",
    "page",
    "number",
    "chapter-number",
    "DOI",
    "ISBN",
    "ISSN",
    "URL",
    "accessed",
    "genre",
    "event-title",
    "event-place",
    "language",
    "medium",
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LibraryIdentity {
    pub work_id: String,
    pub edition_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryProvenance {
    pub version: u16,
    pub identity: LibraryIdentity,
    pub fingerprint: String,
    accepted_fields: Record,
}

pub fn library_provenance(record: &Record) -> Option<LibraryProvenance> {
    let provenance: LibraryProvenance =
        serde_json::from_value(record.get(PROVENANCE_KEY)?.clone()).ok()?;
    (provenance.version == VERSION).then_some(provenance)
}

pub fn accepted_library_record(
    key: &str,
    identity: LibraryIdentity,
    bibliography: &Record,
) -> Result<Record> {
    let mut record = bibliography.clone();
    record.insert("id".into(), Value::String(key.into()));
    put_link(&mut record, identity, bibliography)?;
    super::validate(&record)?;
    Ok(record)
}

fn put_link(record: &mut Record, identity: LibraryIdentity, bibliography: &Record) -> Result<()> {
    let accepted_fields = relevant_fields(bibliography);
    let link = LibraryProvenance {
        version: VERSION,
        identity,
        fingerprint: fingerprint(&accepted_fields)?,
        accepted_fields,
    };
    record.insert(PROVENANCE_KEY.into(), serde_json::to_value(link)?);
    Ok(())
}

pub fn library_fingerprint(bibliography: &Record) -> Result<String> {
    fingerprint(&relevant_fields(bibliography))
}

fn relevant_fields(bibliography: &Record) -> Record {
    CSL_FIELDS
        .iter()
        .filter_map(|&name| {
            bibliography
                .get(name)
                .filter(|value| !value.is_null())
                .map(|value| (name.into(), value.clone()))
        })
        .collect()
}

fn fingerprint(fields: &Record) -> Result<String> {
    let normalized = normalize(&Value::Object(fields.clone()));
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&normalized)?)
    ))
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.trim().nfc().collect()),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(fields) => {
            let mut ordered: Vec<_> = fields.iter().collect();
            ordered.sort_unstable_by_key(|(name, _)| *name);
            Value::Object(
                ordered
                    .into_iter()
                    .fold(Map::new(), |mut normalized, (name, item)| {
                        normalized.insert(name.clone(), normalize(item));
                        normalized
                    }),
            )
        }
        value => value.clone(),
    }
}

/// Preview a proposed Library update. Existing local overrides stay in
/// place, and the original project key and unfamiliar fields round-trip.
pub fn proposed_library_update(record: &Record, bibliography: &Record) -> Result<Option<Record>> {
    let Some(accepted) = library_provenance(record) else {
        return Ok(None);
    };
    if accepted.fingerprint == library_fingerprint(bibliography)? {
        return Ok(None);
    }
    anyhow::ensure!(
        bibliography
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| !kind.is_empty()),
        "The new edition needs a source type"
    );
    let mut proposed = record.clone();
    for &field in CSL_FIELDS {
        if record.get(field) == accepted.accepted_fields.get(field) {
            match bibliography.get(field).filter(|value| !value.is_null()) {
                Some(value) => {
                    proposed.insert(field.into(), value.clone());
                }
                None => {
                    proposed.remove(field);
                }
            }
        }
    }
    put_link(&mut proposed, accepted.identity, bibliography)?;
    super::validate(&proposed).context("The project source is no longer valid")?;
    Ok(Some(proposed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(value: Value) -> Record {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn an_explicit_update_keeps_the_key_and_local_author_override() {
        let original = record(json!({
            "id": "shared", "type": "book", "title": "Initial",
            "author": [{"family":"Hood"}], "publisher": "First"
        }));
        let identity = LibraryIdentity {
            work_id: "work-a".into(),
            edition_id: "edition-b".into(),
        };
        let mut snapshot =
            accepted_library_record("local-key", identity.clone(), &original).unwrap();
        snapshot.insert("author".into(), json!([{"family":"Local editor"}]));
        snapshot.insert("personal".into(), json!("do not replace"));
        let incoming = record(json!({
            "id": "renamed-import-key", "type": "book", "title": "Second",
            "author": [{"family":"Hood"}], "publisher": "Second"
        }));
        let proposal = proposed_library_update(&snapshot, &incoming)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot["title"], "Initial");
        assert_eq!(proposal["title"], "Second");
        assert_eq!(proposal["author"], snapshot["author"]);
        assert_eq!(proposal["id"], "local-key");
        assert_eq!(proposal["personal"], "do not replace");
        assert_eq!(library_provenance(&proposal).unwrap().identity, identity);
        assert!(
            proposed_library_update(&proposal, &incoming)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn fingerprint_covers_bibliography_and_keeps_author_order() {
        let a = record(json!({
            "id": "a", "title": "Cafe\u{301}", "author": [{"family":"A"}, {"family":"B"}],
        }));
        let b = record(json!({
            "id": "b", "title": " Café ", "author": [{"family":"A"}, {"family":"B"}],
        }));
        let c = record(json!({
            "id": "b", "title": " Café ", "author": [{"family":"B"}, {"family":"A"}],
        }));
        assert_eq!(
            library_fingerprint(&a).unwrap(),
            library_fingerprint(&b).unwrap()
        );
        assert_ne!(
            library_fingerprint(&a).unwrap(),
            library_fingerprint(&c).unwrap()
        );
    }
}
