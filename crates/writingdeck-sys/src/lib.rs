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

/// Protected Wi-Fi passphrase. Never written to Debug output, command-line
/// arguments, or public serialized requests, and overwritten on drop.
#[derive(Clone, Default)]
pub struct WifiSecret(String);

impl WifiSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for WifiSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\"[hidden]\"")
    }
}

impl PartialEq for WifiSecret {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for WifiSecret {}

impl Drop for WifiSecret {
    fn drop(&mut self) {
        let bytes = unsafe { self.0.as_bytes_mut() };
        for byte in bytes {
            unsafe {
                std::ptr::write_volatile(byte, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

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
pub struct RecoveryInfo {
    pub root_filesystem: String,
    pub root_device: String,
    pub separate_home: bool,
    pub rollback_capable: bool,
    pub rollback_strategy: String,
    pub interrupted_dpkg: bool,
    pub unconfigured_packages: Vec<String>,
    pub staged_update: bool,
    pub staged_packages: Vec<String>,
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
    // Passwords are redacted from Debug and skipped by Serialize; the Linux
    // adapter sends them only via private stdin to nmcli --ask.
    ConnectWifi {
        ssid: String,
        #[serde(skip)]
        password: Option<WifiSecret>,
    },
    ActivateSavedWifi(String),
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
    VerifyUpdateBundle {
        bundle_dir: String,
    },
    CancelStagedUpdates,
    RecoverUpdates,
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
    Recovery(RecoveryInfo),
    StorageDevices(Vec<StorageDevice>),
}

pub trait Backend: Send {
    fn request(&mut self, request: Request) -> Result<Reply>;
    fn simulated(&self) -> bool {
        false
    }
}

/// Explicitly labeled simulator; no process, network, sysfs or power operations.
pub struct Simulated {
    pub snapshot: Snapshot,
    pub requests: Vec<Request>,
    pub fail_next: bool,
    pub wifi_networks: Vec<WifiNetwork>,
    pub saved_wifi: Vec<SavedWifi>,
    pub recovery: RecoveryInfo,
    preview: Option<String>,
    display_preview: Option<(String, display::Warmth)>,
    warmth: display::Warmth,
}

impl Default for Simulated {
    fn default() -> Self {
        Self {
            snapshot: Snapshot::default(),
            requests: Vec::new(),
            fail_next: false,
            wifi_networks: vec![
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
            ],
            saved_wifi: vec![SavedWifi {
                uuid: "00000000-0000-4000-8000-000000000001".into(),
                name: "SIMULATED HomeOffice".into(),
            }],
            recovery: RecoveryInfo {
                root_filesystem: "ext4".into(),
                root_device: "/dev/nvme0n1p2".into(),
                separate_home: false,
                rollback_capable: false,
                rollback_strategy: "SIMULATED ext4 root: automatic filesystem snapshot rollback is not available (installing Snapper on ext4 does not provide rollback). Create an encrypted Restic backup before applying updates.".into(),
                interrupted_dpkg: false,
                unconfigured_packages: vec![],
                staged_update: false,
                staged_packages: vec![],
                status: "SIMULATED recovery status: Root filesystem: ext4 (no automatic filesystem rollback) · dpkg: clean · no offline update staged.".into(),
            },
            preview: None,
            display_preview: None,
            warmth: display::Warmth::default(),
        }
    }
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
            Request::ScanWifi => Ok(Reply::WifiScan(self.wifi_networks.clone())),
            Request::ConnectWifi { ssid, password } => {
                system_views::validate_wifi_ssid(&ssid)?;
                let is_open = self
                    .wifi_networks
                    .iter()
                    .find(|n| n.ssid == ssid)
                    .is_some_and(|n| n.security.is_empty() || n.security == "--");
                if is_open {
                    if password.is_some() {
                        bail!("open Wi-Fi network does not accept a password");
                    }
                } else {
                    let Some(secret) = password.as_ref() else {
                        bail!("password required for secured Wi-Fi network");
                    };
                    system_views::validate_wifi_password(secret.as_str())?;
                    if secret.as_str() == "wrong-password" {
                        bail!("Wi-Fi authentication failed; check password and retry");
                    }
                }
                for network in &mut self.wifi_networks {
                    network.in_use = network.ssid == ssid;
                }
                if !self.saved_wifi.iter().any(|s| s.name == ssid) {
                    let next = self.saved_wifi.len() + 1;
                    self.saved_wifi.push(SavedWifi {
                        uuid: format!("00000000-0000-4000-8000-{next:012x}"),
                        name: ssid.clone(),
                    });
                }
                self.snapshot.network = Some(format!("connected ({ssid}, simulated)"));
                Ok(Reply::Message(format!(
                    "Connected to Wi-Fi network {ssid} (simulated)"
                )))
            }
            Request::ActivateSavedWifi(uuid) => {
                system_views::validate_saved_wifi_uuid(&uuid)?;
                let Some(profile) = self.saved_wifi.iter().find(|s| s.uuid == uuid).cloned() else {
                    bail!("saved Wi-Fi profile not found");
                };
                for network in &mut self.wifi_networks {
                    network.in_use = network.ssid == profile.name;
                }
                self.snapshot.network = Some(format!("connected ({}, simulated)", profile.name));
                Ok(Reply::Message(format!(
                    "Connected saved Wi-Fi profile {} (simulated)",
                    profile.name
                )))
            }
            Request::DisconnectWifi => {
                for network in &mut self.wifi_networks {
                    network.in_use = false;
                }
                self.snapshot.network = Some("disconnected".into());
                Ok(Reply::Message("Wi-Fi disconnected (simulated)".into()))
            }
            Request::ListSavedWifi => Ok(Reply::SavedWifi(self.saved_wifi.clone())),
            Request::ForgetWifi(uuid) => {
                system_views::validate_saved_wifi_uuid(&uuid)?;
                let before = self.saved_wifi.len();
                self.saved_wifi.retain(|s| s.uuid != uuid);
                if self.saved_wifi.len() == before {
                    bail!("saved Wi-Fi profile not found");
                }
                Ok(Reply::Message(format!("Removed saved connection {uuid}")))
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
            Request::VerifyUpdateBundle { bundle_dir } => {
                system_views::validate_update_bundle_path(&bundle_dir)?;
                Ok(Reply::Message(format!(
                    "SIMULATED signature inspection of {bundle_dir}; no files verified or packages installed."
                )))
            }
            Request::CancelStagedUpdates | Request::RecoverUpdates => {
                bail!(
                    "Update installation and package repair are unavailable; no system changes were made"
                )
            }
            Request::RecoveryStatus => Ok(Reply::Recovery(self.recovery.clone())),
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
                    ssid: "WrOS-Studio".into(),
                    password: None,
                })
                .is_err()
        );
        let secret = WifiSecret::new("PRIVATE_WIFI_PASSPHRASE_123");
        let req = Request::ConnectWifi {
            ssid: "WriterDeck-Cafe".into(),
            password: Some(secret),
        };
        assert!(!format!("{req:?}").contains("PRIVATE_WIFI_PASSPHRASE_123"));
        assert!(
            !serde_json::to_string(&req)
                .unwrap()
                .contains("PRIVATE_WIFI_PASSPHRASE_123")
        );
        assert!(backend.request(req).is_ok());

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

        let Reply::Recovery(recovery) = backend.request(Request::RecoveryStatus).unwrap() else {
            panic!()
        };
        assert_eq!(recovery.root_filesystem, "ext4");
        assert!(!recovery.rollback_capable);

        let Reply::StorageDevices(devices) = backend.request(Request::ListStorageDevices).unwrap()
        else {
            panic!()
        };
        assert_eq!(devices[0].mountpoint, "/");
    }
}
