use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use ureq::http::Uri;

const API_CAP: u64 = 1024 * 1024;
const PAGE_CAP: usize = 2 * 1024 * 1024;
const HEAD_END: &[u8] = b"</head>";
const MAX_PAGE_REDIRECTS: u8 = 3;

pub const OFFLINE: &str = "Offline: showing local sources";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Api,
    Page,
    Head,
}

pub struct Request {
    pub url: String,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub trait Fetch: Send + Sync {
    fn fetch(&self, request: &Request) -> Result<Response>;
}

pub struct HttpFetch;

impl Fetch for HttpFetch {
    fn fetch(&self, request: &Request) -> Result<Response> {
        let mut url = https_uri(&request.url)?.to_string();
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .https_only(true)
                .max_redirects(0)
                .http_status_as_error(false)
                .timeout_global(Some(Duration::from_secs(10)))
                .build(),
        );
        let accept = match request.kind {
            Kind::Api | Kind::Head => "application/json",
            Kind::Page => "text/html, application/xhtml+xml",
        };
        let mut redirects = 0;
        loop {
            let call = match request.kind {
                Kind::Api | Kind::Page => agent.get(url.as_str()),
                Kind::Head => agent.head(url.as_str()),
            };
            let mut reply = call
                .header("User-Agent", "WrOS Library")
                .header("Accept", accept)
                .call()?;
            let status = reply.status().as_u16();
            if request.kind == Kind::Page && matches!(status, 301 | 302 | 303 | 307 | 308) {
                let location = reply
                    .headers()
                    .get("location")
                    .context("The web page redirected without a location")?
                    .to_str()
                    .context("Invalid web page redirect location")?;
                redirects += 1;
                url = next_hop(&url, location, redirects)?;
                continue;
            }

            let headers = reply
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let body = match request.kind {
                Kind::Api => reply.body_mut().with_config().limit(API_CAP).read_to_vec()?,
                Kind::Page => read_page_head(reply.body_mut().as_reader())?,
                Kind::Head => Vec::new(),
            };
            return Ok(Response {
                status,
                headers,
                body,
            });
        }
    }
}

fn https_uri(url: &str) -> Result<Uri> {
    let uri: Uri = without_fragment(url).parse().context("Invalid HTTPS URL")?;
    ensure!(
        uri.scheme_str() == Some("https") && uri.authority().is_some(),
        "Catalogue requests require HTTPS"
    );
    Ok(uri)
}

fn without_fragment(url: &str) -> &str {
    url.split_once('#').map_or(url, |(url, _)| url)
}

fn next_hop(from: &str, location: &str, hop: u8) -> Result<String> {
    ensure!(hop <= MAX_PAGE_REDIRECTS, "Too many web page redirects");
    let from = https_uri(from)?;
    let location = without_fragment(location.trim());
    let target = if location.starts_with("//") {
        format!("https:{location}")
    } else if location
        .split(['/', '?'])
        .next()
        .is_some_and(|first| first.contains(':'))
    {
        location.to_owned()
    } else {
        let authority = from.authority().context("Invalid HTTPS URL")?;
        let path = redirect_path(&from, location);
        format!("https://{authority}{path}")
    };
    Ok(https_uri(&target)?.to_string())
}

fn redirect_path(from: &Uri, location: &str) -> String {
    if location.is_empty() {
        return from
            .path_and_query()
            .map_or("/", |path| path.as_str())
            .to_owned();
    }
    if location.starts_with('?') {
        return format!("{}{location}", from.path());
    }

    let (path, query) = location.split_at(location.find('?').unwrap_or(location.len()));
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        let directory = from.path().rsplit_once('/').map_or("", |(dir, _)| dir);
        format!("{directory}/{path}")
    };
    let mut segments = path[1..].split('/').peekable();
    let mut resolved = Vec::new();
    while let Some(segment) = segments.next() {
        match segment {
            "." => {}
            ".." => {
                resolved.pop();
            }
            _ => {
                resolved.push(segment);
                continue;
            }
        }
        if segments.peek().is_none() {
            resolved.push("");
        }
    }
    format!("/{}{query}", resolved.join("/"))
}

fn head_of(body: &[u8], cap: usize) -> &[u8] {
    let body = &body[..body.len().min(cap)];
    match body
        .windows(HEAD_END.len())
        .position(|window| window.eq_ignore_ascii_case(HEAD_END))
    {
        Some(start) => &body[..start + HEAD_END.len()],
        None => body,
    }
}

fn read_page_head(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut reader = reader.take(PAGE_CAP as u64);
    let mut body = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let read = match reader.read(&mut chunk) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        // Recheck the overlap in case the closing tag crosses a chunk boundary.
        let start = body.len().saturating_sub(HEAD_END.len() - 1);
        body.extend_from_slice(&chunk[..read]);
        let head = head_of(&body[start..], PAGE_CAP);
        let tail = &head[head.len().saturating_sub(HEAD_END.len())..];
        if tail.eq_ignore_ascii_case(HEAD_END) {
            body.truncate(start + head.len());
            break;
        }
    }
    Ok(body)
}

#[cfg(test)]
#[derive(Default)]
pub struct FixtureFetch {
    replies: Vec<(String, Response)>,
    delays: Vec<(String, Duration)>,
    requests: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
impl FixtureFetch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(self, url_prefix: &str, status: u16, body: &str) -> Self {
        self.reply_with_headers(url_prefix, status, &[], body)
    }

    pub fn reply_with_headers(
        mut self,
        url_prefix: &str,
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Self {
        self.replies.push((
            url_prefix.to_owned(),
            Response {
                status,
                headers: headers
                    .iter()
                    .map(|(name, value)| (name.to_ascii_lowercase(), (*value).to_owned()))
                    .collect(),
                body: body.as_bytes().to_vec(),
            },
        ));
        self
    }

    pub fn slow(mut self, url_prefix: &str, delay: Duration) -> Self {
        self.delays.push((url_prefix.to_owned(), delay));
        self
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Fetch for FixtureFetch {
    fn fetch(&self, request: &Request) -> Result<Response> {
        self.requests.lock().unwrap().push(request.url.clone());
        let (_, reply) = self
            .replies
            .iter()
            .filter(|(prefix, _)| request.url.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .context("offline")?;
        if let Some((_, delay)) = self
            .delays
            .iter()
            .filter(|(prefix, _)| request.url.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
        {
            std::thread::sleep(*delay);
        }
        Ok(reply.clone())
    }
}

pub fn default_fetch() -> Arc<dyn Fetch> {
    #[cfg(not(test))]
    let fetch = HttpFetch;
    #[cfg(test)]
    let fetch = FixtureFetch::new();
    Arc::new(fetch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_fetch_refuses_plain_http_before_connecting() {
        let error = HttpFetch
            .fetch(&Request {
                url: "http://example.com".into(),
                kind: Kind::Page,
            })
            .unwrap_err();
        assert!(error.to_string().contains("HTTPS"));
    }

    #[test]
    fn page_redirects_must_stay_https_and_stop_after_three() {
        assert!(next_hop("https://a.test/x", "http://a.test/y", 1).is_err());
        assert_eq!(
            next_hop("https://a.test/x", "/y", 1).unwrap(),
            "https://a.test/y"
        );
        assert_eq!(
            next_hop("https://a.test/books/x", "../y?view=full", 3).unwrap(),
            "https://a.test/y?view=full"
        );
        assert!(next_hop("https://a.test/x", "/y", 4).is_err());
    }

    #[test]
    fn head_reading_stops_after_head() {
        let page = b"<html><HEAD><title>t</title></HEAD><body>";
        let cap = 2 * 1024 * 1024;
        let head = b"<html><HEAD><title>t</title></HEAD>";
        assert_eq!(head_of(page, cap), head);
        let (first, rest) = page.split_at(b"<html><HEAD><title>t</title></".len());
        assert_eq!(read_page_head(first.chain(rest)).unwrap().as_slice(), head);
        let large = vec![b'x'; 3 * 1024 * 1024];
        assert_eq!(head_of(&large, cap), &large[..cap]);
    }

    #[test]
    fn fixture_fetch_records_requests_and_is_offline_for_unknown_urls() {
        let prefix = "https://a.test/known";
        let known = "https://a.test/known?q=test";
        let unknown = "https://a.test/unknown";
        let fetch = FixtureFetch::new().reply(prefix, 200, "fixture");
        let response = fetch
            .fetch(&Request {
                url: known.into(),
                kind: Kind::Api,
            })
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body.as_slice(), b"fixture");
        let error = fetch
            .fetch(&Request {
                url: unknown.into(),
                kind: Kind::Api,
            })
            .unwrap_err();
        assert_eq!(error.to_string(), "offline");
        assert_eq!(fetch.requests(), vec![known.to_owned(), unknown.to_owned()]);
    }
}
