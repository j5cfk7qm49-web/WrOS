//! Crossref searches and exact DOI replies.

use grimoire_core::sources::Record;
use serde_json::{Value, json};

use super::{Hit, encode};

pub fn search_url(text: &str) -> String {
    format!(
        concat!(
            "https://api.crossref.org/works?query.bibliographic={}&rows=20",
            "&select=DOI,title,author,editor,type,issued,container-title,publisher,volume,issue,",
            "page,ISBN,URL",
        ),
        encode(text, true),
    )
}

pub fn parse_search(reply: &Value) -> Vec<Hit> {
    reply["message"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let kind = csl_type(&item["type"])?;
            let doi = item["DOI"].as_str().filter(|doi| !doi.trim().is_empty());
            work_hit(item, doi, kind)
        })
        .take(8)
        .collect()
}

pub fn doi_url(doi: &str) -> String {
    format!("https://api.crossref.org/works/{}", encode(doi, false))
}

pub fn parse_doi(reply: &Value, doi: &str) -> Option<Hit> {
    let work = &reply["message"];
    let kind = csl_type(&work["type"]).unwrap_or("article-journal");
    work_hit(work, Some(doi), kind)
}

fn csl_type(kind: &Value) -> Option<&'static str> {
    Some(match kind.as_str()? {
        "book" | "monograph" | "edited-book" => "book",
        "book-chapter" => "chapter",
        "proceedings-article" => "paper-conference",
        "report" => "report",
        "dissertation" => "thesis",
        "reference-entry" => "entry-encyclopedia",
        "posted-content" => "article",
        "journal-article" => "article-journal",
        _ => return None,
    })
}

fn work_hit(work: &Value, doi: Option<&str>, kind: &str) -> Option<Hit> {
    let title = work["title"][0]
        .as_str()
        .filter(|title| !title.trim().is_empty())?;
    let mut record = Record::new();
    record.insert("title".into(), json!(title));
    record.insert("type".into(), json!(kind));
    if let Some(doi) = doi {
        record.insert("DOI".into(), json!(doi));
    }
    record.insert(
        "x-wros-catalogue".into(),
        json!({"doi": doi.map(str::to_ascii_lowercase)}),
    );
    put(&mut record, "container-title", &work["container-title"][0]);
    put(&mut record, "ISBN", &work["ISBN"][0]);
    for field in ["publisher", "volume", "issue", "page", "issued", "URL"] {
        put(&mut record, field, &work[field]);
    }
    for field in ["author", "editor"] {
        let names: Vec<_> = work[field]
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
    Some(Hit {
        record,
        work: None,
        editions: None,
        authors: Vec::new(),
    })
}

fn put(record: &mut Record, field: &str, value: &Value) {
    if !value.is_null() {
        record.insert(field.to_owned(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fetch::{Fetch, FixtureFetch, Kind, Request};

    #[test]
    fn crossref_search_drops_reviews_and_keeps_eight() {
        let types = [
            "peer-review",
            "component",
            "proceedings-article",
            "journal-article",
            "book",
            "book-chapter",
            "monograph",
            "edited-book",
            "reference-entry",
            "report",
            "dissertation",
            "posted-content",
        ];
        let items: Vec<_> = types
            .iter()
            .map(|kind| json!({"type": kind, "title": [kind], "DOI": format!("10.1000/{kind}")}))
            .collect();
        let reply = json!({"message": {"items": items}});
        let url = concat!(
            "https://api.crossref.org/works?query.bibliographic=Le+Guin+%26+co.&rows=20",
            "&select=DOI,title,author,editor,type,issued,container-title,publisher,volume,issue,",
            "page,ISBN,URL",
        );
        let fetch = FixtureFetch::new().reply(url, 200, &reply.to_string());
        let response = fetch
            .fetch(&Request {
                url: search_url("Le Guin & co."),
                kind: Kind::Api,
            })
            .unwrap();
        assert_eq!(fetch.requests(), vec![url.to_owned()]);
        let hits = parse_search(&serde_json::from_slice(&response.body).unwrap());
        assert_eq!(hits.len(), 8);
        assert!(hits.iter().all(|hit| {
            let title = &hit.record["title"];
            title != "peer-review" && title != "component"
        }));
        assert_eq!(hits[0].record["type"], "paper-conference");
        assert_eq!(
            hits[0].record["x-wros-catalogue"],
            json!({"doi": "10.1000/proceedings-article"})
        );
    }
}
