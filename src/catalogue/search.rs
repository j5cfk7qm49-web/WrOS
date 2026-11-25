//! Debounced catalogue searches and Open Library edition paging.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use serde_json::Value;

use super::fetch::{Kind, Request};
use super::{Fetch, Hit, Query, crossref, openlibrary, query, webpage};

const PAUSE: Duration = Duration::from_millis(500);
const REQUEST_GAP: Duration = Duration::from_secs(1);
const CACHE_SIZE: usize = 16;
const EDITIONS_PAGE_SIZE: u32 = 50;
const OPEN_LIBRARY: usize = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchStatus {
    Off,
    Idle,
    Searching,
    Offline,
    Error(String),
    Done,
}

pub struct Search {
    fetch: Arc<dyn Fetch>,
    line: String,
    generation: u64,
    current: Option<Line>,
    // Open Library (including edition pages), Crossref, and web pages, in display order.
    lanes: [Lane; 3],
    cache: VecDeque<Cached>,
    hits: Vec<Hit>,
    status: SearchStatus,
    editions: Option<Editions>,
    changed: bool,
}

impl Search {
    pub fn new(fetch: Arc<dyn Fetch>, online: bool) -> Self {
        Self {
            fetch,
            line: String::new(),
            generation: 0,
            current: None,
            lanes: Default::default(),
            cache: VecDeque::new(),
            hits: Vec::new(),
            status: if online {
                SearchStatus::Idle
            } else {
                SearchStatus::Off
            },
            editions: None,
            changed: false,
        }
    }

    pub fn edit(&mut self, line: &str, now: Instant) {
        if self.status == SearchStatus::Off || line == self.line {
            return;
        }
        line.clone_into(&mut self.line);
        self.generation += 1;
        self.current = query::classify(line).map(|query| Line {
            generation: self.generation,
            due: (!matches!(query, Query::Address(_))).then_some(now + PAUSE),
            query,
            requests: Default::default(),
        });
        self.close_editions();
        self.refresh_hits();
        self.refresh_status();
    }

    pub fn submit(&mut self, now: Instant) {
        if let Some(line) = &mut self.current {
            // Enter and paste skip the typing pause, but still go through the request limit.
            if !line.searching() {
                line.due = Some(now);
            }
            self.advance(now);
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        self.advance(now);
        // Edition replies also need to redraw the picker.
        std::mem::take(&mut self.changed)
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    pub fn status(&self) -> &SearchStatus {
        &self.status
    }

    pub fn open_editions(&mut self, index: usize, now: Instant) {
        let Some(work) = self.hits.get(index).filter(|hit| hit.work.is_some()) else {
            return;
        };
        self.generation += 1;
        self.editions = Some(Editions {
            generation: self.generation,
            work: work.clone(),
            hits: Vec::new(),
            next: Some(0),
            loading: true,
            error: None,
        });
        self.changed = true;
        self.advance(now);
    }

    pub fn more_editions(&mut self, now: Instant) {
        if let Some(editions) = &mut self.editions
            && !editions.loading
            && editions.next.is_some()
        {
            editions.loading = true;
            editions.error = None;
            self.changed = true;
            self.advance(now);
        }
    }

    pub fn close_editions(&mut self) {
        if self.editions.take().is_some() {
            self.changed = true;
            self.refresh_status();
        }
    }

    pub fn editions(&self) -> Option<(&Hit, &[Hit], bool)> {
        self.editions
            .as_ref()
            .map(|editions| (&editions.work, editions.hits.as_slice(), editions.loading))
    }

    fn advance(&mut self, now: Instant) {
        for index in 0..self.lanes.len() {
            if let Some((tag, result)) = self.lanes[index].poll() {
                self.complete(index, tag, result);
            }
        }

        if let Some(line) = &mut self.current
            && line.due.is_some_and(|due| now >= due)
        {
            line.due = None;
            let cached = take_cached(&mut self.cache, &line.query);
            line.prepare(cached.as_ref());
            if let Some(cached) = cached {
                self.cache.push_back(cached);
            }
            self.refresh_hits();
        }

        if let Some(editions) = &self.editions
            && editions.loading
            && let Some(offset) = editions.next
            && self.lanes[OPEN_LIBRARY].ready(now)
        {
            let tag = Tag::Editions(editions.generation);
            let job = Job::Editions(editions.work.clone(), offset);
            if let Err(error) = self.lanes[OPEN_LIBRARY].start(now, &self.fetch, tag, job) {
                self.complete(OPEN_LIBRARY, tag, Err(error));
            }
        }

        if let Some(line) = &mut self.current {
            for (lane, request) in self.lanes.iter_mut().zip(&mut line.requests) {
                if !lane.ready(now) {
                    continue;
                }
                let job = match std::mem::replace(request, LineRequest::Running) {
                    LineRequest::Queued(job) => job,
                    previous => {
                        *request = previous;
                        continue;
                    }
                };
                let tag = Tag::Line(line.generation);
                if let Err(error) = lane.start(now, &self.fetch, tag, job) {
                    *request = LineRequest::Finished(Err(error));
                }
            }
        }
        self.refresh_status();
    }

    fn complete(&mut self, index: usize, tag: Tag, result: Reply) {
        match tag {
            Tag::Line(generation) => {
                let Some(line) = &mut self.current else {
                    return;
                };
                if line.generation != generation {
                    return;
                }
                let result = result.map(|rows| rows.hits);
                // Keep successful replies so a partial outage can be retried on its own.
                if let Ok(hits) = &result {
                    let mut cached = take_cached(&mut self.cache, &line.query)
                        .unwrap_or_else(|| Cached::new(line.query.clone()));
                    cached.hits[index] = Some(hits.clone());
                    self.cache.push_back(cached);
                    if self.cache.len() > CACHE_SIZE {
                        self.cache.pop_front();
                    }
                }
                line.requests[index] = LineRequest::Finished(result);
                self.refresh_hits();
            }
            Tag::Editions(generation) => {
                let Some(editions) = &mut self.editions else {
                    return;
                };
                if editions.generation != generation {
                    return;
                }
                editions.loading = false;
                match result {
                    Ok(rows) => {
                        editions.hits.extend(rows.hits);
                        editions.next = editions
                            .next
                            .and_then(|offset| offset.checked_add(EDITIONS_PAGE_SIZE))
                            .filter(|_| rows.more);
                    }
                    Err(error) => editions.error = Some(error),
                }
                self.changed = true;
            }
        }
    }

    fn refresh_hits(&mut self) {
        let hits: Vec<_> = self
            .current
            .iter()
            .flat_map(|line| &line.requests)
            .flat_map(|request| match request {
                LineRequest::Finished(Ok(hits)) => hits.as_slice(),
                _ => &[],
            })
            .cloned()
            .collect();
        if hits != self.hits {
            self.hits = hits;
            self.changed = true;
        }
    }

    fn refresh_status(&mut self) {
        if self.status == SearchStatus::Off {
            return;
        }
        let status = self
            .editions
            .as_ref()
            .and_then(|editions| editions.error.clone())
            .unwrap_or_else(|| {
                self.current.as_ref().map_or(SearchStatus::Idle, |line| {
                    line.status(!self.hits.is_empty())
                })
            });
        if status != self.status {
            self.status = status;
            self.changed = true;
        }
    }
}

struct Line {
    generation: u64,
    query: Query,
    due: Option<Instant>,
    requests: [LineRequest; 3],
}

impl Line {
    fn prepare(&mut self, cached: Option<&Cached>) {
        use LineRequest::{Queued, Unused};

        self.requests = match &self.query {
            Query::Text(text) => [
                Queued(Job::Books(text.clone())),
                Queued(Job::Articles(text.clone())),
                Unused,
            ],
            Query::Isbn(isbn) => [Queued(Job::Isbn(isbn.clone())), Unused, Unused],
            Query::Doi(doi) => [Unused, Queued(Job::Doi(doi.clone())), Unused],
            Query::Address(url) => [Unused, Unused, Queued(Job::Page(url.clone()))],
        };
        if let Some(cached) = cached {
            for (request, hits) in self.requests.iter_mut().zip(&cached.hits) {
                if let Some(hits) = hits {
                    *request = LineRequest::Finished(Ok(hits.clone()));
                }
            }
        }
    }

    fn searching(&self) -> bool {
        self.requests
            .iter()
            .any(|request| matches!(request, LineRequest::Queued(_) | LineRequest::Running))
    }

    fn status(&self, has_hits: bool) -> SearchStatus {
        use LineRequest::Finished;
        use SearchStatus::{Done, Error, Idle, Offline, Searching};

        if self.searching() {
            return Searching;
        }
        let mut status = Idle;
        for request in &self.requests {
            match request {
                Finished(Err(error @ Error(_))) => return error.clone(),
                Finished(Err(Offline)) if !has_hits => status = Offline,
                Finished(_) if status == Idle => status = Done,
                _ => {}
            }
        }
        status
    }
}

#[derive(Default)]
enum LineRequest {
    #[default]
    Unused,
    Queued(Job),
    Running,
    Finished(Result<Vec<Hit>, SearchStatus>),
}

struct Cached {
    query: Query,
    hits: [Option<Vec<Hit>>; 3],
}

impl Cached {
    fn new(query: Query) -> Self {
        Self {
            query,
            hits: Default::default(),
        }
    }
}

fn take_cached(cache: &mut VecDeque<Cached>, query: &Query) -> Option<Cached> {
    let index = cache.iter().position(|entry| &entry.query == query)?;
    cache.remove(index)
}

struct Editions {
    generation: u64,
    work: Hit,
    hits: Vec<Hit>,
    next: Option<u32>,
    loading: bool,
    error: Option<SearchStatus>,
}

#[derive(Clone, Copy)]
enum Tag {
    Line(u64),
    Editions(u64),
}

#[derive(Default)]
struct Lane {
    last_start: Option<Instant>,
    flight: Option<Flight>,
}

struct Flight {
    tag: Tag,
    receiver: mpsc::Receiver<Reply>,
}

impl Lane {
    fn ready(&self, now: Instant) -> bool {
        self.flight.is_none()
            && self
                .last_start
                .is_none_or(|last| now.saturating_duration_since(last) >= REQUEST_GAP)
    }

    fn start(
        &mut self,
        now: Instant,
        fetch: &Arc<dyn Fetch>,
        tag: Tag,
        job: Job,
    ) -> Result<(), SearchStatus> {
        let fetch = Arc::clone(fetch);
        let (send, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("Catalogue search".into())
            .spawn(move || {
                let _ = send.send(job.run(fetch.as_ref()).map_err(search_error));
            })
            .map_err(|error| {
                SearchStatus::Error(format!("Could not start catalogue search ({error})"))
            })?;
        drop(worker);
        self.last_start = Some(now);
        self.flight = Some(Flight { tag, receiver });
        Ok(())
    }

    fn poll(&mut self) -> Option<(Tag, Reply)> {
        let flight = self.flight.as_ref()?;
        let result = match flight.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(SearchStatus::Error("Catalogue search stopped".into()))
            }
        };
        let tag = flight.tag;
        self.flight = None;
        Some((tag, result))
    }
}

type Reply = Result<Rows, SearchStatus>;

struct Rows {
    hits: Vec<Hit>,
    more: bool,
}

enum Job {
    Books(String),
    Articles(String),
    Isbn(String),
    Doi(String),
    Page(String),
    Editions(Hit, u32),
}

impl Job {
    fn run(self, fetch: &dyn Fetch) -> anyhow::Result<Rows> {
        let json = |url| -> anyhow::Result<Value> {
            let body = fetch_body(fetch, url, Kind::Api)?;
            serde_json::from_slice(&body).context("The catalogue reply was incomplete")
        };
        let hits = match self {
            Self::Books(text) => openlibrary::parse_search(&json(openlibrary::search_url(&text))?),
            Self::Articles(text) => crossref::parse_search(&json(crossref::search_url(&text))?),
            Self::Isbn(isbn) => {
                openlibrary::parse_isbn(&json(openlibrary::isbn_url(&isbn))?, &isbn)
                    .into_iter()
                    .collect()
            }
            Self::Doi(doi) => crossref::parse_doi(&json(crossref::doi_url(&doi))?, &doi)
                .into_iter()
                .collect(),
            Self::Page(url) => {
                let body = fetch_body(fetch, url.clone(), Kind::Page)?;
                vec![webpage::page_record(
                    &url,
                    &body,
                    chrono::Local::now().date_naive(),
                )?]
            }
            Self::Editions(work, offset) => {
                let id = work
                    .work
                    .as_deref()
                    .context("The book has no work identifier")?;
                let (hits, more) = openlibrary::parse_editions(
                    &json(openlibrary::editions_url(id, offset))?,
                    &work,
                );
                return Ok(Rows { hits, more });
            }
        };
        Ok(Rows { hits, more: false })
    }
}

fn fetch_body(fetch: &dyn Fetch, url: String, kind: Kind) -> anyhow::Result<Vec<u8>> {
    let response = fetch.fetch(&Request { url, kind })?;
    let status = response.status;
    ensure!(
        (200..300).contains(&status),
        "The catalogue returned HTTP {status}"
    );
    Ok(response.body)
}

fn search_error(error: anyhow::Error) -> SearchStatus {
    let message = error.to_string();
    if message == "offline"
        || error
            .chain()
            .any(|cause| match cause.downcast_ref::<ureq::Error>() {
                Some(ureq::Error::HostNotFound | ureq::Error::ConnectionFailed) => true,
                Some(ureq::Error::Io(error)) => connection_error(error),
                _ => cause
                    .downcast_ref::<io::Error>()
                    .is_some_and(connection_error),
            })
    {
        SearchStatus::Offline
    } else {
        SearchStatus::Error(message.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

fn connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::NetworkDown
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::HostUnreachable
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fetch::FixtureFetch;

    const DUNE: &str = r#"{"docs":[{"key":"/works/OL1W","title":"Dune","edition_count":2}]}"#;
    const DUNE_MESSIAH: &str = r#"{"docs":[{"title":"Dune Messiah"}]}"#;
    const ARTICLES: &str = r#"{"message":{"items":[]}}"#;

    fn dune_fixture() -> FixtureFetch {
        FixtureFetch::new()
            .reply(&openlibrary::search_url("dune"), 200, DUNE)
            .reply(&openlibrary::search_url("dune messiah"), 200, DUNE_MESSIAH)
            .reply(&openlibrary::search_url("dun"), 200, r#"{"docs":[]}"#)
            .reply("https://api.crossref.org/works?", 200, ARTICLES)
    }

    fn poll_until(mut ready: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ready() {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "catalogue reply timed out"
            );
            thread::yield_now();
        }
    }

    fn settle(search: &mut Search, now: Instant) {
        poll_until(|| {
            search.tick(now);
            !matches!(search.status(), SearchStatus::Searching)
        });
    }

    #[test]
    fn nothing_is_sent_before_three_letters_and_a_pause() {
        let fetch = Arc::new(FixtureFetch::new());
        let mut search = Search::new(fetch.clone(), true);
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(600);

        search.edit("le", t0);
        assert!(!search.tick(t0 + Duration::from_millis(600)));
        assert!(fetch.requests().is_empty());
        search.edit("le guin", t1);
        assert!(!search.tick(t1 + Duration::from_millis(400)));
        assert!(fetch.requests().is_empty());

        let due = t1 + Duration::from_millis(500);
        assert!(search.tick(due));
        settle(&mut search, due);
        let mut requests = fetch.requests();
        requests.sort();
        let expected = vec![
            crossref::search_url("le guin"),
            openlibrary::search_url("le guin"),
        ];
        assert_eq!(requests, expected);
    }

    #[test]
    fn an_address_is_fetched_only_on_submit() {
        let url = "https://example.co";
        let fetch = Arc::new(FixtureFetch::new().reply(url, 200, "<title>Example</title>"));
        let mut search = Search::new(fetch.clone(), true);
        let t = Instant::now();
        let submitted = t + Duration::from_secs(2);

        search.edit(url, t);
        assert!(!search.tick(submitted));
        assert!(fetch.requests().is_empty());
        search.submit(submitted);
        poll_until(|| !fetch.requests().is_empty());
        settle(&mut search, submitted);

        assert_eq!(fetch.requests(), vec![url.to_owned()]);
        assert_eq!(search.hits().len(), 1);
        assert_eq!(search.hits()[0].record["type"], "webpage");
        assert_eq!(search.hits()[0].record["URL"], url);
    }

    #[test]
    fn a_slow_reply_for_an_old_line_is_dropped() {
        let fetch = Arc::new(
            dune_fixture().slow(&openlibrary::search_url("dune"), Duration::from_millis(300)),
        );
        let mut search = Search::new(fetch.clone(), true);
        let t = Instant::now();
        let first = t + Duration::from_millis(500);
        let tick = |search: &mut Search, now| {
            let start = Instant::now();
            search.tick(now);
            assert!(start.elapsed() <= Duration::from_millis(20), "tick blocked");
        };

        search.edit("dune", t);
        tick(&mut search, first);
        let old_url = openlibrary::search_url("dune");
        poll_until(|| fetch.requests().contains(&old_url));

        let edited = first + Duration::from_millis(600);
        search.edit("dune messiah", edited);
        let due = edited + Duration::from_millis(500);
        poll_until(|| {
            tick(&mut search, due);
            assert!(
                search
                    .hits()
                    .iter()
                    .all(|hit| hit.record["title"] == "Dune Messiah")
            );
            search.status() == &SearchStatus::Done
        });

        assert_eq!(search.hits().len(), 1);
        assert!(
            fetch
                .requests()
                .contains(&openlibrary::search_url("dune messiah"))
        );
    }

    #[test]
    fn one_request_per_catalogue_and_a_second_between() {
        let fetch = Arc::new(dune_fixture());
        let mut search = Search::new(fetch.clone(), true);
        let t = Instant::now();
        let first = t + Duration::from_millis(500);

        search.edit("dune", t);
        settle(&mut search, first);
        assert_eq!(fetch.requests().len(), 2);

        let edited = t + Duration::from_millis(600);
        search.edit("dun", edited);
        search.tick(edited + Duration::from_millis(500));
        search.tick(first + Duration::from_millis(999));
        assert_eq!(search.status(), &SearchStatus::Searching);
        assert_eq!(search.lanes[OPEN_LIBRARY].last_start, Some(first));
        assert_eq!(fetch.requests().len(), 2);

        let next = first + Duration::from_secs(1);
        settle(&mut search, next);
        assert_eq!(search.lanes[OPEN_LIBRARY].last_start, Some(next));
        let requests = fetch.requests();
        assert_eq!(requests.len(), 4);
        assert!(requests[2..].contains(&openlibrary::search_url("dun")));
    }

    #[test]
    fn repeated_lines_come_from_the_cache() {
        let fetch = Arc::new(dune_fixture());
        let mut search = Search::new(fetch.clone(), true);
        let t = Instant::now();
        let first = t + Duration::from_millis(500);

        search.edit("dune", t);
        settle(&mut search, first);
        search.edit("dun", t + Duration::from_secs(1));
        settle(&mut search, first + Duration::from_secs(1));
        assert!(search.hits().is_empty());
        search.edit("dune", t + Duration::from_secs(2));
        settle(&mut search, first + Duration::from_secs(2));

        let url = openlibrary::search_url("dune");
        let count = fetch
            .requests()
            .iter()
            .filter(|request| request.as_str() == url.as_str())
            .count();
        assert_eq!(count, 1);
        assert_eq!(search.hits().len(), 1);
        assert_eq!(search.hits()[0].record["title"], "Dune");
    }

    #[test]
    fn offline_is_one_quiet_status() {
        let fetch = Arc::new(FixtureFetch::new());
        let mut search = Search::new(fetch, true);
        let t = Instant::now();
        search.edit("dune", t);
        settle(&mut search, t + Duration::from_millis(500));
        assert_eq!(search.status(), &SearchStatus::Offline);
        assert!(search.hits().is_empty());
    }

    #[test]
    fn off_sends_nothing() {
        let fetch = Arc::new(FixtureFetch::new());
        let mut search = Search::new(fetch.clone(), false);
        let t = Instant::now();
        search.edit("dune", t);
        search.submit(t);
        assert!(!search.tick(t + Duration::from_secs(2)));
        assert_eq!(search.status(), &SearchStatus::Off);
        assert!(fetch.requests().is_empty());
    }

    #[test]
    fn editions_open_and_page() {
        let first_page = openlibrary::editions_url("OL1W", 0);
        let second_page = openlibrary::editions_url("OL1W", 50);
        let fetch = Arc::new(
            dune_fixture()
                .reply(
                    &first_page,
                    200,
                    r#"{"entries":[{"title":"Dune"}],"links":{"next":"?limit=50&offset=50"}}"#,
                )
                .reply(
                    &second_page,
                    200,
                    r#"{"entries":[{"title":"Dune, second edition"}]}"#,
                ),
        );
        let mut search = Search::new(fetch.clone(), true);
        let t = Instant::now();
        let found = t + Duration::from_millis(500);
        search.edit("dune", t);
        settle(&mut search, found);

        let opened = found + Duration::from_secs(1);
        search.open_editions(0, opened);
        assert!(search.editions().unwrap().2);
        poll_until(|| {
            search.tick(opened);
            !search.editions().unwrap().2
        });
        assert_eq!(fetch.requests().last(), Some(&first_page));
        assert_eq!(search.editions().unwrap().1.len(), 1);

        let more = opened + Duration::from_secs(1);
        search.more_editions(more);
        poll_until(|| {
            search.tick(more);
            !search.editions().unwrap().2
        });
        assert_eq!(fetch.requests().last(), Some(&second_page));
        let (work, editions, _) = search.editions().unwrap();
        assert_eq!(work.work.as_deref(), Some("OL1W"));
        assert_eq!(editions.len(), 2);
        assert_eq!(editions[1].record["title"], "Dune, second edition");
        search.close_editions();
        assert!(search.editions().is_none());
    }
}
