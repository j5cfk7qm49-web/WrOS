//! The one place this crate touches the network. HTTPS only, short timeouts,
//! bounded response size, no redirects to other schemes, no cookies, no proxy
//! environment surprises beyond what ureq reads, and error values that never carry
//! the request or response.

use crate::Error;
use crate::device::{Response, Transport};
use std::time::Duration;

const MAX_BODY: u64 = 64 * 1024;

pub struct HttpsTransport {
    agent: ureq::Agent,
}

impl HttpsTransport {
    pub fn new() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent("WrOS-Google/0.1")
            .build()
            .into();
        Self { agent }
    }
}

impl Default for HttpsTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for HttpsTransport {
    fn post_form(&mut self, url: &str, form: &[(&str, &str)]) -> Result<Response, Error> {
        if !url.starts_with("https://") {
            return Err(Error::Protocol("refusing a non-HTTPS endpoint"));
        }
        let mut response = self
            .agent
            .post(url)
            .send_form(form.iter().copied())
            .map_err(|_| Error::Unavailable("cannot reach Google"))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|_| Error::Unavailable("cannot read Google's answer"))?;
        Ok(Response { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_endpoints_are_refused_without_any_connection() {
        let mut t = HttpsTransport::new();
        assert!(matches!(t.post_form("http://127.0.0.1:9/token", &[("a", "b")]), Err(Error::Protocol(_))));
    }

    #[test]
    fn an_unreachable_host_is_reported_as_unavailable_without_leaking_the_request() {
        let mut t = HttpsTransport::new();
        let e = t.post_form("https://127.0.0.1:9/token", &[("client_secret", "SHHH")]).unwrap_err();
        assert!(matches!(e, Error::Unavailable(_)));
        assert!(!e.to_string().contains("SHHH"));
    }
}
