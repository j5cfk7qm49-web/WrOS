//! Explicit single-identifier requests. The library's contents stay local.

use anyhow::{Context, Result, bail};
use grimoire_core::sources::Record;
use serde_json::Value;

use crate::catalogue::fetch::{Kind as FetchKind, Request};
use crate::catalogue::query::{doi, isbn};
use crate::catalogue::{crossref, default_fetch, openlibrary};

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
            Kind::Isbn => {
                isbn(input).context("Enter an ISBN-10 or ISBN-13 with its check digit")?
            }
        };
        Ok(Self { kind, id })
    }

    fn url(&self) -> String {
        match self.kind {
            Kind::Doi => crossref::doi_url(&self.id),
            Kind::Isbn => openlibrary::isbn_url(&self.id),
        }
    }

    pub(super) fn fetch(&self) -> Result<Record> {
        let response = default_fetch()
            .fetch(&Request {
                url: self.url(),
                kind: FetchKind::Api,
            })
            .context("Could not reach the catalog. Check the connection and retry")?;
        match response.status {
            200..=299 => {}
            404 => bail!("No edition matched that identifier"),
            429 => bail!("The catalog is busy. Try again in a minute"),
            code => bail!("The catalog returned HTTP {code}"),
        }
        let document =
            serde_json::from_slice(&response.body).context("The catalog reply was incomplete")?;
        self.metadata(&document)
    }

    fn metadata(&self, reply: &Value) -> Result<Record> {
        let hit = match self.kind {
            Kind::Doi => crossref::parse_doi(reply, &self.id),
            Kind::Isbn => openlibrary::parse_isbn(reply, &self.id),
        }
        .context("No title was returned for that identifier")?;
        let mut record = hit.record;
        record.insert("id".into(), Value::String("source".into()));
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let article = doi
            .metadata(&json!({"message": {
                "type": "journal-article",
                "title": ["Article"],
                "author": [{"family":"Kim","ORCID":"hidden"}],
            }}))
            .unwrap();
        assert_eq!(article["type"], "article-journal");
        assert!(article["author"][0].get("ORCID").is_none());
    }
}
