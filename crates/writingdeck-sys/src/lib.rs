//! OS capabilities, not a privileged writing application.
//!
//! Linux commands are fixed executables and argument vectors, never shell text.
//! Call the Linux backend on a worker, not the input/render thread. Unsupported
//! capabilities remain unavailable instead of being reported as successful.
pub mod display;
pub mod keyboard;
pub mod linux;
pub mod shortcuts;
pub mod system_views;
pub mod timezone;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Fail closed until a separate locker can acknowledge protected input ownership.
pub const SUSPEND_UNAVAILABLE: &str =
    "Suspend unavailable: secure screen locking has not been qualified.";

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
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub in_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWifi {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAccount {
    pub username: String,
    pub uid: u32,
    pub home: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleAccountStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub status_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
    pub is_cached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResticSnapshot {
    pub id: String,
    pub time: String,
    pub paths: Vec<String>,
    pub hostname: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub count: usize,
    pub packages: Vec<String>,
    pub removals: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDevice {
    pub filesystem: String,
    pub mountpoint: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub use_percent: u8,
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
    // Enrollment is unavailable until a separate protected secret transport is
    // implemented. Requests are logged/serialized and must never carry secrets.
    ConnectWifi {
        ssid: String,
    },
    DisconnectWifi,
    ListSavedWifi,
    ForgetWifi(String),
    Brightness(u8),
    Power(PowerAction),
    PowerSave(bool),
    ListTimezones,
    SetTimezone(String),
    KeyboardPreview(keyboard::Layout),
    KeyboardConfirm(String),
    KeyboardCancel(String),
    DisplayStatus,
    DisplayPreview(display::Warmth),
    DisplayConfirm(String),
    DisplayCancel(String),
    ListLocalAccounts,
    GetGoogleStatus,
    ConnectGoogleAccount,
    DisconnectGoogleAccount,
    ListGoogleDriveFiles,
    MaterializeDriveFile {
        file_id: String,
        local_name: String,
    },
    ListBackups,
    CreateBackup {
        target_dir: String,
    },
    RestoreBackup {
        snapshot_id: String,
        restore_dir: String,
    },
    VerifyBackups,
    CheckUpdates,
    StageUpdates,
    RecoveryStatus,
    RebootRecovery,
    RebootFirmware,
    ListStorageDevices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Snapshot(Snapshot),
    Message(String),
    Timezones(Vec<String>),
    Timezone(String),
    KeyboardPreview { token: String, seconds: u64 },
    DisplayStatus(display::Status),
    DisplayPreview { token: String, seconds: u64 },
    WifiScan(Vec<WifiNetwork>),
    SavedWifi(Vec<SavedWifi>),
    LocalAccounts(Vec<LocalAccount>),
    GoogleStatus(GoogleAccountStatus),
    GoogleDriveFiles(Vec<RemoteFile>),
    Backups(Vec<ResticSnapshot>),
    Updates(UpdateInfo),
    StorageDevices(Vec<StorageDevice>),
}

pub trait Backend: Send {
    fn request(&mut self, request: Request) -> Result<Reply>;
    fn simulated(&self) -> bool {
        false
    }
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
    fn simulated(&self) -> bool {
        true
    }
    fn request(&mut self, request: Request) -> Result<Reply> {
        self.requests.push(request.clone());
        if std::mem::take(&mut self.fail_next) {
            bail!("simulated service unavailable");
        }
        match request {
            Request::ListTimezones => Ok(Reply::Timezones(vec![
                "America/Los_Angeles".into(),
                "Asia/Tokyo".into(),
                "Europe/London".into(),
                "UTC".into(),
            ])),
            Request::SetTimezone(zone) => {
                timezone::validate(&zone)?;
                self.snapshot.timezone = Some(zone.clone());
                Ok(Reply::Timezone(zone))
            }
            Request::Power(PowerAction::Suspend) => bail!(SUSPEND_UNAVAILABLE),
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
            Request::ScanWifi => Ok(Reply::WifiScan(vec![
                WifiNetwork {
                    ssid: "WrOS-Studio".into(),
                    signal: 95,
                    security: "WPA2".into(),
                    in_use: true,
                },
                WifiNetwork {
                    ssid: "WriterDeck-Cafe".into(),
                    signal: 78,
                    security: "WPA2".into(),
                    in_use: false,
                },
                WifiNetwork {
                    ssid: "PublicLibrary".into(),
                    signal: 52,
                    security: "--".into(),
                    in_use: false,
                },
            ])),
            Request::ConnectWifi { .. } => bail!("native credential enrollment is not implemented"),
            Request::DisconnectWifi => {
                self.snapshot.network = Some("disconnected".into());
                Ok(Reply::Message("Wi-Fi disconnected (simulated)".into()))
            }
            Request::ListSavedWifi => Ok(Reply::SavedWifi(vec![SavedWifi {
                uuid: "00000000-0000-4000-8000-000000000001".into(),
                name: "SIMULATED HomeOffice".into(),
            }])),
            Request::ForgetWifi(ssid) => {
                Ok(Reply::Message(format!("Removed saved connection {ssid}")))
            }
            Request::ListLocalAccounts => Ok(Reply::LocalAccounts(vec![LocalAccount {
                username: "writer".into(),
                uid: 1000,
                home: "/home/writer".into(),
                is_current: true,
            }])),
            Request::GetGoogleStatus => Ok(Reply::GoogleStatus(GoogleAccountStatus {
                connected: false,
                email: None,
                status_message: "SIMULATED: Google enrollment is not implemented.".into(),
            })),
            Request::ConnectGoogleAccount => bail!("Google enrollment is not implemented"),
            Request::DisconnectGoogleAccount => Ok(Reply::GoogleStatus(GoogleAccountStatus {
                connected: false,
                email: None,
                status_message: "Disconnected. Credentials cleared.".into(),
            })),
            Request::ListGoogleDriveFiles => Ok(Reply::GoogleDriveFiles(vec![
                RemoteFile {
                    id: "doc-1".into(),
                    name: "The_Quiet_Forest.md".into(),
                    size_bytes: 45200,
                    modified: "2026-10-28".into(),
                    is_cached: true,
                },
                RemoteFile {
                    id: "doc-2".into(),
                    name: "Act2_Revisions.md".into(),
                    size_bytes: 18400,
                    modified: "2026-10-27".into(),
                    is_cached: false,
                },
            ])),
            Request::MaterializeDriveFile {
                file_id,
                local_name,
            } => Ok(Reply::Message(format!(
                "Materialized {file_id} to local file {local_name} (simulated)"
            ))),
            Request::ListBackups => Ok(Reply::Backups(vec![ResticSnapshot {
                id: "a8f3b9c1".into(),
                time: "2026-10-28 14:00".into(),
                paths: vec!["/home/writer/novels".into()],
                hostname: "wros-laptop".into(),
            }])),
            Request::CreateBackup { target_dir } => Ok(Reply::Message(format!(
                "SIMULATED backup for {target_dir}; no files copied or verified"
            ))),
            Request::RestoreBackup {
                snapshot_id,
                restore_dir,
            } => Ok(Reply::Message(format!(
                "SIMULATED restore {snapshot_id} into {restore_dir}; no files changed"
            ))),
            Request::VerifyBackups => Ok(Reply::Message(
                "SIMULATED repository check; no backup has been verified.".into(),
            )),
            Request::CheckUpdates => Ok(Reply::Updates(UpdateInfo {
                count: 0,
                packages: vec![],
                removals: vec![],
                status: "SIMULATED cached package plan; no repository freshness check.".into(),
            })),
            Request::StageUpdates => Ok(Reply::Message(
                "SIMULATED update staging; no update was downloaded or scheduled.".into(),
            )),
            Request::RecoveryStatus => Ok(Reply::Message(
                "SIMULATED recovery status; recovery is not qualified.".into(),
            )),
            Request::RebootRecovery => Ok(Reply::Message(
                "SIMULATED recovery restart; hardware unchanged.".into(),
            )),
            Request::RebootFirmware => Ok(Reply::Message(
                "SIMULATED firmware restart; hardware unchanged.".into(),
            )),
            Request::ListStorageDevices => Ok(Reply::StorageDevices(vec![StorageDevice {
                filesystem: "/dev/nvme0n1p2".into(),
                mountpoint: "/".into(),
                total_bytes: 256 * 1024 * 1024 * 1024,
                used_bytes: 28 * 1024 * 1024 * 1024,
                available_bytes: 228 * 1024 * 1024 * 1024,
                use_percent: 11,
            }])),
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

    #[test]
    fn simulator_handles_everyday_os_requests() {
        let mut backend = Simulated::laptop();
        let Reply::WifiScan(networks) = backend.request(Request::ScanWifi).unwrap() else {
            panic!()
        };
        assert!(!networks.is_empty());
        assert_eq!(networks[0].ssid, "WrOS-Studio");

        assert!(
            backend
                .request(Request::ConnectWifi {
                    ssid: "WrOS-Studio".into()
                })
                .is_err()
        );

        let Reply::LocalAccounts(accounts) = backend.request(Request::ListLocalAccounts).unwrap()
        else {
            panic!()
        };
        assert_eq!(accounts[0].username, "writer");

        let Reply::GoogleStatus(status) = backend.request(Request::GetGoogleStatus).unwrap() else {
            panic!()
        };
        assert!(!status.connected);

        let Reply::GoogleDriveFiles(files) =
            backend.request(Request::ListGoogleDriveFiles).unwrap()
        else {
            panic!()
        };
        assert!(!files.is_empty());

        let Reply::Backups(backups) = backend.request(Request::ListBackups).unwrap() else {
            panic!()
        };
        assert_eq!(backups[0].id, "a8f3b9c1");

        let Reply::Updates(updates) = backend.request(Request::CheckUpdates).unwrap() else {
            panic!()
        };
        assert_eq!(updates.count, 0);

        let Reply::StorageDevices(devices) = backend.request(Request::ListStorageDevices).unwrap()
        else {
            panic!()
        };
        assert_eq!(devices[0].mountpoint, "/");
    }
}
