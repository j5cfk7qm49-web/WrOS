//! Adapter from shared presentation to retained editor state and existing services.
//! No manuscript context is swapped, reopened or cloned to navigate Settings.
use super::*;
use grimoire_core::tutorial;
use serde::{Deserialize, Serialize};
use wros_shell::{Navigation, Notices, Preferences};

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
    SavedWifi,
    LocalAccounts,
    GoogleAccount,
    GoogleDrive,
    SyncPanel,
    Backups,
    UpdatesView,
    RecoveryView,
    StorageDevicesView,
}

pub struct Shell {
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
    pub wifi_saved: Option<Vec<writingdeck_sys::SavedWifi>>,
    pub local_accounts: Option<Vec<writingdeck_sys::LocalAccount>>,
    pub google_status: Option<writingdeck_sys::GoogleAccountStatus>,
    pub drive_files: Option<Vec<writingdeck_sys::RemoteFile>>,
    pub backups: Option<Vec<writingdeck_sys::ResticSnapshot>>,
    pub updates: Option<writingdeck_sys::UpdateInfo>,
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
            wifi_saved: None,
            local_accounts: None,
            google_status: None,
            drive_files: None,
            backups: None,
            updates: None,
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
                    Entry::new("Revision notes", Choice::Action(Action::ReviewNotes)),
                    Entry::info("Footnotes · not implemented yet"),
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
            Entry::new(
                "Google account",
                Choice::View(
                    Route::GoogleAccount,
                    writingdeck_sys::Request::GetGoogleStatus,
                ),
            ),
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
                    Entry::info(
                        "New-network credentials · protected enrollment is not implemented",
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
                    Entry::info("Update installation and rollback are not available yet"),
                ]),
                Panel::Recovery => rows.push(Entry::new(
                    "Recovery availability",
                    Choice::Open(Route::RecoveryView),
                )),
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
                    rows.push(Entry::info(format!(
                        "{}{}\nSignal: {}% · Security: {}",
                        if network.in_use { "Connected: " } else { "" },
                        network.ssid,
                        network.signal,
                        network.security
                    )));
                }
                if networks.is_empty() {
                    rows.push(Entry::info("No visible named Wi-Fi networks returned."));
                }
            } else {
                rows.push(Entry::info("Scan unavailable; refresh to try again."));
            }
            rows.push(Entry::info(
                "New-network credentials · protected enrollment is not implemented.",
            ));
            rows
        }
        Route::SavedWifi => {
            let mut rows = vec![Entry::new(
                "Refresh saved profiles",
                Choice::Read(writingdeck_sys::Request::ListSavedWifi),
            )];
            if let Some(profiles) = &app.deck.shell.wifi_saved {
                for profile in profiles {
                    rows.push(Entry::info(format!(
                        "{}\nUUID: {}",
                        profile.name, profile.uuid
                    )));
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
                "This view does not change or delete network profiles.",
            ));
            rows
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
        Route::GoogleAccount => vec![
            Entry::info(app.deck.shell.google_status.as_ref().map_or(
                "Google enrollment is not configured in this build.",
                |status| status.status_message.as_str(),
            )),
            Entry::info("No account has been linked or verified by this screen."),
        ],
        Route::GoogleDrive => vec![
            Entry::info("Google Drive browsing is not configured."),
            Entry::info("No remote files have been fetched or cached."),
        ],
        Route::SyncPanel => vec![
            Entry::info("Conflict-safe synchronization is not implemented."),
            Entry::info("Local saves remain local."),
        ],
        Route::Backups => vec![
            Entry::info("Verified backup and restore are not configured."),
            Entry::info("Local history is not an independent backup."),
        ],
        Route::RecoveryView => vec![
            Entry::info("Recovery boot and restore are not qualified."),
            Entry::info("Use the documented maintenance media."),
        ],
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
            Request::GetGoogleStatus => self.deck.shell.google_status = None,
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
        self.deck.shell.nav.reset(Route::Settings);
        self.overlay = Overlay::Deck(writingdeck::Page::Settings);
        self.observe_deck(tutorial::Event::SystemOpened);
    }
    pub fn on_settings_key(&mut self, key: Key) {
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
                    Choice::Lock => self.request_console_lock(),
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
                        let availability = match route {
                            Route::GoogleDrive => Some("Drive browsing is not configured."),
                            Route::SyncPanel => Some("Synchronization is not configured."),
                            Route::Backups => {
                                Some("Verified backup and restore are not configured.")
                            }
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
        Route::SavedWifi => "WrOS · Settings › Saved Wi-Fi profiles".into(),
        Route::LocalAccounts => "WrOS · Settings › Local accounts".into(),
        Route::GoogleAccount => "WrOS · Settings › Google account".into(),
        Route::GoogleDrive => "WrOS · Settings › Google Drive".into(),
        Route::SyncPanel => "WrOS · Settings › Synchronization".into(),
        Route::Backups => "WrOS · Settings › Backups".into(),
        Route::UpdatesView => "WrOS · Settings › Cached package plan".into(),
        Route::RecoveryView => "WrOS · Settings › Recovery".into(),
        Route::StorageDevicesView => "WrOS · Settings › Mounted filesystems".into(),
    };
    if app.deck.live_session {
        title.push_str(" · Live session");
    }
    if app.deck.simulated() {
        title = format!("SIMULATED · {title}");
    }
    let note = if app.deck.busy {
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
            selected: app.deck.shell.nav.selected(),
            notice: note,
            hint: "↑↓ choose  Enter open  Esc back  F1 help",
        },
        theme.into(),
    );
}
