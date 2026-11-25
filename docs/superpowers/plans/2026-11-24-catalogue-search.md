# Catalogue Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One search box (`@` in writing, the Brain and the Library, and the top of Add a source)
that finds books, articles and web pages in Open Library and Crossref, and fills in the
citation details.

**Architecture:**
- A new crate-level module, `src/catalogue/`, holds everything network-facing:
  - a `Fetch` trait with one HTTPS implementation and one fixture implementation for tests;
  - input classification and per-catalogue request/reply mapping;
  - author-name resolution and web-page metadata;
  - a `Search` state machine (debounce, one request per catalogue, stale-reply dropping, cache).
- The UI owns a `Search` per open picker and polls it every tick.
- Picks go into the Library through one new store helper, then follow the rc27 Library-pick
  path into `sources.json` and the citation.

**Tech Stack:**
- Rust 2024; ratatui; `ureq` 3.4 (already a dependency, `json` feature); `serde_json`.
- `grimoire-brain` `PagedBrain` and `grimoire-core` `sources`/`citations::mla` (hayagriva).

**Spec:** `docs/superpowers/specs/2026-11-24-catalogue-search-design.md`. Read it first: its
numbered "Calls" 1–7 are referred to below as Call N.

## Global Constraints

- **HTTPS only.** User-Agent `WrOS Library`, timeout 10 s.
- **Reply caps:** 1 MiB for the catalogue APIs. Web pages may read 2 MiB, stopping after
  `</head>`.
- **Redirects:** none for the catalogue APIs. Web pages: at most 3, HTTPS → HTTPS only.
- **When a search goes out:** only after a 500 ms pause with ≥3 characters in the line.
- **Rate:** at most one request in flight per catalogue, ≥1 s between requests to the same
  catalogue.
- **Stale replies:** the reply to an older line is dropped. A 16-entry in-memory reply cache.
- **Crossref types kept:** `journal-article`, `book`, `book-chapter`, `monograph`, `edited-book`,
  `reference-entry`, `proceedings-article`, `report`, `dissertation`, `posted-content`. At most
  8 Crossref rows, shown after the book rows.
- **Web addresses** are fetched only on paste or Enter, never while being typed (Call 2).
- **What is sent:** only the search line, plus author identifiers for name lookups. Never
  project text, notes or the sentence around `@`.
- **Offline:** the Catalogue section shows exactly `Offline: showing local sources`.
- **Off switch:** the Setting label is `Search catalogues online from @ and Add a source`,
  default on. The Catalogue header shows `online search on` or `online search off`.
- **No test touches the real network.** Under `cfg(test)` the default `Fetch` is the fixture
  one, and an unknown URL replies as offline.
- **Finish line:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, the
  grimoire-tui test suite and `tools/nix-gates.sh` pass. Build and test in NixOS-WSL, as for
  every WrOS change; Codex's sandbox cannot run them.

## Review Focus

1. **Typing fast while a slow reply is in flight.** Nothing blocks a keystroke, and the slow
   reply for an older line never replaces newer results. Test in Task 6
   (`a_slow_reply_for_an_old_line_is_dropped`).
2. **Catalogue replies missing fields** (no publisher, no year, no authors, a title only in
   Russian). Rows still render, the edition's own title is used, and nothing panics. Test in
   Task 3 (`a_sparse_search_doc_still_makes_a_row`).
3. **The writer's text changes under an open picker** (an autosave reload, Drive sync). A
   catalogue pick is refused with rc27's "The writing cursor or text changed" message, and the
   edition already added to the Library stays there. Test in Task 9.
4. **The Library store is absent or locked.** A pick goes into `sources.json` only and is still
   cited. Test in Task 9 (`a_catalogue_pick_without_a_library_still_cites`).
5. **A malicious or broken web page** (giant `<head>`, a redirect to http, a redirect loop,
   non-UTF-8 bytes, JSON-LD that is an array or invalid). Each gives a clear one-line error or
   a `<title>`-only record, never a hang. Tests in Tasks 1 and 5.

---

## File Structure

- **Create `src/catalogue.rs`:** module root. Re-exports `Fetch`, `Query`, `Hit`, `Search`,
  `SearchStatus`.
- **Create `src/catalogue/fetch.rs`:**
  - `Request`, `Response`, the `Fetch` trait;
  - `HttpFetch` (ureq, with the network rules);
  - `FixtureFetch` (tests).
- **Create `src/catalogue/query.rs`:** `Query` and `classify`. ISBN and DOI validation moves
  here from `lookup.rs`.
- **Create `src/catalogue/openlibrary.rs`:** search/edition/ISBN URLs; reply → `Hit` / CSL
  record (absorbs `lookup.rs`'s `open_library()`).
- **Create `src/catalogue/crossref.rs`:** search/DOI URLs; reply → `Hit` (absorbs `crossref()`).
- **Create `src/catalogue/names.rs`:** author-name resolution chain (spec Call 4) and its disk
  cache.
- **Create `src/catalogue/webpage.rs`:** HTML head → CSL `webpage` record.
- **Create `src/catalogue/search.rs`:** `Search`, the debounce and request state machine.
- **Modify `src/main.rs`:** `mod catalogue;`.
- **Modify `src/app/brain/library/lookup.rs`:** shrink to a thin wrapper over
  `catalogue::{query, openlibrary, crossref}` so `d`/`l` keep working.
- **Modify `src/app/brain/library.rs`:**
  - `keep_in_library` (Task 8);
  - `@` opens the search; `d`/`l` become shortcuts (Task 11).
- **Modify `src/app/overlays/sources.rs`:**
  - the Catalogue section in `Page::Pick` / `Page::BrainPick`, the editions view and the
    page-step source line (Tasks 9, 11);
  - the search line and new fields in `Page::Form` (Task 10).
- **Modify `crates/grimoire-core/src/citations/mla.rs`:** `works_cited_line`.
- **Modify `crates/grimoire-core/src/settings.rs`:** `catalogue_search` setting.
- **Modify `src/app.rs`, `src/palette.rs`, `src/app/shell.rs`:**
  - `App.catalogue_on` and the `catalogue_fetch` handle;
  - `Action::CatalogueSearch` and its Settings row;
  - the tick hook.
- **Modify docs:**
  - `docs/API_AND_NETWORK_USE.md` ("Library lookups" section);
  - the help page covering `@` (find it with `grep -rl "@" assets/help`).

---

### Task 1: Fetch seam and network rules

**Files:**
- Create: `src/catalogue.rs`, `src/catalogue/fetch.rs`
- Modify: `src/main.rs` (add `mod catalogue;`)
- Test: `src/catalogue/fetch.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum Kind { Api, Page, Head }`: the caps and redirect rules per kind.
  - `pub struct Request { pub url: String, pub kind: Kind }`
  - `pub struct Response { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }`
  - `pub trait Fetch: Send + Sync { fn fetch(&self, request: &Request) -> anyhow::Result<Response>; }`
  - `pub struct HttpFetch;` implements `Fetch`.
  - `pub struct FixtureFetch` with:
    - `FixtureFetch::new() -> Self`;
    - `.reply(url_prefix: &str, status: u16, body: &str) -> Self`;
    - `.reply_with_headers(url_prefix: &str, status: u16, headers: &[(&str, &str)], body: &str) -> Self`,
      for Task 4's `X-PrefLabel`;
    - `.slow(url_prefix: &str, delay: Duration) -> Self`;
    - `.requests() -> Vec<String>`, the URLs asked for, in order.
    It implements `Fetch`. An unmatched URL returns `Err("offline")`.
  - `pub fn default_fetch() -> Arc<dyn Fetch>`: `HttpFetch` normally, an empty `FixtureFetch`
    under `cfg(test)`.
  - `pub const OFFLINE: &str = "Offline: showing local sources";`

- [ ] **Step 1: Write the failing tests**
  - `http_fetch_refuses_plain_http_before_connecting`: `HttpFetch.fetch(&Request{url:"http://example.com".into(), kind: Kind::Page})` is `Err`, and its message contains `HTTPS`.
  - `page_redirects_must_stay_https_and_stop_after_three`: put the redirect rule in a pure function, `fn next_hop(from: &str, location: &str, hop: u8) -> anyhow::Result<String>`. Assert:
    - `next_hop("https://a.test/x", "http://a.test/y", 1)` is `Err`;
    - `next_hop("https://a.test/x", "/y", 1)` == `"https://a.test/y"`;
    - `next_hop(_, _, 4)` is `Err`.
  - `head_reading_stops_after_head`: a pure function, `fn head_of(body: &[u8], cap: usize) -> &[u8]`, returns bytes up to and including the first case-insensitive `</head>`, else at most `cap`. Assert it on `"<html><HEAD><title>t</title></HEAD><body>"`, and on a 3 MiB body with cap `2 * 1024 * 1024`.
  - `fixture_fetch_records_requests_and_is_offline_for_unknown_urls`.

- [ ] **Step 2: Run them to see them fail**
  Run (NixOS-WSL dev shell): `cargo test -p grimoire-tui catalogue::fetch`
  Expected: FAIL to compile (module missing).

- [ ] **Step 3: Implement** `fetch.rs` as specified.
  - Build `HttpFetch` on the `ureq::Agent` config that `lookup.rs` uses today: `https_only(true)`, `max_redirects(0)`, `timeout_global(10 s)`.
  - Follow `Page` redirects manually through `next_hop`.
  - Read `Page` bodies through `head_of`. `Head` uses an HTTP HEAD request.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p grimoire-tui catalogue::fetch`
  Expected: PASS

- [ ] **Step 5: Commit**
  `git commit -m "catalogue: fetch seam with WrOS's network rules"`

---

### Task 2: One box, many kinds of input

**Files:**
- Create: `src/catalogue/query.rs`
- Modify: `src/app/brain/library/lookup.rs` (use `query::{doi, isbn}`, delete its own copies of
  `valid_isbn` and the DOI check)
- Test: `src/catalogue/query.rs`

**Interfaces:**
- Produces:
  - `pub enum Query { Isbn(String), Doi(String), Address(String), Text(String) }`
  - `pub fn classify(line: &str) -> Option<Query>`:
    - `None` for a blank line, or for text with fewer than 3 non-space characters;
    - an `Address` is any `http://` or `https://` input not on `doi.org`;
    - a `https://doi.org/…` input is a `Doi`.
  - `pub fn isbn(input: &str) -> Option<String>` and `pub fn doi(input: &str) -> Option<String>`:
    the normalised identifier, using today's rules from `Lookup::new`.

- [ ] **Step 1: Write the failing test**
  `classify_recognises_each_kind`. Expected results:
  - `"978-0-14-032872-1"` → `Isbn("9780140328721")`
  - `"0140621806"` → `Isbn`
  - `"978-0-14-032872-2"` (bad check digit) → `Text`
  - `"doi:10.2307/40149212"` and `"https://doi.org/10.2307/40149212"` → `Doi("10.2307/40149212")`
  - `"https://example.com/a"` → `Address`
  - `"le guin dispossessed"` → `Text`
  - `"le"` → `None`

  The existing `library_lookups_send_only_the_identifier_to_the_expected_host` test must still
  pass unchanged.

- [ ] **Step 2: Run it and see it fail:** `cargo test -p grimoire-tui catalogue::query`. Expected: FAIL to compile.
- [ ] **Step 3: Implement** `query.rs`, and point `Lookup::new` at `query::doi`/`query::isbn`.
- [ ] **Step 4: Run** `cargo test -p grimoire-tui catalogue::query library_lookups`. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "catalogue: classify ISBN, DOI, address or text"`

---

### Task 3: Open Library and Crossref requests and replies

**Files:**
- Create: `src/catalogue/openlibrary.rs`, `src/catalogue/crossref.rs`
- Modify: `src/app/brain/library/lookup.rs` (call these mappers; remove `crossref()` and
  `open_library()` there)
- Test: both new files; fixtures as inline `json!` built from the reply shapes in the spec

**Interfaces:**
- Produces:
  - `pub struct Hit { pub record: sources::Record, pub work: Option<String>, pub editions: Option<u32>, pub authors: Vec<String> }`
    - `work`: the Open Library work ID (`OL166894W`), or `None` for Crossref.
    - `editions`: the edition count, for the "N editions →" label.
    - `authors`: Open Library author keys (`OL27363A`), for Task 4.
  - In `openlibrary`:
    - `search_url(text: &str) -> String`. The path is `https://openlibrary.org/search.json`, the query is form-encoded `q` plus `limit=10`, and the fields are `fields=key,title,author_name,author_key,edition_count,editions,editions.key,editions.title,editions.subtitle,editions.publisher,editions.publish_date,editions.publish_place,editions.isbn,editions.language,editions.edition_name,editions.by_statement`.
    - `parse_search(reply: &Value) -> Vec<Hit>`
    - `editions_url(work: &str, offset: u32) -> String`: `/works/{work}/editions.json?limit=50&offset={offset}`.
    - `parse_editions(reply: &Value, work: &Hit) -> (Vec<Hit>, bool /*more*/)`: English editions (`languages` containing `/languages/eng`) first, otherwise in reply order.
    - `isbn_url(isbn: &str) -> String`: today's Books API URL.
    - `parse_isbn(reply: &Value, isbn: &str) -> Option<Hit>`
  - In `crossref`:
    - `search_url(text: &str) -> String`: `https://api.crossref.org/works?query.bibliographic=…&rows=20&select=DOI,title,author,editor,type,issued,container-title,publisher,volume,issue,page,ISBN,URL`.
    - `parse_search(reply: &Value) -> Vec<Hit>`: filters to the kept types; at most 8.
    - `doi_url(doi: &str) -> String` and `parse_doi(reply: &Value, doi: &str) -> Option<Hit>`.
  - Every `Hit.record` carries `"x-wros-catalogue": {"openlibrary_work": …, "openlibrary_edition": …}` or `{"doi": …}`, which Task 8 uses to join works.
  - CSL type mapping (extend today's `crossref()`):
    - `book`/`monograph`/`edited-book` → `book`
    - `book-chapter` → `chapter`
    - `proceedings-article` → `paper-conference`
    - `report` → `report`
    - `dissertation` → `thesis`
    - `reference-entry` → `entry-encyclopedia`
    - `posted-content` → `article`
    - otherwise → `article-journal`
  - A translator comes from Open Library's `by_statement` when it matches
    `(?i)translated (?:from the \w+ )?by (.+?)(?:[.;]|$)`. It becomes `translator` as
    literal-named entries, which Task 4 re-splits.

- [ ] **Step 1: Write the failing tests**
  - `open_library_search_rows_use_the_matching_edition`:
    - Fixture: a work titled `"Преступление и наказание"` with `edition_count: 1178`, and an edition `{title:"Crime and Punishment", publisher:["Penguin Books"], publish_date:["August 30, 1998"], isbn:["9780140621808","0140621806"], language:["eng"]}`.
    - Assert `record["title"] == "Crime and Punishment"`, `record["publisher"] == "Penguin Books"`, `issued` year 1998, `ISBN == "9780140621808"`, `editions == Some(1178)` and `work == Some("OL166894W")`.
  - `a_sparse_search_doc_still_makes_a_row`: a doc with only `key` and `title`; one `Hit` whose title is that title, with no panic.
  - `editions_list_puts_english_first_and_reports_more`: three entries (German, English, Portuguese) and a `links.next`. The English one comes first, and `more == true`.
  - `crossref_search_drops_reviews_and_keeps_eight`: 12 items, including `type:"peer-review"` and `type:"component"`. The result has ≤8 items, none of them those types; a `proceedings-article` maps to `paper-conference`.
  - `translator_is_read_from_the_by_statement`: `"translated by David McDuff."` → `translator[0]["literal"] == "David McDuff"`.
  - The existing `library_lookups_send_only_the_identifier_to_the_expected_host` still passes.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue::`. Expected: FAIL.
- [ ] **Step 3: Implement** both modules and repoint `lookup.rs`.
- [ ] **Step 4: Run** `cargo test -p grimoire-tui catalogue:: library_lookups`. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "catalogue: Open Library and Crossref search and mapping"`

---

### Task 4: Author names from the Library of Congress

**Files:**
- Create: `src/catalogue/names.rs`
- Test: `src/catalogue/names.rs`

**Interfaces:**
- Consumes: `Fetch`, `Request`, `Kind` (Task 1).
- Produces:
  - `pub fn from_heading(heading: &str) -> Value`. It drops trailing dates (`, 1927-2014`,
    `, 1937-`). It splits "Family, Given" into `{"family","given"}`, and keeps a heading with
    no comma as `{"literal"}`.
  - `pub fn by_rule(name: &str) -> Value`. The last word is the family name, keeping preceding
    lower-case particles `de van von wa da del le la` with it. One word, a name that already has
    a comma, or one containing `&`/`Inc`/`Press`/`University` stays `{"literal"}`.
  - `pub struct Names { cache_path: PathBuf, cache: BTreeMap<String, Value> }`, with:
    - `Names::load(path: PathBuf) -> Self` (the path is `~/.cache/grimoire/catalogue-names.json`);
    - `fn resolve(&mut self, fetch: &dyn Fetch, author_key: &str, as_written: &str) -> Value`;
    - `fn save(&self) -> io::Result<()>`, an atomic write.
  - The chain, run only at pick time:
    1. Fetch `https://openlibrary.org/authors/{key}.json`.
    2. Read `remote_ids.wikidata`.
    3. Run one SPARQL `SELECT ?lc WHERE { wd:{Q} wdt:P244 ?lc }` against
       `https://query.wikidata.org/sparql?format=json&query=…`.
    4. `Kind::Head` on `https://id.loc.gov/authorities/names/{lc}.json`, reading header
       `X-PrefLabel`, then `from_heading`.
    5. Else Open Library's `personal_name`, if it contains a comma, via `from_heading`.
    6. Else `by_rule(as_written)`.

    A failure at any step falls through to the next, so a lookup never fails outright.

- [ ] **Step 1: Write the failing tests**
  - `headings_become_family_and_given`:
    - `"García Márquez, Gabriel, 1927-2014"` → `{"family":"García Márquez","given":"Gabriel"}`
    - `"Beauvoir, Simone de, 1908-1986"` → `{"family":"Beauvoir","given":"Simone de"}`
    - `"Ngũgĩ wa Thiongʼo, 1938-2025"` → `{"literal":"Ngũgĩ wa Thiongʼo"}`
    - `"Murakami, Haruki, 1949-"` → `{"family":"Murakami","given":"Haruki"}`
  - `the_rule_keeps_particles`:
    - `"Ursula K. Le Guin"` → family `"Le Guin"`
    - `"Ludwig van Beethoven"` → family `"van Beethoven"`
    - `"Gabriel García Márquez"` → family `"Márquez"`. This documents the known wrong split,
      which is why the chain prefers the Library of Congress.
    - `"Penguin Random House"` → literal
  - `resolve_follows_open_library_wikidata_and_loc`:
    - `FixtureFetch` replies: OL author `{remote_ids:{wikidata:"Q72334"}}`, SPARQL
      `{"results":{"bindings":[{"lc":{"value":"n80131379"}}]}}`, and a LoC HEAD with
      `X-PrefLabel: Morrison, Toni, 1931-2019`.
    - Expect `{"family":"Morrison","given":"Toni"}`.
    - The requests, in order, contain no text other than `OL…A`, `Q72334` and `n80131379`.
  - `resolve_falls_back_to_personal_name_then_rule`: with no Wikidata ID, `personal_name: "Beauvoir, Simone de"` → family `Beauvoir`; with everything offline, the rule.
  - `resolved_names_are_cached_on_disk`: a second `resolve` for the same key makes no requests, and a fresh `Names::load` of the same path also makes none.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue::names`. Expected: FAIL.
- [ ] **Step 3: Implement** `names.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "catalogue: author names from the Library of Congress"`

---

### Task 5: Web page details

**Files:**
- Create: `src/catalogue/webpage.rs`
- Test: `src/catalogue/webpage.rs`

**Interfaces:**
- Produces: `pub fn page_record(url: &str, head: &[u8], today: chrono::NaiveDate) -> anyhow::Result<Hit>`.
  - It decodes UTF-8 lossily and reads tags with a small hand-written scanner: no HTML crate is
    in the closure, so don't add one.
  - Priority, per field: `citation_*`, then JSON-LD, then `DC.*`, then Open Graph, then
    `<title>`.
  - The result has `type: "webpage"`, `URL: url`, `accessed: {"date-parts": [[y, m, d]]}` from
    `today`, and `container-title` from `og:site_name` or the JSON-LD publisher's name.
  - `Err("This page has no title")` only when every source lacks a title.

- [ ] **Step 1: Write the failing tests**
  - `scholar_meta_wins`: a head with `citation_title`, two `citation_author` tags ("Kim, Ana" and "Lee, Bo") and `citation_publication_date` `2021/03/04`, plus a different `<title>`. Expect the title from `citation_title`, two split authors and issued `[2021,3,4]`.
  - `json_ld_article_is_read_even_inside_a_graph`: an `@graph` array holding a `NewsArticle` with `headline`, `author: {"name":"Ana Kim"}` and `datePublished`.
  - `open_graph_and_title_fall_back`: only `og:title` and `og:site_name`, then only `<title>`.
  - `broken_json_ld_and_odd_bytes_do_not_fail`: invalid JSON-LD plus `0xff` bytes and a `<title>` gives a `<title>`-only record.
  - `accessed_is_today`.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue::webpage`. Expected: FAIL.
- [ ] **Step 3: Implement** `webpage.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "catalogue: read a web page's own citation details"`

---

### Task 6: The search state machine

**Files:**
- Create: `src/catalogue/search.rs`
- Test: `src/catalogue/search.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
  - `pub enum SearchStatus { Off, Idle, Searching, Offline, Error(String), Done }`
  - `pub struct Search` (one per open picker), with:
    - `Search::new(fetch: Arc<dyn Fetch>, online: bool) -> Self`
    - `fn edit(&mut self, line: &str, now: Instant)`: records the line. It never fetches an
      `Address` (Call 2).
    - `fn submit(&mut self, now: Instant)`: Enter or paste. It fetches now, including an
      `Address`.
    - `fn tick(&mut self, now: Instant) -> bool`: starts due requests, collects replies, and
      returns true when `hits()` or `status()` changed.
    - `fn hits(&self) -> &[Hit]`: books first, then Crossref, for the current line only.
    - `fn status(&self) -> &SearchStatus`
    - `fn open_editions(&mut self, index: usize, now: Instant)`,
      `fn more_editions(&mut self, now: Instant)` and `fn close_editions(&mut self)`.
    - `fn editions(&self) -> Option<(&Hit, &[Hit], bool /*loading*/)>`
  - Each request runs on its own `std::thread`, named `"Catalogue search"`, and answers through
    an `mpsc` channel tagged with the line it was for.
  - `Isbn`/`Doi` lines make only the exact request; `Text` makes both catalogue searches;
    `Address` makes one `Kind::Page` fetch on `submit`, mapped through `page_record`.
  - Errors: a `Fetch` error whose message is `"offline"`, or a connect failure, gives
    `Offline`. Any other error gives `Error(one line)`.

- [ ] **Step 1: Write the failing tests** (they pass `Instant`s in; no sleeps except for slow
  fixtures):
  - `nothing_is_sent_before_three_letters_and_a_pause`:
    - `edit("le", t0)`, then `tick(t0 + 600 ms)`: no requests.
    - `edit("le guin", t1)`, then `tick(t1 + 400 ms)`: no requests.
    - `tick(t1 + 500 ms)`: exactly one Open Library and one Crossref request.
  - `an_address_is_fetched_only_on_submit`: `edit("https://example.co", t)` and `tick(t + 2 s)` send nothing; `submit` sends one request, to that address.
  - `a_slow_reply_for_an_old_line_is_dropped`:
    - The Open Library fixture for `q=dune` is slow (300 ms).
    - `edit("dune", t)` and tick past 500 ms; then `edit("dune messiah", …)` and tick until
      both replies have arrived.
    - `hits()` holds only the `dune messiah` rows, and `tick` itself never takes more than
      20 ms (measure it).
  - `one_request_per_catalogue_and_a_second_between`: two lines 600 ms apart. The second Open Library request starts only at ≥1 s after the first.
  - `repeated_lines_come_from_the_cache`: `"dune"`, then `"dun"`, then `"dune"` again; only one Open Library request for `dune`.
  - `offline_is_one_quiet_status`: with an empty fixture, `status() == Offline` and `hits()` is empty.
  - `off_sends_nothing`: `Search::new(fetch, false)` with any line; `status() == Off` and no requests.
  - `editions_open_and_page`: `open_editions(0)` asks `editions_url(work, 0)`; `more_editions` asks offset 50.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue::search`. Expected: FAIL.
- [ ] **Step 3: Implement** `search.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "catalogue: debounced search with stale-reply dropping"`

---

### Task 7: The off switch

**Files:**
- Modify: `crates/grimoire-core/src/settings.rs`
  - field `catalogue_search: bool`, default `true`;
  - parses `catalogue_search = false`;
  - saved.
- Modify: `src/app.rs`
  - `App.catalogue_on: bool`, loaded from Settings;
  - `App.catalogue_fetch: Arc<dyn catalogue::Fetch>` = `default_fetch()`;
  - `fn toggle_catalogue_search(&mut self)`, which saves through `Self::save_setting` like
    `toggle_spellcheck`;
  - a row in `setting_rows()`.
- Modify: `src/palette.rs` (`Action::CatalogueSearch`, listed with the other setting actions)
  and `src/app/shell.rs` (add the action to `Route::WritingManuscript`'s `setting_rows`, after
  `MlaPaperDetails`).
- Test: `crates/grimoire-core/src/settings.rs` tests; `src/shell_ui_tests.rs`

**Interfaces:**
- Produces: `App.catalogue_on`, `App.catalogue_fetch` and
  `App::new_catalogue_search(&self) -> catalogue::Search`. Tasks 9–11 create every `Search`
  through this.

- [ ] **Step 1: Write the failing tests**
  - `catalogue_search_defaults_on_and_round_trips` (settings).
  - `the_manuscript_page_turns_catalogue_search_off`: walk to Settings › Writing › Manuscript and choose the row labelled with the label from Global Constraints. `app.catalogue_on` becomes false, and `Settings::load().catalogue_search == false`.
- [ ] **Step 2: Run** `cargo test -p grimoire-core settings` and `cargo test -p grimoire-tui the_manuscript_page_turns`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same commands. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "settings: search catalogues online, on by default"`

---

### Task 8: Keeping a pick in the Library

**Files:**
- Modify: `src/app/brain/library.rs`
- Test: `src/app/brain/library.rs` tests

**Interfaces:**
- Consumes: `Hit` (Task 3); `PagedBrain::{find_editions, create_work, add_edition, edition}`.
- Produces: `pub(crate) fn keep_in_library(store: &mut PagedBrain, record: &sources::Record) -> anyhow::Result<EditionKey>`.
  - It reuses an existing edition with the same `sources::library_fingerprint`.
  - Otherwise, when an existing edition's `x-wros-catalogue.openlibrary_work` (or `doi`)
    equals the record's, it calls `add_edition(that work, …)` (Call 3).
  - Otherwise it calls `create_work(Place::default(), Edition::new(record, now))`.
  - Candidates come from `find_editions(title, 32)`.
  - `accept_import` keeps its behaviour (it doesn't call this).

- [ ] **Step 1: Write the failing test** `catalogue_picks_join_their_work_and_reuse_editions`:
  - Keep a Penguin 1998 record. Then keep a Vintage 1993 record with the same
    `openlibrary_work`: same `EditionKey.work`, a different edition.
  - Keep the Penguin record again: the same `EditionKey` as the first.
  - Keep a record with a different work: a different work.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue_picks_join`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "library: catalogue picks join their work"`

---

### Task 9: Catalogue results in the writing `@` picker

**Files:**
- Modify: `src/app/overlays/sources.rs`
  - `SourceDialog` gains `catalogue: Option<catalogue::Search>` (set for `Page::Pick` and
    `Page::BrainPick` when opened) and `catalogue_view: Option<usize>` (the editions list being
    shown).
  - Rows after the local ones; key handling; drawing.
- Modify: `crates/grimoire-core/src/citations/mla.rs`:
  `pub fn works_cited_line(record: &sources::Record) -> anyhow::Result<String>` (one-record
  `render` flattened to plain text).
- Modify: `src/app.rs` `tick_deck` (or the overlay tick): when the overlay is
  `ProjectSources` with a `catalogue`, call `Search::tick(Instant::now())` and redraw on `true`.
- Test: `src/ui_tests.rs`

**Interfaces:**
- Consumes: `App::new_catalogue_search`, `Search` (Task 6), `keep_in_library` (Task 8),
  `works_cited_line`.
- Produces: the Catalogue section, as in the spec's mock-up.
  - Header line: `Catalogue` plus `online search on`/`off`, then the status text when Offline
    or Error.
  - Rows: `title · author · publisher year · ISBN|article|DOI`, with `N editions →` when
    `editions > 1`.
- Keys:
  - **typing / Backspace:** `edit`.
  - **paste:** `submit`.
  - **Enter on a catalogue row:** pick it.
  - **Enter on an empty selection:** `submit`.
  - **→ on a book row:** `open_editions`. **←/Esc** in the editions list: `close_editions`.
    Scrolling to the last loaded edition calls `more_editions`.
- What a pick does:
  1. Resolve its authors with `Names` (Task 4) when they came from Open Library.
  2. If `library_store_if_present()` is `Some`: `keep_in_library`, then build a `LibraryPick`
     exactly as `refresh_citation_library` does (key `library-…`,
     `accepted_library_record`).
  3. If it is `None`: add the record straight to the dialog's `sources` snapshot, using
     `suggested_key`.
  4. Go to rc27's page step, `chosen = Some(Citation{…})`.
- In the page step, the first line under the key is `works_cited_line(record)`. `e` opens
  `Page::Form` on that record; saving the form returns to the page step with the edited record
  (Task 10 supplies the form changes).
- Existing behaviour stays: `@@` literal, local rows first, `insert_citation`'s checks.

- [ ] **Step 1: Write the failing tests** (each sets `desk.app.catalogue_fetch` to a
  `FixtureFetch`; none waits for real time except polling ≤2 s):
  - `typing_at_shows_catalogue_books_after_local_sources`: type `@crime and punishment penguin`. Tick until the hits arrive. The screen shows a project source row above `Catalogue`, and a row containing `Penguin Books 1998` and `1178 editions →`.
  - `picking_a_catalogue_book_shows_its_works_cited_line_then_cites_with_a_page`:
    - Enter on the row; the screen shows `García Márquez, Gabriel. One Hundred Years of Solitude.` (fixture-resolved name).
    - Type `194` and press Enter. The manuscript contains `[@library-`, and `, p. 194]`.
    - `sources.json` holds the record, and the Library holds the edition.
  - `the_editions_list_scrolls_and_loads_more`: → on the row shows editions, English first; ↓ to the end requests offset 50.
  - `a_catalogue_pick_without_a_library_still_cites`: no Library store. The citation is written, and `sources.json` holds the record under a suggested key.
  - `a_catalogue_pick_after_the_text_changed_is_refused`: change the editor text while the picker is open; the pick gives rc27's message, and the Library edition remains.
  - `with_catalogue_search_off_at_is_local_only`: `catalogue_on = false`. The header says `online search off`, and the fixture records no requests.
  - `offline_shows_one_quiet_line`: the screen contains `Offline: showing local sources`, once.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui catalogue_ typing_at picking_a_catalogue the_editions_list with_catalogue offline_shows`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command, plus the existing picker tests (`cargo test -p grimoire-tui citation picker`). Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "@: catalogue results, editions and the source line in the page step"`

---

### Task 10: Add a source searches too

**Files:**
- Modify: `src/app/overlays/sources.rs`
  - `TYPES` grows to: Book, Chapter, Journal article, Web page, Conference paper
    (`paper-conference`), Report (`report`), Thesis (`thesis`), Other (`document`).
  - `FIELDS` gains: Translator (`translator`, a name list like editor), Place
    (`publisher-place`), ISBN (`ISBN`), Accessed (`accessed`, a date like issued).
  - `Page::Form` gains a search line above `Type`, with its own `catalogue::Search`.
  - Choosing a result fills `fields` and `kind` via `form(Some(("", &hit.record)))`.
  - A record whose type isn't in `TYPES` keeps its type; show it as `Type [its type]`, the way
    editing an existing source already does.
- Modify: the form save. When the form was filled from a catalogue result and a Library store
  is present, call `keep_in_library` with the **saved** record and attach provenance via
  `accepted_library_record` (Call 1), so the record links to the edition.
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Write the failing tests**
  - `add_a_source_fills_from_a_search_and_saves_the_corrected_version`:
    - Search, pick, change Publisher, save.
    - `sources.json` has the corrected publisher; the Library edition has the corrected
      publisher; `review_library_changes(false)` reports no changes.
  - `a_conference_paper_keeps_its_type`: a Crossref `proceedings-article` result saves as `paper-conference`, not `book`.
  - `the_form_shows_translator_isbn_place_and_accessed`: all four labels are on screen and saved when filled.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui add_a_source a_conference_paper the_form_shows`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command and the existing form tests (`cargo test -p grimoire-tui source`). Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "Add a source: search a catalogue and keep what you correct"`

---

### Task 11: `@` in the Brain and the Library

**Files:**
- Modify: `src/app/overlays/sources.rs` (the `Page::BrainPick` catalogue rows, using Task 9's
  code)
- Modify: `src/app/brain/library.rs`
  - `@` in the desk's browse keys opens a new `Input::Catalogue` search prompt using
    `Search`. A pick calls `keep_in_library` and selects the result in the list.
  - `d` and `l` open the same prompt pre-filled with `doi:` / no prefix, and submit on Enter.
  - With online search off, `d`/`l` still do today's exact lookup on Enter, while `@` shows
    `Online search is off · Settings › Writing › Manuscript`.
  - Drop the `Input::Lookup` hints in favour of `Input::Catalogue`'s
    `"Search Open Library and Crossref · ISBN, DOI, title or author"`.
- Test: `src/ui_tests.rs`

- [ ] **Step 1: Write the failing tests**
  - `brain_at_links_a_catalogue_book_with_no_page_step`: in a Brain note, type `@dispossessed` and pick. The note contains `[The Dispossessed](brain:`, the Library holds it, and no Page prompt appears.
  - `library_at_adds_a_catalogue_edition`: in Library view, press `@`, search and pick. The edition is in the list, and nothing is cited.
  - `library_d_and_l_still_work_with_online_search_off`: with `catalogue_on = false`, `l` + an ISBN + Enter makes one Books API request (the fixture records it).
  - The existing `brain_today_*` and library lookup tests still pass.
- [ ] **Step 2: Run** `cargo test -p grimoire-tui brain_at library_at library_d_and_l`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command and `cargo test -p grimoire-tui brain library`. Expected: PASS.
- [ ] **Step 5: Commit:** `git commit -m "@ searches catalogues in the Brain and the Library"`

---

### Task 12: Docs and the full gate

**Files:**
- Modify: `docs/API_AND_NETWORK_USE.md` "Library lookups". Replace it with:
  - What each site receives: Open Library and Crossref get the search line;
    `query.wikidata.org` and `id.loc.gov` get only author identifiers, at pick time; a web
    page's own site gets a fetch on paste/Enter.
  - When: the 3-letter / 500 ms rule.
  - The off switch and where it lives.
  - The caps, redirects and User-Agent from Global Constraints.
- Modify: the help page that documents `@` (find it with `grep -rln "@" assets/help`). Add:
  catalogue rows, → for editions, the page-step source line, `e` to edit, the off switch.
- Test: the full gate.

- [ ] **Step 1: Run the full suite in NixOS-WSL:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -p grimoire-tui && cargo test -p grimoire-core`. Expected: all pass.
- [ ] **Step 2: Run `tools/nix-gates.sh`** on a copy, in `nix develop`. Expected: pass. Refresh any gate content hash it names for edited help or docs files.
- [ ] **Step 3: Commit:** `git commit -m "docs: catalogue search network use and help"`
