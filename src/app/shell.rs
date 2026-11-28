//! Adapter from shared presentation to retained editor state and existing services.
//! No manuscript context is swapped, reopened or cloned to navigate Settings.

use super::{App, Key, Overlay, wros};
use crate::music;
use crate::palette::Action;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use wros_shell::{Navigation, Notices, Preferences};

mod accounts;
mod display;
mod draw;
mod input;
mod music_setup;
mod navigation;
mod pages;
mod power;
mod system;
mod update_flow;
mod update_form;
mod wifi;

use accounts::PasswordForm;
use display::KeyboardWizardForm;
pub(super) use draw::draw;
#[cfg(test)]
pub(crate) use draw::settings_box;
use music_setup::{MusicServerForm, MusicSetup};
pub(crate) use navigation::path_to;
#[cfg(test)]
pub(crate) use navigation::{menu, page_label, parent};
#[cfg(feature = "test-driver")]
pub(crate) use pages::driver_page;
#[cfg(test)]
pub(crate) use pages::{page_actions, page_labels};
use system::ClockForm;
use update_flow::UpdateFlow;
use update_form::UpdateBundleForm;
pub(super) use wifi::WIFI_SCAN_FAILED;
use wifi::{SavedWifiAction, WifiForm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Writing,
    /// Shown as "Look and keyboard".
    Interface,
    System,
    /// Shown as "Backups and accounts".
    Accounts,
    /// Updates and Recovery (#90; they sat under System before rc9).
    Updates,
    /// Music (rc21; it sat under Look and keyboard before).
    Music,
}
impl Category {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Writing => "Writing",
            Self::Interface => "Look and keyboard",
            Self::System => "System",
            Self::Accounts => "Backups and accounts",
            Self::Updates => "Updates and recovery",
            Self::Music => "Music",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    Network,
    Display,
    Power,
    Storage,
    DateTime,
    Updates,
    Recovery,
}
impl Panel {
    fn label(self) -> &'static str {
        match self {
            Self::Network => "Wi-Fi and networking",
            Self::Display => "Display",
            Self::Power => "Battery and power",
            Self::Storage => "Storage",
            Self::DateTime => "Date and time",
            Self::Updates => "Updates",
            Self::Recovery => "Recovery",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Settings,
    Category(Category),
    Panel(Panel),
    WarmScreen,
    WarmPreview,
    Timezones,
    TimezoneConfirm,
    WifiNetworks,
    WifiConnect,
    SavedWifi,
    SavedWifiProfile,
    LocalAccounts,
    ChangePassword,
    GoogleAccount,
    GoogleDisconnectConfirm,
    GoogleDrive,
    LinkPhone,
    SyncPanel,
    Backups,
    BackupForm,
    UpdatesView,
    UpdateBundleForm,
    UpdateReview,
    UpdateConfirm,
    UpdateStaged,
    GoBackConfirm,
    RecoveryView,
    RepairConfirm,
    StorageDevicesView,
    InstallerConfirm,
    ManualClock,
    ManualClockConfirm,
    SoundView,
    KeyboardWizard,
    // Settings groups from rc9 (#90). Routes are saved by name in the shell
    // preferences, so retired ones (GoogleDrive, SyncPanel) stay in the enum.
    WritingEditor,
    WritingManuscript,
    WritingGoals,
    WritingRevision,
    LookPage,
    KeyboardPage,
    MusicPage,
    MusicServerForm,
    BackupDestinations,
    BackupKey,
    BackupQueue,
}

pub struct Shell {
    pub backup: super::backup::Backups,
    pub nav: Navigation<Route>,
    pub notices: Notices,
    pub external: bool,
    path: PathBuf,
    seen: Option<grimoire_core::sync::Fingerprint>,
    writable: bool,
    pub display: Option<wros_sys::display::Status>,
    pub warmth: wros_sys::display::Warmth,
    pub warm_wait: bool,
    pub warm_preview: Option<(String, Instant)>,
    pub timezones: Option<Vec<String>>,
    pub timezone_query: String,
    pub timezone_choice: Option<String>,
    pub wifi_networks: Option<Vec<wros_sys::WifiNetwork>>,
    pub wifi_form: Option<WifiForm>,
    pub password_form: Option<PasswordForm>,
    music_server_form: Option<MusicServerForm>,
    music_setup: Option<MusicSetup>,
    music_file_counts: (usize, usize),
    pub wifi_saved: Option<Vec<wros_sys::SavedWifi>>,
    pub wifi_saved_choice: Option<wros_sys::SavedWifi>,
    pub local_accounts: Option<Vec<wros_sys::LocalAccount>>,
    pub google_status: Option<wros_sys::GoogleAccountStatus>,
    pub drive_files: Option<Vec<wros_sys::RemoteFile>>,
    pub backups: Option<Vec<wros_sys::ResticSnapshot>>,
    pub updates: Option<wros_sys::UpdateInfo>,
    pub(super) update_form: Option<UpdateBundleForm>,
    pub update_flow: UpdateFlow,
    pub recovery: Option<wros_sys::RecoveryInfo>,
    pub storage_devices: Option<Vec<wros_sys::StorageDevice>>,
    pub(super) clock_form: Option<ClockForm>,
    pub clock_choice: Option<String>,
    pub audio_status: Option<wros_sys::AudioStatus>,
    pub custom_keymaps: Vec<wros_sys::keyboard::CustomMapRegistration>,
    keyboard_wizard: Option<KeyboardWizardForm>,
    /// A row the menu bar chose while its page was still reading the system status; it is
    /// chosen when the reading finishes, if the writer is still on that page.
    pending_row: Option<SettingsRow>,
}
impl Shell {
    pub(super) fn load(root: &Path, background: bool) -> Self {
        use std::io::Read;
        let path = if background {
            grimoire_core::paths::data_dir().join("wros/ui.json")
        } else {
            root.join(".grimoire/writingdeck/shell.json")
        };
        let mut state = Self {
            backup: super::backup::Backups::new(root, background),
            nav: Navigation::new(Route::Settings),
            notices: Notices::default(),
            external: false,
            path,
            seen: None,
            writable: true,
            display: None,
            warmth: wros_sys::display::Warmth::default(),
            warm_wait: false,
            warm_preview: None,
            timezones: None,
            timezone_query: String::new(),
            timezone_choice: None,
            wifi_networks: None,
            wifi_form: None,
            password_form: None,
            music_server_form: None,
            music_setup: None,
            music_file_counts: (0, 0),
            wifi_saved: None,
            wifi_saved_choice: None,
            local_accounts: None,
            google_status: None,
            drive_files: None,
            backups: None,
            updates: None,
            update_form: None,
            update_flow: UpdateFlow::default(),
            recovery: None,
            storage_devices: None,
            clock_form: None,
            clock_choice: None,
            audio_status: None,
            custom_keymaps: Vec::new(),
            keyboard_wizard: None,
            pending_row: None,
        };
        let result = (|| -> Result<()> {
            if state.path.is_symlink() {
                anyhow::bail!("shell preference symlink preserved");
            }
            let file = match fs::File::open(&state.path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            let mut text = String::new();
            file.take(65_537).read_to_string(&mut text)?;
            if text.len() > 65_536 {
                anyhow::bail!("oversized shell preferences preserved");
            }
            let preferences: Preferences<Route> = serde_json::from_str(&text)?;
            state.nav.restore(preferences).map_err(anyhow::Error::msg)?;
            state.seen = Some(grimoire_core::sync::Fingerprint::of(&text));
            Ok(())
        })();
        if let Err(error) = result {
            state.writable = false;
            state
                .notices
                .push(&format!("Shell preferences preserved: {error}"));
        }
        state
    }
    fn save(&mut self) {
        let result = (|| -> Result<()> {
            if !self.writable {
                anyhow::bail!("original shell preferences preserved");
            }
            let text = serde_json::to_string_pretty(&self.nav.preferences())? + "\n";
            fs::create_dir_all(
                self.path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("missing shell preferences directory"))?,
            )?;
            if self.path.is_symlink() {
                anyhow::bail!("shell preference symlink preserved");
            }
            match grimoire_core::sync::save_guarded(&self.path, &text, self.seen)? {
                grimoire_core::sync::SaveOutcome::Written => {}
                grimoire_core::sync::SaveOutcome::Conflict { .. } => {
                    anyhow::bail!("shell preferences changed externally; original preserved")
                }
            }
            self.seen = Some(grimoire_core::sync::Fingerprint::of(&text));
            Ok(())
        })();
        if let Err(error) = result {
            self.notices.push(&error.to_string());
        }
    }
}

/// The Settings rows the menu bar chooses on the writer's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsRow {
    Sleep,
    Restart,
    ShutDown,
    Install,
    BackUpNow,
}

enum Choice {
    BackupCommonplace,
    YoutubePlayToggle,
    BackupRequest(crate::backup_service::Request),
    BackupForm(super::backup::FormKind, String),
    BackupNow,
    BackupQueueNow,
    SyncSwitch(bool),
    SyncNow,
    SyncConfirmDeletes,
    BackupCancel,
    LinkPhone,
    GoogleDisconnectResolve(bool),
    WifiSelect(wros_sys::WifiNetwork),
    WifiDisconnect,
    PasswordOpen,
    SavedWifiSelect(wros_sys::SavedWifi),
    SavedWifiResolve(SavedWifiAction),
    UpdateBundleOpen,
    UpdateFindMedia,
    UpdateOnlineCheck,
    UpdateOnlineFetch,
    AddDeviceFirmware,
    UpdateContinue,
    UpdateStage,
    UpdateBackupThenStage(bool),
    UpdateAbandon,
    UpdateStagedOpen,
    UpdateRestart,
    UpdateCancelStaged,
    UpdateApplyNow,
    UpdateGoBackOpen,
    UpdateGoBack,
    RepairOpen,
    RepairRun,
    Open(Route),
    /// Show a Google Drive folder ("drive:" is the top) on the Drive files page.
    DriveFolder(String),
    /// Open Import with this Drive book selected.
    DriveCopyBook(String),
    Action(Action),
    System(usize),
    Information,
    View(Route, wros_sys::Request),
    Read(wros_sys::Request),
    WarmAdjust(i16, i16),
    WarmToggle,
    WarmStatus,
    WarmApply,
    WarmResolve(bool),
    TimezoneOpen,
    TimezoneChoose(String),
    TimezoneResolve(bool),
    Lock,
    /// Restart only the writing app; the writer stays signed in.
    RestartApp,
    LidClose,
    GraphicalSession,
    BootScreen(bool),
    InstallerOpen,
    InstallerResolve(bool),
    ClockOpen,
    ClockResolve(bool),
    SoundVolume(i16),
    SoundMuteToggle,
    KeyboardWizardOpen,
}
struct Entry {
    label: String,
    choice: Choice,
}
impl Entry {
    fn new(label: impl Into<String>, choice: Choice) -> Self {
        Self {
            label: label.into(),
            choice,
        }
    }
    fn info(label: impl Into<String>) -> Self {
        Self::new(label, Choice::Information)
    }
    fn kind(&self) -> wros_shell::RowKind {
        match self.choice {
            Choice::Information => wros_shell::RowKind::Info,
            Choice::Open(_)
            | Choice::View(..)
            | Choice::BackupForm(..)
            | Choice::TimezoneOpen
            | Choice::InstallerOpen
            | Choice::UpdateBundleOpen
            | Choice::UpdateStagedOpen
            | Choice::UpdateGoBackOpen
            | Choice::RepairOpen
            | Choice::DriveCopyBook(_) => wros_shell::RowKind::Opens,
            _ => wros_shell::RowKind::Option,
        }
    }
}
