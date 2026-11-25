//! Open Library work searches and edition replies.

use grimoire_core::sources::Record;
use serde_json::{Value, json};

use super::{Hit, encode};

pub fn search_url(text: &str) -> String {
    format!(
        concat!(
            "https://openlibrary.org/search.json?q={}&limit=10",
            "&fields=key,title,author_name,author_key,edition_count,editions,editions.key,",
            "editions.title,editions.subtitle,editions.publisher,editions.publish_date,",
            "editions.publish_place,editions.isbn,editions.language,editions.edition_name,",
            "editions.by_statement",
        ),
        encode(text, true),
    )
}

pub fn parse_search(reply: &Value) -> Vec<Hit> {
    reply["docs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|doc| {
            let edition = &doc["editions"]["docs"][0];
            let title = text(&edition["title"]).or_else(|| text(&doc["title"]))?;
            let mut record = book_record(edition, title);
            put_names(&mut record, "author", &doc["author_name"], nonempty);
            let work = text(&doc["key"]).and_then(|key| ol_id(key, 'W'));
            let edition = text(&edition["key"]).and_then(|key| ol_id(key, 'M'));
            let editions = doc["edition_count"]
                .as_u64()
                .and_then(|n| n.try_into().ok());
            let authors = author_ids(&doc["author_key"], nonempty);
            Some(book_hit(record, work, edition, editions, authors))
        })
        .collect()
}

pub fn editions_url(work: &str, offset: u32) -> String {
    let work = encode(work, false);
    format!("https://openlibrary.org/works/{work}/editions.json?limit=50&offset={offset}")
}

pub fn parse_editions(reply: &Value, work: &Hit) -> (Vec<Hit>, bool) {
    let mut editions: Vec<_> = reply["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let mut record = book_record(entry, text(&entry["title"])?);
            if let Some(author) = work.record.get("author") {
                record.insert("author".into(), author.clone());
            }
            let edition = text(&entry["key"]).and_then(|key| ol_id(key, 'M'));
            let hit = book_hit(
                record,
                work.work.as_deref(),
                edition,
                None,
                work.authors.clone(),
            );
            let english = entry["languages"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|lang| {
                    lang.as_str().or_else(|| lang["key"].as_str()) == Some("/languages/eng")
                });
            Some((!english, hit))
        })
        .collect();
    editions.sort_by_key(|(non_english, _)| *non_english);
    let more = nonempty(&reply["links"]["next"]).is_some();
    (editions.into_iter().map(|(_, hit)| hit).collect(), more)
}

pub fn isbn_url(isbn: &str) -> String {
    format!("https://openlibrary.org/api/books?bibkeys=ISBN%3A{isbn}&format=json&jscmd=data")
}

pub fn parse_isbn(reply: &Value, isbn: &str) -> Option<Hit> {
    let book = &reply[format!("ISBN:{isbn}")];
    let mut record = book_record(book, text(&book["title"])?);
    record.insert("ISBN".into(), json!(isbn));
    put_names(&mut record, "author", &book["authors"], |author| {
        nonempty(&author["name"])
    });
    let work = text(&book["works"][0]["key"]).and_then(|key| ol_id(key, 'W'));
    let edition = [
        &book["key"],
        &book["identifiers"]["openlibrary"],
        &book["url"],
    ]
    .into_iter()
    .filter_map(text)
    .find_map(|key| ol_id(key, 'M'));
    let authors = author_ids(&book["authors"], |author| {
        text(&author["key"]).or_else(|| text(&author["url"]))
    });
    Some(book_hit(record, work, edition, None, authors))
}

fn book_hit(
    mut record: Record,
    work: Option<&str>,
    edition: Option<&str>,
    editions: Option<u32>,
    authors: Vec<String>,
) -> Hit {
    record.insert(
        "x-wros-catalogue".into(),
        json!({"openlibrary_work": work, "openlibrary_edition": edition}),
    );
    Hit {
        record,
        work: work.map(str::to_owned),
        editions,
        authors,
    }
}

fn book_record(book: &Value, title: &str) -> Record {
    let mut record = Record::new();
    let title = match text(&book["subtitle"]) {
        Some(subtitle) if !title.ends_with(subtitle) => format!("{title}: {subtitle}"),
        _ => title.to_owned(),
    };
    record.insert("title".into(), json!(title));
    record.insert("type".into(), json!("book"));
    for (field, value) in [
        (
            "publisher",
            text(&book["publisher"]).or_else(|| text(&book["publishers"])),
        ),
        (
            "publisher-place",
            text(&book["publish_place"]).or_else(|| text(&book["publish_places"])),
        ),
        (
            "ISBN",
            text(&book["isbn"])
                .or_else(|| text(&book["isbn_13"]))
                .or_else(|| text(&book["isbn_10"])),
        ),
        ("edition", text(&book["edition_name"])),
        ("URL", text(&book["url"])),
    ] {
        if let Some(value) = value {
            record.insert(field.to_owned(), json!(value));
        }
    }
    if let Some(year) = text(&book["publish_date"])
        .into_iter()
        .flat_map(|date| date.split(|c: char| !c.is_ascii_digit()))
        .find(|part| part.len() == 4)
        .and_then(|year| year.parse::<u16>().ok())
    {
        record.insert("issued".into(), json!({"date-parts": [[year]]}));
    }
    if let Some(name) = text(&book["by_statement"]).and_then(translated_by) {
        record.insert("translator".into(), json!([{"literal": name}]));
    } else {
        put_names(&mut record, "translator", &book["contributors"], |name| {
            nonempty(&name["role"])
                .filter(|role| role.eq_ignore_ascii_case("translator"))
                .and_then(|_| nonempty(&name["name"]))
        });
    }
    record
}

fn translated_by(statement: &str) -> Option<&str> {
    let lower = statement.to_ascii_lowercase();
    lower
        .match_indices("translated ")
        .find_map(|(start, marker)| {
            let mut rest = &lower[start + marker.len()..];
            if let Some(from) = rest.strip_prefix("from the ") {
                let (language, after_language) = from.split_once(' ')?;
                if language.is_empty() || !language.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    return None;
                }
                rest = after_language;
            }
            let name = rest.strip_prefix("by ")?;
            let original = &statement[statement.len() - name.len()..];
            original
                .split(['.', ';'])
                .next()
                .map(str::trim)
                .filter(|name| !name.is_empty())
        })
}

fn nonempty(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn text(value: &Value) -> Option<&str> {
    nonempty(value).or_else(|| {
        value
            .as_array()?
            .iter()
            .find_map(|item| nonempty(item).or_else(|| nonempty(&item["name"])))
    })
}

fn ol_id(path: &str, suffix: char) -> Option<&str> {
    path.split('/').find(|part| {
        part.strip_prefix("OL")
            .and_then(|id| id.strip_suffix(suffix))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn author_ids(values: &Value, key: impl Fn(&Value) -> Option<&str>) -> Vec<String> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(key)
        .filter_map(|key| ol_id(key, 'A'))
        .map(str::to_owned)
        .collect()
}

fn put_names(
    record: &mut Record,
    field: &str,
    values: &Value,
    name: impl Fn(&Value) -> Option<&str>,
) {
    let names: Vec<_> = values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(name)
        .map(|name| json!({"literal": name}))
        .collect();
    if !names.is_empty() {
        record.insert(field.to_owned(), json!(names));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fetch::{Fetch, FixtureFetch, Kind, Request};

    #[test]
    fn open_library_search_rows_use_the_matching_edition() {
        let reply = json!({"docs": [{
            "key": "/works/OL166894W",
            "title": "Преступление и наказание",
            "edition_count": 1178,
            "author_name": ["Fyodor Dostoevsky"],
            "author_key": ["OL27363A"],
            "editions": {"docs": [{
                "key": "/books/OL1M",
                "title": "Crime and Punishment",
                "publisher": ["Penguin Books"],
                "publish_date": ["August 30, 1998"],
                "isbn": ["9780140621808", "0140621806"],
                "language": ["eng"],
            }]},
        }]});
        let url = concat!(
            "https://openlibrary.org/search.json?q=crime+and+punishment+penguin&limit=10",
            "&fields=key,title,author_name,author_key,edition_count,editions,editions.key,",
            "editions.title,editions.subtitle,editions.publisher,editions.publish_date,",
            "editions.publish_place,editions.isbn,editions.language,editions.edition_name,",
            "editions.by_statement",
        );
        let fetch = FixtureFetch::new().reply(url, 200, &reply.to_string());
        let response = fetch
            .fetch(&Request {
                url: search_url("crime and punishment penguin"),
                kind: Kind::Api,
            })
            .unwrap();
        assert_eq!(fetch.requests(), vec![url.to_owned()]);
        let hits = parse_search(&serde_json::from_slice(&response.body).unwrap());
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.record["title"], "Crime and Punishment");
        assert_eq!(hit.record["publisher"], "Penguin Books");
        assert_eq!(hit.record["issued"]["date-parts"][0][0], 1998);
        assert_eq!(hit.record["ISBN"], "9780140621808");
        assert_eq!(hit.editions, Some(1178));
        assert_eq!(hit.work.as_deref(), Some("OL166894W"));
        assert_eq!(hit.authors, vec!["OL27363A".to_owned()]);
        assert_eq!(
            hit.record["x-wros-catalogue"],
            json!({"openlibrary_work": "OL166894W", "openlibrary_edition": "OL1M"})
        );
    }

    #[test]
    fn a_sparse_search_doc_still_makes_a_row() {
        let hits = parse_search(&json!({"docs": [{
            "key": "/works/OL166894W",
            "title": "Преступление и наказание",
        }]}));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record["title"], "Преступление и наказание");
    }

    #[test]
    fn editions_list_puts_english_first_and_reports_more() {
        let works = parse_search(&json!({"docs": [{
            "key": "/works/OL166894W",
            "title": "Crime and Punishment",
        }]}));
        let work = &works[0];
        let reply = json!({
            "entries": [
                {"title": "Schuld und Sühne", "languages": [{"key": "/languages/ger"}]},
                {"title": "Crime and Punishment", "languages": [{"key": "/languages/eng"}]},
                {"title": "Crime e Castigo", "languages": [{"key": "/languages/por"}]},
            ],
            "links": {"next": "/works/OL166894W/editions.json?limit=50&offset=50"},
        });
        let url = "https://openlibrary.org/works/OL166894W/editions.json?limit=50&offset=0";
        let fetch = FixtureFetch::new().reply(url, 200, &reply.to_string());
        let response = fetch
            .fetch(&Request {
                url: editions_url(work.work.as_deref().unwrap(), 0),
                kind: Kind::Api,
            })
            .unwrap();
        assert_eq!(fetch.requests(), vec![url.to_owned()]);
        let (hits, more) = parse_editions(&serde_json::from_slice(&response.body).unwrap(), work);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].record["title"], "Crime and Punishment");
        assert_eq!(hits[1].record["title"], "Schuld und Sühne");
        assert_eq!(hits[2].record["title"], "Crime e Castigo");
        assert!(more);
    }

    #[test]
    fn translator_is_read_from_the_by_statement() {
        let hits = parse_search(&json!({"docs": [{
            "editions": {"docs": [{
                "title": "Crime and Punishment",
                "by_statement": "translated by David McDuff.",
            }]},
        }]}));
        assert_eq!(hits[0].record["translator"][0]["literal"], "David McDuff");
    }
}
