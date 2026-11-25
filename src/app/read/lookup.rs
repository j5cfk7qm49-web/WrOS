//! Validation for the read view’s explicit DOI and ISBN shortcuts.

use anyhow::{Context, Result};

use crate::catalogue::query::{doi, isbn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Doi,
    Isbn,
}

impl Kind {
    pub(super) fn identifier(self, input: &str) -> Result<String> {
        match self {
            Self::Doi => {
                let input = input.trim();
                doi(input.strip_prefix("doi:").unwrap_or(input))
                    .context("Enter a DOI beginning with 10.")
            }
            Self::Isbn => isbn(input).context("Enter an ISBN-10 or ISBN-13 with its check digit"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{crossref, openlibrary};
    use serde_json::json;

    #[test]
    fn read_lookups_send_only_the_identifier_to_the_expected_host() {
        let doi = Kind::Doi
            .identifier("https://doi.org/10.1000/a?b#c")
            .unwrap();
        assert_eq!(
            crossref::doi_url(&doi),
            "https://api.crossref.org/works/10.1000%2Fa%3Fb%23c"
        );
        assert!(
            Kind::Doi
                .identifier("https://example.org/10.1000/a")
                .is_err()
        );
        let isbn = Kind::Isbn.identifier("978-0-14-032872-1").unwrap();
        assert!(openlibrary::isbn_url(&isbn).contains("ISBN%3A9780140328721"));
        assert!(Kind::Isbn.identifier("978-0-14-032872-2").is_err());
        let book = openlibrary::parse_isbn(
            &json!({"ISBN:9780140328721": {
                "title": "Fantastic Mr. Fox", "publish_date": "July 1988",
                "authors": [{"name": "Roald Dahl"}],
            }}),
            &isbn,
        )
        .unwrap()
        .record;
        assert_eq!(book["title"], "Fantastic Mr. Fox");
        assert_eq!(book["issued"]["date-parts"][0][0], 1988);
        let article = crossref::parse_doi(
            &json!({"message": {
                "type": "journal-article",
                "title": ["Article"],
                "author": [{"family":"Kim","ORCID":"hidden"}],
            }}),
            &doi,
        )
        .unwrap()
        .record;
        assert_eq!(article["type"], "article-journal");
        assert!(article["author"][0].get("ORCID").is_none());
    }
}
