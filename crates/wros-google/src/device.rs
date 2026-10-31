//! OAuth 2.0 device flow (RFC 8628 as implemented by Google) over an injected
//! transport. Time is a caller-supplied monotonic count of seconds, so tests never
//! sleep and the provider's `interval`/`slow_down` pacing is enforced here, not by
//! the UI.

use crate::{ALLOWED_SCOPES, Error};
use serde_json::Value;
use zeroize::Zeroizing;

pub const DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const GRANT_DEVICE: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Google's `slow_down` guidance: add five seconds to the polling interval.
const SLOW_DOWN_STEP: u64 = 5;
const MAX_INTERVAL: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// One HTTPS POST of `application/x-www-form-urlencoded` data. Return
/// `Err(Error::Unavailable(..))` when the machine is offline or the provider is
/// unreachable; never put request data in the error.
pub trait Transport {
    fn post_form(&mut self, url: &str, form: &[(&str, &str)]) -> Result<Response, Error>;
}

/// The registered "TVs and Limited Input devices" client. Google documents these
/// values as non-confidential; they are shipped configuration, never user secrets.
#[derive(Clone)]
pub struct Client {
    pub id: String,
    pub secret: String,
}

pub struct Tokens {
    pub access_token: Zeroizing<String>,
    pub refresh_token: Option<Zeroizing<String>>,
    pub expires_in: u64,
    pub scopes: Vec<String>,
}
impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("expires_in", &self.expires_in)
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    /// Ask again after `wait` seconds (no request was made if the interval had not elapsed).
    Pending { wait: u64 },
    Complete(Tokens),
}

/// An in-progress sign-in. The device code is zeroized when this is dropped, which
/// is also how a cancelled enrollment is discarded.
pub struct Flow {
    device_code: Zeroizing<String>,
    pub user_code: String,
    pub verification_url: String,
    expires_at: u64,
    interval: u64,
    next_poll: u64,
}
impl Flow {
    /// Seconds until the code expires, for the on-screen countdown.
    pub fn seconds_left(&self, now: u64) -> u64 {
        self.expires_at.saturating_sub(now)
    }
}
impl std::fmt::Debug for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow")
            .field("user_code", &self.user_code)
            .field("verification_url", &self.verification_url)
            .field("device_code", &"<redacted>")
            .finish()
    }
}

pub struct DeviceFlow<T: Transport> {
    transport: T,
    client: Client,
    scopes: Vec<String>,
}

fn json(response: &Response) -> Result<Value, Error> {
    if response.body.len() > 64 * 1024 {
        return Err(Error::Protocol("oversized response"));
    }
    serde_json::from_str(&response.body).map_err(|_| Error::Protocol("response was not JSON"))
}
fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn provider_error(response: &Response) -> Option<String> {
    json(response).ok().and_then(|v| text(&v, "error"))
}
/// Map statuses that mean "try later" without leaking a body.
fn transient(response: &Response) -> bool {
    response.status == 429 || response.status >= 500 || provider_error(response).as_deref() == Some("rate_limit_exceeded")
}

impl<T: Transport> DeviceFlow<T> {
    /// Refuses any scope outside [`ALLOWED_SCOPES`] before anything is sent.
    pub fn new(transport: T, client: Client, scopes: &[&str]) -> Result<Self, Error> {
        if scopes.is_empty() {
            return Err(Error::ScopeNotAllowed(String::new()));
        }
        for scope in scopes {
            if !ALLOWED_SCOPES.contains(scope) {
                return Err(Error::ScopeNotAllowed((*scope).to_owned()));
            }
        }
        Ok(Self { transport, client, scopes: scopes.iter().map(|s| (*s).to_owned()).collect() })
    }

    pub fn transport(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn start(&mut self, now: u64) -> Result<Flow, Error> {
        let scope = self.scopes.join(" ");
        let response = self.transport.post_form(DEVICE_CODE_URL, &[("client_id", &self.client.id), ("scope", &scope)])?;
        if transient(&response) {
            return Err(Error::Unavailable("Google is busy; try again later"));
        }
        let body = json(&response)?;
        if response.status != 200 {
            return Err(match provider_error(&response).as_deref() {
                Some("invalid_client") => Error::Protocol("OAuth client is not configured for this device type"),
                Some("invalid_scope") => Error::Protocol("OAuth scope was rejected"),
                _ => Error::Protocol("device code request failed"),
            });
        }
        let device_code = text(&body, "device_code").ok_or(Error::Protocol("missing device code"))?;
        let user_code = text(&body, "user_code").ok_or(Error::Protocol("missing user code"))?;
        let verification_url = text(&body, "verification_url")
            .or_else(|| text(&body, "verification_uri"))
            .ok_or(Error::Protocol("missing verification URL"))?;
        // The writer reads this URL off the screen and types it elsewhere: only
        // Google's own https origin is acceptable.
        if !(verification_url.starts_with("https://www.google.com/") || verification_url.starts_with("https://google.com/")) {
            return Err(Error::Protocol("unexpected verification URL"));
        }
        let expires_in = body.get("expires_in").and_then(Value::as_u64).filter(|s| (1..=3600).contains(s)).ok_or(Error::Protocol("bad expiry"))?;
        let interval = body.get("interval").and_then(Value::as_u64).unwrap_or(5).clamp(1, MAX_INTERVAL);
        Ok(Flow {
            device_code: Zeroizing::new(device_code),
            user_code,
            verification_url,
            expires_at: now + expires_in,
            interval,
            next_poll: now + interval,
        })
    }

    pub fn poll(&mut self, flow: &mut Flow, now: u64) -> Result<PollOutcome, Error> {
        if now >= flow.expires_at {
            return Err(Error::Expired);
        }
        if now < flow.next_poll {
            return Ok(PollOutcome::Pending { wait: flow.next_poll - now });
        }
        let response = self.transport.post_form(
            TOKEN_URL,
            &[
                ("client_id", &self.client.id),
                ("client_secret", &self.client.secret),
                ("device_code", &flow.device_code),
                ("grant_type", GRANT_DEVICE),
            ],
        )?;
        if response.status == 200 {
            let tokens = self.tokens(&response)?;
            return Ok(PollOutcome::Complete(tokens));
        }
        if transient(&response) && provider_error(&response).is_none() {
            // Offline-ish failure: keep the flow, retry at the normal pace.
            flow.next_poll = now + flow.interval;
            return Err(Error::Unavailable("Google did not answer; will retry"));
        }
        match provider_error(&response).as_deref() {
            Some("authorization_pending") => {
                flow.next_poll = now + flow.interval;
                Ok(PollOutcome::Pending { wait: flow.interval })
            }
            Some("slow_down") => {
                flow.interval = (flow.interval + SLOW_DOWN_STEP).min(MAX_INTERVAL);
                flow.next_poll = now + flow.interval;
                Ok(PollOutcome::Pending { wait: flow.interval })
            }
            Some("rate_limit_exceeded") => Err(Error::Unavailable("Google is busy; try again later")),
            Some("access_denied" | "admin_policy_enforced" | "org_internal" | "invalid_grant") => Err(Error::Denied),
            Some("expired_token") => Err(Error::Expired),
            Some("invalid_client") => Err(Error::Protocol("OAuth client is not configured for this device type")),
            _ => Err(Error::Protocol("unrecognized sign-in response")),
        }
    }

    fn tokens(&self, response: &Response) -> Result<Tokens, Error> {
        let body = json(response)?;
        let access = text(&body, "access_token").ok_or(Error::Protocol("missing access token"))?;
        let scopes: Vec<String> = text(&body, "scope").unwrap_or_default().split_whitespace().map(str::to_owned).collect();
        // Fail closed: never keep a grant broader than WrOS asked for and supports.
        if scopes.iter().any(|s| !ALLOWED_SCOPES.contains(&s.as_str()) && !s.starts_with("https://www.googleapis.com/auth/userinfo.")) {
            return Err(Error::ScopeNotAllowed("granted scope".into()));
        }
        Ok(Tokens {
            access_token: Zeroizing::new(access),
            refresh_token: text(&body, "refresh_token").map(Zeroizing::new),
            expires_in: body.get("expires_in").and_then(Value::as_u64).unwrap_or(0),
            scopes,
        })
    }

    /// Exchange the stored refresh token for a short-lived access token. A refused
    /// grant (`invalid_grant`) means the sign-in was revoked: the caller should
    /// delete the stored credentials and show "signed out"; local writing continues.
    pub fn refresh(&mut self, refresh_token: &str) -> Result<Tokens, Error> {
        let response = self.transport.post_form(
            TOKEN_URL,
            &[
                ("client_id", &self.client.id),
                ("client_secret", &self.client.secret),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ],
        )?;
        if response.status == 200 {
            return self.tokens(&response);
        }
        if transient(&response) {
            return Err(Error::Unavailable("Google did not answer; will retry"));
        }
        match provider_error(&response).as_deref() {
            Some("invalid_grant") => Err(Error::Denied),
            _ => Err(Error::Protocol("token refresh failed")),
        }
    }

    /// Revoke on disconnect. A token Google no longer knows counts as revoked.
    pub fn revoke(&mut self, token: &str) -> Result<(), Error> {
        let response = self.transport.post_form(REVOKE_URL, &[("token", token)])?;
        if response.status == 200 || provider_error(&response).as_deref() == Some("invalid_token") {
            return Ok(());
        }
        if transient(&response) {
            return Err(Error::Unavailable("Google did not answer; the token stays revoked locally only"));
        }
        Err(Error::Protocol("revocation failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Mock {
        replies: VecDeque<Result<Response, Error>>,
        sent: Vec<(String, Vec<(String, String)>)>,
    }
    impl Transport for Mock {
        fn post_form(&mut self, url: &str, form: &[(&str, &str)]) -> Result<Response, Error> {
            self.sent.push((url.into(), form.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect()));
            self.replies.pop_front().expect("unexpected request")
        }
    }
    fn ok(body: &str) -> Result<Response, Error> {
        Ok(Response { status: 200, body: body.into() })
    }
    fn err(status: u16, code: &str) -> Result<Response, Error> {
        Ok(Response { status, body: format!(r#"{{"error":"{code}"}}"#) })
    }
    const CODE: &str = r#"{"device_code":"DEVICE-SECRET","user_code":"ABCD-EFGH","verification_url":"https://www.google.com/device","expires_in":1800,"interval":5}"#;
    fn client() -> Client {
        Client { id: "client-id".into(), secret: "client-secret".into() }
    }
    fn flow(replies: Vec<Result<Response, Error>>) -> DeviceFlow<Mock> {
        let mock = Mock { replies: replies.into(), sent: vec![] };
        DeviceFlow::new(mock, client(), &["https://www.googleapis.com/auth/drive.file"]).unwrap()
    }

    #[test]
    fn restricted_or_unknown_scopes_are_refused_before_any_request() {
        for scope in ["https://www.googleapis.com/auth/drive", "https://www.googleapis.com/auth/drive.readonly", "https://mail.google.com/"] {
            assert!(matches!(DeviceFlow::new(Mock::default(), client(), &[scope]), Err(Error::ScopeNotAllowed(_))));
        }
        assert!(DeviceFlow::new(Mock::default(), client(), &[]).is_err());
    }

    #[test]
    fn enrollment_completes_after_pending_and_slow_down_and_respects_the_interval() {
        let tokens = r#"{"access_token":"ACCESS","refresh_token":"REFRESH","expires_in":3599,"scope":"https://www.googleapis.com/auth/drive.file","token_type":"Bearer"}"#;
        let mut f = flow(vec![ok(CODE), err(428, "authorization_pending"), err(403, "slow_down"), ok(tokens)]);
        let mut state = f.start(1000).unwrap();
        assert_eq!(state.user_code, "ABCD-EFGH");
        // Too early: no request is made.
        assert!(matches!(f.poll(&mut state, 1002).unwrap(), PollOutcome::Pending { wait: 3 }));
        assert_eq!(f.transport().sent.len(), 1);
        assert!(matches!(f.poll(&mut state, 1005).unwrap(), PollOutcome::Pending { wait: 5 }));
        // slow_down adds five seconds.
        assert!(matches!(f.poll(&mut state, 1010).unwrap(), PollOutcome::Pending { wait: 10 }));
        assert!(matches!(f.poll(&mut state, 1019).unwrap(), PollOutcome::Pending { wait: 1 }));
        let PollOutcome::Complete(t) = f.poll(&mut state, 1020).unwrap() else { panic!() };
        assert_eq!(&**t.access_token, "ACCESS");
        assert_eq!(&**t.refresh_token.as_ref().unwrap(), "REFRESH");
        assert!(!format!("{t:?}{state:?}").contains("ACCESS") && !format!("{state:?}").contains("DEVICE-SECRET"));
        let (url, form) = &f.transport().sent[1];
        assert_eq!(url, TOKEN_URL);
        assert!(form.contains(&("grant_type".into(), GRANT_DEVICE.into())));
    }

    #[test]
    fn denial_expiry_and_cancel_end_cleanly() {
        let mut f = flow(vec![ok(CODE), err(403, "access_denied")]);
        let mut state = f.start(0).unwrap();
        assert_eq!(f.poll(&mut state, 5).unwrap_err(), Error::Denied);
        let mut f = flow(vec![ok(CODE)]);
        let mut state = f.start(0).unwrap();
        assert_eq!(state.seconds_left(100), 1700);
        assert_eq!(f.poll(&mut state, 1800).unwrap_err(), Error::Expired);
        assert_eq!(f.transport().sent.len(), 1, "expired codes are not polled");
        // Cancelling is just dropping the flow: nothing is sent and nothing is stored.
        let mut f = flow(vec![ok(CODE)]);
        drop(f.start(0).unwrap());
        assert_eq!(f.transport().sent.len(), 1);
    }

    #[test]
    fn offline_or_busy_provider_keeps_the_flow_and_never_leaks_a_body() {
        let mut f = flow(vec![ok(CODE), Err(Error::Unavailable("offline")), err(403, "rate_limit_exceeded")]);
        let mut state = f.start(0).unwrap();
        assert!(matches!(f.poll(&mut state, 5), Err(Error::Unavailable(_))));
        assert!(matches!(f.poll(&mut state, 10), Err(Error::Unavailable(_))));
        let mut f = flow(vec![Ok(Response { status: 503, body: "<html>secret internals</html>".into() })]);
        let e = f.start(0).unwrap_err();
        assert!(matches!(e, Error::Unavailable(_)) && !e.to_string().contains("secret internals"));
    }

    #[test]
    fn misconfigured_clients_and_untrusted_verification_urls_are_rejected() {
        let mut f = flow(vec![err(401, "invalid_client")]);
        assert!(matches!(f.start(0), Err(Error::Protocol(_))));
        let phish = CODE.replace("https://www.google.com/device", "https://evil.example/device");
        let mut f = flow(vec![ok(&phish)]);
        assert!(matches!(f.start(0), Err(Error::Protocol(_))));
        let bad_expiry = CODE.replace("1800", "999999");
        let mut f = flow(vec![ok(&bad_expiry)]);
        assert!(matches!(f.start(0), Err(Error::Protocol(_))));
    }

    #[test]
    fn a_broader_grant_than_requested_is_refused() {
        let broad = r#"{"access_token":"A","refresh_token":"R","expires_in":1,"scope":"https://www.googleapis.com/auth/drive"}"#;
        let mut f = flow(vec![ok(CODE), ok(broad)]);
        let mut state = f.start(0).unwrap();
        assert!(matches!(f.poll(&mut state, 5), Err(Error::ScopeNotAllowed(_))));
    }

    #[test]
    fn refresh_distinguishes_revocation_from_being_offline() {
        let fresh = r#"{"access_token":"NEW","expires_in":3599,"scope":"https://www.googleapis.com/auth/drive.file"}"#;
        let mut f = flow(vec![ok(fresh), err(400, "invalid_grant"), Err(Error::Unavailable("offline")), err(500, "backend_error")]);
        let t = f.refresh("REFRESH").unwrap();
        assert_eq!(&**t.access_token, "NEW");
        assert_eq!(f.refresh("REFRESH").unwrap_err(), Error::Denied);
        assert!(matches!(f.refresh("REFRESH"), Err(Error::Unavailable(_))));
        assert!(matches!(f.refresh("REFRESH"), Err(Error::Unavailable(_))));
    }

    #[test]
    fn revocation_treats_unknown_tokens_as_done_and_reports_offline() {
        let mut f = flow(vec![Ok(Response { status: 200, body: String::new() }), err(400, "invalid_token"), Err(Error::Unavailable("offline"))]);
        f.revoke("T").unwrap();
        f.revoke("T").unwrap();
        assert!(matches!(f.revoke("T"), Err(Error::Unavailable(_))));
        assert_eq!(f.transport().sent[0].0, REVOKE_URL);
    }
}
