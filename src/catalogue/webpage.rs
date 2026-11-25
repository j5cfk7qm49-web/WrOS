//! Citation details from a web page's HTML head.

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use grimoire_core::sources::Record;
use serde_json::{Value, json};

use super::{Hit, names};

pub fn page_record(url: &str, head: &[u8], today: NaiveDate) -> Result<Hit> {
    let html = String::from_utf8_lossy(head);
    let Head {
        scholar,
        linked_data,
        dublin_core,
        open_graph,
        title,
    } = scan_head(&html);
    let sources = [scholar, linked_data, dublin_core, open_graph];
    let title = sources
        .iter()
        .find_map(|source| source.title.as_deref())
        .or(title.as_deref())
        .context("This page has no title")?;

    let mut record = Record::new();
    record.insert("type".into(), json!("webpage"));
    record.insert("title".into(), json!(title));
    record.insert("URL".into(), json!(url));
    record.insert("accessed".into(), full_date(today));
    if let Some(source) = sources.iter().find(|source| !source.authors.is_empty()) {
        record.insert("author".into(), json!(source.authors));
    }
    if let Some(date) = sources.iter().find_map(|source| source.issued.as_ref()) {
        record.insert("issued".into(), date.clone());
    }
    if let Some(site) = sources.iter().find_map(|source| source.site.as_deref()) {
        record.insert("container-title".into(), json!(site));
    }
    Ok(Hit {
        record,
        work: None,
        editions: None,
        authors: Vec::new(),
    })
}

#[derive(Default)]
struct Details {
    title: Option<String>,
    authors: Vec<Value>,
    issued: Option<Value>,
    site: Option<String>,
}

#[derive(Default)]
struct Head {
    scholar: Details,
    linked_data: Details,
    dublin_core: Details,
    open_graph: Details,
    title: Option<String>,
}

impl Head {
    fn add_meta(&mut self, attributes: &str) {
        let Some(key) = attribute(attributes, "name").or_else(|| attribute(attributes, "property"))
        else {
            return;
        };
        let Some(value) = attribute(attributes, "content").and_then(html_text) else {
            return;
        };
        match key.trim_ascii().to_ascii_lowercase().as_str() {
            "citation_title" => {
                self.scholar.title.get_or_insert(value);
            }
            "citation_author" => self.scholar.authors.push(author_name(&value)),
            "citation_publication_date" | "citation_date" => {
                set_date(&mut self.scholar.issued, &value);
            }
            "dc.title" | "dcterms.title" => {
                self.dublin_core.title.get_or_insert(value);
            }
            "dc.creator" | "dcterms.creator" => {
                self.dublin_core.authors.push(author_name(&value));
            }
            "dc.date" | "dc.date.issued" | "dcterms.date" | "dcterms.issued" => {
                set_date(&mut self.dublin_core.issued, &value);
            }
            "og:title" => {
                self.open_graph.title.get_or_insert(value);
            }
            "og:site_name" => {
                self.open_graph.site.get_or_insert(value);
            }
            "article:published_time" => set_date(&mut self.open_graph.issued, &value),
            _ => {}
        }
    }
}

fn scan_head(mut html: &str) -> Head {
    let mut head = Head::default();
    while let Some((_, after_open)) = html.split_once('<') {
        if let Some(comment) = after_open.strip_prefix("!--") {
            let Some((_, rest)) = comment.split_once("-->") else {
                break;
            };
            html = rest;
            continue;
        }
        let (tag, rest) = split_tag(after_open);
        html = rest;
        let Some(tag) = tag else { continue };
        let end = tag.find(|c: char| c.is_ascii_whitespace());
        let (name, attributes) = tag.split_at(end.unwrap_or(tag.len()));
        let name = name.trim_end_matches('/').to_ascii_lowercase();
        match name.as_str() {
            "/head" | "body" => break,
            "meta" => head.add_meta(attributes),
            "script" | "style" | "title" => {
                let Some((text, rest)) = raw_text(html, &name) else {
                    break;
                };
                html = rest;
                if name == "title" && head.title.is_none() {
                    head.title = html_text(text);
                }
                if name == "script"
                    && attribute(attributes, "type").is_some_and(|kind| {
                        kind.trim_ascii()
                            .eq_ignore_ascii_case("application/ld+json")
                    })
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                {
                    read_linked_data(&mut head.linked_data, &value);
                }
            }
            _ => {}
        }
    }
    head
}

// Quoted attribute values may contain angle brackets. Resume at a stray unquoted '<'.
fn split_tag(after_open: &str) -> (Option<&str>, &str) {
    let mut quote = None;
    for (index, byte) in after_open.bytes().enumerate() {
        match quote {
            Some(end) if byte == end => quote = None,
            Some(_) => {}
            None => match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'>' => return (Some(&after_open[..index]), &after_open[index + 1..]),
                b'<' => return (None, &after_open[index..]),
                _ => {}
            },
        }
    }
    (None, "")
}

fn raw_text<'a>(html: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    for (start, _) in html.match_indices("</") {
        let tail = &html[start + 2..];
        if let Some(candidate) = tail.get(..name.len())
            && candidate.eq_ignore_ascii_case(name)
            && let Some(rest) = tail[name.len()..].trim_ascii_start().strip_prefix('>')
        {
            return Some((&html[..start], rest));
        }
    }
    None
}

fn attribute<'a>(mut attributes: &'a str, key: &str) -> Option<&'a str> {
    loop {
        attributes = attributes.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if attributes.is_empty() {
            return None;
        }
        let end = attributes.find(|c: char| c.is_ascii_whitespace() || c == '=');
        let (name, rest) = attributes.split_at(end.unwrap_or(attributes.len()));
        attributes = rest.trim_ascii_start();
        let Some(value) = attributes.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_ascii_start();
        let (text, rest) = match value.chars().next() {
            Some(quote @ ('\'' | '"')) => value[1..].split_once(quote)?,
            _ => {
                let end = value.find(|c: char| c.is_ascii_whitespace());
                value.split_at(end.unwrap_or(value.len()))
            }
        };
        if name.eq_ignore_ascii_case(key) {
            return Some(text);
        }
        attributes = rest;
    }
}

fn html_text(text: &str) -> Option<String> {
    let mut chunks = text.split('&');
    let mut decoded = String::from(chunks.next().unwrap_or_default());
    for chunk in chunks {
        if let Some((code, rest)) = chunk.split_once(';')
            && let Some(character) = entity(code)
        {
            decoded.push(character);
            decoded.push_str(rest);
        } else {
            decoded.push('&');
            decoded.push_str(chunk);
        }
    }
    let text = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

fn entity(code: &str) -> Option<char> {
    Some(match code {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let number = code.strip_prefix('#')?;
            let scalar = if let Some(hex) = number.strip_prefix(['x', 'X']) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                number.parse().ok()?
            };
            char::from_u32(scalar)?
        }
    })
}

fn read_linked_data(details: &mut Details, value: &Value) {
    const PAGE_TYPES: &[&str] = &["Article", "NewsArticle", "BlogPosting", "WebPage"];

    if let Some(entries) = value.as_array() {
        for entry in entries {
            read_linked_data(details, entry);
        }
        return;
    }
    let kind = &value["@type"];
    let is_page = |kind: &str| PAGE_TYPES.contains(&kind);
    if kind.as_str().is_some_and(is_page)
        || kind
            .as_array()
            .is_some_and(|types| types.iter().filter_map(Value::as_str).any(is_page))
    {
        if details.title.is_none() {
            details.title = text(&value["headline"])
                .or_else(|| text(&value["name"]))
                .map(str::to_owned);
        }
        if details.authors.is_empty() {
            let author = &value["author"];
            let authors = author
                .as_array()
                .map_or(std::slice::from_ref(author), Vec::as_slice);
            details.authors = authors
                .iter()
                .filter_map(|author| {
                    let name = text(author).or_else(|| text(&author["name"]))?;
                    if author["@type"] == "Organization" {
                        Some(json!({"literal": name}))
                    } else {
                        Some(author_name(name))
                    }
                })
                .collect();
        }
        if let Some(date) = text(&value["datePublished"]) {
            set_date(&mut details.issued, date);
        }
        if details.site.is_none() {
            let publisher = &value["publisher"];
            details.site = text(&publisher["name"])
                .or_else(|| text(publisher))
                .map(str::to_owned);
        }
    }
    if let Some(graph) = value.get("@graph") {
        read_linked_data(details, graph);
    }
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn author_name(name: &str) -> Value {
    if name.contains(',') {
        names::from_heading(name)
    } else {
        names::by_rule(name)
    }
}

fn set_date(issued: &mut Option<Value>, value: &str) {
    if issued.is_none() {
        *issued = date_parts(value);
    }
}

fn date_parts(value: &str) -> Option<Value> {
    let date = value.split(['T', 't', ' ']).next()?;
    let mut parts = date.split(['-', '/']);
    let year = parts.next()?;
    if year.len() != 4 || !year.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year: i32 = year.parse().ok().filter(|&year| year > 0)?;
    let Some(month) = parts.next() else {
        return Some(json!({"date-parts": [[year]]}));
    };
    let month = month.parse().ok()?;
    let day = parts.next().map(str::parse).transpose().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year, month, day.unwrap_or(1))?;
    Some(match day {
        Some(_) => full_date(date),
        None => json!({"date-parts": [[year, month]]}),
    })
}

fn full_date(date: NaiveDate) -> Value {
    json!({"date-parts": [[date.year(), date.month(), date.day()]]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fetch::{Fetch, FixtureFetch, Kind, Request};

    const URL: &str = "https://example.test/article";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
    }

    fn fixture_head(html: &str) -> Vec<u8> {
        FixtureFetch::new()
            .reply(URL, 200, html)
            .fetch(&Request {
                url: URL.into(),
                kind: Kind::Page,
            })
            .unwrap()
            .body
    }

    fn record(html: &str) -> Record {
        page_record(URL, &fixture_head(html), today())
            .unwrap()
            .record
    }

    #[test]
    fn scholar_meta_wins() {
        let record = record(
            r#"<head>
                <meta name="citation_title" content="The Scholar Title">
                <meta name="citation_author" content="Kim, Ana">
                <meta name="citation_author" content="Lee, Bo">
                <meta name="citation_publication_date" content="2021/03/04">
                <meta property="og:site_name" content="The Site">
                <title>A Different Title</title>
            </head>"#,
        );
        assert_eq!(record["title"], "The Scholar Title");
        assert_eq!(
            record["author"],
            json!([{"family": "Kim", "given": "Ana"}, {"family": "Lee", "given": "Bo"}])
        );
        assert_eq!(record["issued"], json!({"date-parts": [[2021, 3, 4]]}));
        assert_eq!(record["container-title"], "The Site");
    }

    #[test]
    fn json_ld_article_is_read_even_inside_a_graph() {
        let record = record(
            r#"<script type="application/ld+json">
                [{"@graph":[{"@type":"NewsArticle","headline":"The News",
                    "author":{"name":"Ana Kim"},"datePublished":"2021-03-04T09:30:00Z",
                    "publisher":{"name":"Daily News"}}]}]
            </script>
            <meta name="DC.title" content="Dublin Core Title">
            <meta property="og:site_name" content="A Different Site">"#,
        );
        assert_eq!(record["title"], "The News");
        assert_eq!(record["author"], json!([{"family": "Kim", "given": "Ana"}]));
        assert_eq!(record["issued"], json!({"date-parts": [[2021, 3, 4]]}));
        assert_eq!(record["container-title"], "Daily News");
    }

    #[test]
    fn open_graph_and_title_fall_back() {
        let graph = record(
            r#"<META content='Writing &amp; Reading > Notes' property=og:title>
            <meta property="og:site_name" content="The Site">"#,
        );
        assert_eq!(graph["title"], "Writing & Reading > Notes");
        assert_eq!(graph["container-title"], "The Site");
        assert_eq!(
            record("<TITLE>Just a title</TITLE>")["title"],
            "Just a title"
        );
    }

    #[test]
    fn broken_json_ld_and_odd_bytes_do_not_fail() {
        let mut head = vec![0xff, 0xff];
        head.extend(fixture_head(
            r#"<script type="application/ld+json">{broken</script><title>Still here</title>"#,
        ));
        let hit = page_record(URL, &head, today()).unwrap();
        assert_eq!(hit.record, record("<title>Still here</title>"));
        assert_eq!(hit.record["title"], "Still here");
    }

    #[test]
    fn accessed_is_today() {
        let record = record("<title>A Web Page</title>");
        assert_eq!(record["type"], "webpage");
        assert_eq!(record["URL"], URL);
        assert_eq!(record["accessed"], json!({"date-parts": [[2026, 11, 24]]}));
    }
}
