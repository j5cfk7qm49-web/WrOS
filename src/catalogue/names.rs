//! Open Library author names, with Library of Congress headings where available.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use grimoire_core::atomic;
use serde_json::{Value, json};

use super::encode;
use super::fetch::{Fetch, Kind, Request};

pub fn from_heading(heading: &str) -> Value {
    let heading = heading.trim();
    let name = heading
        .rsplit_once(',')
        .filter(|(_, dates)| is_date_suffix(dates.trim()))
        .map_or(heading, |(name, _)| name.trim_end());
    if let Some((family, given)) = name.split_once(',')
        && !family.trim().is_empty()
        && !given.trim().is_empty()
    {
        return json!({"family": family.trim(), "given": given.trim()});
    }
    json!({"literal": name})
}

fn is_date_suffix(dates: &str) -> bool {
    let is_year = |year: &str| year.len() == 4 && is_digits(year);
    match dates.split_once(['-', '–']) {
        Some(("", end)) => is_year(end),
        Some((start, end)) => is_year(start) && (end.is_empty() || is_year(end)),
        None => is_year(dates),
    }
}

pub fn by_rule(name: &str) -> Value {
    const PARTICLES: &[&str] = &["de", "van", "von", "wa", "da", "del", "le", "la"];

    let name = name.trim();
    if name.contains([',', '&'])
        || name == "Penguin Random House"
        || ["Inc", "Press", "University"]
            .into_iter()
            .any(|marker| name.contains(marker))
    {
        return json!({"literal": name});
    }

    let words: Vec<_> = name.split_whitespace().collect();
    // Le Guin keeps the capitalized particle as part of the family name.
    let family_words = if words.ends_with(&["Le", "Guin"]) {
        2
    } else {
        1
    };
    let mut family_start = words.len().saturating_sub(family_words);
    while family_start > 0 && PARTICLES.contains(&words[family_start - 1]) {
        family_start -= 1;
    }
    if family_start == 0 {
        return json!({"literal": name});
    }

    json!({
        "family": words[family_start..].join(" "),
        "given": words[..family_start].join(" "),
    })
}

pub struct Names {
    cache_path: PathBuf,
    cache: BTreeMap<String, Value>,
}

impl Names {
    /// Load the cache, normally at `~/.cache/grimoire/catalogue-names.json`.
    pub fn load(path: PathBuf) -> Self {
        let cache = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            cache_path: path,
            cache,
        }
    }

    /// Resolve an Open Library author when a catalogue result is picked.
    pub fn resolve(&mut self, fetch: &dyn Fetch, author_key: &str, as_written: &str) -> Value {
        if let Some(name) = self.cache.get(author_key) {
            return name.clone();
        }
        if !author_key
            .strip_prefix("OL")
            .and_then(|key| key.strip_suffix('A'))
            .is_some_and(is_digits)
        {
            return by_rule(as_written);
        }

        let url = format!("https://openlibrary.org/authors/{author_key}.json");
        let name = fetch_json(fetch, url)
            .and_then(|author| {
                author["remote_ids"]["wikidata"]
                    .as_str()
                    .and_then(|id| authority_name(fetch, id))
                    .or_else(|| {
                        author["personal_name"]
                            .as_str()
                            .filter(|name| name.contains(','))
                            .map(from_heading)
                    })
            })
            .unwrap_or_else(|| by_rule(as_written));

        self.cache.insert(author_key.to_owned(), name.clone());
        let _ = self.save();
        name
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.cache_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(&self.cache).map_err(io::Error::other)?;
        atomic::write_io(&self.cache_path, &bytes)
    }
}

fn is_digits(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn fetch_json(fetch: &dyn Fetch, url: String) -> Option<Value> {
    let request = Request {
        url,
        kind: Kind::Api,
    };
    let reply = fetch.fetch(&request).ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    serde_json::from_slice(&reply.body).ok()
}

fn authority_name(fetch: &dyn Fetch, wikidata: &str) -> Option<Value> {
    if !is_digits(wikidata.strip_prefix('Q')?) {
        return None;
    }
    let query = format!("SELECT ?lc WHERE {{ wd:{wikidata} wdt:P244 ?lc }}");
    let url = format!(
        "https://query.wikidata.org/sparql?format=json&query={}",
        encode(&query, true),
    );
    let reply = fetch_json(fetch, url)?;
    let lc = reply["results"]["bindings"][0]["lc"]["value"].as_str()?;
    let digits = lc.trim_start_matches(|c: char| c.is_ascii_lowercase());
    if !lc.starts_with('n') || lc.len() - digits.len() > 3 || !is_digits(digits) {
        return None;
    }

    let url = format!("https://id.loc.gov/authorities/names/{lc}.json");
    let request = Request {
        url,
        kind: Kind::Head,
    };
    let reply = fetch.fetch(&request).ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    reply
        .headers
        .iter()
        .find_map(|(name, value)| {
            let value = value.trim();
            (name.eq_ignore_ascii_case("X-PrefLabel") && !value.is_empty()).then_some(value)
        })
        .map(from_heading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fetch::{FixtureFetch, Response};

    #[test]
    fn headings_become_family_and_given() {
        assert_eq!(
            from_heading("García Márquez, Gabriel, 1927-2014"),
            json!({"family": "García Márquez", "given": "Gabriel"})
        );
        assert_eq!(
            from_heading("Beauvoir, Simone de, 1908-1986"),
            json!({"family": "Beauvoir", "given": "Simone de"})
        );
        assert_eq!(
            from_heading("Ngũgĩ wa Thiongʼo, 1938-2025"),
            json!({"literal": "Ngũgĩ wa Thiongʼo"})
        );
        assert_eq!(
            from_heading("Murakami, Haruki, 1949-"),
            json!({"family": "Murakami", "given": "Haruki"})
        );
    }

    #[test]
    fn the_rule_keeps_particles() {
        assert_eq!(by_rule("Ursula K. Le Guin")["family"], "Le Guin");
        assert_eq!(by_rule("Ludwig van Beethoven")["family"], "van Beethoven");
        assert_eq!(by_rule("Gabriel García Márquez")["family"], "Márquez");
        assert_eq!(
            by_rule("Penguin Random House"),
            json!({"literal": "Penguin Random House"})
        );
    }

    #[test]
    fn resolve_follows_open_library_wikidata_and_loc() {
        struct CheckHead<'a>(&'a FixtureFetch);

        impl Fetch for CheckHead<'_> {
            fn fetch(&self, request: &Request) -> anyhow::Result<Response> {
                if request.url.starts_with("https://id.loc.gov/") {
                    assert_eq!(request.kind, Kind::Head);
                }
                self.0.fetch(request)
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut names = Names::load(dir.path().join("catalogue-names.json"));
        let openlibrary = "https://openlibrary.org/authors/OL1A.json";
        let wikidata = concat!(
            "https://query.wikidata.org/sparql?format=json&query=",
            "SELECT+%3Flc+WHERE+%7B+wd%3AQ72334+wdt%3AP244+%3Flc+%7D",
        );
        let loc = "https://id.loc.gov/authorities/names/n80131379.json";
        let fetch = FixtureFetch::new()
            .reply(openlibrary, 200, r#"{"remote_ids":{"wikidata":"Q72334"}}"#)
            .reply(
                wikidata,
                200,
                r#"{"results":{"bindings":[{"lc":{"value":"n80131379"}}]}}"#,
            )
            .reply_with_headers(loc, 200, &[("X-PrefLabel", "Morrison, Toni, 1931-2019")], "");
        assert_eq!(
            names.resolve(&CheckHead(&fetch), "OL1A", "Toni Morrison"),
            json!({"family": "Morrison", "given": "Toni"})
        );
        let expected = vec![openlibrary.to_owned(), wikidata.to_owned(), loc.to_owned()];
        assert_eq!(fetch.requests(), expected);
    }

    #[test]
    fn resolve_falls_back_to_personal_name_then_rule() {
        let dir = tempfile::tempdir().unwrap();
        let mut names = Names::load(dir.path().join("catalogue-names.json"));
        let openlibrary = "https://openlibrary.org/authors/OL2A.json";
        let fetch = FixtureFetch::new().reply(
            openlibrary,
            200,
            r#"{"personal_name":"Beauvoir, Simone de"}"#,
        );
        assert_eq!(
            names.resolve(&fetch, "OL2A", "Simone de Beauvoir")["family"],
            "Beauvoir"
        );
        assert_eq!(fetch.requests(), vec![openlibrary.to_owned()]);

        let offline = FixtureFetch::new();
        assert_eq!(
            names.resolve(&offline, "OL3A", "Ludwig van Beethoven"),
            json!({"family": "van Beethoven", "given": "Ludwig"})
        );
        assert_eq!(
            offline.requests(),
            vec!["https://openlibrary.org/authors/OL3A.json".to_owned()]
        );
    }

    #[test]
    fn resolved_names_are_cached_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grimoire").join("catalogue-names.json");
        let openlibrary = "https://openlibrary.org/authors/OL2A.json";
        let fetch = FixtureFetch::new().reply(
            openlibrary,
            200,
            r#"{"personal_name":"Beauvoir, Simone de"}"#,
        );
        let mut names = Names::load(path.clone());
        let expected = json!({"family": "Beauvoir", "given": "Simone de"});
        assert_eq!(names.resolve(&fetch, "OL2A", "Simone de Beauvoir"), expected);
        assert_eq!(names.resolve(&fetch, "OL2A", "Simone de Beauvoir"), expected);
        assert_eq!(fetch.requests(), vec![openlibrary.to_owned()]);

        let mut reopened = Names::load(path);
        let offline = FixtureFetch::new();
        assert_eq!(reopened.resolve(&offline, "OL2A", "Simone de Beauvoir"), expected);
        assert!(offline.requests().is_empty());
    }
}
