//! OS capabilities, not a privileged writing application.
//!
//! Linux commands are fixed executables and argument vectors, never shell text.
//! Call the Linux backend on a worker, not the input/render thread. Unsupported
//! capabilities remain unavailable instead of being reported as successful.
pub mod boot;
pub mod display;
pub mod keyboard;
pub mod linux;
pub mod paths;
pub mod shortcuts;
pub mod system_views;
pub mod timezone;

#[cfg(test)]
mod update_tests;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// The generic system backend cannot retire the editor's input reader.
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
    /// timedated's NTP setting and whether the clock is synchronized yet.
    #[serde(default)]
    pub network_time: Option<bool>,
    #[serde(default)]
    pub time_synchronized: Option<bool>,
    pub available_bytes: Option<u64>,
    pub keyboard: Option<String>,
    pub warnings: Vec<String>,
}

/// What `wros-update check-online` found on the update server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineUpdate {
    /// available, needs-earlier-update, current, clock, expired, unreachable, unpublished or
    /// invalid.
    pub status: String,
    pub version: Option<String>,
    pub installed: Option<String>,
    /// The required baseline version or store name for a delta update.
    pub needs: Option<String>,
    /// The time this computer read when it checked, `YYYY-MM-DD HH:MM`.
    pub clock: String,
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

/// Journal state of the offline update engine, exactly as the privileged
/// helper recorded it. `Staged` is the only state in which cancel is possible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateState {
    #[default]
    Idle,
    Staged,
    Applying,
    Failed,
    Recovered,
    Completed,
    Interrupted,
}

impl UpdateState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle (no update staged)",
            Self::Staged => "staged (waiting for restart)",
            Self::Applying => "applying (do not power off)",
            Self::Failed => "failed",
            Self::Recovered => "recovered after an interruption",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted (repair needed)",
        }
    }
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
    pub update_state: UpdateState,
    /// The version going back would install ("wros 0.1.0~rc5"), when one is kept.
    #[serde(default)]
    pub previous_version: Option<String>,
    /// A new version has not yet shown that the writing app starts.
    #[serde(default)]
    pub probation: bool,
    /// The last transaction went back to the previous version (by itself, if `went_back_automatically`).
    #[serde(default)]
    pub went_back: bool,
    #[serde(default)]
    pub went_back_automatically: bool,
    pub status: String,
}

/// One exact package change in a reviewed, authenticated update bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePackage {
    pub name: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub action: String,
}

/// Result of authenticating a bundle. `review_id` binds a later stage request to
/// exactly the signed content the writer saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateReview {
    pub review_id: String,
    pub signer: String,
    pub suite: String,
    pub date: String,
    pub valid_until: String,
    pub packages: Vec<UpdatePackage>,
    pub removals: Vec<String>,
    pub backup_recommended: bool,
}

/// The privileged helper's acknowledgement that a reviewed update is staged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStaged {
    pub transaction_id: String,
    pub review_id: String,
    pub packages: usize,
    pub restart_required: bool,
}

/// A staged app-only update installed while WrOS runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateApplied {
    pub transaction_id: String,
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStatus {
    pub available: bool,
    pub volume_percent: Option<u8>,
    pub muted: bool,
    pub message: String,
}

impl Default for AudioStatus {
    fn default() -> Self {
        Self {
            available: true,
            volume_percent: Some(70),
            muted: false,
            message: "SIMULATED mixer: Master 70% unmuted (hardware unchanged)".into(),
        }
    }
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
    BootStatus,
    BootTheme(String),
    BootScreen {
        enabled: bool,
        theme: String,
    },
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
    SetLocalClock(String),
    /// Hand the clock back to network time after a manual setting.
    UseNetworkTime,
    AudioStatus,
    SetVolume(u8),
    SetMute(bool),
    KeyboardPreview(keyboard::Layout),
    KeyboardConfirm(String),
    KeyboardCancel(String),
    DisplayStatus,
    DisplayPreview(display::Warmth),
    DisplayConfirm(String),
    DisplayCancel(String),
    ListLocalAccounts,
    // The writer's own login password (#121). Both secrets are redacted from
    // Debug, skipped by Serialize, and reach /usr/libexec/wros-passwd only on
    // its private stdin.
    ChangePassword {
        #[serde(skip)]
        current: Option<WifiSecret>,
        #[serde(skip)]
        new: Option<WifiSecret>,
    },
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
    /// Authenticate a bundle and list its exact package changes. Unprivileged;
    /// installs nothing.
    ReviewUpdateBundle {
        bundle_dir: String,
    },
    /// Stage the bundle the writer reviewed. The helper re-verifies and refuses
    /// when the freshly computed review identity differs from `review_id`.
    StageUpdates {
        bundle_dir: String,
        review_id: String,
    },
    /// Ask the narrow privileged helper to mount `WROS-UPDATE` media read-only.
    FindUpdateMedia,
    ListImportMedia,
    MountImportMedia(String),
    UnmountImportMedia,
    /// Unprivileged: is a newer signed WrOS published on the fixed update server?
    CheckOnlineUpdates,
    /// The narrow privileged helper downloads and verifies the newer update.
    FetchOnlineUpdate,
    VerifyUpdateBundle {
        bundle_dir: String,
    },
    CancelStagedUpdates,
    /// Install the staged update now; the engine refuses unless it is app-only.
    ApplyStagedUpdate,
    /// Stage going back to the previous version this computer installed.
    GoBackToPreviousVersion,
    /// The writing app drew its first screen: a new version on probation starts.
    ReportUpdateReady,
    RecoverUpdates,
    RecoveryStatus,
    RebootRecovery,
    RebootFirmware,
    ListStorageDevices,
}

impl Request {
    /// What the writer is told while this request runs (#167). Every request has its own
    /// line: there is no catch-all arm, so a new request can't be added without one.
    pub fn activity(&self) -> String {
        match self {
            Request::BootStatus => "Reading the start screen settings…".into(),
            Request::BootTheme(_) => "Saving the start screen colours…".into(),
            Request::BootScreen { enabled: true, .. } => "Turning the start screen on…".into(),
            Request::BootScreen { enabled: false, .. } => "Turning the start screen off…".into(),
            Request::Refresh => "Reading the system status…".into(),
            Request::Wifi(true) => "Turning Wi-Fi on…".into(),
            Request::Wifi(false) => "Turning Wi-Fi off…".into(),
            Request::ScanWifi => "Looking for Wi-Fi networks…".into(),
            Request::ConnectWifi { ssid, .. } | Request::ActivateSavedWifi(ssid) => {
                format!("Connecting to {ssid}…")
            }
            Request::DisconnectWifi => "Disconnecting from Wi-Fi…".into(),
            Request::ListSavedWifi => "Reading the saved Wi-Fi networks…".into(),
            Request::ForgetWifi(name) => format!("Forgetting {name}…"),
            Request::Brightness(_) => "Setting the brightness…".into(),
            Request::Power(PowerAction::Suspend) => "Going to sleep…".into(),
            Request::Power(PowerAction::Shutdown) => "Shutting down…".into(),
            Request::Power(PowerAction::Restart) => "Restarting…".into(),
            Request::PowerSave(true) => "Turning power saving on…".into(),
            Request::PowerSave(false) => "Turning power saving off…".into(),
            Request::ListTimezones => "Reading the time zones…".into(),
            Request::SetTimezone(zone) => format!("Setting the time zone to {zone}…"),
            Request::SetLocalClock(_) => "Setting the clock…".into(),
            Request::UseNetworkTime => "Handing the clock back to network time…".into(),
            Request::AudioStatus => "Reading the sound settings…".into(),
            Request::SetVolume(_) => "Setting the volume…".into(),
            Request::SetMute(true) => "Muting the sound…".into(),
            Request::SetMute(false) => "Unmuting the sound…".into(),
            Request::KeyboardPreview(_) => "Trying the keyboard layout…".into(),
            Request::KeyboardConfirm(_) => "Keeping the keyboard layout…".into(),
            Request::KeyboardCancel(_) => "Putting the keyboard layout back…".into(),
            Request::DisplayStatus => "Reading the display settings…".into(),
            Request::DisplayPreview(_) => "Trying the screen warmth…".into(),
            Request::DisplayConfirm(_) => "Keeping the screen warmth…".into(),
            Request::DisplayCancel(_) => "Putting the screen warmth back…".into(),
            Request::ListLocalAccounts => "Reading the accounts on this computer…".into(),
            Request::ChangePassword { .. } => "Changing your password…".into(),
            Request::GetGoogleStatus => "Reading the Google account status…".into(),
            Request::ConnectGoogleAccount => "Connecting the Google account…".into(),
            Request::DisconnectGoogleAccount => "Disconnecting the Google account…".into(),
            Request::ListGoogleDriveFiles => "Listing your Google Drive files…".into(),
            Request::MaterializeDriveFile { local_name, .. } => {
                format!("Downloading {local_name} from Google Drive…")
            }
            Request::ListBackups => "Reading the backup list…".into(),
            Request::CreateBackup { .. } => "Making a backup…".into(),
            Request::RestoreBackup { .. } => "Restoring the backup…".into(),
            Request::VerifyBackups => "Checking the backups…".into(),
            Request::CheckUpdates => "Reading the cached package plan…".into(),
            Request::ReviewUpdateBundle { .. } => {
                "Checking the update's signatures and what it changes…".into()
            }
            Request::StageUpdates { .. } => {
                "Checking the update again and getting it ready to install…".into()
            }
            Request::FindUpdateMedia => "Looking for a WROS-UPDATE disc or drive…".into(),
            Request::ListImportMedia => "Looking for USB sticks…".into(),
            Request::MountImportMedia(_) => "Opening the USB stick…".into(),
            Request::UnmountImportMedia => "Closing the USB stick…".into(),
            Request::CheckOnlineUpdates => "Asking the WrOS update server for news…".into(),
            Request::FetchOnlineUpdate => "Downloading the WrOS update…".into(),
            Request::VerifyUpdateBundle { .. } => "Checking the update's signatures…".into(),
            Request::CancelStagedUpdates => "Cancelling the staged update…".into(),
            Request::ApplyStagedUpdate => "Installing the update…".into(),
            Request::GoBackToPreviousVersion => {
                "Getting the previous version ready to start…".into()
            }
            Request::ReportUpdateReady => "Telling the system this version starts…".into(),
            Request::RecoverUpdates => "Repairing the update system…".into(),
            Request::RecoveryStatus => "Reading the recovery status…".into(),
            Request::RebootRecovery => "Restarting into recovery…".into(),
            Request::RebootFirmware => "Restarting into the firmware settings…".into(),
            Request::ListStorageDevices => "Reading the storage devices…".into(),
        }
    }
}

/// How far the update engine's download got (#167), from the progress file it keeps in the
/// root-owned online cache while it downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchProgress {
    pub verifying: bool,
    pub version: String,
    pub done: u64,
    pub total: u64,
}

pub const FETCH_PROGRESS_PATH: &str = "/var/cache/wros-update/online/progress.json";

impl FetchProgress {
    pub fn read() -> Option<Self> {
        let file = std::fs::File::open(FETCH_PROGRESS_PATH).ok()?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::Read::take(file, 4096), &mut text).ok()?;
        Self::parse(&text)
    }

    /// Untrusted numbers: anything odd and the plain busy line is shown instead.
    pub fn parse(text: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Record {
            step: String,
            version: String,
            done: u64,
            total: u64,
        }
        let record: Record = serde_json::from_str(text).ok()?;
        let verifying = match record.step.as_str() {
            "download" => false,
            "verify" => true,
            _ => return None,
        };
        let version_ok = !record.version.is_empty()
            && record.version.len() <= 40
            && record
                .version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".~-+_".contains(c));
        (version_ok && record.done <= record.total).then_some(Self {
            verifying,
            version: record.version,
            done: record.done,
            total: record.total,
        })
    }

    /// "Downloading WrOS 0.1.0~rc20: 34 of 72 MB" or "Checking the downloaded WrOS 0.1.0~rc20…".
    pub fn line(&self) -> String {
        if self.verifying {
            return format!(
                "Checking the signatures of the downloaded WrOS {}…",
                self.version
            );
        }
        const MB: u64 = 1_000_000;
        format!(
            "Downloading WrOS {}: {} of {} MB",
            self.version,
            self.done / MB,
            self.total.div_ceil(MB)
        )
    }
}

#[cfg(test)]
mod progress_tests {
    use super::{FetchProgress, Request, elapsed_label};
    use std::time::Duration;

    #[test]
    fn fetch_progress_is_read_as_untrusted_numbers() {
        let good = FetchProgress::parse(
            r#"{"step":"download","version":"0.1.0~rc20","done":1500000,"total":72400000}"#,
        )
        .unwrap();
        assert_eq!(good.line(), "Downloading WrOS 0.1.0~rc20: 1 of 73 MB");
        let verify =
            FetchProgress::parse(r#"{"step":"verify","version":"0.1.0~rc20","done":5,"total":5}"#)
                .unwrap();
        assert!(verify.line().starts_with("Checking the signatures"));
        for bad in [
            r#"{"step":"download","version":"0.1.0~rc20","done":9,"total":5}"#,
            r#"{"step":"other","version":"0.1.0~rc20","done":1,"total":5}"#,
            r#"{"step":"download","version":"\u001b[2J","done":1,"total":5}"#,
            r#"{"step":"download","version":"","done":1,"total":5}"#,
            r#"{"step":"download","version":"1","done":-1,"total":5}"#,
            "not json",
        ] {
            assert_eq!(FetchProgress::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn busy_lines_name_the_request_and_count_once_it_takes_a_while() {
        assert_eq!(
            Request::FetchOnlineUpdate.activity(),
            "Downloading the WrOS update…"
        );
        assert_eq!(elapsed_label(Duration::from_millis(1900)), None);
        assert_eq!(
            elapsed_label(Duration::from_secs(7)).as_deref(),
            Some("0:07")
        );
        assert_eq!(
            elapsed_label(Duration::from_secs(102)).as_deref(),
            Some("1:42")
        );
    }
}

/// How long something has run, for a busy line: nothing for the first two seconds
/// (most requests are done by then), then "0:07", "1:42".
pub fn elapsed_label(elapsed: std::time::Duration) -> Option<String> {
    let secs = elapsed.as_secs();
    (secs >= 2).then(|| format!("{}:{:02}", secs / 60, secs % 60))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportStick {
    pub device: String,
    pub label: String,
    pub mount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    BootStatus(boot::Status),
    Snapshot(Snapshot),
    Message(String),
    Timezones(Vec<String>),
    Timezone(String),
    LocalClock(String),
    NetworkTime,
    Audio(AudioStatus),
    KeyboardPreview {
        token: String,
        seconds: u64,
    },
    DisplayStatus(display::Status),
    DisplayPreview {
        token: String,
        seconds: u64,
    },
    WifiScan(Vec<WifiNetwork>),
    SavedWifi(Vec<SavedWifi>),
    LocalAccounts(Vec<LocalAccount>),
    GoogleStatus(GoogleAccountStatus),
    GoogleDriveFiles(Vec<RemoteFile>),
    Backups(Vec<ResticSnapshot>),
    Updates(UpdateInfo),
    UpdateReview(UpdateReview),
    UpdateStaged(UpdateStaged),
    UpdateApplied(UpdateApplied),
    /// Mounted update media path, or `None` when no `WROS-UPDATE` media exists.
    UpdateMedia(Option<String>),
    ImportMedia(Vec<ImportStick>),
    OnlineUpdate(OnlineUpdate),
    /// Verified downloaded update folder, ready for the ordinary review.
    OnlineUpdateFetched(String),
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
    /// Revision of the simulated bundle on the simulated media. Changing it
    /// models media that was swapped or altered after the writer reviewed it.
    pub bundle_revision: u32,
    /// Path returned by the simulated media mount, if any media is "present".
    pub media: Option<String>,
    pub import_media: Vec<ImportStick>,
    import_mounted: Option<String>,
    /// What the simulated update server reports, and where a fetch lands.
    pub online: Option<OnlineUpdate>,
    /// Whether the simulated staged update changes only the writing app.
    pub staged_live: bool,
    pub fetched: Option<String>,
    /// What the simulated fetch helper refuses with, if anything.
    pub fetch_error: Option<String>,
    pub local_clock: String,
    pub audio: AudioStatus,
    preview: Option<String>,
    display_preview: Option<(String, display::Warmth)>,
    warmth: display::Warmth,
    boot: boot::Status,
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
                rollback_strategy: "SIMULATED: Each update keeps the previous WrOS version \
                    in the start menu. Create an encrypted backup before updating."
                    .into(),
                interrupted_dpkg: false,
                unconfigured_packages: vec![],
                staged_update: false,
                staged_packages: vec![],
                update_state: UpdateState::Idle,
                previous_version: None,
                probation: false,
                went_back: false,
                went_back_automatically: false,
                status: "SIMULATED recovery status: Root filesystem: ext4 · dpkg: clean · \
                    no offline update staged."
                    .into(),
            },
            bundle_revision: 1,
            media: Some("/run/wros-update/media/0".into()),
            import_media: Vec::new(),
            import_mounted: None,
            online: None,
            staged_live: false,
            fetched: None,
            fetch_error: None,
            local_clock: "2026-10-31 12:00:00".into(),
            audio: AudioStatus::default(),
            preview: None,
            display_preview: None,
            warmth: display::Warmth::default(),
            boot: boot::Status::default(),
        }
    }
}

impl Simulated {
    /// Review identity of the simulated bundle at its current revision.
    pub fn review_id(&self) -> String {
        format!("{:064x}", self.bundle_revision)
    }
    fn simulated_review(&self) -> UpdateReview {
        UpdateReview {
            review_id: self.review_id(),
            signer: "SIMULATED-NO-KEY-CHECKED".into(),
            suite: "SIMULATED-trixie-wros".into(),
            date: "2026-10-31".into(),
            valid_until: "2026-12-31".into(),
            packages: vec![
                UpdatePackage {
                    name: "wros".into(),
                    from_version: Some("0.1.0~rc1".into()),
                    to_version: "0.1.0~rc2".into(),
                    action: "upgrade".into(),
                },
                UpdatePackage {
                    name: "wros-help".into(),
                    from_version: None,
                    to_version: "0.1.0~rc2".into(),
                    action: "install".into(),
                },
            ],
            removals: vec![],
            backup_recommended: true,
        }
    }
    pub fn laptop() -> Self {
        Self {
            snapshot: Snapshot {
                simulated: true,
                os: "WrOS (Writer's OS) simulator".into(),
                network: Some("offline (simulated)".into()),
                battery_percent: Some(76),
                charging: Some(false),
                brightness_percent: Some(50),
                timezone: Some("America/Los_Angeles".into()),
                network_time: Some(true),
                time_synchronized: Some(false),
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
            Request::BootStatus => Ok(Reply::BootStatus(self.boot.clone())),
            Request::BootTheme(_) => {
                self.boot.pending = true;
                Ok(Reply::BootStatus(self.boot.clone()))
            }
            Request::BootScreen { enabled, .. } => {
                self.boot.enabled = enabled;
                self.boot.pending = true;
                Ok(Reply::BootStatus(self.boot.clone()))
            }
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
            Request::SetLocalClock(ts) => {
                let normalized = timezone::validate_local_datetime(&ts)?;
                self.local_clock = normalized.clone();
                self.snapshot.network_time = Some(false);
                Ok(Reply::LocalClock(normalized))
            }
            Request::UseNetworkTime => {
                self.snapshot.network_time = Some(true);
                Ok(Reply::NetworkTime)
            }
            Request::AudioStatus => Ok(Reply::Audio(self.audio.clone())),
            Request::SetVolume(volume) => {
                if volume > 100 {
                    bail!("volume must be 0..=100");
                }
                self.audio.volume_percent = Some(volume);
                self.audio.muted = false;
                self.audio.message =
                    format!("SIMULATED mixer: Master {volume}% unmuted (hardware unchanged)");
                Ok(Reply::Audio(self.audio.clone()))
            }
            Request::SetMute(muted) => {
                self.audio.muted = muted;
                self.audio.message = format!(
                    "SIMULATED mixer: Master {} {} (hardware unchanged)",
                    self.audio
                        .volume_percent
                        .map_or_else(|| "unavailable".into(), |v| format!("{v}%")),
                    if muted { "muted" } else { "unmuted" }
                );
                Ok(Reply::Audio(self.audio.clone()))
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
            Request::ChangePassword { current, new } => {
                let (Some(current), Some(new)) = (current.as_ref(), new.as_ref()) else {
                    bail!("type your current password and a new one");
                };
                system_views::validate_login_password(current.as_str())?;
                system_views::validate_login_password(new.as_str())?;
                if current == new {
                    bail!("The new password is the same as the current one.");
                }
                Ok(Reply::Message("SIMULATED: password changed.".into()))
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
            Request::ReviewUpdateBundle { bundle_dir } => {
                system_views::validate_update_bundle_path(&bundle_dir)?;
                Ok(Reply::UpdateReview(self.simulated_review()))
            }
            Request::FindUpdateMedia => Ok(Reply::UpdateMedia(self.media.clone())),
            Request::ListImportMedia => {
                let mut sticks = self.import_media.clone();
                for stick in &mut sticks {
                    if self.import_mounted.as_ref() == Some(&stick.device) {
                        stick
                            .mount
                            .get_or_insert_with(|| "/run/wros-import/media/1".into());
                    } else {
                        stick.mount = None;
                    }
                }
                Ok(Reply::ImportMedia(sticks))
            }
            Request::MountImportMedia(device) => {
                let mut stick = self
                    .import_media
                    .iter()
                    .find(|stick| stick.device == device)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("that USB stick is not present"))?;
                stick
                    .mount
                    .get_or_insert_with(|| "/run/wros-import/media/1".into());
                self.import_mounted = Some(device);
                Ok(Reply::ImportMedia(vec![stick]))
            }
            Request::UnmountImportMedia => {
                self.import_mounted = None;
                Ok(Reply::ImportMedia(Vec::new()))
            }
            Request::CheckOnlineUpdates => Ok(Reply::OnlineUpdate(self.online.clone().unwrap_or(
                OnlineUpdate {
                    status: "unreachable".into(),
                    version: None,
                    installed: None,
                    needs: None,
                    clock: String::new(),
                },
            ))),
            Request::FetchOnlineUpdate => {
                if let Some(error) = &self.fetch_error {
                    bail!("{error}");
                }
                let path = self
                    .fetched
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("SIMULATED: no update server"))?;
                system_views::validate_update_bundle_path(&path)?;
                Ok(Reply::OnlineUpdateFetched(path))
            }
            Request::StageUpdates {
                bundle_dir,
                review_id,
            } => {
                system_views::validate_update_bundle_path(&bundle_dir)?;
                system_views::validate_review_id(&review_id)?;
                if self.recovery.update_state == UpdateState::Staged {
                    bail!("SIMULATED: an update is already staged; cancel it first");
                }
                if review_id != self.review_id() {
                    bail!(
                        "SIMULATED: the update media no longer matches the review you approved; nothing was staged"
                    );
                }
                let review = self.simulated_review();
                self.recovery.update_state = UpdateState::Staged;
                self.recovery.staged_update = true;
                self.recovery.staged_packages =
                    review.packages.iter().map(|p| p.name.clone()).collect();
                self.recovery.status = "SIMULATED recovery status: update staged; it would install at the next restart.".into();
                Ok(Reply::UpdateStaged(UpdateStaged {
                    transaction_id: "simulated-transaction-0001".into(),
                    review_id,
                    packages: review.packages.len(),
                    restart_required: !self.staged_live,
                }))
            }
            Request::VerifyUpdateBundle { bundle_dir } => {
                system_views::validate_update_bundle_path(&bundle_dir)?;
                Ok(Reply::Message(format!(
                    "SIMULATED signature inspection of {bundle_dir}; no files verified or packages installed."
                )))
            }
            Request::ApplyStagedUpdate => {
                if self.recovery.update_state != UpdateState::Staged || !self.staged_live {
                    bail!("SIMULATED: this update needs a restart to install; it stays staged");
                }
                let packages = self.recovery.staged_packages.clone();
                self.recovery.update_state = UpdateState::Idle;
                self.recovery.staged_update = false;
                self.recovery.staged_packages.clear();
                self.recovery.status =
                    "SIMULATED recovery status: last update installed while running.".into();
                Ok(Reply::UpdateApplied(UpdateApplied {
                    transaction_id: "simulated-transaction-0001".into(),
                    packages,
                }))
            }
            Request::GoBackToPreviousVersion => {
                if self.recovery.update_state == UpdateState::Staged {
                    bail!("SIMULATED: an update is already staged; cancel it first");
                }
                let Some(previous) = self.recovery.previous_version.take() else {
                    bail!("SIMULATED: there is no previous version to go back to");
                };
                self.recovery.update_state = UpdateState::Staged;
                self.recovery.staged_update = true;
                self.recovery.staged_packages = vec![previous];
                self.recovery.status =
                    "SIMULATED recovery status: going back staged; it installs at the next restart.".into();
                Ok(Reply::UpdateStaged(UpdateStaged {
                    transaction_id: "simulated-rollback-0001".into(),
                    review_id: "0".repeat(64),
                    packages: 1,
                    restart_required: true,
                }))
            }
            Request::ReportUpdateReady => {
                let was = std::mem::take(&mut self.recovery.probation);
                Ok(Reply::Message(if was {
                    "SIMULATED: this WrOS version starts the writing app.".into()
                } else {
                    "SIMULATED: no new version is waiting to prove it starts.".into()
                }))
            }
            Request::CancelStagedUpdates => {
                if self.recovery.update_state == UpdateState::Staged {
                    self.recovery.update_state = UpdateState::Idle;
                    self.recovery.staged_update = false;
                    self.recovery.staged_packages.clear();
                    self.recovery.status =
                        "SIMULATED recovery status: no offline update staged.".into();
                    Ok(Reply::Message(
                        "SIMULATED: staged update cancelled; nothing was installed.".into(),
                    ))
                } else {
                    Ok(Reply::Message(
                        "SIMULATED: no staged update to cancel.".into(),
                    ))
                }
            }
            Request::RecoverUpdates => {
                let needed = self.recovery.interrupted_dpkg
                    || !self.recovery.unconfigured_packages.is_empty()
                    || matches!(
                        self.recovery.update_state,
                        UpdateState::Interrupted | UpdateState::Failed
                    );
                if self.recovery.update_state == UpdateState::Applying {
                    bail!("SIMULATED: an update is being applied; repair must wait for it to end");
                }
                if needed {
                    self.recovery.interrupted_dpkg = false;
                    self.recovery.unconfigured_packages.clear();
                    self.recovery.update_state = UpdateState::Recovered;
                    Ok(Reply::Message(
                        "SIMULATED: repair finished from retained archives; no real packages changed."
                            .into(),
                    ))
                } else {
                    Ok(Reply::Message("SIMULATED: nothing needed repair.".into()))
                }
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
        assert!(
            recovery
                .rollback_strategy
                .contains("Each update keeps the previous WrOS version in the start menu.")
        );
        assert!(
            recovery
                .rollback_strategy
                .contains("Create an encrypted backup")
        );
        assert!(
            recovery
                .status
                .contains("Root filesystem: ext4 · dpkg: clean")
        );

        let Reply::StorageDevices(devices) = backend.request(Request::ListStorageDevices).unwrap()
        else {
            panic!()
        };
        assert_eq!(devices[0].mountpoint, "/");
    }
}
