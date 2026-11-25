//! Explicit single-identifier requests. The library's contents stay local.

use std::fmt::Write;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use grimoire_core::sources::Record;
use serde_json::{Value, json};

use crate::catalogue::query::{doi, isbn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Doi,
    Isbn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Lookup {
    kind: Kind,
    id: String,
}

impl Lookup {
    pub(super) fn new(kind: Kind, input: &str) -> Result<Self> {
        let id = match kind {
            Kind::Doi => doi(input).context("Enter a DOI beginning with 10.")?,
            Kind::Isbn => isbn(input).context("Enter an ISBN-10 or ISBN-13 with its check digit")?,
        };
        Ok(Self { kind, id })
    }

    fn url(&self) -> String {
        match self.kind {
            Kind::Doi => format!(
                "https://api.crossref.org/works/{}",
                escape_segment(&self.id)
            ),
            Kind::Isbn => format!(
                "https://openlibrary.org/api/books?bibkeys=ISBN%3A{}&format=json&jscmd=data",
                self.id
            ),
        }
    }

    pub(super) fn fetch(&self) -> Result<Record> {
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .https_only(true)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(10)))
                .build(),
        );
        let mut response = agent
            .get(self.url())
            .header("User-Agent", "WrOS Library")
            .header("Accept", "application/json")
            .call()
            .map_err(|error| match error {
                ureq::Error::StatusCode(404) => {
                    anyhow::anyhow!("No edition matched that identifier")
                }
                ureq::Error::StatusCode(429) => {
                    anyhow::anyhow!("The catalog is busy. Try again in a minute")
                }
                ureq::Error::StatusCode(code) => {
                    anyhow::anyhow!("The catalog returned HTTP {code}")
                }
                _ => anyhow::anyhow!("Could not reach the catalog. Check the connection and retry"),
            })?;
        let document: Value = response
            .body_mut()
            .with_config()
            .limit(1_048_576)
            .read_json()
            .context("The catalog reply was incomplete")?;
        self.metadata(&document)
    }

    fn metadata(&self, reply: &Value) -> Result<Record> {
        let mut record = match self.kind {
            Kind::Doi => crossref(&reply["message"], &self.id),
            Kind::Isbn => open_library(&reply[format!("ISBN:{}", self.id)], &self.id),
        };
        ensure!(
            record
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| !title.trim().is_empty()),
            "No title was returned for that identifier"
        );
        record.insert("id".into(), json!("source"));
        Ok(record)
    }
}

fn escape_segment(value: &str) -> String {
    let mut url = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url
}

fn put(record: &mut Record, name: &str, value: &Value) {
    if !value.is_null() {
        record.insert(name.to_owned(), value.clone());
    }
}

fn crossref(reply: &Value, doi: &str) -> Record {
    let mut record = Record::new();
    let kind = match reply["type"].as_str() {
        Some("book" | "monograph" | "edited-book") => "book",
        Some("book-chapter") => "chapter",
        Some("proceedings-article") => "paper-conference",
        _ => "article-journal",
    };
    record.insert("type".into(), json!(kind));
    record.insert("DOI".into(), json!(doi));
    put(&mut record, "title", &reply["title"][0]);
    put(&mut record, "container-title", &reply["container-title"][0]);
    put(&mut record, "ISBN", &reply["ISBN"][0]);
    for field in ["publisher", "volume", "issue", "page", "issued", "URL"] {
        put(&mut record, field, &reply[field]);
    }
    for field in ["author", "editor"] {
        let names: Vec<_> = reply[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|name| {
                let family = name["family"].as_str()?;
                Some(json!({"family": family, "given": name["given"].as_str().unwrap_or_default()}))
            })
            .collect();
        if !names.is_empty() {
            record.insert(field.into(), json!(names));
        }
    }
    record
}

fn open_library(reply: &Value, isbn: &str) -> Record {
    let mut record = Record::new();
    record.insert("type".into(), json!("book"));
    record.insert("ISBN".into(), json!(isbn));
    put(&mut record, "title", &reply["title"]);
    put(&mut record, "publisher", &reply["publishers"][0]["name"]);
    put(&mut record, "URL", &reply["url"]);
    let names: Vec<_> = reply["authors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|name| name["name"].as_str().map(|name| json!({"literal": name})))
        .collect();
    if !names.is_empty() {
        record.insert("author".into(), json!(names));
    }
    if let Some(year) = reply["publish_date"]
        .as_str()
        .into_iter()
        .flat_map(|date| date.split(|c: char| !c.is_ascii_digit()))
        .find(|part| part.len() == 4)
        .and_then(|year| year.parse::<u16>().ok())
    {
        record.insert("issued".into(), json!({"date-parts": [[year]]}));
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_lookups_send_only_the_identifier_to_the_expected_host() {
        let doi = Lookup::new(Kind::Doi, "https://doi.org/10.1000/a?b#c").unwrap();
        assert_eq!(
            doi.url(),
            "https://api.crossref.org/works/10.1000%2Fa%3Fb%23c"
        );
        assert!(Lookup::new(Kind::Doi, "https://example.org/10.1000/a").is_err());
        let isbn = Lookup::new(Kind::Isbn, "978-0-14-032872-1").unwrap();
        assert!(isbn.url().contains("ISBN%3A9780140328721"));
        assert!(Lookup::new(Kind::Isbn, "978-0-14-032872-2").is_err());
        let book = isbn
            .metadata(&json!({"ISBN:9780140328721": {
                "title": "Fantastic Mr. Fox", "publish_date": "July 1988",
                "authors": [{"name": "Roald Dahl"}],
            }}))
            .unwrap();
        assert_eq!(book["title"], "Fantastic Mr. Fox");
        assert_eq!(book["issued"]["date-parts"][0][0], 1988);
        let article = doi.metadata(&json!({"message": {
            "type": "journal-article", "title": ["Article"], "author": [{"family":"Kim","ORCID":"hidden"}],
        }})).unwrap();
        assert_eq!(article["type"], "article-journal");
        assert!(article["author"][0].get("ORCID").is_none());
    }
}
