//! Adapter from shared presentation to retained editor state and existing services.
//! No manuscript context is swapped, reopened or cloned to navigate Settings.
use super::*;
use grimoire_core::tutorial;
use serde::{Deserialize, Serialize};
use wros_shell::{Navigation, Notices, Preferences};
use zeroize::Zeroizing;

mod update_flow;
pub use update_flow::UpdateFlow;

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
}
impl Category {
    fn label(self) -> &'static str {
        match self {
            Self::Writing => "Writing",
            Self::Interface => "Look and keyboard",
            Self::System => "System",
            Self::Accounts => "Backups and accounts",
            Self::Updates => "Updates and recovery",
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
    BackupDestinations,
    BackupKey,
    BackupQueue,
}

pub struct WifiForm {
    pub network: wros_sys::WifiNetwork,
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
            format!("Update folder on your media: {}", self.bundle_dir),
            "Cancel and return without reviewing".into(),
            "Review update · checks the signature, installs nothing".into(),
            "Use a folder under /run/wros-update/media, /media, /run/media or /mnt.".into(),
            "The review verifies the WrOS signature, the package list and every .deb checksum."
                .into(),
            "Nothing is installed until you confirm on a later screen.".into(),
            "Create an encrypted backup first; ext4 has no automatic snapshot rollback.".into(),
        ]
    }
}

pub struct ClockForm {
    pub datetime: String,
    pub selected: usize,
}
impl ClockForm {
    pub fn rows(&self) -> Vec<String> {
        vec![
            format!(
                "Local date and time (YYYY-MM-DD HH:MM[:SS]): {}",
                self.datetime
            ),
            "Cancel and keep current local clock".into(),
            "Review manual clock change · default-Cancel confirmation next".into(),
            "Sets local system time via org.wros.clock with network time (NTP) kept off.".into(),
            "Existing writing records keep their recorded timestamps.".into(),
        ]
    }
}

pub struct KeyboardWizardForm {
    pub base_index: usize,
    pub alias_name: String,
    pub selected: usize,
}
impl KeyboardWizardForm {
    pub fn rows(&self, custom_maps: &[wros_sys::keyboard::CustomMapRegistration]) -> Vec<String> {
        let base = wros_sys::keyboard::Layout::ALL[self.base_index].clone();
        let mut rows = vec![
            format!("Base layout: {} · {}", base.label(), base.description()),
            format!("Registered alias name (optional): {}", self.alias_name),
            "Cycle base layout (QWERTY / Colemak / Colemak-DH)".into(),
            "Preview base layout · 15-second hardware-level rollback".into(),
            "Register local alias for this base layout".into(),
            "Cancel and return to Interface settings".into(),
            "Arbitrary keymap compilation stays disabled; custom maps alias verified layouts."
                .into(),
        ];
        for reg in custom_maps {
            rows.push(format!(
                "Registered alias: {} → {}",
                reg.name,
                reg.base.id()
            ));
        }
        rows
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
    pub display: Option<wros_sys::display::Status>,
    pub warmth: wros_sys::display::Warmth,
    pub warm_wait: bool,
    pub warm_preview: Option<(String, Instant)>,
    pub timezones: Option<Vec<String>>,
    pub timezone_query: String,
    pub timezone_choice: Option<String>,
    pub wifi_networks: Option<Vec<wros_sys::WifiNetwork>>,
    pub wifi_form: Option<WifiForm>,
    pub wifi_saved: Option<Vec<wros_sys::SavedWifi>>,
    pub wifi_saved_choice: Option<wros_sys::SavedWifi>,
    pub local_accounts: Option<Vec<wros_sys::LocalAccount>>,
    pub google_status: Option<wros_sys::GoogleAccountStatus>,
    pub drive_files: Option<Vec<wros_sys::RemoteFile>>,
    pub backups: Option<Vec<wros_sys::ResticSnapshot>>,
    pub updates: Option<wros_sys::UpdateInfo>,
    pub update_form: Option<UpdateBundleForm>,
    pub update_flow: UpdateFlow,
    pub recovery: Option<wros_sys::RecoveryInfo>,
    pub storage_devices: Option<Vec<wros_sys::StorageDevice>>,
    pub clock_form: Option<ClockForm>,
    pub clock_choice: Option<String>,
    pub audio_status: Option<wros_sys::AudioStatus>,
    pub custom_keymaps: Vec<wros_sys::keyboard::CustomMapRegistration>,
    pub keyboard_wizard: Option<KeyboardWizardForm>,
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
            warmth: wros_sys::display::Warmth::default(),
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
            update_flow: UpdateFlow::default(),
            recovery: None,
            storage_devices: None,
            clock_form: None,
            clock_choice: None,
            audio_status: None,
            custom_keymaps: Vec::new(),
            keyboard_wizard: None,
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
    SyncSwitch(bool),
    SyncNow,
    SyncConfirmDeletes,
    BackupCancel,
    LinkPhone,
    GoogleDisconnectResolve(bool),
    WifiSelect(wros_sys::WifiNetwork),
    WifiDisconnect,
    SavedWifiSelect(wros_sys::SavedWifi),
    SavedWifiResolve(SavedWifiAction),
    UpdateBundleOpen,
    UpdateFindMedia,
    UpdateOnlineCheck,
    UpdateOnlineFetch,
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
            | Choice::RepairOpen => wros_shell::RowKind::Opens,
            _ => wros_shell::RowKind::Option,
        }
    }
}

/// When the newest listed snapshot was taken, and what is still queued.
fn backup_overview(status: &crate::backup_service::Status) -> String {
    let newest = match status.snapshots.iter().map(|s| s.time.as_str()).max() {
        Some(time) => format!(
            "Newest snapshot: {}",
            time.get(..16).unwrap_or(time).replace('T', " ")
        ),
        None => "No snapshots listed yet; refresh snapshots to check".into(),
    };
    match status.queued_backups.len() {
        0 => newest,
        1 => format!("{newest} · 1 queued backup waiting"),
        n => format!("{newest} · {n} queued backups waiting"),
    }
}

/// Forms list their fields and buttons as descriptions and move their own
/// cursor; the first `options` rows are the ones it moves among.
fn form_options(app: &App) -> Option<usize> {
    let shell = &app.deck.shell;
    match shell.nav.current() {
        Route::BackupForm => shell.backup.form.as_ref().map(|f| f.field_count() + 1),
        Route::WifiConnect => shell.wifi_form.as_ref().map(|f| f.action_count()),
        Route::UpdateBundleForm => shell.update_form.as_ref().map(|_| 3),
        _ => None,
    }
}

fn kinds(app: &App, rows: &[Entry]) -> Vec<wros_shell::RowKind> {
    match form_options(app) {
        Some(options) => (0..rows.len())
            .map(|i| {
                if i < options {
                    wros_shell::RowKind::Option
                } else {
                    wros_shell::RowKind::Info
                }
            })
            .collect(),
        None => rows.iter().map(Entry::kind).collect(),
    }
}

/// Browsing and copying from the app's Google Drive folder, at the end of the Google Drive page.
/// The Drive files page: one Drive folder at a time, folders first. A folder opens;
/// a file, or the whole folder, copies to a new place on this laptop.
fn drive_rows(app: &App) -> Vec<Entry> {
    use super::backup::FormKind;
    let backup = &app.deck.shell.backup;
    if !backup.status.google_connected {
        return vec![Entry::info(
            "Link Google Drive first, on the Google Drive page.",
        )];
    }
    let folder = backup.status.drive_folder.as_str();
    let inside = folder
        .strip_prefix("drive:")
        .unwrap_or("")
        .trim_matches('/');
    let mut rows = vec![Entry::info(if inside.is_empty() {
        "My Drive".to_string()
    } else {
        format!("My Drive › {}", inside.replace('/', " › "))
    })];
    if folder.is_empty() {
        if backup.job.is_some() {
            rows.push(Entry::info("Reading your Drive…"));
        } else {
            rows.push(Entry::new(
                "Retry loading My Drive",
                Choice::DriveFolder("drive:".into()),
            ));
        }
        return rows;
    }
    if !inside.is_empty() {
        let up = inside.rsplit_once('/').map_or("", |(up, _)| up);
        rows.push(Entry::new(
            "Up one folder",
            Choice::DriveFolder(format!("drive:{up}")),
        ));
        rows.push(Entry::new(
            "Copy this whole folder to this laptop",
            Choice::BackupForm(FormKind::DriveMaterialize, folder.to_string()),
        ));
    }
    for entry in &backup.status.drive_entries {
        let mut label = if entry.is_dir {
            format!("{}/", entry.name)
        } else if entry.google_doc {
            format!("{} · Google Doc", entry.name)
        } else {
            format!("{} · {}", entry.name, size_label(entry.size))
        };
        if !entry.local.is_empty() {
            label += if entry.cached {
                " · on this laptop"
            } else {
                " · copy on this laptop"
            };
        }
        let choice = if entry.is_dir {
            Choice::DriveFolder(entry.path.clone())
        } else {
            Choice::BackupForm(FormKind::DriveMaterialize, entry.path.clone())
        };
        rows.push(Entry::new(label, choice));
        let local = std::path::Path::new(&entry.local);
        if entry.is_dir && local.is_absolute() && crate::books::is_book(local) {
            rows.push(Entry::new(
                format!("Open the copy of {}", entry.name),
                Choice::Action(Action::OpenBookAt(local.to_path_buf())),
            ));
        }
    }
    if backup.status.drive_entries.is_empty() {
        rows.push(Entry::info("This folder is empty."));
    }
    rows.push(Entry::new(
        "Go to a Drive folder by name",
        Choice::BackupForm(FormKind::DriveBrowse, String::new()),
    ));
    rows.push(Entry::info(
        "Copies are checked against Google Drive. Your books stay on this laptop.",
    ));
    rows
}

/// "12 KB", "1.2 MB".
fn size_label(bytes: u64) -> String {
    match bytes {
        b if b >= 1_048_576 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} bytes"),
    }
}

/// The Settings menu map (#90): each group page and the pages under it, in order.
/// Group pages, their rows, breadcrumbs and the paths the tutorial opens all come
/// from here, so moving a page to another group is moving its line. Each page still
/// builds its own rows in `entries`.
const MENU: &[(Route, &[Route])] = &[
    (
        Route::Settings,
        &[
            Route::Category(Category::Writing),
            Route::Category(Category::Interface),
            Route::Category(Category::System),
            Route::Category(Category::Accounts),
            Route::Category(Category::Updates),
        ],
    ),
    (
        Route::Category(Category::Writing),
        &[
            Route::WritingEditor,
            Route::WritingManuscript,
            Route::WritingGoals,
            Route::WritingRevision,
        ],
    ),
    (
        Route::Category(Category::Interface),
        &[Route::LookPage, Route::KeyboardPage, Route::MusicPage],
    ),
    // Battery and power stays third: the lock VM test reaches it by position.
    (
        Route::Category(Category::System),
        &[
            Route::Panel(Panel::Network),
            Route::Panel(Panel::Display),
            Route::Panel(Panel::Power),
            Route::SoundView,
            Route::Panel(Panel::DateTime),
            Route::Panel(Panel::Storage),
        ],
    ),
    (
        Route::Category(Category::Accounts),
        &[Route::Backups, Route::GoogleAccount, Route::LocalAccounts],
    ),
    (Route::GoogleAccount, &[Route::GoogleDrive]),
    (
        Route::Category(Category::Updates),
        &[Route::Panel(Panel::Updates), Route::Panel(Panel::Recovery)],
    ),
    (
        Route::Backups,
        &[
            Route::BackupDestinations,
            Route::BackupKey,
            Route::BackupQueue,
        ],
    ),
];

/// The map, for tests that walk it.
#[cfg(test)]
pub(crate) fn menu() -> &'static [(Route, &'static [Route])] {
    MENU
}

/// A page's name in its group's list and in breadcrumbs.
pub(crate) fn page_label(route: Route) -> &'static str {
    match route {
        Route::Settings => "Settings",
        Route::Category(category) => category.label(),
        Route::Panel(panel) => panel.label(),
        Route::WritingEditor => "Editor",
        Route::WritingManuscript => "Manuscript",
        Route::WritingGoals => "Goals",
        Route::WritingRevision => "Revision",
        Route::LookPage => "Look",
        Route::KeyboardPage => "Keyboard",
        Route::MusicPage => "Music",
        Route::SoundView => "Sound and volume",
        Route::Backups => "Backups",
        Route::GoogleAccount => "Google Drive",
        Route::GoogleDrive => "Your Drive files",
        Route::LocalAccounts => "Local accounts",
        Route::BackupDestinations => "Where backups go",
        Route::BackupKey => "Recovery key",
        Route::BackupQueue => "Offline queue",
        _ => "",
    }
}

/// The group a page sits in, from the map.
pub fn parent(route: Route) -> Option<Route> {
    MENU.iter()
        .find(|(_, pages)| pages.contains(&route))
        .map(|(group, _)| *group)
}

/// The groups and page to open, from Settings down, for `route`.
pub fn path_to(route: Route) -> Vec<Route> {
    let mut path = vec![route];
    while let Some(group) = parent(path[0]).filter(|group| *group != Route::Settings) {
        path.insert(0, group);
    }
    path
}

/// `WrOS · Settings › System › Display`, for a page on the map.
fn breadcrumb(route: Route) -> Option<String> {
    (route == Route::Settings || parent(route).is_some()).then(|| {
        std::iter::once("WrOS · Settings")
            .chain(
                path_to(route)
                    .into_iter()
                    .filter(|r| *r != Route::Settings)
                    .map(page_label),
            )
            .collect::<Vec<_>>()
            .join(" › ")
    })
}

/// One row per page under `group`. Pages that load something first ask for it as they open.
fn links(group: Route) -> Vec<Entry> {
    MENU.iter()
        .find(|(page, _)| *page == group)
        .map_or(&[][..], |(_, pages)| pages)
        .iter()
        .map(|&page| {
            let choice = match page {
                Route::SoundView => Choice::View(page, wros_sys::Request::AudioStatus),
                Route::LocalAccounts => Choice::View(page, wros_sys::Request::ListLocalAccounts),
                page => Choice::Open(page),
            };
            Entry::new(page_label(page), choice)
        })
        .collect()
}

/// Rows for `actions`, in that order, labelled as the Settings submenu labels them
/// (a label can say the current state, as "Turn spellcheck off").
fn setting_rows(app: &App, actions: &[Action]) -> Vec<Entry> {
    let menu = app.submenu(Sub::Settings);
    actions
        .iter()
        .filter_map(|wanted| {
            menu.iter()
                .find(|(_, action)| action == wanted)
                .map(|(label, action)| Entry::new(label.clone(), Choice::Action(action.clone())))
        })
        .collect()
}

/// The open Settings page as a test driver sees it: its title, and each row with
/// whether it can be chosen.
#[cfg(feature = "test-driver")]
pub(crate) fn driver_page(app: &App) -> (String, Vec<(String, bool)>) {
    let route = app.deck.shell.nav.current();
    let title = breadcrumb(route).unwrap_or_else(|| "WrOS · Settings".into());
    let rows = entries(app)
        .into_iter()
        .map(|entry| {
            let choosable = !matches!(entry.choice, Choice::Information)
                && !(matches!(entry.choice, Choice::Lock) && app.deck.busy);
            (entry.label, choosable)
        })
        .collect();
    (title, rows)
}

fn entries(app: &App) -> Vec<Entry> {
    match app.deck.shell.nav.current() {
        route @ (Route::Settings | Route::Category(_)) => {
            let mut rows = links(route);
            match route {
                Route::Category(Category::Writing) => {
                    rows.extend(setting_rows(app, &[Action::Voices]))
                }
                Route::Category(Category::Interface) => rows.push(Entry::info(
                    "Accessibility and notifications · more controls planned",
                )),
                _ => {}
            }
            rows
        }
        Route::WritingEditor => {
            let mut rows = setting_rows(
                app,
                &[
                    Action::Spellcheck,
                    Action::SpellingLanguage,
                    Action::LineWidth,
                    Action::Typewriter,
                ],
            );
            rows.push(Entry::new("Focus mode", Choice::Action(Action::FocusMode)));
            rows
        }
        Route::WritingManuscript => {
            let mut rows = setting_rows(app, &[Action::ManuscriptLook, Action::AuthorDetails]);
            rows.push(Entry::new(
                "Footnotes · browse and jump",
                Choice::Action(Action::Footnotes),
            ));
            rows
        }
        Route::WritingGoals => vec![
            Entry::new("Writing goals", Choice::Action(Action::Goals)),
            Entry::new("Daily challenges", Choice::Action(Action::DailyChallenges)),
            Entry::new("Draft milestones", Choice::Action(Action::Milestones)),
        ],
        Route::WritingRevision => vec![
            Entry::new("Revision tools", Choice::Action(Action::ProseDashboard)),
            Entry::new("Continuity tracker", Choice::Action(Action::Continuity)),
            Entry::new("Revision notes", Choice::Action(Action::ReviewNotes)),
        ],
        Route::LookPage => {
            let mut rows = setting_rows(
                app,
                &[
                    Action::Themes,
                    Action::Icons,
                    Action::Workspaces,
                    Action::ToggleOutline,
                ],
            );
            if app.deck.boot_settings_available {
                rows.push(Entry::info(
                    "The start screen takes the new colours at the next restart.",
                ));
            }
            rows
        }
        Route::KeyboardPage => vec![
            Entry::new(
                "Keyboard layout · QWERTY / Colemak / Colemak-DH",
                Choice::System(4),
            ),
            Entry::new(
                "First-run keyboard layout wizard · registered maps only",
                Choice::KeyboardWizardOpen,
            ),
            Entry::new("Keyboard shortcuts", Choice::System(11)),
        ],
        Route::MusicPage => setting_rows(app, &[Action::MusicSource, Action::MusicToggle]),
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
                        Choice::View(Route::WifiNetworks, wros_sys::Request::ScanWifi),
                    ),
                    Entry::new(
                        "Saved Wi-Fi profiles",
                        Choice::View(Route::SavedWifi, wros_sys::Request::ListSavedWifi),
                    ),
                    Entry::new("Disconnect active Wi-Fi", Choice::WifiDisconnect),
                    Entry::info(
                        "Select a network in Scan Wi-Fi networks to connect or enter credentials.",
                    ),
                ]),
                Panel::Display => {
                    rows.extend([
                        Entry::info(format!(
                            "Brightness: {}",
                            status
                                .and_then(|s| s.brightness_percent)
                                .map_or("unavailable".into(), |p| format!("{p}%"))
                        )),
                        Entry::new("Brightness -10%", Choice::System(5)),
                        Entry::new("Brightness +10%", Choice::System(6)),
                        if app.deck.graphical_session
                            && !crate::session::native()
                            && !cfg!(all(target_os = "linux", feature = "compositor-warmth"))
                        {
                            Entry::info("Warm screen · unavailable in this build")
                        } else {
                            Entry::new(
                                "Warm screen · blue light filter",
                                Choice::Open(Route::WarmScreen),
                            )
                        },
                    ]);
                    if app.deck.session_settings.is_some() {
                        rows.extend([
                            Entry::new(
                                format!(
                                    "Graphical session: {} · from the next login",
                                    if app.deck.graphical_chosen { "on" } else { "off" }
                                ),
                                Choice::GraphicalSession,
                            ),
                            Entry::info(
                                "Graphical writing starts by default. Plain text is available for recovery.",
                            ),
                        ]);
                    }
                    if app.deck.boot_settings_available {
                        if let Some(status) = &app.deck.boot_status {
                            rows.push(Entry::new(
                                format!(
                                    "Start screen: {}",
                                    if status.enabled { "WrOS" } else { "plain text" }
                                ),
                                Choice::BootScreen(false),
                            ));
                            rows.push(if status.failed && !status.pending {
                                Entry::new(
                                    "Prepare the start screen again",
                                    Choice::BootScreen(true),
                                )
                            } else {
                                Entry::info(status.preparation())
                            });
                        } else {
                            rows.push(Entry::info("Start screen: refresh to read the choice."));
                        }
                    }
                }
                Panel::Power => {
                    rows.extend([
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
                        if app.deck.suspend_available {
                            Entry::new("Lock and suspend", Choice::System(8))
                        } else {
                            Entry::info(wros_sys::SUSPEND_UNAVAILABLE)
                        },
                    ]);
                    // Above Shutdown, so Lock writing session stays the last row.
                    if app.deck.lock_available && app.deck.lid_settings.is_some() {
                        rows.push(Entry::new(
                            format!("When the lid closes: {}", app.deck.lid_close.label()),
                            Choice::LidClose,
                        ));
                        if app.deck.suspend_available {
                            rows.push(Entry::info(
                                "On battery, WrOS warns at 20% and 10%. At 8% it saves, then locks and sleeps after 30 seconds.",
                            ));
                        }
                    }
                    rows.extend([
                        Entry::new("Shutdown · confirmation required", Choice::System(9)),
                        Entry::new("Restart · confirmation required", Choice::System(10)),
                        if app.deck.live_session {
                            Entry::info("Live session · personal lock unavailable")
                        } else if app.deck.lock_available {
                            Entry::new(
                                if app.deck.busy {
                                    "Lock writing session · waiting for the current task"
                                } else {
                                    "Lock writing session"
                                },
                                Choice::Lock,
                            )
                        } else if app.deck.graphical_session {
                            Entry::info("Lock and sleep · not in the graphical session yet")
                        } else {
                            Entry::info("Console lock unavailable in this image/session")
                        },
                    ]);
                }
                Panel::Storage => {
                    rows.push(Entry::new(
                        "Mounted filesystems",
                        Choice::View(
                            Route::StorageDevicesView,
                            wros_sys::Request::ListStorageDevices,
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
                Panel::DateTime => {
                    rows.extend([
                        Entry::info(
                            match (
                                status.and_then(|s| s.network_time),
                                status.and_then(|s| s.time_synchronized),
                            ) {
                                (Some(true), Some(true)) => "Network time: on · synchronized",
                                (Some(true), _) => "Network time: on · not yet synchronized",
                                (Some(false), _) => "Network time: off",
                                (None, _) => "Network time: unknown",
                            },
                        ),
                        Entry::info(format!(
                            "Timezone: {}",
                            status
                                .and_then(|s| s.timezone.as_deref())
                                .unwrap_or("unknown")
                        )),
                        Entry::new("Choose timezone", Choice::TimezoneOpen),
                        Entry::new(
                            "Set the clock by hand (turns network time off)",
                            Choice::ClockOpen,
                        ),
                    ]);
                    if status.and_then(|s| s.network_time) == Some(false) {
                        rows.push(Entry::new(
                            "Use network time again (sets the clock automatically)",
                            Choice::Read(wros_sys::Request::UseNetworkTime),
                        ));
                    }
                }
                Panel::Updates => rows.extend(update_flow::updates_panel(app)),
                Panel::Recovery => rows.extend(update_flow::recovery_panel()),
            }
            rows
        }
        Route::WifiNetworks => {
            let mut rows = vec![Entry::new(
                "Refresh scan",
                Choice::Read(wros_sys::Request::ScanWifi),
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
            } else if app.deck.busy {
                rows.push(Entry::info("Looking for Wi-Fi networks…"));
            } else {
                rows.push(Entry::info(WIFI_SCAN_FAILED));
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
                Choice::Read(wros_sys::Request::ListSavedWifi),
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
                Choice::Read(wros_sys::Request::ListLocalAccounts),
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
                Choice::Read(wros_sys::Request::ListStorageDevices),
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
                    Choice::Read(wros_sys::Request::CheckUpdates),
                ),
                Entry::new(
                    "Review a signed update bundle by path",
                    Choice::UpdateBundleOpen,
                ),
                Entry::new(
                    "Recovery status, repair and staged update",
                    Choice::View(Route::RecoveryView, wros_sys::Request::RecoveryStatus),
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
        Route::UpdateReview => update_flow::review_rows(app),
        Route::UpdateConfirm => update_flow::confirm_rows(app),
        Route::UpdateStaged => update_flow::staged_rows(app),
        Route::GoBackConfirm => update_flow::go_back_rows(app),
        Route::RepairConfirm => update_flow::repair_rows(),
        Route::GoogleAccount => {
            let backup = &app.deck.shell.backup;
            let mut rows = vec![Entry::info(if backup.status.google_connected {
                "Google Drive credentials saved privately on this device"
            } else {
                "Google Drive is not connected"
            })];
            if backup.status.phone_linking {
                rows.push(Entry::new(
                    if backup.status.google_connected {
                        "Relink Google Drive with your phone"
                    } else {
                        "Link Google Drive with your phone"
                    },
                    Choice::LinkPhone,
                ));
                if backup.status.google_connected && backup.status.google_shared_client {
                    rows.push(Entry::info(
                        "Drive uses rclone's shared Google access, which Google often slows down. Relink with your phone to use WrOS's own.",
                    ));
                }
            }
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
                    "Reauthorize Google Drive from a file on a USB stick",
                    Choice::BackupForm(super::backup::FormKind::GoogleFile, String::new()),
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
                rows.push(Entry::new(
                    "Connect Google Drive from a file on a USB stick",
                    Choice::BackupForm(super::backup::FormKind::GoogleFile, String::new()),
                ));
            }
            let folder = &backup.status.drive_folder;
            rows.push(Entry::new(
                page_label(Route::GoogleDrive),
                Choice::DriveFolder(if folder.is_empty() {
                    "drive:".into()
                } else {
                    folder.clone()
                }),
            ));
            rows
        }
        Route::LinkPhone => {
            let backup = &app.deck.shell.backup;
            let mut rows = Vec::new();
            match &backup.job {
                Some(job) => {
                    rows.push(Entry::info(job.status_line()));
                    rows.push(Entry::new("Cancel linking", Choice::BackupCancel));
                }
                None => rows.push(Entry::info(if backup.status.google_connected {
                    "Google Drive is linked. Escape returns to Settings."
                } else {
                    "Linking stopped. Choose Link Google Drive with your phone to start again."
                })),
            }
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
        Route::GoogleDrive => drive_rows(app),
        Route::SyncPanel => vec![
            Entry::info("Readable Google Drive sync is set per book, in Settings › Backups."),
            Entry::info(if app.sync_enabled() {
                "This book syncs with Google Drive; Backups shows its latest result."
            } else {
                "This book is not synced; its words stay on this computer and in encrypted backups."
            }),
        ],
        Route::Backups => {
            use super::backup::FormKind;
            use crate::backup_service::Request;
            let backup = &app.deck.shell.backup;
            let mut rows = vec![
                Entry::info(backup.auto_line(&app.project.root)),
                Entry::new(
                    "Create encrypted backup of this saved project",
                    Choice::BackupNow,
                ),
                Entry::new("Refresh snapshots", Choice::BackupRequest(Request::List)),
            ];
            rows.extend(links(Route::Backups));
            // Keep action positions stable when an asynchronous job finishes.
            rows.insert(
                0,
                if backup.job.is_some() {
                    Entry::new("Cancel current backup operation", Choice::BackupCancel)
                } else {
                    Entry::info("No backup operation is running")
                },
            );
            // Always present, so positions stay stable here too.
            rows.insert(
                1,
                Entry::info(match &backup.job {
                    Some(job) => job.status_line(),
                    None => backup_overview(&backup.status),
                }),
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
            if backup.status.google_connected {
                if app.sync_enabled() {
                    rows.push(Entry::new("Sync now", Choice::SyncNow));
                    if let Some(deletes) = backup.sync.confirm_deletes() {
                        rows.push(Entry::new(
                            format!("Sync and delete {deletes} files"),
                            Choice::SyncConfirmDeletes,
                        ));
                    }
                    rows.push(Entry::info(backup.sync.line()));
                    rows.push(Entry::new(
                        "Stop syncing this book",
                        Choice::SyncSwitch(false),
                    ));
                } else {
                    rows.push(Entry::new(
                        "Sync this book with Google Drive (readable files)",
                        Choice::SyncSwitch(true),
                    ));
                    rows.push(Entry::info(
                        "Readable copies in Drive are not encrypted: anyone with access to your Google account can read them.",
                    ));
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
                if snapshot.has_profile {
                    rows.push(Entry::new(
                        "  ↳ Restore with your settings",
                        Choice::BackupForm(FormKind::RestoreWithSettings, snapshot.id.clone()),
                    ));
                }
            }
            rows
        }
        Route::BackupDestinations => {
            use super::backup::FormKind;
            let mut rows = vec![
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
            ];
            if !app.deck.shell.backup.status.google_connected {
                rows.push(Entry::new(
                    "Link Google Drive first",
                    Choice::Open(Route::GoogleAccount),
                ));
            }
            rows.push(Entry::info(
                "Keep local backups on another device; folders can share the same disk.",
            ));
            rows
        }
        Route::BackupKey => {
            use super::backup::FormKind;
            use crate::backup_service::Request;
            let backup = &app.deck.shell.backup;
            let mut rows = vec![
                Entry::new(
                    "Export recovery key",
                    Choice::BackupForm(FormKind::ExportKey, String::new()),
                ),
                Entry::new(
                    "Recover an interrupted repository lock",
                    Choice::BackupRequest(Request::Unlock),
                ),
            ];
            if backup.status.configured {
                rows.push(Entry::info(format!(
                    "Private recovery key: {}",
                    backup.status.recovery_key
                )));
            }
            rows
        }
        Route::BackupQueue => {
            use crate::backup_service::Request;
            let backup = &app.deck.shell.backup;
            let mut rows = vec![
                Entry::new(
                    "Queue saved project for offline backup",
                    Choice::BackupQueueNow,
                ),
                Entry::new(
                    "Run queued backups and retry interrupted jobs",
                    Choice::BackupRequest(Request::RunQueue),
                ),
            ];
            if backup.status.queued_backups.is_empty() {
                rows.push(Entry::info("Nothing is waiting."));
            } else {
                rows.push(Entry::new(
                    "Clear pending backup queue",
                    Choice::BackupRequest(Request::ClearQueue),
                ));
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
        Route::RecoveryView => update_flow::recovery_rows(app),
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
                                if crate::session::composited() {
                                    "accepted for this session"
                                } else {
                                    "matches display"
                                }
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
        Route::ManualClock => app
            .deck
            .shell
            .clock_form
            .as_ref()
            .map(|form| form.rows().into_iter().map(Entry::info).collect())
            .unwrap_or_default(),
        Route::ManualClockConfirm => vec![
            Entry::new("Cancel manual clock change", Choice::ClockResolve(false)),
            Entry::new(
                format!(
                    "Set local clock to {}",
                    app.deck
                        .shell
                        .clock_choice
                        .as_deref()
                        .unwrap_or("unavailable")
                ),
                Choice::ClockResolve(true),
            ),
            Entry::info(
                "Network time stays off until you switch it back on. Existing writing records keep their recorded dates.",
            ),
        ],
        Route::SoundView => {
            let mut rows = vec![Entry::new(
                "Refresh sound status",
                Choice::Read(wros_sys::Request::AudioStatus),
            )];
            if let Some(audio) = &app.deck.shell.audio_status {
                rows.push(Entry::info(format!(
                    "Output: {} · Volume: {} · Muted: {}",
                    if audio.available {
                        "available"
                    } else {
                        "unavailable"
                    },
                    audio
                        .volume_percent
                        .map_or("unavailable".into(), |v| format!("{v}%")),
                    if audio.muted { "yes" } else { "no" }
                )));
                rows.push(Entry::info(&audio.message));
                if audio.available {
                    rows.push(Entry::new("Volume -10%", Choice::SoundVolume(-10)));
                    rows.push(Entry::new("Volume +10%", Choice::SoundVolume(10)));
                    rows.push(Entry::new(
                        if audio.muted {
                            "Unmute sound output"
                        } else {
                            "Mute sound output"
                        },
                        Choice::SoundMuteToggle,
                    ));
                }
            } else {
                rows.push(Entry::info(
                    "Sound status unavailable; refresh to check local mixer.",
                ));
            }
            rows.push(Entry::info(
                "Writing never depends on sound; a computer without a sound card says so here.",
            ));
            rows
        }
        Route::KeyboardWizard => app
            .deck
            .shell
            .keyboard_wizard
            .as_ref()
            .map(|form| {
                form.rows(&app.deck.shell.custom_keymaps)
                    .into_iter()
                    .map(Entry::info)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(super) const WIFI_SCAN_FAILED: &str =
    "Couldn't scan for Wi-Fi. Make sure Wi-Fi is on, then try again.";

impl App {
    fn clear_system_view(&mut self, request: &wros_sys::Request) {
        use wros_sys::Request;
        match request {
            Request::ScanWifi => self.deck.shell.wifi_networks = None,
            Request::ListSavedWifi => self.deck.shell.wifi_saved = None,
            Request::ListLocalAccounts => self.deck.shell.local_accounts = None,
            Request::ListStorageDevices => self.deck.shell.storage_devices = None,
            Request::CheckUpdates => self.deck.shell.updates = None,
            Request::RecoveryStatus => self.deck.shell.recovery = None,
            Request::GetGoogleStatus => self.deck.shell.google_status = None,
            Request::AudioStatus => self.deck.shell.audio_status = None,
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
    pub fn paste_clock_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.clock_form else {
            return;
        };
        if form.selected != 0 {
            return;
        }
        if form.datetime.len() + text.len() <= 32 && !text.chars().any(char::is_control) {
            form.datetime.push_str(text);
        }
    }
    pub fn on_clock_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.clock_form else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.clock_form = None;
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
                form.datetime.pop();
            }
            Key::Delete if form.selected == 0 => {
                form.datetime.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 0 => {
                self.paste_clock_form(&c.to_string());
            }
            Key::Enter if form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter if form.selected == 1 => {
                self.deck.shell.clock_form = None;
                self.deck.shell.nav.back();
            }
            Key::Enter if form.selected == 2 => {
                let candidate = form.datetime.trim().to_string();
                match wros_sys::timezone::validate_local_datetime(&candidate) {
                    Ok(normalized) => {
                        self.deck.shell.clock_choice = Some(normalized);
                        self.deck.shell.nav.open(Route::ManualClockConfirm);
                        self.deck.shell.nav.select(0, 3);
                    }
                    Err(err) => {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                }
            }
            _ => {}
        }
    }
    pub fn paste_keyboard_wizard(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.keyboard_wizard else {
            return;
        };
        if form.selected != 1 {
            return;
        }
        if form.alias_name.len() + text.len() <= 32 && !text.chars().any(char::is_control) {
            form.alias_name.push_str(text);
        }
    }
    pub fn on_keyboard_wizard_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.keyboard_wizard else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.keyboard_wizard = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % 6,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    5
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if form.selected == 1 => {
                form.alias_name.pop();
            }
            Key::Delete if form.selected == 1 => {
                form.alias_name.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 1 => {
                self.paste_keyboard_wizard(&c.to_string());
            }
            Key::Enter if form.selected == 0 || form.selected == 2 => {
                form.base_index = (form.base_index + 1) % wros_sys::keyboard::Layout::ALL.len();
            }
            Key::Enter if form.selected == 1 => {
                form.selected = 4;
            }
            Key::Enter if form.selected == 3 => {
                if self.deck.busy {
                    return;
                }
                let base = wros_sys::keyboard::Layout::ALL[form.base_index].clone();
                self.deck.shell.external = true;
                self.deck.request(wros_sys::Request::KeyboardPreview(base));
                if self.deck.busy {
                    self.overlay = Overlay::Deck(wros::Page::KeyboardWait);
                }
            }
            Key::Enter if form.selected == 4 => {
                let base = wros_sys::keyboard::Layout::ALL[form.base_index].clone();
                let name = form.alias_name.trim().to_string();
                match wros_sys::keyboard::CustomMapRegistration::new(&name, base.clone()) {
                    Ok(reg) => {
                        self.deck
                            .shell
                            .custom_keymaps
                            .retain(|existing| existing.name != reg.name);
                        self.deck.shell.custom_keymaps.push(reg);
                        self.deck.note =
                            format!("Registered keyboard alias '{name}' for {}.", base.id());
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                    Err(err) => {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                }
            }
            Key::Enter if form.selected == 5 => {
                self.deck.shell.keyboard_wizard = None;
                self.deck.shell.nav.back();
            }
            _ => {}
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
                if let Err(err) = wros_sys::system_views::validate_update_bundle_path(&bundle_dir) {
                    self.deck.note = format!("Unavailable: {err}");
                    self.deck.shell.notices.push(&self.deck.note);
                    return;
                }
                self.deck.shell.update_flow.last_result = None;
                self.deck
                    .request(wros_sys::Request::ReviewUpdateBundle { bundle_dir });
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
                    self.deck.request(wros_sys::Request::DisconnectWifi);
                    return;
                }
                if selected == connect_idx {
                    let ssid = form.network.ssid.clone();
                    if let Err(err) = wros_sys::system_views::validate_wifi_ssid(&ssid) {
                        self.deck.note = format!("Unavailable: {err}");
                        self.deck.shell.notices.push(&self.deck.note);
                        return;
                    }
                    if requires_password {
                        if let Err(err) =
                            wros_sys::system_views::validate_wifi_password(&form.password)
                        {
                            self.deck.note = format!("Unavailable: {err}");
                            self.deck.shell.notices.push(&self.deck.note);
                            return;
                        }
                        let secret = wros_sys::WifiSecret::new(form.password.as_str());
                        form.password = Zeroizing::new(String::new());
                        self.deck.request(wros_sys::Request::ConnectWifi {
                            ssid,
                            password: Some(secret),
                        });
                    } else {
                        self.deck.request(wros_sys::Request::ConnectWifi {
                            ssid,
                            password: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    /// Open a Settings page and do what choosing it does: fetch the status it shows.
    pub(crate) fn open_route(&mut self, route: Route) {
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
        let availability: Option<&str> = None;
        if let Some(message) = availability {
            self.deck.note = message.into();
            self.deck.shell.notices.push(message);
        }
        if route == Route::WarmScreen && !self.deck.busy {
            self.deck.request(wros_sys::Request::DisplayStatus);
        }
        if route == Route::Panel(Panel::Display)
            && self.deck.boot_settings_available
            && !self.deck.busy
        {
            self.deck.request(wros_sys::Request::BootStatus);
        }
        if matches!(route, Route::Panel(Panel::Updates | Panel::Recovery)) {
            // These panels show what the update helper reports,
            // never a cached guess.
            if !self.deck.busy {
                self.refresh_recovery();
            }
        } else if matches!(route, Route::Panel(_))
            && self.deck.snapshot.is_none()
            && !self.deck.busy
        {
            self.deck.request(wros_sys::Request::Refresh);
        }
    }
    /// Settings, already on the page at the end of `path` (the pages before it are
    /// what Esc returns through).
    /// Opens a page on the Settings menu map, through its groups, so Back retraces them.
    pub(crate) fn open_settings_page(&mut self, page: Route) {
        self.open_settings_at(&path_to(page));
    }
    pub(crate) fn open_settings_at(&mut self, path: &[Route]) {
        self.open_settings();
        if let Some((last, parents)) = path.split_last() {
            for route in parents {
                self.deck.shell.nav.open(*route);
            }
            self.open_route(*last);
        }
    }
    pub fn open_settings(&mut self) {
        let origin = match self.overlay {
            Overlay::Menu { .. } | Overlay::Deck(wros::Page::Home { .. }) => self.overlay.clone(),
            _ => Overlay::None,
        };
        self.deck.shell.origin = Some(origin);
        self.deck.shell.external = false;
        self.deck.shell.wifi_form = None;
        self.deck.shell.wifi_saved_choice = None;
        self.deck.shell.update_form = None;
        self.deck.shell.clock_form = None;
        self.deck.shell.clock_choice = None;
        self.deck.shell.keyboard_wizard = None;
        let flow = &mut self.deck.shell.update_flow;
        flow.review = None;
        flow.dir = None;
        flow.recheck = None;
        flow.stage_error = None;
        flow.changed = None;
        self.deck.shell.nav.reset(Route::Settings);
        self.overlay = Overlay::Deck(wros::Page::Settings);
        self.observe_deck(tutorial::Event::SystemOpened);
    }
    /// The live dashboard's install row: Settings opened straight on the installer
    /// confirmation, whose default is still to cancel.
    pub fn open_installer_confirm(&mut self) {
        if !self.deck.live_session || !self.deck.installer_available || self.deck.busy {
            self.deck
                .shell
                .notices
                .push("Live installer is unavailable or a system/backup operation is pending.");
            return;
        }
        self.open_settings();
        self.deck.shell.nav.open(Route::InstallerConfirm);
        self.deck.shell.nav.select(0, 4);
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
        if self.deck.shell.nav.current() == Route::ManualClock {
            self.on_clock_form_key(key);
            return;
        }
        if self.deck.shell.nav.current() == Route::KeyboardWizard {
            self.on_keyboard_wizard_key(key);
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
        if self.deck.shell.nav.current() == Route::LinkPhone && key == Key::Esc {
            // Leaving the code stops linking: the listener closes and the sign-in is dropped.
            if self.deck.shell.backup.job.take().is_some() {
                self.deck.shell.backup.notice = "Linking cancelled; nothing was changed.".into();
            }
            self.deck.shell.nav.back();
            return;
        }
        let rows = entries(self);
        let kinds = kinds(self, &rows);
        let nav = &mut self.deck.shell.nav;
        match key {
            Key::Up => nav.move_among(-1, &kinds, rows.len()),
            Key::Down => nav.move_among(1, &kinds, rows.len()),
            Key::PageUp => nav.move_among(-10, &kinds, rows.len()),
            Key::PageDown => nav.move_among(10, &kinds, rows.len()),
            Key::Home => {
                nav.select(0, rows.len());
                nav.settle(&kinds, rows.len());
            }
            Key::End => {
                nav.select(rows.len().saturating_sub(1), rows.len());
                nav.settle(&kinds, rows.len());
            }
            Key::Esc | Key::Left => {
                self.deck.shell.warm_wait = false;
                self.deck.shell.wifi_form = None;
                self.deck.shell.wifi_saved_choice = None;
                self.deck.shell.update_form = None;
                self.deck.shell.clock_form = None;
                self.deck.shell.clock_choice = None;
                self.deck.shell.keyboard_wizard = None;
                if nav.current() == Route::UpdateReview {
                    // Backing out of a review drops it; it must be authenticated again.
                    let flow = &mut self.deck.shell.update_flow;
                    flow.review = None;
                    flow.dir = None;
                    flow.changed = None;
                }
                if !nav.back() {
                    self.deck.shell.save();
                    self.overlay = self.deck.shell.origin.take().unwrap_or(Overlay::None);
                }
            }
            Key::Enter | Key::Right => {
                nav.select(nav.selected(), rows.len());
                nav.settle(&kinds, rows.len());
                let Some(row) = rows.into_iter().nth(nav.selected()) else {
                    return;
                };
                match row.choice {
                    Choice::BackupRequest(crate::backup_service::Request::RunQueue) => {
                        self.request_run_backup_queue()
                    }
                    Choice::BackupRequest(request) => self.deck.shell.backup.request(request),
                    Choice::BackupForm(kind, snapshot) => self.open_backup_form(kind, snapshot),
                    Choice::DriveFolder(folder) => {
                        if self.deck.shell.nav.current() != Route::GoogleDrive {
                            self.deck.shell.nav.open(Route::GoogleDrive);
                        }
                        if self.deck.shell.backup.status.google_connected {
                            self.deck
                                .shell
                                .backup
                                .request(crate::backup_service::Request::DriveList { folder });
                        }
                    }
                    Choice::BackupNow => self.request_project_backup(),
                    Choice::SyncSwitch(on) => self.request_sync_switch(on),
                    Choice::SyncNow => self.request_sync_run(false),
                    Choice::SyncConfirmDeletes => self.request_sync_run(true),
                    Choice::BackupQueueNow => self.request_project_backup_queue(),
                    Choice::LinkPhone => {
                        self.deck.shell.nav.open(Route::LinkPhone);
                        self.deck
                            .shell
                            .backup
                            .request(crate::backup_service::Request::LinkPhone);
                    }
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
                        self.deck.request(wros_sys::Request::DisconnectWifi);
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
                                SavedWifiAction::Connect => self
                                    .deck
                                    .request(wros_sys::Request::ActivateSavedWifi(profile.uuid)),
                                SavedWifiAction::Forget => self
                                    .deck
                                    .request(wros_sys::Request::ForgetWifi(profile.uuid)),
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
                    Choice::UpdateFindMedia => self.update_find_media(),
                    Choice::UpdateOnlineCheck => self.update_online_check(),
                    Choice::UpdateOnlineFetch => self.update_online_fetch(),
                    Choice::UpdateContinue => self.update_continue(),
                    Choice::UpdateStage => self.update_stage(),
                    Choice::UpdateBackupThenStage(restart) => {
                        self.update_backup_then_stage(restart)
                    }
                    Choice::UpdateAbandon => self.update_abandon(),
                    Choice::UpdateStagedOpen => self.update_open_staged(),
                    Choice::UpdateRestart => self.update_restart(),
                    Choice::UpdateCancelStaged => self.update_cancel_staged(),
                    Choice::UpdateApplyNow => self.update_apply_now(),
                    Choice::UpdateGoBackOpen => self.update_go_back_open(),
                    Choice::UpdateGoBack => self.update_go_back(),
                    Choice::RepairOpen => self.repair_open(),
                    Choice::RepairRun => self.repair_run(),
                    Choice::Lock => self.request_console_lock(),
                    Choice::LidClose => self.cycle_lid_close(),
                    Choice::GraphicalSession => self.toggle_graphical_session(),
                    Choice::BootScreen(retry) => {
                        if !self.deck.busy
                            && let Some(status) = &self.deck.boot_status
                        {
                            self.deck.request(wros_sys::Request::BootScreen {
                                enabled: if retry {
                                    status.enabled
                                } else {
                                    !status.enabled
                                },
                                theme: crate::theme::to_config(&self.theme),
                            });
                        }
                    }
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
                        self.deck.request(wros_sys::Request::ListTimezones);
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
                            self.deck.request(wros_sys::Request::SetTimezone(zone));
                        }
                    }
                    Choice::TimezoneOpen
                    | Choice::TimezoneChoose(_)
                    | Choice::TimezoneResolve(true) => {}
                    Choice::ClockOpen if !self.deck.busy => {
                        self.deck.shell.clock_form = Some(ClockForm {
                            datetime: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.nav.open(Route::ManualClock);
                    }
                    Choice::ClockResolve(false) => {
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.nav.back();
                    }
                    Choice::ClockResolve(true) if !self.deck.busy => {
                        if let Some(dt) = self.deck.shell.clock_choice.clone() {
                            self.deck.request(wros_sys::Request::SetLocalClock(dt));
                        }
                    }
                    Choice::ClockOpen | Choice::ClockResolve(true) => {}
                    Choice::SoundVolume(delta) if !self.deck.busy => {
                        if let Some(audio) = &self.deck.shell.audio_status
                            && audio.available
                            && let Some(current) = audio.volume_percent
                        {
                            let next = (i16::from(current) + delta).clamp(0, 100) as u8;
                            self.deck.request(wros_sys::Request::SetVolume(next));
                        } else {
                            self.deck.note =
                                "No supported sound output is available; volume unchanged.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
                    }
                    Choice::SoundMuteToggle if !self.deck.busy => {
                        if let Some(audio) = &self.deck.shell.audio_status
                            && audio.available
                        {
                            self.deck.request(wros_sys::Request::SetMute(!audio.muted));
                        } else {
                            self.deck.note =
                                "No supported sound output is available; mute unchanged.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
                    }
                    Choice::SoundVolume(_) | Choice::SoundMuteToggle => {}
                    Choice::KeyboardWizardOpen if !self.deck.busy => {
                        self.deck.shell.keyboard_wizard = Some(KeyboardWizardForm {
                            base_index: 0,
                            alias_name: String::new(),
                            selected: 0,
                        });
                        self.deck.shell.nav.open(Route::KeyboardWizard);
                    }
                    Choice::KeyboardWizardOpen => {}
                    Choice::Open(route) => self.open_route(route),
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
                        self.deck.request(wros_sys::Request::DisplayStatus)
                    }
                    Choice::WarmApply if !self.deck.busy => {
                        if self
                            .deck
                            .shell
                            .display
                            .as_ref()
                            .is_some_and(|s| s.available)
                        {
                            self.deck
                                .request(wros_sys::Request::DisplayPreview(self.deck.shell.warmth));
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
                        self.overlay = Overlay::Deck(wros::Page::System { sel });
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
                wros_sys::Request::DisplayConfirm(token)
            } else {
                wros_sys::Request::DisplayCancel(token)
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
                    | Overlay::Deck(wros::Page::System { .. } | wros::Page::Home { .. })
            )
        {
            self.deck.shell.external = false;
            self.overlay = Overlay::Deck(wros::Page::Settings);
        }
    }
}

pub fn draw(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    if app.deck.shell.nav.current() == Route::LinkPhone
        && let Some(offer) = app
            .deck
            .shell
            .backup
            .job
            .as_ref()
            .and_then(|job| job.offer())
        && draw_link_offer(frame, area, &offer, app.linux_console)
    {
        return;
    }
    let entries = entries(app);
    let kinds = kinds(app, &entries);
    let rows = entries
        .into_iter()
        .map(|entry| entry.label)
        .collect::<Vec<_>>();
    let route = app.deck.shell.nav.current();
    let mut title = breadcrumb(route).unwrap_or_else(|| match route {
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
        Route::GoogleDisconnectConfirm => "WrOS · Disconnect Google Drive?".into(),
        Route::GoogleDrive => "WrOS · Settings › Google Drive".into(),
        Route::LinkPhone => "WrOS · Link Google Drive".into(),
        Route::SyncPanel => "WrOS · Settings › Synchronization".into(),
        Route::BackupForm => app
            .deck
            .shell
            .backup
            .form
            .as_ref()
            .map(|form| format!("WrOS · {}", form.title()))
            .unwrap_or_else(|| "WrOS · Backups".into()),
        Route::UpdatesView => "WrOS · Settings › Cached package plan".into(),
        Route::UpdateBundleForm => "WrOS · Review signed offline update".into(),
        Route::UpdateReview => "WrOS · Review signed update".into(),
        Route::UpdateConfirm => "WrOS · Stage this update?".into(),
        Route::UpdateStaged => "WrOS · Update staged".into(),
        Route::GoBackConfirm => "WrOS · Go back a version".into(),
        Route::RecoveryView => "WrOS · Settings › Recovery".into(),
        Route::RepairConfirm => "WrOS · Repair interrupted update?".into(),
        Route::StorageDevicesView => "WrOS · Settings › Mounted filesystems".into(),
        Route::InstallerConfirm => "WrOS · Start encrypted live installer?".into(),
        Route::ManualClock => "WrOS · Settings › Date and time › Manual clock".into(),
        Route::ManualClockConfirm => "WrOS · Set local clock?".into(),
        Route::KeyboardWizard => {
            "WrOS · Settings › Look and keyboard › Keyboard layout wizard".into()
        }
        // Every other page is on the menu map and has a breadcrumb.
        _ => "WrOS · Settings".into(),
    });
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
        // Backups' own pages say what their rows just did (#90 moved those rows here).
        Route::GoogleAccount
            | Route::GoogleDisconnectConfirm
            | Route::GoogleDrive
            | Route::BackupDestinations
            | Route::BackupKey
            | Route::BackupQueue
    ) && !app.deck.shell.backup.notice.is_empty();
    let chain_line = app.update_chain_line();
    let note = if uses_backup_notice {
        app.deck.shell.backup.notice.as_str()
    } else if let Some(line) = &chain_line {
        line.as_str()
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
            kinds: &kinds,
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
            } else if app.deck.shell.nav.current() == Route::ManualClock {
                app.deck
                    .shell
                    .clock_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::KeyboardWizard {
                app.deck
                    .shell
                    .keyboard_wizard
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else {
                wros_shell::settle(app.deck.shell.nav.selected(), &kinds, rows.len())
            },
            notice: note,
            hint: if matches!(
                app.deck.shell.nav.current(),
                Route::BackupForm
                    | Route::WifiConnect
                    | Route::UpdateBundleForm
                    | Route::ManualClock
                    | Route::KeyboardWizard
            ) {
                "Type/paste field  Tab/↑↓ choose  Enter next/confirm  Esc cancel"
            } else {
                "↑↓ choose  Enter open  Esc back  F1 help"
            },
        },
        theme.into(),
    );
}

/// Draw the QR code and phone instructions. Keep full blocks where they fit
/// and on the Linux console, whose font may lack half blocks.
fn draw_link_offer(
    frame: &mut ratatui::Frame,
    area: Rect,
    offer: &crate::backup_service::LinkOffer,
    linux_console: bool,
) -> bool {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph, Wrap};
    let modules = offer.qr.first().map_or(0, String::len) as u16;
    let rows = offer.qr.len() as u16;
    let (compact, width, height) = if area.width >= modules * 2 + 30 && area.height >= rows {
        (false, modules * 2, rows)
    } else if !linux_console && area.width >= modules + 30 && area.height >= rows.div_ceil(2) {
        (true, modules, rows.div_ceil(2))
    } else {
        return false;
    };
    frame.render_widget(Clear, area);
    let top = area.y + (area.height - height) / 2;
    let color = |module| {
        if module == b'0' {
            Color::White
        } else {
            Color::Black
        }
    };
    let code: Vec<Line> = offer
        .qr
        .chunks(if compact { 2 } else { 1 })
        .map(|rows| {
            let lower = rows.get(1).map(String::as_bytes);
            Line::from(
                rows[0]
                    .bytes()
                    .enumerate()
                    .map(|(column, module)| {
                        let style = Style::default().fg(color(module));
                        if compact {
                            let lower_module = lower.map_or(b'0', |row| row[column]);
                            Span::styled("▀", style.bg(color(lower_module)))
                        } else {
                            Span::styled("██", style)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(code),
        Rect::new(area.x + 1, top, width, height),
    );
    let text = vec![
        Line::from("Link Google Drive"),
        Line::from(""),
        Line::from("1. Scan this code with your phone's camera."),
        Line::from("2. Sign in to Google. On the notice about an unverified app, tap"),
        Line::from("   Advanced, then Go to WrOS. Tap Allow."),
        Line::from("3. On the WrOS page, tap Finish linking."),
        Line::from(""),
        Line::from("If the laptop does not answer (school or guest Wi-Fi), tap"),
        Line::from("Approve the laptop instead and type this code on Google's page:"),
        Line::from(""),
        Line::from(Span::styled(
            format!("   {}", offer.code),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from("This screen says when Drive is linked. Escape cancels."),
    ];
    let left = area.x + 1 + width + 3;
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        Rect::new(left, top, area.x + area.width - left, height),
    );
    true
}
