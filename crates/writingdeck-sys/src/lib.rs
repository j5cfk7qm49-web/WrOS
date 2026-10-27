//! OS capabilities, not a privileged writing application.
//!
//! Linux commands are fixed executables and argument vectors, never shell text.
//! Call the Linux backend on a worker, not the input/render thread. Unsupported
//! capabilities remain unavailable instead of being reported as successful.
pub mod display;
pub mod keyboard;
pub mod linux;
pub mod shortcuts;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub simulated: bool,
    pub os: String,
    pub network: Option<String>,
    pub battery_percent: Option<u8>,
    pub charging: Option<bool>,
    pub brightness_percent: Option<u8>,
    pub timezone: Option<String>,
    pub available_bytes: Option<u64>,
    pub keyboard: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerAction {
    Suspend,
    Shutdown,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Refresh,
    Wifi(bool),
    ScanWifi,
    Brightness(u8),
    Power(PowerAction),
    PowerSave(bool),
    KeyboardPreview(keyboard::Layout),
    KeyboardConfirm(String),
    KeyboardCancel(String),
    DisplayStatus,
    DisplayPreview(display::Warmth),
    DisplayConfirm(String),
    DisplayCancel(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Snapshot(Snapshot),
    Message(String),
    KeyboardPreview { token: String, seconds: u64 },
    DisplayStatus(display::Status),
    DisplayPreview { token: String, seconds: u64 },
}

pub trait Backend: Send {
    fn request(&mut self, request: Request) -> Result<Reply>;
}

/// Explicitly labeled simulator; no process, network, sysfs or power operations.
#[derive(Default)]
pub struct Simulated {
    pub snapshot: Snapshot,
    pub requests: Vec<Request>,
    pub fail_next: bool,
    preview: Option<String>,
    display_preview: Option<(String, display::Warmth)>,
    warmth: display::Warmth,
}

impl Simulated {
    pub fn laptop() -> Self {
        Self {
            snapshot: Snapshot {
                simulated: true,
                os: "WritingDeck OS simulator".into(),
                network: Some("offline (simulated)".into()),
                battery_percent: Some(76),
                charging: Some(false),
                brightness_percent: Some(50),
                timezone: Some("America/Los_Angeles".into()),
                available_bytes: Some(8 * 1024 * 1024 * 1024),
                keyboard: Some("qwerty".into()),
                warnings: vec![],
            },
            ..Self::default()
        }
    }
}

impl Backend for Simulated {
    fn request(&mut self, request: Request) -> Result<Reply> {
        self.requests.push(request.clone());
        if std::mem::take(&mut self.fail_next) {
            bail!("simulated service unavailable");
        }
        match request {
            Request::DisplayStatus => Ok(Reply::DisplayStatus(display::Status {
                warmth: self.warmth,
                available: true,
                applied: true,
                message: "SIMULATED display; hardware unchanged".into(),
            })),
            Request::DisplayPreview(choice) => {
                choice.validate()?;
                let token = "0123456789abcdef0123456789abcdef".to_string();
                self.display_preview = Some((token.clone(), choice));
                Ok(Reply::DisplayPreview { token, seconds: 15 })
            }
            Request::DisplayConfirm(token) | Request::DisplayCancel(token) => {
                let Some((expected, choice)) = self.display_preview.as_ref() else {
                    bail!("no simulated display preview");
                };
                if expected != &token {
                    bail!("invalid simulated display token");
                }
                if matches!(self.requests.last(), Some(Request::DisplayConfirm(_))) {
                    self.warmth = *choice;
                }
                self.display_preview = None;
                Ok(Reply::Message(
                    "SIMULATED display transaction resolved; hardware unchanged".into(),
                ))
            }
            Request::Refresh => Ok(Reply::Snapshot(self.snapshot.clone())),
            Request::Brightness(p) => {
                if !(1..=100).contains(&p) {
                    bail!("brightness must be 1..100");
                }
                self.snapshot.brightness_percent = Some(p);
                Ok(Reply::Snapshot(self.snapshot.clone()))
            }
            Request::KeyboardPreview(layout) => {
                layout.validate()?;
                let token = "simulated-preview-token".to_string();
                self.preview = Some(token.clone());
                Ok(Reply::KeyboardPreview { token, seconds: 15 })
            }
            Request::KeyboardConfirm(token) | Request::KeyboardCancel(token) => {
                if self.preview.as_ref() != Some(&token) {
                    bail!("invalid preview token");
                }
                self.preview = None;
                Ok(Reply::Message(
                    "simulated keyboard transaction resolved".into(),
                ))
            }
            _ => Ok(Reply::Message(
                "simulated request accepted; hardware unchanged".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simulator_never_claims_to_be_hardware() {
        let mut backend = Simulated::laptop();
        let Reply::Snapshot(s) = backend.request(Request::Refresh).unwrap() else {
            panic!()
        };
        assert!(s.simulated);
        backend.fail_next = true;
        assert!(backend.request(Request::Wifi(true)).is_err());
        assert!(backend.request(Request::Brightness(0)).is_err());
    }
}
