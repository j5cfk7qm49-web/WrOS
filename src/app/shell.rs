//! Adapter from shared presentation to retained editor state and existing services.
//! No manuscript context is swapped, reopened or cloned to navigate Settings.
use super::*;
use grimoire_core::tutorial;
use serde::{Deserialize, Serialize};
use wros_shell::{Navigation, Notices, Preferences};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Writing,
    Interface,
    System,
    Accounts,
}
impl Category {
    const ALL: [Self; 4] = [Self::Writing, Self::Interface, Self::System, Self::Accounts];
    fn label(self) -> &'static str {
        match self {
            Self::Writing => "Writing",
            Self::Interface => "Interface",
            Self::System => "System",
            Self::Accounts => "Accounts",
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
    const ALL: [Self; 7] = [
        Self::Network,
        Self::Display,
        Self::Power,
        Self::Storage,
        Self::DateTime,
        Self::Updates,
        Self::Recovery,
    ];
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
    GoogleAccount,
    GoogleDisconnectConfirm,
    GoogleDrive,
    SyncPanel,
    Backups,
    BackupForm,
    UpdatesView,
    UpdateBundleForm,
    RecoveryView,
    StorageDevicesView,
    InstallerConfirm,
}

pub struct WifiForm {
    pub network: writingdeck_sys::WifiNetwork,
    pub password: Zeroizing<String>,
    pub selected: usize,
}
impl WifiForm {
    pub fn requires_password(&self) -> bool {
        !self.network.security.is_empty() && self.network.security != "--"
    }
    pub fn action_count(&self) -> usize {
        if self.requires_password() {
            if self.network.in_use { 4 } else { 3 }
        } else if self.network.in_use {
            3
        } else {
            2
        }
    }
    pub fn rows(&self) -> Vec<String> {
        if self.requires_password() {
            let mut rows = vec![
                format!(
                    "Password (hidden): {}",
                    if self.password.is_empty() {
                        ""
                    } else {
                        "[hidden]"
                    }
                ),
                "Cancel and return to Wi-Fi networks".into(),
                format!("Connect to {}", self.network.ssid),
            ];
            if self.network.in_use {
                rows.push("Disconnect current Wi-Fi connection".into());
            }
            rows.push(format!(
                "Network: {} · Signal: {}% · Security: {}",
                self.network.ssid, self.network.signal, self.network.security
            ));
            rows.push(
                "WPA password: 8–63 bytes, or a 64-digit hexadecimal key. Input stays hidden."
                    .into(),
            );
            rows.push("Before Connect, Escape discards this form and password.".into());
            rows
        } else {
            let mut rows = vec![
                "Cancel and return to Wi-Fi networks".into(),
                format!("Connect to open network {}", self.network.ssid),
            ];
            if self.network.in_use {
                rows.push("Disconnect current Wi-Fi connection".into());
            }
            rows.push(format!(
                "Network: {} · Signal: {}% · Security: open",
                self.network.ssid, self.network.signal
            ));
            rows.push("Open networks do not encrypt wireless traffic.".into());
            rows
        }
    }
}

pub struct UpdateBundleForm {
    pub bundle_dir: String,
    pub selected: usize,
}
impl UpdateBundleForm {
    pub fn rows(&self) -> Vec<String> {
        vec![
            format!("Signed update bundle path: {}", self.bundle_dir),
            "Cancel and return without staging updates".into(),
            "Verify signatures and contents (inspection only)".into(),
            "Requires Debian gpgv verification of InRelease (or Release + Release.gpg),".into(),
            "authenticated Packages SHA-256, and matching SHA-256 for every .deb archive.".into(),
            "Create an encrypted Restic backup first; ext4 has no automatic snapshot rollback."
                .into(),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SavedWifiAction {
    Cancel,
    Connect,
    Forget,
}

pub struct Shell {
    pub backup: super::backup::Backups,
    pub nav: Navigation<Route>,
    pub notices: Notices,
    pub external: bool,
    origin: Option<Overlay>,
    path: PathBuf,
    seen: Option<grimoire_core::sync::Fingerprint>,
    writable: bool,
    pub display: Option<writingdeck_sys::display::Status>,
    pub warmth: writingdeck_sys::display::Warmth,
    pub warm_wait: bool,
    pub warm_preview: Option<(String, Instant)>,
    pub timezones: Option<Vec<String>>,
    pub timezone_query: String,
    pub timezone_choice: Option<String>,
    pub wifi_networks: Option<Vec<writingdeck_sys::WifiNetwork>>,
    pub wifi_form: Option<WifiForm>,
    pub wifi_saved: Option<Vec<writingdeck_sys::SavedWifi>>,
    pub wifi_saved_choice: Option<writingdeck_sys::SavedWifi>,
    pub local_accounts: Option<Vec<writingdeck_sys::LocalAccount>>,
    pub google_status: Option<writingdeck_sys::GoogleAccountStatus>,
    pub drive_files: Option<Vec<writingdeck_sys::RemoteFile>>,
    pub backups: Option<Vec<writingdeck_sys::ResticSnapshot>>,
    pub updates: Option<writingdeck_sys::UpdateInfo>,
    pub update_form: Option<UpdateBundleForm>,
    pub recovery: Option<writingdeck_sys::RecoveryInfo>,
    pub storage_devices: Option<Vec<writingdeck_sys::StorageDevice>>,
}
impl Shell {
    pub fn load(root: &Path, background: bool) -> Self {
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
            origin: None,
            path,
            seen: None,
            writable: true,
            display: None,
            warmth: writingdeck_sys::display::Warmth::default(),
            warm_wait: false,
            warm_preview: None,
            timezones: None,
            timezone_query: String::new(),
            timezone_choice: None,
            wifi_networks: None,
            wifi_form: None,
            wifi_saved: None,
            wifi_saved_choice: None,
            local_accounts: None,
            google_status: None,
            drive_files: None,
            backups: None,
            updates: None,
            update_form: None,
            recovery: None,
            storage_devices: None,
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
            fs::create_dir_all(self.path.parent().unwrap())?;
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

enum Choice {
    BackupRequest(crate::backup_service::Request),
    BackupForm(super::backup::FormKind, String),
    BackupNow,
    BackupQueueNow,
    BackupCancel,
    GoogleDisconnectResolve(bool),
    WifiSelect(writingdeck_sys::WifiNetwork),
    WifiDisconnect,
    SavedWifiSelect(writingdeck_sys::SavedWifi),
    SavedWifiResolve(SavedWifiAction),
    UpdateBundleOpen,
    Open(Route),
    Action(Action),
    System(usize),
    Information,
    View(Route, writingdeck_sys::Request),
    Read(writingdeck_sys::Request),
    WarmAdjust(i16, i16),
    WarmToggle,
    WarmStatus,
    WarmApply,
    WarmResolve(bool),
    TimezoneOpen,
    TimezoneChoose(String),
    TimezoneResolve(bool),
    Lock,
    InstallerOpen,
    InstallerResolve(bool),
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
}

fn entries(app: &App) -> Vec<Entry> {
    match app.deck.shell.nav.current() {
        Route::Settings => Category::ALL
            .into_iter()
            .map(|category| Entry::new(category.label(), Choice::Open(Route::Category(category))))
            .collect(),
        Route::Category(category @ (Category::Writing | Category::Interface)) => {
            let mut rows = app
                .submenu(Sub::Settings)
                .into_iter()
                .filter(|(_, action)| {
                    let writing = matches!(
                        action,
                        Action::Spellcheck
                            | Action::SpellingLanguage
                            | Action::LineWidth
                            | Action::Typewriter
                            | Action::Voices
                            | Action::ManuscriptLook
                            | Action::AuthorDetails
                    );
                    let interface = matches!(
                        action,
                        Action::Workspaces
                            | Action::ToggleOutline
                            | Action::Themes
                            | Action::MusicSource
                            | Action::MusicToggle
                            | Action::Icons
                    );
                    if category == Category::Writing {
                        writing
                    } else {
                        interface
                    }
                })
                .map(|(label, action)| Entry::new(label, Choice::Action(action)))
                .collect::<Vec<_>>();
            if category == Category::Writing {
                rows.extend([
                    Entry::new("Focus mode", Choice::Action(Action::FocusMode)),
                    Entry::new("Writing goals", Choice::Action(Action::Goals)),
                    Entry::new("Daily challenges", Choice::Action(Action::DailyChallenges)),
                    Entry::new("Revision tools", Choice::Action(Action::ProseDashboard)),
                    Entry::new("Continuity tracker", Choice::Action(Action::Continuity)),
                    Entry::new("Draft milestones", Choice::Action(Action::Milestones)),
                    Entry::new("Revision notes", Choice::Action(Action::ReviewNotes)),
                    Entry::new(
                        "Footnotes · browse and jump",
                        Choice::Action(Action::Footnotes),
                    ),
                ]);
            } else {
                rows.extend([
                    Entry::new(
                        "Keyboard layout · QWERTY / Colemak / Colemak-DH",
                        Choice::System(4),
                    ),
                    Entry::new("Keyboard shortcuts", Choice::System(11)),
                    Entry::info("Accessibility and notifications · more controls planned"),
                ]);
            }
            rows
        }
        Route::Category(Category::System) => Panel::ALL
            .into_iter()
            .map(|panel| Entry::new(panel.label(), Choice::Open(Route::Panel(panel))))
            .collect(),
        Route::Category(Category::Accounts) => vec![
            Entry::new(
                "Local accounts",
                Choice::View(
                    Route::LocalAccounts,
                    writingdeck_sys::Request::ListLocalAccounts,
                ),
            ),
            Entry::new("Google account", Choice::Open(Route::GoogleAccount)),
            Entry::new("Google Drive", Choice::Open(Route::GoogleDrive)),
            Entry::new("Synchronization", Choice::Open(Route::SyncPanel)),
            Entry::new("Backups", Choice::Open(Route::Backups)),
        ],
        Route::Panel(panel) => {
            let status = app.deck.snapshot.as_ref();
            let mut rows = vec![Entry::new("Refresh local system status", Choice::System(0))];
            match panel {
                Panel::Network => rows.extend([
                    Entry::info(format!(
                        "Connection: {}",
                        status
                            .and_then(|s| s.network.as_deref())
                            .unwrap_or("unknown; refresh status")
                    )),
                    Entry::new("Enable Wi-Fi radio", Choice::System(1)),
                    Entry::new("Disable Wi-Fi radio", Choice::System(2)),
                    Entry::new(
                        "Scan Wi-Fi networks",
                        Choice::View(Route::WifiNetworks, writingdeck_sys::Request::ScanWifi),
                    ),
                    Entry::new(
                        "Saved Wi-Fi profiles",
                        Choice::View(Route::SavedWifi, writingdeck_sys::Request::ListSavedWifi),
                    ),
                    Entry::new("Disconnect active Wi-Fi", Choice::WifiDisconnect),
                    Entry::info(
                        "Select a network in Scan Wi-Fi networks to connect or enter credentials.",
                    ),
                ]),
                Panel::Display => rows.extend([
                    Entry::info(format!(
                        "Brightness: {}",
                        status
                            .and_then(|s| s.brightness_percent)
                            .map_or("unavailable".into(), |p| format!("{p}%"))
                    )),
                    Entry::new("Brightness -10%", Choice::System(5)),
                    Entry::new("Brightness +10%", Choice::System(6)),
                    Entry::new(
                        "Warm screen · blue light filter",
                        Choice::Open(Route::WarmScreen),
                    ),
                ]),
                Panel::Power => rows.extend([
                    Entry::info(format!(
                        "Battery: {}",
                        status
                            .and_then(|s| s.battery_percent)
                            .map_or("unavailable".into(), |p| format!("{p}%"))
                    )),
                    Entry::new(
                        format!(
                            "Low-power interface and OS request: {}",
                            if app.deck.state.low_power {
                                "on"
                            } else {
                                "off"
                            }
                        ),
                        Choice::System(7),
                    ),
                    Entry::info(writingdeck_sys::SUSPEND_UNAVAILABLE),
                    Entry::new("Shutdown · confirmation required", Choice::System(9)),
                    Entry::new("Restart · confirmation required", Choice::System(10)),
                    if app.deck.live_session {
                        Entry::info("Live session · personal lock unavailable")
                    } else if app.deck.lock_available {
                        Entry::new("Lock writing session", Choice::Lock)
                    } else {
                        Entry::info("Console lock unavailable in this image/session")
                    },
                ]),
                Panel::Storage => {
                    rows.push(Entry::new(
                        "Encrypted backups and restore",
                        Choice::Open(Route::Backups),
                    ));
                    rows.push(Entry::new(
                        "Mounted filesystems",
                        Choice::View(
                            Route::StorageDevicesView,
                            writingdeck_sys::Request::ListStorageDevices,
                        ),
                    ));
                    rows.push(Entry::info(format!(
                        "Available local storage: {}",
                        status
                            .and_then(|s| s.available_bytes)
                            .map_or("unknown".into(), |n| format!("{} MiB", n / (1024 * 1024)))
                    )));
                    if app.deck.live_session && app.deck.installer_available {
                        rows.push(Entry::new(
                            "Install WrOS to disk · encrypted live installer",
                            Choice::InstallerOpen,
                        ));
                    } else if app.deck.live_session {
                        rows.push(Entry::info(
                            "Live installer unavailable on this console or image",
                        ));
                    } else {
                        rows.push(Entry::info(
                            "System installed · live installer runs only from live media",
                        ));
                    }
                }
                Panel::DateTime => rows.extend([
                    Entry::info(format!(
                        "Timezone: {}",
                        status
                            .and_then(|s| s.timezone.as_deref())
                            .unwrap_or("unknown")
                    )),
                    Entry::new("Choose timezone", Choice::TimezoneOpen),
                    Entry::info("Manual clock / network time controls are not available yet"),
                ]),
                Panel::Updates => rows.extend([
                    Entry::new(
                        "Preview cached package plan",
                        Choice::View(Route::UpdatesView, writingdeck_sys::Request::CheckUpdates),
                    ),
                    Entry::new(
                        "Inspect signed offline update bundle",
                        Choice::UpdateBundleOpen,
                    ),
                    Entry::info("Update installation is unavailable pending an offline transaction and recovery service."),
                    Entry::new(
                        "Inspect filesystem and dpkg recovery",
                        Choice::View(Route::RecoveryView, writingdeck_sys::Request::RecoveryStatus),
                    ),
                    Entry::info(
                        "Signed bundles require Debian GPG verification (InRelease + Packages + .deb SHA-256).",
                    ),
                ]),
                Panel::Recovery => rows.extend([
                    Entry::new("Recovery availability", Choice::Open(Route::RecoveryView)),
                    Entry::new(
                        "Check filesystem and dpkg recovery status",
                        Choice::View(Route::RecoveryView, writingdeck_sys::Request::RecoveryStatus),
                    ),
                    Entry::info("Native package repair is unavailable; use Debian rescue media."),
                ]),
            }
            rows
        }
        Route::WifiNetworks => {
            let mut rows = vec![Entry::new(
                "Refresh scan",
                Choice::Read(writingdeck_sys::Request::ScanWifi),
            )];
            if let Some(networks) = &app.deck.shell.wifi_networks {
                for network in networks {
                    rows.push(Entry::new(
                        format!(
                            "{}{}\nSignal: {}% · Security: {}",
                            if network.in_use { "Connected: " } else { "" },
                            network.ssid,
                            network.signal,
                            network.security
                        ),
                        Choice::WifiSelect(network.clone()),
                    ));
                }
                if networks.is_empty() {
                    rows.push(Entry::info("No visible named Wi-Fi networks returned."));
                }
            } else {
                rows.push(Entry::info("Scan unavailable; refresh to try again."));
            }
            rows.push(Entry::info(
                "Select a network above to connect or enter protected credentials.",
            ));
            rows
        }
        Route::WifiConnect => app
            .deck
            .shell
            .wifi_form
            .as_ref()
            .map(|form| form.rows().into_iter().map(Entry::info).collect())
            .unwrap_or_default(),
        Route::SavedWifi => {
            let mut rows = vec![Entry::new(
                "Refresh saved profiles",
                Choice::Read(writingdeck_sys::Request::ListSavedWifi),
            )];
            if let Some(profiles) = &app.deck.shell.wifi_saved {
                for profile in profiles {
                    rows.push(Entry::new(
                        format!("{}\nUUID: {}", profile.name, profile.uuid),
                        Choice::SavedWifiSelect(profile.clone()),
                    ));
                }
                if profiles.is_empty() {
                    rows.push(Entry::info(
                        "No saved Wi-Fi profiles are visible to this session.",
                    ));
                }
            } else {
                rows.push(Entry::info(
                    "Saved profiles unavailable; refresh to try again.",
                ));
            }
            rows.push(Entry::info(
                "Select a saved profile to connect, reconnect or remove it by UUID.",
            ));
            rows
        }
        Route::SavedWifiProfile => {
            let name = app
                .deck
                .shell
                .wifi_saved_choice
                .as_ref()
                .map_or("unavailable", |p| p.name.as_str());
            let uuid = app
                .deck
                .shell
                .wifi_saved_choice
                .as_ref()
                .map_or("unavailable", |p| p.uuid.as_str());
            vec![
                Entry::new(
                    "Cancel and keep saved profile unchanged",
                    Choice::SavedWifiResolve(SavedWifiAction::Cancel),
                ),
                Entry::new(
                    format!("Connect or reconnect {name}"),
                    Choice::SavedWifiResolve(SavedWifiAction::Connect),
                ),
                Entry::new(
                    format!("Forget saved profile {name}"),
                    Choice::SavedWifiResolve(SavedWifiAction::Forget),
                ),
                Entry::info(format!("Profile: {name} · UUID: {uuid}")),
                Entry::info("Operations target this exact 36-character NetworkManager UUID."),
            ]
        }
        Route::LocalAccounts => {
            let mut rows = vec![Entry::new(
                "Refresh local accounts",
                Choice::Read(writingdeck_sys::Request::ListLocalAccounts),
            )];
            if let Some(accounts) = &app.deck.shell.local_accounts {
                for account in accounts {
                    rows.push(Entry::info(format!(
                        "{}{}\nUID {} · Home: {}",
                        account.username,
                        if account.is_current {
                            " (this session)"
                        } else {
                            ""
                        },
                        account.uid,
                        account.home
                    )));
                }
                if accounts.is_empty() {
                    rows.push(Entry::info(
                        "No local login accounts found in the supported UID range.",
                    ));
                }
            } else {
                rows.push(Entry::info(
                    "Local accounts unavailable; refresh to try again.",
                ));
            }
            rows.push(Entry::info(
                "Account creation and password changes are not available here yet.",
            ));
            rows
        }
        Route::StorageDevicesView => {
            let mut rows = vec![Entry::new(
                "Refresh mounted filesystems",
                Choice::Read(writingdeck_sys::Request::ListStorageDevices),
            )];
            if let Some(devices) = &app.deck.shell.storage_devices {
                for device in devices {
                    rows.push(Entry::info(format!(
                        "{}\n{}\n{} MiB free of {} MiB · {}% used",
                        device.mountpoint,
                        device.filesystem,
                        device.available_bytes / 1_048_576,
                        device.total_bytes / 1_048_576,
                        device.use_percent
                    )));
                }
                if devices.is_empty() {
                    rows.push(Entry::info("No supported filesystems returned."));
                }
            } else {
                rows.push(Entry::info(
                    "Mounted filesystems unavailable; refresh to try again.",
                ));
            }
            rows
        }
        Route::UpdatesView => {
            let mut rows = vec![
                Entry::new(
                    "Refresh cached package plan",
                    Choice::Read(writingdeck_sys::Request::CheckUpdates),
                ),
                Entry::new(
                    "Inspect signed offline update bundle",
                    Choice::UpdateBundleOpen,
                ),
                Entry::new(
                    "Inspect filesystem and dpkg recovery",
                    Choice::View(
                        Route::RecoveryView,
                        writingdeck_sys::Request::RecoveryStatus,
                    ),
                ),
                Entry::info("Preview only. No package download or installation is performed."),
                Entry::info("Repository freshness and recovery are unchecked."),
            ];
            if let Some(plan) = &app.deck.shell.updates {
                rows.push(Entry::info(&plan.status));
                rows.extend(
                    plan.packages
                        .iter()
                        .map(|name| Entry::info(format!("Install/upgrade: {name}"))),
                );
                rows.extend(
                    plan.removals
                        .iter()
                        .map(|name| Entry::info(format!("Remove: {name}"))),
                );
            } else {
                rows.push(Entry::info(
                    "Cached package plan unavailable; refresh to try again.",
                ));
            }
            rows
        }
        Route::UpdateBundleForm => app
            .deck
            .shell
            .update_form
            .as_ref()
            .map(|form| form.rows().into_iter().map(Entry::info).collect())
            .unwrap_or_default(),
        Route::GoogleAccount => {
            let backup = &app.deck.shell.backup;
            let mut rows = vec![Entry::info(if backup.status.google_connected {
                "Google Drive credentials saved privately on this device"
            } else {
                "Google Drive is not connected"
            })];
            if backup.status.google_connected {
                if !backup.status.google_state.is_empty() {
                    rows.push(Entry::info(format!(
                        "Credential state: {}{}",
                        backup.status.google_state,
                        if backup.status.google_verified_at.is_empty() {
                            String::new()
                        } else {
                            format!(" · verified {}", backup.status.google_verified_at)
                        }
                    )));
                }
                if !backup.status.google_expiry.is_empty() {
                    rows.push(Entry::info(format!(
                        "Token expiry: {}",
                        backup.status.google_expiry
                    )));
                }
                rows.push(Entry::new(
                    "Verify Google Drive connection",
                    Choice::BackupRequest(crate::backup_service::Request::CheckGoogle),
                ));
                rows.push(Entry::new(
                    "Reauthorize Google Drive",
                    Choice::BackupForm(super::backup::FormKind::Google, String::new()),
                ));
                rows.push(Entry::new(
                    "Disconnect Google Drive and remove local credentials",
                    Choice::Open(Route::GoogleDisconnectConfirm),
                ));
            } else {
                rows.push(Entry::new(
                    "Connect Google Drive",
                    Choice::BackupForm(super::backup::FormKind::Google, String::new()),
                ));
            }
            rows.push(Entry::new(
                "Encrypted backups and restore",
                Choice::Open(Route::Backups),
            ));
            rows
        }
        Route::GoogleDisconnectConfirm => vec![
            Entry::new(
                "Cancel and keep saved Google Drive credentials",
                Choice::GoogleDisconnectResolve(false),
            ),
            Entry::new(
                "Disconnect and erase local Google Drive credentials",
                Choice::GoogleDisconnectResolve(true),
            ),
            Entry::info("Local manuscripts, recovery keys and encrypted snapshots are preserved."),
            Entry::info("Revoke access in your Google account settings to end the provider grant."),
        ],
        Route::GoogleDrive => vec![
            Entry::new(
                "Encrypted Google Drive backups and restore",
                Choice::Open(Route::Backups),
            ),
            Entry::info(
                "General Drive file browsing and document synchronization are not implemented.",
            ),
        ],
        Route::SyncPanel => vec![
            Entry::info("Conflict-safe synchronization is not implemented."),
            Entry::info("Local saves remain local."),
        ],
        Route::Backups => {
            use super::backup::FormKind;
            use crate::backup_service::Request;
            let backup = &app.deck.shell.backup;
            let mut rows = vec![
                Entry::new(
                    "Create encrypted backup of this saved project",
                    Choice::BackupNow,
                ),
                Entry::new("Refresh snapshots", Choice::BackupRequest(Request::List)),
                Entry::new(
                    "New backup on a local drive",
                    Choice::BackupForm(FormKind::NewLocal, String::new()),
                ),
                Entry::new(
                    "New backup on Google Drive",
                    Choice::BackupForm(FormKind::NewDrive, String::new()),
                ),
                Entry::new(
                    "Connect existing repository and recovery key",
                    Choice::BackupForm(FormKind::Connect, String::new()),
                ),
                Entry::new(
                    "Connect Google Drive",
                    Choice::BackupForm(FormKind::Google, String::new()),
                ),
                Entry::new(
                    "Export recovery key",
                    Choice::BackupForm(FormKind::ExportKey, String::new()),
                ),
                Entry::new(
                    "Recover an interrupted repository lock",
                    Choice::BackupRequest(Request::Unlock),
                ),
                Entry::new(
                    "Queue saved project for offline backup",
                    Choice::BackupQueueNow,
                ),
                Entry::new(
                    "Run queued backups and retry interrupted jobs",
                    Choice::BackupRequest(Request::RunQueue),
                ),
            ];
            if !backup.status.queued_backups.is_empty() {
                rows.push(Entry::new(
                    "Clear pending backup queue",
                    Choice::BackupRequest(Request::ClearQueue),
                ));
            }
            // Keep action positions stable when an asynchronous job finishes.
            rows.insert(
                0,
                if backup.job.is_some() {
                    Entry::new("Cancel current backup operation", Choice::BackupCancel)
                } else {
                    Entry::info("No backup operation is running")
                },
            );
            if backup.status.configured {
                rows.push(Entry::info(format!(
                    "Destination: {}",
                    backup.status.repository
                )));
                rows.push(Entry::info(format!(
                    "Private recovery key: {}",
                    backup.status.recovery_key
                )));
            } else {
                rows.push(Entry::info(
                    "No repository configured. Choose a local drive or connect Google Drive.",
                ));
            }
            rows.push(Entry::info(if backup.status.google_connected {
                "Google Drive connection saved privately"
            } else {
                "Google Drive is not connected"
            }));
            if !backup.status.queued_backups.is_empty() {
                rows.push(Entry::info(format!(
                    "Queued offline backups: {} pending",
                    backup.status.queued_backups.len()
                )));
                for item in &backup.status.queued_backups {
                    rows.push(Entry::info(format!(
                        "Queued: {} · attempts {}{}",
                        item.label,
                        item.attempts,
                        if item.last_error.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", item.last_error)
                        }
                    )));
                }
            }
            rows.push(Entry::info(
                "Select a snapshot below to restore into a NEW folder.",
            ));
            rows.push(Entry::info(
                "Keep local backups on another device; folders can share the same disk.",
            ));
            // The fixed notice line is clipped at 80 columns. Keep the complete
            // result, including recovery paths, available in the scrollable body.
            let detail: Vec<char> = backup
                .notice
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            let width = usize::from(app.screen.0.saturating_sub(8)).max(8) / 2;
            for chunk in detail.chunks(width) {
                rows.push(Entry::info(chunk.iter().collect::<String>()));
            }
            for snapshot in &backup.status.snapshots {
                rows.push(Entry::new(
                    format!(
                        "{} · {} · {}\n{}",
                        snapshot.project,
                        snapshot.time,
                        snapshot.id.get(..8).unwrap_or("unknown"),
                        if snapshot.restore_checked_at.is_empty() {
                            "Restore has not been checked on this device".into()
                        } else {
                            format!("Restore checked: {}", snapshot.restore_checked_at)
                        }
                    ),
                    Choice::BackupForm(FormKind::Restore, snapshot.id.clone()),
                ));
            }
            rows
        }
        Route::BackupForm => app
            .deck
            .shell
            .backup
            .form
            .as_ref()
            .map(|form| form.rows().into_iter().map(Entry::info).collect())
            .unwrap_or_default(),
        Route::RecoveryView => {
            let mut rows = vec![
                Entry::new(
                    "Check filesystem and dpkg recovery status",
                    Choice::Read(writingdeck_sys::Request::RecoveryStatus),
                ),
                Entry::info("Native package repair is unavailable; use Debian rescue media."),
                Entry::new(
                    "Encrypted project backups and restore",
                    Choice::Open(Route::Backups),
                ),
            ];
            if let Some(info) = &app.deck.shell.recovery {
                rows.push(Entry::info(&info.status));
                rows.push(Entry::info(format!(
                    "Root filesystem: {} · Device: {} · Separate /home: {}",
                    info.root_filesystem,
                    info.root_device,
                    if info.separate_home { "yes" } else { "no" }
                )));
                rows.push(Entry::info(format!(
                    "Automatic filesystem snapshot rollback: {}",
                    if info.rollback_capable {
                        "available"
                    } else {
                        "unavailable"
                    }
                )));
                rows.push(Entry::info(&info.rollback_strategy));
                if info.interrupted_dpkg {
                    rows.push(Entry::info(format!(
                        "Interrupted dpkg packages: {}",
                        info.unconfigured_packages.join(", ")
                    )));
                }
                if info.staged_update {
                    rows.push(Entry::info(format!(
                        "Staged offline update: {}",
                        info.staged_packages.join(", ")
                    )));
                    rows.push(Entry::info("Update installation is unavailable pending an offline transaction and recovery service."));
                }
            } else {
                rows.push(Entry::info("Recovery boot and restore are not qualified."));
                rows.push(Entry::info("Use the documented maintenance media."));
            }
            rows
        }
        Route::Timezones => {
            let mut rows = vec![Entry::info(format!(
                "Filter: {} · type a city or region",
                app.deck.shell.timezone_query
            ))];
            if let Some(zones) = &app.deck.shell.timezones {
                let query = app
                    .deck
                    .shell
                    .timezone_query
                    .to_ascii_lowercase()
                    .replace(' ', "_");
                rows.extend(
                    zones
                        .iter()
                        .filter(|zone| zone.to_ascii_lowercase().contains(&query))
                        .map(|zone| Entry::new(zone, Choice::TimezoneChoose(zone.clone()))),
                );
                if rows.len() == 1 {
                    rows.push(Entry::info(
                        "No matching timezones; Backspace edits the filter",
                    ));
                }
            } else {
                rows.push(Entry::info(
                    "Timezone list unavailable; return and try again",
                ));
            }
            rows
        }
        Route::TimezoneConfirm => vec![
            Entry::new("Cancel timezone change", Choice::TimezoneResolve(false)),
            Entry::new(
                format!(
                    "Use {}",
                    app.deck
                        .shell
                        .timezone_choice
                        .as_deref()
                        .unwrap_or("unavailable")
                ),
                Choice::TimezoneResolve(true),
            ),
            Entry::info(
                "Changes future local dates. Existing writing records keep their recorded dates.",
            ),
        ],
        Route::WarmScreen => {
            let shell = &app.deck.shell;
            let choice = shell.warmth;
            let status = shell
                .display
                .as_ref()
                .map_or("Not checked; check display support", |s| s.message.as_str());
            vec![
                Entry::new(
                    "Check display support and saved settings",
                    Choice::WarmStatus,
                ),
                Entry::info(status),
                Entry::info(shell.display.as_ref().map_or(
                    "Display support: unknown".into(),
                    |s| {
                        format!(
                            "Display support: {} · saved preference {}",
                            if s.available {
                                "available"
                            } else {
                                "unavailable"
                            },
                            if s.applied {
                                "matches display"
                            } else {
                                "not applied"
                            }
                        )
                    },
                )),
                Entry::new(
                    format!("Filter: {}", if choice.enabled { "on" } else { "off" }),
                    Choice::WarmToggle,
                ),
                Entry::new(
                    format!("Temperature: {} K · warmer -100 K", choice.kelvin),
                    Choice::WarmAdjust(-100, 0),
                ),
                Entry::new("Temperature: cooler +100 K", Choice::WarmAdjust(100, 0)),
                Entry::new(
                    format!("Strength: {}% · decrease 10%", choice.strength),
                    Choice::WarmAdjust(0, -10),
                ),
                Entry::new("Strength: increase 10%", Choice::WarmAdjust(0, 10)),
                Entry::new("Preview choice · keep within 15 seconds", Choice::WarmApply),
                Entry::info("1000–6500 K; 2200 K is very warm. Changes apply only after Preview."),
                Entry::info(
                    "Off restores the original color curves. Unsupported drivers stay unchanged.",
                ),
            ]
        }
        Route::InstallerConfirm => vec![
            Entry::new(
                "Cancel and remain in the live writing session",
                Choice::InstallerResolve(false),
            ),
            Entry::new(
                "Start encrypted WrOS installer on console tty7",
                Choice::InstallerResolve(true),
            ),
            Entry::info(
                "Retires writing input on tty1 while the shared Grimoire installer runs on tty7.",
            ),
            Entry::info(
                "Disk writes remain default-No inside the installer until you explicitly confirm.",
            ),
        ],
        Route::WarmPreview => vec![
            Entry::info(format!(
                "Temporary display preview · {} seconds remaining",
                app.deck
                    .shell
                    .warm_preview
                    .as_ref()
                    .map_or(0, |(_, end)| end
                        .saturating_duration_since(Instant::now())
                        .as_secs())
            )),
            Entry::new(
                "Cancel and restore previous colors",
                Choice::WarmResolve(false),
            ),
            Entry::new("Keep these display settings", Choice::WarmResolve(true)),
            Entry::info("The independent system timer restores unconfirmed changes if WrOS exits."),
        ],
    }
}

impl App {
    fn clear_system_view(&mut self, request: &writingdeck_sys::Request) {
        use writingdeck_sys::Request;
        match request {
            Request::ScanWifi => self.deck.shell.wifi_networks = None,
            Request::ListSavedWifi => self.deck.shell.wifi_saved = None,
            Request::ListLocalAccounts => self.deck.shell.local_accounts = None,
            Request::ListStorageDevices => self.deck.shell.storage_devices = None,
            Request::CheckUpdates => self.deck.shell.updates = None,
            Request::RecoveryStatus => self.deck.shell.recovery = None,
            Request::GetGoogleStatus => self.deck.shell.google_status = None,
            _ => {}
        }
    }
    pub fn paste_wifi_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.wifi_form else {
            return;
        };
        if !form.requires_password() || form.selected != 0 {
            return;
        }
        if form.password.len() + text.len() <= 64 && !text.chars().any(char::is_control) {
            form.password.push_str(text);
        }
    }
    pub fn paste_update_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.update_form else {
            return;
        };
        if form.selected != 0 {
            return;
        }
        if form.bundle_dir.len() + text.len() <= 512 && !text.chars().any(char::is_control) {
            form.bundle_dir.push_str(text);
        }
    }
    pub fn on_update_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.update_form else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.update_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % 3,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    2
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if form.selected == 0 => {
                form.bundle_dir.pop();
            }
            Key::Delete if form.selected == 0 => {
                form.bundle_dir.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 0 => {
                self.paste_update_form(&c.to_string());
            }
            Key::Enter if form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter if form.selected == 1 => {
                self.deck.shell.update_form = None;
                self.deck.shell.nav.back();
            }
            Key::Enter if form.selected == 2 => {
                if self.deck.busy {
                    return;
                }
                let bundle_dir = form.bundle_dir.trim().to_string();
                if let Err(err) =
                    writingdeck_sys::system_views::validate_update_bundle_path(&bundle_dir)
                {
                    self.deck.note = format!("Unavailable: {err}");
                    self.deck.shell.notices.push(&self.deck.note);
                    return;
                }
                self.deck
                    .request(writingdeck_sys::Request::VerifyUpdateBundle { bundle_dir });
            }
            _ => {}
        }
    }
    pub fn on_wifi_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.wifi_form else {
            return;
        };
        let count = form.action_count();
        let requires_password = form.requires_password();
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.wifi_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % count,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    count.saturating_sub(1)
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if requires_password && form.selected == 0 => {
                form.password.pop();
            }
            Key::Delete if requires_password && form.selected == 0 => {
                form.password.clear();
            }
            Key::Char(c) if !c.is_control() && requires_password && form.selected == 0 => {
                self.paste_wifi_form(&c.to_string());
            }
            Key::Enter if requires_password && form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter => {
                let selected = form.selected;
                let cancel_idx = if requires_password { 1 } else { 0 };
                let connect_idx = if requires_password { 2 } else { 1 };
                let disconnect_idx = if requires_password { 3 } else { 2 };
                if selected == cancel_idx {
                    self.deck.shell.wifi_form = None;
                    self.deck.shell.nav.back();
                    return;
                }
                if self.deck.busy {
                    return;
                }
                if selected == disconnect_idx && form.network.in_use {
                    self.deck.request(writingdeck_sys::Request::DisconnectWifi);
                    return;
                }
                if selected == connect_idx {
                    let ssid = form.network.ssid.clone();
                    if let Err(err) = writingdeck_sys::system_views::validate_wifi_ssid(&ssid) {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                        return;
                    }
                    if requires_password {
                        if let Err(err) =
                            writingdeck_sys::system_views::validate_wifi_password(&form.password)
                        {
                            self.deck.note = format!("Unavailable: {err}");
                            self.deck.shell.notices.push(&self.deck.note);
                            return;
                        }
                        let secret = writingdeck_sys::WifiSecret::new(form.password.as_str());
                        form.password = Zeroizing::new(String::new());
                        self.deck.request(writingdeck_sys::Request::ConnectWifi {
                            ssid,
                            password: Some(secret),
                        });
                    } else {
                        self.deck.request(writingdeck_sys::Request::ConnectWifi {
                            ssid,
                            password: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    pub fn open_settings(&mut self) {
        let origin = match self.overlay {
            Overlay::Menu { .. } | Overlay::Deck(writingdeck::Page::Home { .. }) => {
                self.overlay.clone()
            }
            _ => Overlay::None,
        };
        self.deck.shell.origin = Some(origin);
        self.deck.shell.external = false;
        self.deck.shell.wifi_form = None;
        self.deck.shell.wifi_saved_choice = None;
        self.deck.shell.update_form = None;
        self.deck.shell.nav.reset(Route::Settings);
        self.overlay = Overlay::Deck(writingdeck::Page::Settings);
        self.observe_deck(tutorial::Event::SystemOpened);
    }
    pub fn on_settings_key(&mut self, key: Key) {
        if self.deck.shell.nav.current() == Route::BackupForm {
            self.on_backup_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::WifiConnect {
            self.on_wifi_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::UpdateBundleForm {
            self.on_update_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::Timezones {
            let query = &mut self.deck.shell.timezone_query;
            let changed = match key {
                Key::Char(c)
                    if query.len() < 64 && (c.is_ascii_alphanumeric() || " /_+-".contains(c)) =>
                {
                    query.push(c);
                    true
                }
                Key::Backspace => query.pop().is_some(),
                _ => false,
            };
            if changed {
                self.deck.shell.nav.select(1, entries(self).len());
                return;
            }
        }
        if self.deck.shell.nav.current() == Route::WarmPreview
            && matches!(key, Key::Esc | Key::Left)
        {
            self.resolve_warm_preview(false);
            return;
        }
        let rows = entries(self);
        let nav = &mut self.deck.shell.nav;
        match key {
            Key::Up => nav.move_by(-1, rows.len()),
            Key::Down => nav.move_by(1, rows.len()),
            Key::PageUp => nav.move_by(-10, rows.len()),
            Key::PageDown => nav.move_by(10, rows.len()),
            Key::Home => nav.select(0, rows.len()),
            Key::End => nav.select(rows.len().saturating_sub(1), rows.len()),
            Key::Esc | Key::Left => {
                self.deck.shell.warm_wait = false;
                self.deck.shell.wifi_form = None;
                self.deck.shell.wifi_saved_choice = None;
                self.deck.shell.update_form = None;
                if !nav.back() {
                    self.deck.shell.save();
                    self.overlay = self.deck.shell.origin.take().unwrap_or(Overlay::None);
                }
            }
            Key::Enter | Key::Right => {
                nav.select(nav.selected(), rows.len());
                let Some(row) = rows.into_iter().nth(nav.selected()) else {
                    return;
                };
                match row.choice {
                    Choice::BackupRequest(crate::backup_service::Request::RunQueue) => {
                        self.request_run_backup_queue()
                    }
                    Choice::BackupRequest(request) => self.deck.shell.backup.request(request),
                    Choice::BackupForm(kind, snapshot) => self.open_backup_form(kind, snapshot),
                    Choice::BackupNow => self.request_project_backup(),
                    Choice::BackupQueueNow => self.request_project_backup_queue(),
                    Choice::BackupCancel => {
                        self.deck.shell.backup.job = None;
                        self.deck.shell.backup.notice = "Cancelling; refresh snapshots before retrying. Completed snapshots remain recoverable.".into();
                    }
                    Choice::GoogleDisconnectResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::GoogleDisconnectResolve(true) => {
                        self.deck.shell.backup.tick();
                        if self.deck.shell.backup.job.is_some() {
                            self.deck.shell.backup.notice = "Wait for the current backup operation, then confirm disconnect again.".into();
                            return;
                        }
                        self.deck.shell.nav.back();
                        self.deck
                            .shell
                            .backup
                            .request(crate::backup_service::Request::DisconnectGoogle);
                    }
                    Choice::WifiSelect(network) if !self.deck.busy => {
                        if !network.security.is_empty()
                            && network.security != "--"
                            && !network
                                .security
                                .split_whitespace()
                                .all(|kind| matches!(kind, "WPA1" | "WPA2" | "WPA3"))
                        {
                            self.deck.note = "This network needs enrollment beyond a personal WPA password; use a supported saved profile or another network.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                            return;
                        }
                        self.deck.shell.wifi_form = Some(WifiForm {
                            network,
                            password: Zeroizing::new(String::new()),
                            selected: 0,
                        });
                        self.deck.shell.nav.open(Route::WifiConnect);
                    }
                    Choice::WifiDisconnect if !self.deck.busy => {
                        self.deck.request(writingdeck_sys::Request::DisconnectWifi);
                    }
                    Choice::SavedWifiSelect(profile) if !self.deck.busy => {
                        self.deck.shell.wifi_saved_choice = Some(profile);
                        self.deck.shell.nav.open(Route::SavedWifiProfile);
                        self.deck.shell.nav.select(0, 5);
                    }
                    Choice::SavedWifiResolve(SavedWifiAction::Cancel) => {
                        self.deck.shell.wifi_saved_choice = None;
                        self.deck.shell.nav.back();
                    }
                    Choice::SavedWifiResolve(action) if !self.deck.busy => {
                        if let Some(profile) = self.deck.shell.wifi_saved_choice.take() {
                            self.deck.shell.nav.back();
                            match action {
                                SavedWifiAction::Cancel => {}
                                SavedWifiAction::Connect => {
                                    self.deck
                                        .request(writingdeck_sys::Request::ActivateSavedWifi(
                                            profile.uuid,
                                        ))
                                }
                                SavedWifiAction::Forget => self
                                    .deck
                                    .request(writingdeck_sys::Request::ForgetWifi(profile.uuid)),
                            }
                        }
                    }
                    Choice::UpdateBundleOpen if !self.deck.busy => {
                        self.deck.shell.update_form = Some(UpdateBundleForm {
                            bundle_dir: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.nav.open(Route::UpdateBundleForm);
                    }
                    Choice::WifiSelect(_)
                    | Choice::WifiDisconnect
                    | Choice::SavedWifiSelect(_)
                    | Choice::SavedWifiResolve(_)
                    | Choice::UpdateBundleOpen => {}
                    Choice::Lock => self.request_console_lock(),
                    Choice::InstallerOpen if !self.deck.busy => {
                        self.deck.shell.nav.open(Route::InstallerConfirm);
                        self.deck.shell.nav.select(0, 4);
                    }
                    Choice::InstallerResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::InstallerResolve(true) if !self.deck.busy => {
                        self.deck.shell.nav.back();
                        self.request_live_installer();
                    }
                    Choice::InstallerOpen | Choice::InstallerResolve(true) => {}
                    Choice::TimezoneOpen if !self.deck.busy => {
                        self.deck.shell.timezones = None;
                        self.deck.shell.timezone_query.clear();
                        self.deck.shell.timezone_choice = None;
                        self.deck.shell.nav.open(Route::Timezones);
                        self.deck.request(writingdeck_sys::Request::ListTimezones);
                    }
                    Choice::TimezoneChoose(zone) if !self.deck.busy => {
                        self.deck.shell.timezone_choice = Some(zone);
                        self.deck.shell.nav.open(Route::TimezoneConfirm);
                        self.deck.shell.nav.select(0, 3);
                    }
                    Choice::TimezoneResolve(false) => {
                        self.deck.shell.nav.back();
                    }
                    Choice::TimezoneResolve(true) if !self.deck.busy => {
                        if let Some(zone) = self.deck.shell.timezone_choice.clone()
                            && self
                                .deck
                                .shell
                                .timezones
                                .as_ref()
                                .is_some_and(|zones| zones.contains(&zone))
                        {
                            self.deck
                                .request(writingdeck_sys::Request::SetTimezone(zone));
                        }
                    }
                    Choice::TimezoneOpen
                    | Choice::TimezoneChoose(_)
                    | Choice::TimezoneResolve(true) => {}
                    Choice::Open(route) => {
                        self.deck.shell.nav.open(route);
                        if route == Route::GoogleDisconnectConfirm {
                            self.deck.shell.nav.select(0, 4);
                        }
                        if matches!(route, Route::Backups | Route::GoogleAccount) {
                            self.deck
                                .shell
                                .backup
                                .request(crate::backup_service::Request::Status);
                        }
                        let availability = match route {
                            Route::GoogleDrive => Some("Drive browsing is not configured."),
                            Route::SyncPanel => Some("Synchronization is not configured."),
                            Route::RecoveryView => Some("Recovery is not qualified."),
                            _ => None,
                        };
                        if let Some(message) = availability {
                            self.deck.note = message.into();
                            self.deck.shell.notices.push(message);
                        }
                        if route == Route::WarmScreen && !self.deck.busy {
                            self.deck.request(writingdeck_sys::Request::DisplayStatus);
                        }
                        if matches!(route, Route::Panel(_))
                            && self.deck.snapshot.is_none()
                            && !self.deck.busy
                        {
                            self.deck.request(writingdeck_sys::Request::Refresh);
                        }
                    }
                    Choice::View(route, request) if !self.deck.busy => {
                        self.deck.shell.nav.open(route);
                        self.clear_system_view(&request);
                        self.deck.request(request);
                    }
                    Choice::Read(request) if !self.deck.busy => {
                        self.clear_system_view(&request);
                        self.deck.request(request);
                    }
                    Choice::View(..) | Choice::Read(_) => {}
                    Choice::Information => self.deck.shell.notices.push(&row.label),
                    Choice::WarmAdjust(kelvin, strength) if !self.deck.busy => {
                        let choice = &mut self.deck.shell.warmth;
                        choice.kelvin =
                            (i32::from(choice.kelvin) + i32::from(kelvin)).clamp(1000, 6500) as u16;
                        choice.strength =
                            (i16::from(choice.strength) + strength).clamp(0, 100) as u8;
                    }
                    Choice::WarmToggle if !self.deck.busy => {
                        self.deck.shell.warmth.enabled = !self.deck.shell.warmth.enabled
                    }
                    Choice::WarmStatus if !self.deck.busy => {
                        self.deck.request(writingdeck_sys::Request::DisplayStatus)
                    }
                    Choice::WarmApply if !self.deck.busy => {
                        if self
                            .deck
                            .shell
                            .display
                            .as_ref()
                            .is_some_and(|s| s.available)
                        {
                            self.deck.request(writingdeck_sys::Request::DisplayPreview(
                                self.deck.shell.warmth,
                            ));
                            self.deck.shell.warm_wait = self.deck.busy;
                        } else {
                            self.deck.shell.notices.push(
                                "Display support is unavailable or unchecked; colors unchanged.",
                            );
                        }
                    }
                    Choice::WarmResolve(keep) if !self.deck.busy => self.resolve_warm_preview(keep),
                    Choice::WarmAdjust(..)
                    | Choice::WarmToggle
                    | Choice::WarmStatus
                    | Choice::WarmApply
                    | Choice::WarmResolve(_) => {}
                    Choice::Action(action) => {
                        self.deck.shell.external = true;
                        self.run_action(action);
                        self.return_to_settings();
                    }
                    Choice::System(sel) => {
                        self.deck.shell.external = true;
                        self.overlay = Overlay::Deck(writingdeck::Page::System { sel });
                        self.on_deck_key(Key::Enter);
                        self.return_to_settings();
                    }
                }
            }
            _ => {}
        }
    }
    fn resolve_warm_preview(&mut self, keep: bool) {
        if self.deck.busy {
            return;
        }
        if let Some((token, _)) = self.deck.shell.warm_preview.take() {
            self.deck.request(if keep {
                writingdeck_sys::Request::DisplayConfirm(token)
            } else {
                writingdeck_sys::Request::DisplayCancel(token)
            });
        }
        self.deck.shell.nav.back();
    }
    pub fn return_to_settings(&mut self) {
        if self.deck.shell.external
            && matches!(
                self.overlay,
                Overlay::None
                    | Overlay::Menu { .. }
                    | Overlay::Deck(
                        writingdeck::Page::System { .. } | writingdeck::Page::Home { .. }
                    )
            )
        {
            self.deck.shell.external = false;
            self.overlay = Overlay::Deck(writingdeck::Page::Settings);
        }
    }
}

pub fn draw(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    let rows = entries(app)
        .into_iter()
        .map(|entry| entry.label)
        .collect::<Vec<_>>();
    let mut title = match app.deck.shell.nav.current() {
        Route::Settings => "WrOS · Settings".into(),
        Route::Category(category) => format!("WrOS · Settings › {}", category.label()),
        Route::Panel(panel) => format!("WrOS · Settings › System › {}", panel.label()),
        Route::WarmScreen => "WrOS · Settings › Display › Warm screen".into(),
        Route::WarmPreview => "WrOS · Keep display preview?".into(),
        Route::Timezones => "WrOS · Settings › Date and time › Timezone".into(),
        Route::TimezoneConfirm => "WrOS · Change timezone?".into(),
        Route::WifiNetworks => "WrOS · Settings › Wi-Fi networks".into(),
        Route::WifiConnect => app
            .deck
            .shell
            .wifi_form
            .as_ref()
            .map(|f| format!("WrOS · Connect Wi-Fi › {}", f.network.ssid))
            .unwrap_or_else(|| "WrOS · Connect Wi-Fi".into()),
        Route::SavedWifi => "WrOS · Settings › Saved Wi-Fi profiles".into(),
        Route::SavedWifiProfile => "WrOS · Saved Wi-Fi profile".into(),
        Route::LocalAccounts => "WrOS · Settings › Local accounts".into(),
        Route::GoogleAccount => "WrOS · Settings › Google account".into(),
        Route::GoogleDisconnectConfirm => "WrOS · Disconnect Google Drive?".into(),
        Route::GoogleDrive => "WrOS · Settings › Google Drive".into(),
        Route::SyncPanel => "WrOS · Settings › Synchronization".into(),
        Route::Backups => "WrOS · Settings › Backups".into(),
        Route::BackupForm => app
            .deck
            .shell
            .backup
            .form
            .as_ref()
            .map(|form| format!("WrOS · {}", form.title()))
            .unwrap_or_else(|| "WrOS · Backups".into()),
        Route::UpdatesView => "WrOS · Settings › Cached package plan".into(),
        Route::UpdateBundleForm => "WrOS · Inspect signed offline update".into(),
        Route::RecoveryView => "WrOS · Settings › Recovery".into(),
        Route::StorageDevicesView => "WrOS · Settings › Mounted filesystems".into(),
        Route::InstallerConfirm => "WrOS · Start encrypted live installer?".into(),
    };
    if app.deck.live_session {
        title.push_str(" · Live session");
    }
    if app.deck.simulated() {
        title = format!("SIMULATED · {title}");
    }
    let uses_backup_notice = matches!(
        app.deck.shell.nav.current(),
        Route::Backups | Route::BackupForm
    ) || matches!(
        app.deck.shell.nav.current(),
        Route::GoogleAccount | Route::GoogleDisconnectConfirm
    ) && !app.deck.shell.backup.notice.is_empty();
    let note = if uses_backup_notice {
        app.deck.shell.backup.notice.as_str()
    } else if app.deck.busy {
        "System request pending; writing remains responsive"
    } else {
        app.deck.shell.notices.latest().unwrap_or(&app.deck.note)
    };
    frame.render_widget(ratatui::widgets::Clear, area);
    wros_shell::render_menu(
        frame,
        area,
        wros_shell::Menu {
            title: &title,
            rows: &rows,
            selected: if app.deck.shell.nav.current() == Route::BackupForm {
                app.deck
                    .shell
                    .backup
                    .form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::WifiConnect {
                app.deck
                    .shell
                    .wifi_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::UpdateBundleForm {
                app.deck
                    .shell
                    .update_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else {
                app.deck.shell.nav.selected()
            },
            notice: note,
            hint: if matches!(
                app.deck.shell.nav.current(),
                Route::BackupForm | Route::WifiConnect | Route::UpdateBundleForm
            ) {
                "Type/paste field  Tab/↑↓ choose  Enter next/confirm  Esc cancel"
            } else {
                "↑↓ choose  Enter open  Esc back  F1 help"
            },
        },
        theme.into(),
    );
}
