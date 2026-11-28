#![warn(clippy::print_stdout, clippy::print_stderr)]

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
mod progress;
pub mod shortcuts;
mod simulated;
pub mod system_views;
pub mod timezone;

#[cfg(test)]
mod update_tests;

pub use progress::{FETCH_PROGRESS_PATH, FetchProgress, elapsed_label};
pub use simulated::Simulated;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The generic system backend cannot retire the editor's input reader.
pub const SUSPEND_UNAVAILABLE: &str =
    "Suspend unavailable: secure screen locking has not been qualified.";

/// Protected Wi-Fi passphrase. Never written to Debug output, command-line
/// arguments, or public serialized requests, and overwritten on drop.
#[derive(Clone, Default)]
pub struct WifiSecret(String);

impl WifiSecret {
    #[must_use]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirmwareStatus {
    Added,
    NothingNeeded,
    NotInChannel,
    UpdateFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareResult {
    pub status: FirmwareStatus,
    pub packages: Vec<String>,
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
    /// Add signed firmware for the devices detected on the running system.
    AddDeviceFirmware,
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
            Request::AddDeviceFirmware => "Checking for device firmware…".into(),
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
    DeviceFirmware(FirmwareResult),
    Recovery(RecoveryInfo),
    StorageDevices(Vec<StorageDevice>),
}

pub trait Backend: Send {
    fn request(&mut self, request: Request) -> Result<Reply>;
    fn simulated(&self) -> bool {
        false
    }
}
