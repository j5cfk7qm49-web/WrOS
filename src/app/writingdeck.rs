//! WritingDeck home, layouts, tutorial and narrow system settings.
//! Render/input paths never run a system command. A single bounded worker does.
use super::*;
use grimoire_core::{
    tutorial,
    workspaces::{self, State, Widget},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::sync::mpsc::{self, Receiver, SyncSender};
use writingdeck_sys::{
    Backend, PowerAction, Reply, Request, Snapshot,
    keyboard::Layout as KeyboardLayout,
    shortcuts::{self, Action as ShortcutAction, Binding, Shortcuts},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Settings,
    Home {
        sel: usize,
    },
    Widgets {
        sel: usize,
    },
    Workspaces {
        sel: usize,
    },
    WorkspacePins {
        workspace: usize,
        sel: usize,
        ids: Vec<u64>,
    },
    System {
        sel: usize,
    },
    Keyboard {
        sel: usize,
    },
    KeyboardWait,
    KeyboardTest {
        text: String,
    },
    Power {
        action: PowerAction,
        yes: bool,
    },
    Tutorial {
        sel: usize,
    },
    Lesson {
        id: u8,
    },
    Shortcuts {
        sel: usize,
        capture: bool,
    },
}
struct Worker {
    tx: SyncSender<Request>,
    rx: Receiver<(Request, Result<Reply, String>)>,
    simulated: bool,
}
impl Worker {
    fn spawn(mut backend: Box<dyn Backend>) -> Self {
        let simulated = backend.simulated();
        let (tx, requests) = mpsc::sync_channel::<Request>(1);
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let result = backend
                    .request(request.clone())
                    .map_err(|e| format!("{e:#}"));
                if results.send((request, result)).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx, simulated }
    }
}
const SHORTCUT_ACTIONS: [(ShortcutAction, &str); 22] = [
    (ShortcutAction::Dashboard, "WritingDeck dashboard"),
    (ShortcutAction::SystemSettings, "System settings"),
    (ShortcutAction::Tutorial, "Interactive tutorial"),
    (ShortcutAction::Outline, "Toggle outline"),
    (ShortcutAction::Focus, "Focus mode"),
    (ShortcutAction::Save, "Save"),
    (ShortcutAction::Find, "Find in scene"),
    (ShortcutAction::Palette, "Find anything"),
    (ShortcutAction::Secondary, "Editable secondary pane"),
    (ShortcutAction::Pinboard, "Pinboard"),
    (ShortcutAction::Pins, "Toggle pins sidebar"),
    (ShortcutAction::PinCurrent, "Pin current document"),
    (ShortcutAction::Journal, "Writing journal"),
    (ShortcutAction::Scratchpad, "Scratchpad"),
    (ShortcutAction::Handoff, "Session handoff"),
    (ShortcutAction::Productivity, "Productivity ledger"),
    (ShortcutAction::MeasuredGoals, "Advanced goals"),
    (ShortcutAction::Calendar, "Calendar and rest days"),
    (ShortcutAction::AdvancedSprints, "Advanced sprints"),
    (ShortcutAction::Achievements, "Milestone collection"),
    (ShortcutAction::DailyChallenges, "Daily challenges"),
    (ShortcutAction::StorySpark, "Story Spark"),
];

pub struct Deck {
    pub shell: super::shell::Shell,
    pub state: State,
    layout_store: Option<workspaces::Store>,
    pub progress: tutorial::Progress,
    tutorial_store: Option<tutorial::Store>,
    pub shortcuts: Shortcuts,
    shortcuts_path: PathBuf,
    shortcuts_seen: Option<grimoire_core::sync::Fingerprint>,
    shortcuts_writable: bool,
    pub snapshot: Option<Snapshot>,
    pub note: String,
    pub busy: bool,
    pub lock_available: bool,
    pub installer_available: bool,
    pub live_session: bool,
    pub lock_requested: bool,
    pub installer_requested: bool,
    worker: Option<Worker>,
    enable_system: bool,
    pub preview: Option<(String, KeyboardLayout, Instant)>,
    pub recent: String,
    pub weekly: i64,
    pub dirty: bool,
}
impl Deck {
    pub fn load(root: &Path, background: bool) -> Self {
        let mut warnings = Vec::new();
        let live_session = background
            && (std::env::var_os("WROS_LIVE_SESSION").is_some()
                || Path::new("/run/live/medium/live/filesystem.squashfs").is_file());
        if live_session {
            warnings
                .push("Live session · save work somewhere persistent before restarting.".into());
        }
        let layout_store = workspaces::Store::load(root)
            .map_err(|e| warnings.push(e.to_string()))
            .ok();
        let mut state = layout_store
            .as_ref()
            .map(|s| s.state.clone())
            .unwrap_or_default();
        let pins_widget_added = !state.dashboard.iter().any(|w| w.widget == Widget::Pins);
        if pins_widget_added {
            state.dashboard.push(workspaces::WidgetConfig {
                widget: Widget::Pins,
                visible: true,
            });
        }
        let journal_widget_added = !state.dashboard.iter().any(|w| w.widget == Widget::Journal);
        if journal_widget_added {
            state.dashboard.push(workspaces::WidgetConfig {
                widget: Widget::Journal,
                visible: true,
            });
        }
        let mut workflow_widgets_added = false;
        for widget in [
            Widget::Scratchpad,
            Widget::Handoff,
            Widget::Accounting,
            Widget::AdvancedGoals,
            Widget::Calendar,
            Widget::AdvancedSprints,
            Widget::Achievements,
            Widget::DailyChallenges,
            Widget::StorySpark,
        ] {
            if !state.dashboard.iter().any(|w| w.widget == widget) {
                state.dashboard.push(workspaces::WidgetConfig {
                    widget,
                    visible: true,
                });
                workflow_widgets_added = true;
            }
        }
        if background
            && !workspaces::Store::path(root).exists()
            && std::env::var_os("WRITINGDECK_OS").is_some()
        {
            state.launch_dashboard = true;
        }
        let tutorial_store = tutorial::Store::load(root)
            .map_err(|e| warnings.push(e.to_string()))
            .ok();
        let progress = tutorial_store
            .as_ref()
            .map(|s| s.progress.clone())
            .unwrap_or_default();
        let shortcuts_path = if background {
            grimoire_core::paths::data_dir().join("writingdeck/shortcuts.json")
        } else {
            root.join(".grimoire/writingdeck/shortcuts.json")
        };
        let mut seen = None;
        let mut writable = true;
        let shortcuts = match fs::read_to_string(&shortcuts_path) {
            Ok(text) => match serde_json::from_str::<Shortcuts>(&text)
                .map_err(anyhow::Error::from)
                .and_then(|s| {
                    s.validate()?;
                    Ok(s)
                }) {
                Ok(s) => {
                    seen = Some(grimoire_core::sync::Fingerprint::of(&text));
                    s
                }
                Err(e) => {
                    warnings.push(format!("shortcut file preserved: {e}"));
                    writable = false;
                    Shortcuts::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Shortcuts::default(),
            Err(e) => {
                warnings.push(e.to_string());
                writable = false;
                Shortcuts::default()
            }
        };
        Self {
            shell: super::shell::Shell::load(root, background),
            state,
            layout_store,
            progress,
            tutorial_store,
            shortcuts,
            shortcuts_path,
            shortcuts_seen: seen,
            shortcuts_writable: writable,
            snapshot: None,
            note: warnings.join("; "),
            busy: false,
            live_session,
            lock_available: {
                #[cfg(target_os = "linux")]
                {
                    background && crate::console_lock::available()
                }
                #[cfg(not(target_os = "linux"))]
                {
                    false
                }
            },
            installer_available: {
                #[cfg(target_os = "linux")]
                {
                    background && live_session && crate::live_installer::available()
                }
                #[cfg(not(target_os = "linux"))]
                {
                    false
                }
            },
            lock_requested: false,
            installer_requested: false,
            worker: None,
            enable_system: background,
            preview: None,
            recent: String::new(),
            weekly: 0,
            dirty: pins_widget_added || journal_widget_added || workflow_widgets_added,
        }
    }
    pub fn observe(&mut self, event: tutorial::Event, demo: bool) {
        if self.progress.observe(event, demo) {
            self.note = "Tutorial exercise verified in the actual application.".into();
            self.save_progress();
        }
    }
    fn save_progress(&mut self) {
        if let Some(store) = &mut self.tutorial_store {
            store.progress = self.progress.clone();
            if let Err(e) = store.save() {
                self.note = format!("Tutorial progress not saved: {e}");
            }
        }
    }
    fn save_layout(&mut self) {
        if let Some(store) = &mut self.layout_store {
            store.state = self.state.clone();
            match store.save() {
                Ok(()) => self.dirty = false,
                Err(e) => self.note = format!("Layout not saved: {e}"),
            }
        } else {
            self.note = "Original layout could not be loaded; not overwriting it.".into();
        }
    }
    pub fn simulated(&self) -> bool {
        self.worker.as_ref().is_some_and(|worker| worker.simulated)
            || self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.simulated)
    }
    pub fn request(&mut self, request: Request) {
        if self.busy {
            self.note = "A system request is still running; please wait.".into();
            return;
        }
        if self.worker.is_none() {
            if !self.enable_system {
                self.note = "System operations disabled in this test session.".into();
                return;
            }
            self.worker = Some(Worker::spawn(Box::new(
                writingdeck_sys::linux::Linux::default(),
            )));
        }
        match self.worker.as_ref().unwrap().tx.try_send(request) {
            Ok(()) => {
                self.busy = true;
                self.note = "Waiting for the system service…".into();
            }
            Err(e) => self.note = format!("System worker unavailable: {e}"),
        }
    }
    #[cfg(test)]
    pub fn simulate(&mut self) {
        self.worker = Some(Worker::spawn(
            Box::new(writingdeck_sys::Simulated::laptop()),
        ));
    }
    #[cfg(test)]
    pub fn use_test_backend(&mut self, backend: Box<dyn Backend>) {
        self.worker = Some(Worker::spawn(backend));
    }
}

impl App {
    pub fn request_console_lock(&mut self) {
        if self.deck.live_session {
            self.deck
                .shell
                .notices
                .push("Live sessions have no personal screen lock.");
            return;
        }
        if !self.deck.lock_available || self.deck.busy {
            self.deck
                .shell
                .notices
                .push("Console locking is unavailable or a system request is pending.");
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck
                .shell
                .notices
                .push("Lock not started: unsaved buffers or source conflicts need attention.");
            return;
        }
        self.save_deck();
        self.deck.lock_requested = true;
    }
    pub fn request_live_installer(&mut self) {
        if !self.deck.live_session {
            self.deck
                .shell
                .notices
                .push("The live installer runs only from a WrOS live session.");
            return;
        }
        self.deck.shell.backup.tick();
        if !self.deck.installer_available || self.deck.busy || self.deck.shell.backup.job.is_some()
        {
            self.deck
                .shell
                .notices
                .push("Live installer is unavailable or a system/backup operation is pending.");
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck
                .shell
                .notices
                .push("Installer not started: unsaved buffers or source conflicts need attention.");
            return;
        }
        self.save_deck();
        self.deck.installer_requested = true;
    }
    pub fn init_writingdeck(&mut self, launch: bool) {
        if launch {
            self.restore_workspace_views(resume::read(&self.project.root).is_none());
            self.focus_mode = self.deck.state.current().focus && self.open.is_some();
            if self.focus_mode {
                self.activate_editor(grimoire_core::documents::Pane::Primary);
            } else if !self.deck.state.current().outline {
                self.focus = self.editor_focus();
            }
            // Recovered words always take precedence over onboarding/dashboard.
            if self.deck.state.launch_dashboard && matches!(self.overlay, Overlay::None) {
                self.open_dashboard();
            }
        }
    }
    pub fn observe_deck(&mut self, event: tutorial::Event) {
        self.deck
            .observe(event, tutorial::is_demo(&self.project.root));
    }
    pub fn remember_deck(&mut self) {
        let primary = self
            .view_position(grimoire_core::documents::Pane::Primary)
            .unwrap_or_default();
        let secondary = self.view_position(grimoire_core::documents::Pane::Secondary);
        let w = self.deck.state.current_mut();
        if w.primary != primary
            || w.secondary != secondary
            || w.secondary_visible != self.views.secondary_visible
            || w.active_editor != self.views.active
            || w.focus != self.focus_mode
        {
            w.primary = primary;
            w.secondary = secondary;
            w.secondary_visible = self.views.secondary_visible;
            w.active_editor = self.views.active;
            w.focus = self.focus_mode;
            self.deck.dirty = true;
        }
    }
    pub fn save_deck(&mut self) {
        self.remember_deck();
        if self.deck.dirty {
            self.deck.save_layout();
        }
    }
    fn restore_deck_position(&mut self) {
        self.restore_workspace_views(true);
    }
    pub fn open_dashboard(&mut self) {
        self.flush();
        self.save_deck();
        let mut recent: Vec<_> = self
            .project
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Scene && n.in_manuscript)
            .collect();
        recent.sort_by_key(|n| std::cmp::Reverse(n.stat.and_then(|s| s.modified)));
        self.deck.recent = recent
            .iter()
            .take(3)
            .map(|n| n.title.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        let days = grimoire_core::days::read(&self.project.root);
        self.deck.weekly = grimoire_core::days::weeks(&days, chrono::Local::now().date_naive(), 1)
            .last()
            .map_or(0, |(_, n)| *n);
        self.overlay = Overlay::Deck(Page::Home { sel: 0 });
        self.observe_deck(tutorial::Event::DashboardOpened);
        if self.background && self.deck.snapshot.is_none() && !self.deck.busy {
            self.deck.request(Request::Refresh);
        }
    }
    pub fn open_system_settings(&mut self) {
        self.overlay = Overlay::Deck(Page::System { sel: 0 });
        self.observe_deck(tutorial::Event::SystemOpened);
        if self.deck.snapshot.is_none() {
            self.deck.request(Request::Refresh);
        }
    }
    pub fn toggle_outline(&mut self) {
        let editor_focus = self.editor_focus();
        let w = self.deck.state.current_mut();
        w.outline = !w.outline;
        if !w.outline && self.focus == Focus::Tree {
            self.focus = editor_focus;
        }
        self.msg = if w.outline {
            "outline shown"
        } else {
            "outline hidden · F10 restores it"
        }
        .into();
        self.deck.dirty = true;
        self.save_deck();
        self.observe_deck(tutorial::Event::OutlineToggled);
    }
    pub fn deck_shortcut(&mut self, action: ShortcutAction) {
        match action {
            ShortcutAction::Dashboard => self.open_dashboard(),
            ShortcutAction::SystemSettings => self.open_system_settings(),
            ShortcutAction::Tutorial => self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 }),
            ShortcutAction::Outline => self.toggle_outline(),
            ShortcutAction::Secondary => self.toggle_secondary(),
            ShortcutAction::Focus => self.toggle_focus_mode(),
            ShortcutAction::Save => self.save(),
            ShortcutAction::Find => self.open_find(),
            ShortcutAction::Palette => self.open_palette(),
            ShortcutAction::Pinboard => self.open_pinboard(),
            ShortcutAction::Pins => self.toggle_pins(),
            ShortcutAction::PinCurrent => self.pin_current(),
            ShortcutAction::Journal => self.open_journal(),
            ShortcutAction::Scratchpad => self.open_scratchpad(),
            ShortcutAction::Handoff => self.open_handoff(),
            ShortcutAction::Productivity => self.run_action(Action::Productivity),
            ShortcutAction::MeasuredGoals => self.run_action(Action::MeasuredGoals),
            ShortcutAction::Calendar => self.run_action(Action::Calendar),
            ShortcutAction::AdvancedSprints => self.run_action(Action::AdvancedSprints),
            ShortcutAction::Achievements => self.run_action(Action::Achievements),
            ShortcutAction::DailyChallenges => self.run_action(Action::DailyChallenges),
            ShortcutAction::StorySpark => self.run_action(Action::StorySpark),
        }
    }
    pub fn add_deck_shortcut(&mut self, chord: &str) {
        let Overlay::Deck(Page::Shortcuts { sel, capture: true }) = self.overlay else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            if !self.deck.shortcuts_writable {
                anyhow::bail!("original shortcut file is unreadable; preserved");
            }
            let chord = shortcuts::canonical(chord)?;
            let action = SHORTCUT_ACTIONS[sel].0;
            let mut shortcuts = self.deck.shortcuts.clone();
            shortcuts.bindings.retain(|b| b.chord != chord);
            shortcuts.bindings.push(Binding { chord, action });
            shortcuts.validate()?;
            let text = serde_json::to_string_pretty(&shortcuts)? + "\n";
            fs::create_dir_all(self.deck.shortcuts_path.parent().unwrap())?;
            match grimoire_core::sync::save_guarded(
                &self.deck.shortcuts_path,
                &text,
                self.deck.shortcuts_seen,
            )? {
                grimoire_core::sync::SaveOutcome::Written => {}
                grimoire_core::sync::SaveOutcome::Conflict { .. } => {
                    anyhow::bail!("shortcuts changed externally; not overwritten")
                }
            }
            self.deck.shortcuts = shortcuts;
            self.deck.shortcuts_seen = Some(grimoire_core::sync::Fingerprint::of(&text));
            Ok(())
        })();
        self.overlay = Overlay::Deck(Page::Shortcuts {
            sel,
            capture: false,
        });
        match result {
            Ok(()) => {
                self.deck.note = "Shortcut alias saved; established bindings are unchanged.".into();
                self.observe_deck(tutorial::Event::ShortcutAdded);
            }
            Err(e) => self.deck.note = format!("Shortcut not saved: {e}"),
        }
    }
    pub fn tick_deck(&mut self) {
        self.deck.shell.backup.tick();
        self.tick_productivity();
        let result = self.deck.worker.as_ref().and_then(|w| w.rx.try_recv().ok());
        if let Some((request, result)) = result {
            self.deck.busy = false;
            match result {
                Ok(Reply::Snapshot(s)) => {
                    self.deck.note = if s.simulated {
                        "SIMULATED system status; not hardware verified"
                    } else {
                        "System status refreshed locally."
                    }
                    .into();
                    self.deck.snapshot = Some(s);
                }
                Ok(Reply::Message(message)) => {
                    self.deck.note = message;
                    if matches!(request, Request::KeyboardConfirm(_)) {
                        self.observe_deck(tutorial::Event::LayoutConfirmed);
                    }
                    if matches!(
                        request,
                        Request::DisplayConfirm(_) | Request::DisplayCancel(_)
                    ) {
                        self.deck.request(Request::DisplayStatus);
                    }
                    if let Request::ConnectWifi { ssid, .. } = &request {
                        self.deck.shell.wifi_form = None;
                        if let Some(networks) = &mut self.deck.shell.wifi_networks {
                            for network in networks {
                                network.in_use = &network.ssid == ssid;
                            }
                        }
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == super::shell::Route::WifiConnect
                        {
                            self.deck.shell.nav.back();
                        }
                    }
                    if matches!(request, Request::DisconnectWifi) {
                        if let Some(networks) = &mut self.deck.shell.wifi_networks {
                            for network in networks {
                                network.in_use = false;
                            }
                        }
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == super::shell::Route::WifiConnect
                        {
                            self.deck.shell.wifi_form = None;
                            self.deck.shell.nav.back();
                        }
                    }
                    if let Request::ForgetWifi(uuid) = &request
                        && let Some(saved) = &mut self.deck.shell.wifi_saved
                    {
                        saved.retain(|s| &s.uuid != uuid);
                    }
                    if let Request::ActivateSavedWifi(uuid) = &request
                        && let Some(profile) = self
                            .deck
                            .shell
                            .wifi_saved
                            .as_ref()
                            .and_then(|s| s.iter().find(|p| &p.uuid == uuid))
                            .cloned()
                        && let Some(networks) = &mut self.deck.shell.wifi_networks
                    {
                        for network in networks {
                            network.in_use = network.ssid == profile.name;
                        }
                    }
                    if matches!(request, Request::VerifyUpdateBundle { .. }) {
                        self.deck.shell.update_form = None;
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current()
                                == super::shell::Route::UpdateBundleForm
                        {
                            self.deck.shell.nav.back();
                        }
                    }
                    if matches!(request, Request::CancelStagedUpdates)
                        && let Some(recovery) = &mut self.deck.shell.recovery
                    {
                        recovery.staged_update = false;
                        recovery.staged_packages.clear();
                    }
                    if matches!(request, Request::RecoverUpdates)
                        && let Some(recovery) = &mut self.deck.shell.recovery
                    {
                        recovery.interrupted_dpkg = false;
                        recovery.unconfigured_packages.clear();
                    }
                }
                Ok(Reply::WifiScan(networks)) => {
                    self.deck.note = format!(
                        "{} named Wi-Fi networks returned. Select a network to connect.",
                        networks.len()
                    );
                    self.deck.shell.wifi_networks = Some(networks);
                }
                Ok(Reply::SavedWifi(profiles)) => {
                    self.deck.note = format!(
                        "{} saved Wi-Fi profiles visible to this session.",
                        profiles.len()
                    );
                    self.deck.shell.wifi_saved = Some(profiles);
                }
                Ok(Reply::LocalAccounts(accounts)) => {
                    self.deck.note = "Local account list loaded; no account was changed.".into();
                    self.deck.shell.local_accounts = Some(accounts);
                }
                Ok(Reply::StorageDevices(devices)) => {
                    self.deck.note = "Mounted filesystem usage loaded locally.".into();
                    self.deck.shell.storage_devices = Some(devices);
                }
                Ok(Reply::Updates(plan)) => {
                    self.deck.note = plan.status.clone();
                    self.deck.shell.updates = Some(plan);
                }
                Ok(Reply::Recovery(info)) => {
                    self.deck.note = info.status.clone();
                    self.deck.shell.recovery = Some(info);
                }
                Ok(Reply::GoogleStatus(status)) => {
                    self.deck.note = status.status_message.clone();
                    self.deck.shell.google_status = Some(status);
                }
                Ok(Reply::GoogleDriveFiles(files)) => {
                    self.deck.note =
                        "Development service reply; Drive enrollment is not implemented.".into();
                    self.deck.shell.drive_files = Some(files);
                }
                Ok(Reply::Backups(backups)) => {
                    self.deck.note =
                        "Development service reply; verified backup is not configured.".into();
                    self.deck.shell.backups = Some(backups);
                }
                Ok(Reply::Timezones(zones)) => {
                    self.deck.note = "Local timezone catalog loaded. Type to filter.".into();
                    self.deck.shell.timezones = Some(zones);
                }
                Ok(Reply::Timezone(zone)) => {
                    self.deck.note = format!(
                        "Timezone verified: {zone}{}.",
                        if self.deck.snapshot.as_ref().is_some_and(|s| s.simulated) {
                            " (simulated)"
                        } else {
                            ""
                        }
                    );
                    self.deck
                        .snapshot
                        .get_or_insert_with(Snapshot::default)
                        .timezone = Some(zone);
                    if matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == super::shell::Route::TimezoneConfirm
                    {
                        self.deck.shell.timezone_choice = None;
                        self.deck.shell.nav.back();
                        self.deck.shell.nav.back();
                    }
                }
                Ok(Reply::DisplayStatus(status)) => {
                    self.deck.shell.warmth = status.warmth;
                    self.deck.note = status.message.clone();
                    self.deck.shell.display = Some(status);
                }
                Ok(Reply::DisplayPreview { token, seconds }) => {
                    if self.deck.shell.warm_wait
                        && matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == super::shell::Route::WarmScreen
                    {
                        self.deck.shell.warm_preview =
                            Some((token, Instant::now() + Duration::from_secs(seconds.min(15))));
                        self.deck.shell.nav.open(super::shell::Route::WarmPreview);
                        self.deck.note = "Temporary warmth applied. Choose Keep or Cancel; automatic rollback is armed.".into();
                    } else {
                        self.deck.request(Request::DisplayCancel(token));
                    }
                    self.deck.shell.warm_wait = false;
                }
                Ok(Reply::KeyboardPreview { token, seconds }) => {
                    if let Request::KeyboardPreview(layout) = request {
                        if matches!(self.overlay, Overlay::Deck(Page::KeyboardWait)) {
                            self.deck.preview = Some((
                                token,
                                layout,
                                Instant::now() + Duration::from_secs(seconds.min(15)),
                            ));
                            self.overlay = Overlay::Deck(Page::KeyboardTest {
                                text: String::new(),
                            });
                            self.deck.note =
                                "Test the layout here. Enter keeps it; Esc reverts.".into();
                        } else {
                            self.deck.request(Request::KeyboardCancel(token));
                        }
                    }
                }
                Err(e) => {
                    self.deck.shell.warm_wait = false;
                    if matches!(request, Request::DisplayStatus) {
                        self.deck.shell.display = None;
                    }
                    if matches!(request, Request::ConnectWifi { .. })
                        && let Some(form) = &mut self.deck.shell.wifi_form
                    {
                        form.password.clear();
                    }
                    self.deck.note = format!("Unavailable: {e}");
                    if matches!(self.overlay, Overlay::Deck(Page::KeyboardWait)) {
                        self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                    }
                }
            }
            if self.deck.simulated() && !self.deck.note.contains("SIMULATED") {
                self.deck.note = format!("SIMULATED · {}", self.deck.note);
            }
            self.deck.shell.notices.push(&self.deck.note);
        }
        if self
            .deck
            .shell
            .warm_preview
            .as_ref()
            .is_some_and(|(_, end)| {
                Instant::now() >= *end || !matches!(self.overlay, Overlay::Deck(Page::Settings))
            })
            && !self.deck.busy
        {
            let (token, _) = self.deck.shell.warm_preview.take().unwrap();
            self.deck.request(Request::DisplayCancel(token));
            if self.deck.shell.nav.current() == super::shell::Route::WarmPreview {
                self.deck.shell.nav.back();
            }
        }
        if self
            .deck
            .preview
            .as_ref()
            .is_some_and(|(_, _, end)| Instant::now() >= *end)
        {
            let (token, _, _) = self.deck.preview.take().unwrap();
            self.deck.request(Request::KeyboardCancel(token));
            if matches!(self.overlay, Overlay::Deck(Page::KeyboardTest { .. })) {
                self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
            }
        }
    }
    pub fn on_deck_key(&mut self, key: Key) {
        let Overlay::Deck(page) = self.overlay.clone() else {
            return;
        };
        match page {
            Page::Settings => self.on_settings_key(key),
            Page::Home { mut sel } => {
                let widgets: Vec<_> = self
                    .deck
                    .state
                    .dashboard
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| w.visible)
                    .map(|(i, _)| i)
                    .collect();
                let n = widgets.len();
                if nav(key, &mut sel, n + 7) {
                    self.overlay = Overlay::Deck(Page::Home { sel });
                    return;
                }
                match key {
                    Key::Esc | Key::F(12) => {
                        self.overlay = Overlay::None;
                        if self.open.is_some() {
                            self.focus = self.editor_focus();
                        }
                    }
                    Key::Char('p') => self.open_pinboard(),
                    Key::Char('j') => self.open_journal(),
                    Key::Char('s') => self.open_scratchpad(),
                    Key::Char('o') => self.open_handoff(),
                    Key::Char('w') => self.overlay = Overlay::Deck(Page::Widgets { sel: 0 }),
                    Key::Char('h') if sel < n => {
                        self.deck.state.dashboard[widgets[sel]].visible = false;
                        self.deck.dirty = true;
                        self.save_deck();
                        self.overlay = Overlay::Deck(Page::Home {
                            sel: sel.min(n.saturating_sub(1)),
                        });
                    }
                    Key::Char('J' | 'K') if sel < n => {
                        let next = if key == Key::Char('J') {
                            (sel + 1).min(n - 1)
                        } else {
                            sel.saturating_sub(1)
                        };
                        self.deck.state.dashboard.swap(widgets[sel], widgets[next]);
                        self.deck.dirty = true;
                        self.save_deck();
                        self.overlay = Overlay::Deck(Page::Home { sel: next });
                    }
                    Key::Enter => {
                        if sel < n {
                            self.open_widget(self.deck.state.dashboard[widgets[sel]].widget);
                        } else {
                            match sel - n {
                                0 => self.overlay = Overlay::None,
                                1 => self.run_action(Action::Goals),
                                2 => self.run_action(Action::Progress),
                                3 => {
                                    self.overlay = Overlay::Deck(Page::Workspaces {
                                        sel: self.deck.state.selected,
                                    })
                                }
                                4 => self.open_settings(),
                                5 => self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 }),
                                _ => self.overlay = Overlay::Deck(Page::Widgets { sel: 0 }),
                            }
                        }
                    }
                    _ => {}
                }
            }
            Page::Widgets { mut sel } => {
                if nav(key, &mut sel, self.deck.state.dashboard.len()) {
                    self.overlay = Overlay::Deck(Page::Widgets { sel });
                    return;
                }
                match key {
                    Key::Enter | Key::Char(' ') => {
                        self.deck.state.dashboard[sel].visible =
                            !self.deck.state.dashboard[sel].visible;
                        self.deck.dirty = true;
                        self.save_deck();
                    }
                    Key::Esc => self.open_dashboard(),
                    _ => {}
                }
            }
            Page::Workspaces { mut sel } => {
                if nav(key, &mut sel, self.deck.state.workspaces.len()) {
                    self.overlay = Overlay::Deck(Page::Workspaces { sel });
                    return;
                }
                match key {
                    Key::Enter => {
                        if !self.commit_saves() || self.project.dirty_count() != 0 {
                            return;
                        }
                        self.remember_deck();
                        self.deck.state.selected = sel;
                        self.restore_deck_position();
                        self.focus_mode = self.deck.state.current().focus && self.open.is_some();
                        if self.focus_mode {
                            self.activate_editor(grimoire_core::documents::Pane::Primary);
                        } else if !self.deck.state.current().outline {
                            self.focus = self.editor_focus();
                        }
                        self.deck.dirty = true;
                        self.save_deck();
                        self.overlay = Overlay::None;
                        self.observe_deck(tutorial::Event::WorkspaceSelected);
                    }
                    Key::Char('+' | '-') => {
                        let w = &mut self.deck.state.workspaces[sel];
                        w.outline_width = if key == Key::Char('+') {
                            (w.outline_width + 2).min(60)
                        } else {
                            w.outline_width.saturating_sub(2).max(18)
                        };
                        self.deck.dirty = true;
                        self.deck.save_layout();
                    }
                    Key::Char('<' | '>') => {
                        let w = &mut self.deck.state.workspaces[sel];
                        w.secondary_percent = if key == Key::Char('>') {
                            (w.secondary_percent + 5).min(75)
                        } else {
                            w.secondary_percent.saturating_sub(5).max(25)
                        };
                        self.deck.dirty = true;
                        self.deck.save_layout();
                    }
                    Key::Char('b') => self.starter_preset_picker(sel),
                    Key::Char('p') => {
                        let ids = self.deck.state.workspaces[sel]
                            .pin_ids
                            .clone()
                            .unwrap_or_else(|| {
                                self.pins.state().pins.iter().map(|p| p.id).collect()
                            });
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace: sel,
                            sel: 0,
                            ids,
                        });
                    }
                    Key::Char('n') => {
                        if self.deck.state.workspaces.len() < 64 {
                            let mut w = self.deck.state.current().clone();
                            let mut n = 1;
                            while self
                                .deck
                                .state
                                .workspaces
                                .iter()
                                .any(|w| w.name == format!("Custom {n}"))
                            {
                                n += 1;
                            }
                            w.name = format!("Custom {n}");
                            self.deck.state.workspaces.push(w);
                            self.deck.dirty = true;
                            self.deck.save_layout();
                        }
                    }
                    Key::Esc => self.open_dashboard(),
                    _ => {}
                }
            }
            Page::WorkspacePins {
                workspace,
                mut sel,
                mut ids,
            } => {
                if nav(key, &mut sel, self.pins.state().pins.len()) {
                    self.overlay = Overlay::Deck(Page::WorkspacePins {
                        workspace,
                        sel,
                        ids,
                    });
                    return;
                }
                match key {
                    Key::Char(' ') => {
                        if let Some(pin) = self.pins.state().pins.get(sel) {
                            if ids.contains(&pin.id) {
                                ids.retain(|id| *id != pin.id);
                            } else {
                                ids.push(pin.id);
                            }
                        }
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids,
                        });
                    }
                    Key::Char('z') => {
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids: vec![],
                        })
                    }
                    Key::Char('a') => {
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids: self.pins.state().pins.iter().map(|p| p.id).collect(),
                        })
                    }
                    Key::Enter | Key::Char('A') => {
                        let result = self.save_workspace_pin_set(
                            workspace,
                            if key == Key::Char('A') {
                                None
                            } else {
                                Some(ids)
                            },
                        );
                        self.deck.note = match result {
                            Ok(()) => {
                                "Workspace pin set saved; global cards/sources unchanged".into()
                            }
                            Err(e) => format!("Pin set not saved: {e:#}"),
                        };
                        self.overlay = Overlay::Deck(Page::Workspaces { sel: workspace });
                    }
                    Key::Esc => self.overlay = Overlay::Deck(Page::Workspaces { sel: workspace }),
                    _ => {}
                }
            }
            Page::System { mut sel } => {
                if nav(key, &mut sel, 13) {
                    self.overlay = Overlay::Deck(Page::System { sel });
                    return;
                }
                if key == Key::Esc {
                    self.open_dashboard();
                    return;
                }
                if key != Key::Enter {
                    return;
                }
                match sel {
                    0 => self.deck.request(Request::Refresh),
                    1 => self.deck.request(Request::Wifi(true)),
                    2 => self.deck.request(Request::Wifi(false)),
                    3 => self.deck.request(Request::ScanWifi),
                    4 => self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 }),
                    5 | 6 => {
                        let now = self
                            .deck
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.brightness_percent);
                        if let Some(now) = now {
                            self.deck.request(Request::Brightness(if sel == 5 {
                                now.saturating_sub(10).max(1)
                            } else {
                                now.saturating_add(10).min(100)
                            }));
                        } else {
                            self.deck.note =
                                "No supported backlight is available; brightness unchanged.".into();
                        }
                    }
                    7 => {
                        self.deck.state.low_power = !self.deck.state.low_power;
                        self.deck.dirty = true;
                        self.save_deck();
                        self.deck
                            .request(Request::PowerSave(self.deck.state.low_power));
                    }
                    8 => {
                        self.deck.note = writingdeck_sys::SUSPEND_UNAVAILABLE.into();
                        self.deck.shell.notices.push(&self.deck.note);
                    }
                    9..=10 => {
                        self.overlay = Overlay::Deck(Page::Power {
                            action: match sel {
                                9 => PowerAction::Shutdown,
                                _ => PowerAction::Restart,
                            },
                            yes: false,
                        })
                    }
                    11 => {
                        self.overlay = Overlay::Deck(Page::Shortcuts {
                            sel: 0,
                            capture: false,
                        })
                    }
                    12 => {
                        self.deck.state.launch_dashboard = !self.deck.state.launch_dashboard;
                        self.deck.dirty = true;
                        self.save_deck();
                    }
                    _ => {}
                }
            }
            Page::Keyboard { mut sel } => {
                if nav(key, &mut sel, KeyboardLayout::ALL.len()) {
                    self.overlay = Overlay::Deck(Page::Keyboard { sel });
                    return;
                }
                match key {
                    Key::Enter if !self.deck.busy => {
                        self.deck
                            .request(Request::KeyboardPreview(KeyboardLayout::ALL[sel].clone()));
                        if self.deck.busy {
                            self.overlay = Overlay::Deck(Page::KeyboardWait);
                        }
                    }
                    Key::Esc => self.open_system_settings(),
                    _ => {}
                }
            }
            Page::KeyboardWait => {
                if key == Key::Esc {
                    self.open_system_settings();
                }
            }
            Page::KeyboardTest { mut text } => match key {
                Key::Enter | Key::Esc => {
                    if let Some((token, _, _)) = self.deck.preview.take() {
                        self.deck.request(if key == Key::Enter {
                            Request::KeyboardConfirm(token)
                        } else {
                            Request::KeyboardCancel(token)
                        });
                    }
                    self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                }
                Key::Char(c) if !c.is_control() && text.chars().count() < 160 => {
                    text.push(c);
                    self.overlay = Overlay::Deck(Page::KeyboardTest { text });
                }
                Key::Backspace => {
                    text.pop();
                    self.overlay = Overlay::Deck(Page::KeyboardTest { text });
                }
                _ => {}
            },
            Page::Power { action, mut yes } => match key {
                Key::Left | Key::Right | Key::Tab => {
                    yes = !yes;
                    self.overlay = Overlay::Deck(Page::Power { action, yes });
                }
                Key::Esc => self.open_system_settings(),
                Key::Enter => {
                    if yes && action == PowerAction::Suspend {
                        self.deck.note = writingdeck_sys::SUSPEND_UNAVAILABLE.into();
                        self.deck.shell.notices.push(&self.deck.note);
                    } else if yes {
                        if !self.commit_saves()
                            || self.project.dirty_count() != 0
                            || self.tree_stale
                            || !self.conflicts().is_empty()
                        {
                            self.deck.note = "Power unchanged: unsaved buffers or source conflicts need attention. Return to writing to review them.".into();
                            self.deck.shell.notices.push(&self.deck.note);
                            return;
                        }
                        self.save_deck();
                        self.deck.request(Request::Power(action));
                    }
                    self.open_system_settings();
                }
                _ => {}
            },
            Page::Tutorial { mut sel } => {
                if nav(key, &mut sel, tutorial::lessons().len()) {
                    self.overlay = Overlay::Deck(Page::Tutorial { sel });
                    return;
                }
                match key {
                    Key::Enter => self.overlay = Overlay::Deck(Page::Lesson { id: sel as u8 + 1 }),
                    Key::Char('s') => {
                        self.deck.progress.skip(sel as u8 + 1);
                        self.deck.save_progress();
                    }
                    Key::Char('d') => self.start_tutorial_demo(),
                    Key::Esc => self.open_dashboard(),
                    _ => {}
                }
            }
            Page::Lesson { id } => match key {
                Key::Enter => self.start_deck_lesson(id),
                Key::Esc => {
                    self.overlay = Overlay::Deck(Page::Tutorial {
                        sel: id as usize - 1,
                    })
                }
                Key::Char('h') => {
                    let topic = &tutorial::lessons()[id as usize - 1].help_topic;
                    let topic = crate::help::TOPICS
                        .iter()
                        .find(|t| t.id == topic)
                        .map_or("writingdeck", |t| t.id);
                    self.open_help(Some(topic));
                }
                _ => {}
            },
            Page::Shortcuts { mut sel, capture } => {
                if capture {
                    if key == Key::Esc {
                        self.overlay = Overlay::Deck(Page::Shortcuts {
                            sel,
                            capture: false,
                        });
                    }
                    return;
                }
                if nav(key, &mut sel, SHORTCUT_ACTIONS.len()) {
                    self.overlay = Overlay::Deck(Page::Shortcuts { sel, capture });
                    return;
                }
                match key {
                    Key::Enter => {
                        self.overlay = Overlay::Deck(Page::Shortcuts { sel, capture: true })
                    }
                    Key::Esc => self.open_system_settings(),
                    _ => {}
                }
            }
        }
    }
    fn open_widget(&mut self, w: Widget) {
        match w {
            Widget::Resume | Widget::Manuscript => {
                self.overlay = Overlay::None;
                if self.open.is_some() {
                    self.focus = self.editor_focus();
                }
            }
            Widget::DailyGoal | Widget::NovelGoal => self.run_action(Action::Goals),
            Widget::WeeklyProgress => self.run_action(Action::Progress),
            Widget::Sprint => self.run_action(Action::StartSprint),
            Widget::RecentScenes => self.open_palette(),
            Widget::Battery | Widget::System => self.open_system_settings(),
            Widget::Backup => {
                self.open_settings();
                self.deck.shell.nav.open(super::shell::Route::Backups);
                self.deck
                    .shell
                    .backup
                    .request(crate::backup_service::Request::Status);
            }
            Widget::Pins => self.open_pinboard(),
            Widget::Journal => self.open_journal(),
            Widget::Scratchpad => self.open_scratchpad(),
            Widget::Handoff => self.open_handoff(),
            Widget::Accounting => self.run_action(Action::Productivity),
            Widget::AdvancedGoals => self.run_action(Action::MeasuredGoals),
            Widget::Calendar => self.run_action(Action::Calendar),
            Widget::AdvancedSprints => self.run_action(Action::AdvancedSprints),
            Widget::Achievements => self.run_action(Action::Achievements),
            Widget::DailyChallenges => self.run_action(Action::DailyChallenges),
            Widget::StorySpark => self.open_spark(),
        }
    }
    fn start_tutorial_demo(&mut self) {
        let parent = if self.background {
            grimoire_core::paths::data_dir().join("writingdeck")
        } else {
            self.project.root.parent().unwrap().to_path_buf()
        };
        if let Err(e) = fs::create_dir_all(&parent) {
            self.deck.note = e.to_string();
            return;
        }
        let mut n = 1;
        let mut root = parent.join("Tutorial-1");
        while root.exists() {
            n += 1;
            root = parent.join(format!("Tutorial-{n}"));
        }
        match tutorial::create_demo(&root) {
            Ok(()) => {
                if self.open_book(&root) {
                    self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 });
                    self.deck.note =
                        "Safe demo project opened; your novel was saved and left untouched.".into();
                }
            }
            Err(e) => self.deck.note = format!("Demo not created: {e:#}"),
        }
    }
    fn start_deck_lesson(&mut self, id: u8) {
        if tutorial::lessons()
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.event)
            .is_some_and(|e| !e.safe_outside_demo())
            && !tutorial::is_demo(&self.project.root)
        {
            self.deck.note = "This exercise needs a safe demo: press d in Interactive tutorial first. Your novel is unchanged.".into();
            return;
        }
        if let Err(e) = self.deck.progress.start(id) {
            self.deck.note = e.to_string();
            return;
        }
        self.deck.save_progress();
        match id {
            1 => self.open_dashboard(),
            2 => self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 }),
            4 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
            }
            5 => {
                self.overlay = Overlay::None;
                self.deck.state.current_mut().outline = true;
                self.focus_mode = false;
                self.focus = Focus::Tree;
            }
            6 => self.run_action(Action::Corkboard),
            7 => {
                self.overlay = Overlay::Deck(Page::Workspaces {
                    sel: self.deck.state.selected,
                })
            }
            8 => {
                self.overlay = Overlay::None;
                if !self.focus_mode {
                    self.toggle_focus_mode();
                }
            }
            9 => {
                self.overlay = Overlay::None;
                self.toggle_outline();
            }
            10 => {
                self.overlay = Overlay::Deck(Page::Shortcuts {
                    sel: 0,
                    capture: false,
                })
            }
            11 => self.pin_picker(None),
            12 => self.open_pinboard(),
            13 => {
                self.focus_mode = false;
                self.open_secondary_picker();
            }
            14 => {
                self.journal.review_lesson_start();
                self.open_journal();
            }
            15 => self.open_scratchpad(),
            16 => self.open_handoff(),
            17 => self.run_action(Action::Goals),
            18 => self.open_productivity(super::productivity::Page::Calendar),
            19 => self.run_action(Action::StartSprint),
            20 => self.open_productivity(super::productivity::Page::Achievements),
            21 => self.open_productivity(super::productivity::Page::Challenges),
            25 => self.open_lexicon(),
            27..=29 => self.open_prose_dashboard(),
            30 => {
                self.open_prose_dashboard();
                self.prose_dashboard.page = super::prose_dashboard::Page::Voice;
            }
            26 => self.spelling(),
            40 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.msg = "Optional exercise: type an article mistake in this demo manuscript, then open Writing checks and read its model disclosure. No permission or text has been inserted.".into();
            }
            31 => self.open_continuity(),
            32 => self.open_review_notes(),
            33 => self.open_history(),
            34 | 35 => self.run_action(Action::Export),
            38 => self.open_system_settings(),
            39 => {
                self.open_help(Some("history"));
                self.observe_deck(tutorial::Event::RecoveryHelpOpened);
            }
            _ => {}
        }
    }
}
fn nav(key: Key, sel: &mut usize, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = match key {
        Key::Up => sel.saturating_sub(1),
        Key::Down => (*sel + 1).min(len - 1),
        Key::Home => 0,
        Key::End => len - 1,
        Key::PageUp => sel.saturating_sub(10),
        Key::PageDown => (*sel + 10).min(len - 1),
        _ => return false,
    };
    *sel = next;
    true
}

/// Cached-only rendering, responsive down to 80x24 (and safely clipped below).
pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Deck(page) = &app.overlay else {
        return;
    };
    if matches!(page, Page::Settings) {
        super::shell::draw(f, app, area, t);
        return;
    }
    f.render_widget(Clear, area);
    let heading = if app.deck.live_session {
        "WrOS · Live session"
    } else {
        "WrOS · offline first"
    };
    let block = crate::ui::pane_block(heading, true, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 4 || inner.width < 10 {
        return;
    }
    let mut rows: Vec<String> = vec![];
    let mut selected = None;
    let title: String;
    let hint: &str;
    match page {
        Page::Settings => unreachable!("shared shell draws Settings"),
        Page::Home { sel } => {
            title = format!(
                "{} · {}",
                app.project.meta.title,
                app.deck.state.current().name
            );
            selected = Some(*sel);
            for w in app.deck.state.dashboard.iter().filter(|w| w.visible) {
                let value = match w.widget {
                    Widget::Resume => app.open_title(),
                    Widget::Manuscript => format!("{} words", app.project.total_words()),
                    Widget::DailyGoal => format!(
                        "{:+} net / {}",
                        app.today_words(),
                        app.project.meta.daily_target
                    ),
                    Widget::NovelGoal => format!(
                        "{} / {}",
                        app.project.total_words(),
                        app.project.meta.target_words
                    ),
                    Widget::WeeklyProgress => format!("{:+} net words recorded", app.deck.weekly),
                    Widget::Sprint => {
                        if app.sprint.is_some() {
                            "active · F2 pause/resume".into()
                        } else {
                            "none · Enter to start".into()
                        }
                    }
                    Widget::Accounting => format!(
                        "{} · {} pending",
                        app.productivity.summary,
                        app.productivity.ledger.pending_count()
                    ),
                    Widget::AdvancedGoals => app.productivity.goal_summary.clone(),
                    Widget::Calendar => app.productivity.calendar_summary.clone(),
                    Widget::AdvancedSprints => app.productivity.sprint.as_ref().map_or(
                        "none · Enter to start".into(),
                        |s| {
                            format!(
                                "{:?} {:?} {:.1} min{}",
                                s.kind,
                                s.status,
                                s.elapsed_ms as f64 / 60000.,
                                if s.effective_paused() {
                                    " · resume explicitly"
                                } else {
                                    ""
                                }
                            )
                        },
                    ),
                    Widget::Achievements => format!(
                        "{} recorded · enabled {} · quiet notices {}",
                        app.productivity.config.awards.len(),
                        app.productivity.config.achievements,
                        !app.productivity.config.notifications
                    ),
                    Widget::DailyChallenges => {
                        if !app.productivity.config.challenges {
                            "disabled; no task generated".into()
                        } else {
                            app.productivity.day.as_ref().map_or(
                                "not assigned · Enter opens today's selection".into(),
                                |d| {
                                    format!(
                                        "{} · {} completed / {} assigned",
                                        d.value.date,
                                        d.value
                                            .assignments
                                            .iter()
                                            .filter(|a| a.status
                                                == grimoire_core::challenges::Status::Completed)
                                            .count(),
                                        d.value.assignments.len()
                                    )
                                },
                            )
                        }
                    }
                    Widget::StorySpark => app.spark.summary.clone(),
                    Widget::RecentScenes => app.deck.recent.clone(),
                    Widget::Scratchpad => app
                        .workflow
                        .state()
                        .map(|s| {
                            format!(
                                "{} original note references · Enter to open",
                                s.notes
                                    .iter()
                                    .filter(|n| n.life == grimoire_core::pins::Life::Active)
                                    .count()
                            )
                        })
                        .unwrap_or_else(|| "metadata unavailable; originals preserved".into()),
                    Widget::Handoff => app
                        .workflow
                        .state()
                        .map(|s| {
                            if s.handoff.intention.is_some() {
                                format!(
                                    "intention left · next-open display {}",
                                    s.handoff.show_on_open
                                )
                            } else {
                                "not armed · Enter to write an original intention".into()
                            }
                        })
                        .unwrap_or_else(|| "metadata unavailable; original preserved".into()),
                    Widget::Journal => {
                        if !app.journal.pending.is_empty() {
                            format!(
                                "{} pending · Enter to review/retry",
                                app.journal.pending.len()
                            )
                        } else if !app.journal.warning.is_empty() {
                            "unavailable; Enter to review".into()
                        } else {
                            format!(
                                "{}: {} saved observations",
                                app.journal.date,
                                app.journal.fact_count()
                            )
                        }
                    }
                    Widget::Pins => {
                        format!("{} in scope · Enter opens Pinboard", app.pin_count(true))
                    }
                    Widget::Battery => app
                        .deck
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.battery_percent)
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "unavailable; refresh in System settings".into()),
                    Widget::Backup => {
                        if app.deck.shell.backup.job.is_some() {
                            "backup operation running".into()
                        } else if app.deck.shell.backup.status.configured {
                            format!(
                                "encrypted repository · {} loaded snapshots",
                                app.deck.shell.backup.status.snapshots.len()
                            )
                        } else {
                            "set up encrypted backup and restore".into()
                        }
                    }
                    Widget::System => app
                        .deck
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.network.clone())
                        .unwrap_or_else(|| "status not loaded".into()),
                };
                rows.push(format!("{:<21} {}", w.widget.name(), value));
            }
            rows.extend(
                [
                    "Resume writing",
                    "Goals",
                    "Progress",
                    "Workspaces",
                    "Settings",
                    "Interactive tutorial",
                    "Configure widgets",
                ]
                .map(str::to_string),
            );
            hint = "↑↓ choose  Enter open  h hide widget  J/K reorder  w widgets  p Pins  j Journal  s Scratchpad  o Handoff  Esc";
        }
        Page::Widgets { sel } => {
            title = "Dashboard widgets".into();
            selected = Some(*sel);
            rows = app
                .deck
                .state
                .dashboard
                .iter()
                .map(|w| {
                    format!(
                        "[{}] {}",
                        if w.visible { 'x' } else { ' ' },
                        w.widget.name()
                    )
                })
                .collect();
            hint = "↑↓ choose  Enter toggle  Esc home";
        }
        Page::Workspaces { sel } => {
            title = "Saved workspaces · two independent editors".into();
            selected = Some(*sel);
            rows = app
                .deck
                .state
                .workspaces
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    format!(
                        "{} {} · outline {} · {} columns{}",
                        if i == app.deck.state.selected {
                            "*"
                        } else {
                            " "
                        },
                        w.name,
                        if w.outline { "on" } else { "off" },
                        w.outline_width,
                        if w.focus { " · focus" } else { "" }
                    )
                })
                .collect();
            hint = "↑↓ choose  Enter restore  +/- outline  < > secondary size  n clone  b starter  p pin set  Esc home";
        }
        Page::WorkspacePins {
            workspace,
            sel,
            ids,
        } => {
            title = format!("Pin set · {}", app.deck.state.workspaces[*workspace].name);
            selected = Some(*sel);
            rows = app
                .pins
                .state()
                .pins
                .iter()
                .map(|p| {
                    format!(
                        "[{}] #{} {} · {:?}",
                        if ids.contains(&p.id) { 'x' } else { ' ' },
                        p.id,
                        p.source.display(),
                        p.life
                    )
                })
                .collect();
            if rows.is_empty() {
                rows.push(
                    "No cards; create references in Pinboard first. A follows all future cards."
                        .into(),
                );
            }
            hint =
                "↑↓ choose  Space toggle  a all  z none  Enter save set  A follow all  Esc cancel";
        }
        Page::System { sel } => {
            title = app
                .deck
                .snapshot
                .as_ref()
                .map(|s| format!("{}{}", if s.simulated { "SIMULATED · " } else { "" }, s.os))
                .unwrap_or_else(|| "System settings · services loaded on demand".into());
            selected = Some(*sel);
            let s = app.deck.snapshot.as_ref();
            rows = vec![
                "Refresh local status".into(),
                "Enable Wi-Fi radio".into(),
                "Disable Wi-Fi radio".into(),
                "Scan Wi-Fi (explicit radio scan)".into(),
                format!(
                    "Keyboard layout · {}",
                    s.and_then(|s| s.keyboard.as_deref()).unwrap_or("unknown")
                ),
                format!(
                    "Brightness -10% · {}",
                    s.and_then(|s| s.brightness_percent)
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "unavailable".into())
                ),
                "Brightness +10%".into(),
                format!(
                    "Low-power UI + request OS power saver · {}",
                    if app.deck.state.low_power {
                        "on"
                    } else {
                        "off"
                    }
                ),
                writingdeck_sys::SUSPEND_UNAVAILABLE.into(),
                "Shutdown (confirmation required)".into(),
                "Restart (confirmation required)".into(),
                "Keyboard shortcuts".into(),
                format!(
                    "Show dashboard on startup · {}",
                    app.deck.state.launch_dashboard
                ),
            ];
            hint = "↑↓ choose  Enter request  Esc home · unavailable services never imply success";
        }
        Page::Keyboard { sel } => {
            title = "Console keyboard · 15-second safe preview".into();
            selected = Some(*sel);
            rows = KeyboardLayout::ALL
                .iter()
                .map(|l| l.id().to_string())
                .collect();
            rows.extend([
                "Appliance tty1 only; desktop/SSH layouts belong to their OS.".into(),
                "Custom layouts require administrator-reviewed registration (planned).".into(),
            ]);
            hint = "↑↓ choose  Enter preview  Esc system · no character remapping in Grimoire";
        }
        Page::KeyboardWait => {
            title = "Applying a temporary keyboard layout…".into();
            rows.push(
                "An independent systemd timer owns rollback even if Grimoire crashes.".into(),
            );
            hint = "Esc cancels when the service replies";
        }
        Page::KeyboardTest { text } => {
            title = app
                .deck
                .preview
                .as_ref()
                .map(|(_, l, end)| {
                    format!(
                        "Test {} · {} seconds left",
                        l.id(),
                        end.saturating_duration_since(Instant::now()).as_secs()
                    )
                })
                .unwrap_or_else(|| "Preview expired".into());
            rows.push("Type here, not in your manuscript:".into());
            rows.push(text.clone());
            rows.push("Enter confirms this layout; Escape restores the committed layout.".into());
            hint = "Enter keep  Esc revert · test text is never saved";
        }
        Page::Power { action, yes } => {
            if *action == PowerAction::Suspend {
                title = "Suspend unavailable".into();
                rows.push(writingdeck_sys::SUSPEND_UNAVAILABLE.into());
                hint = "Esc return · no power operation is available";
            } else {
                title = format!("Confirm {action:?}");
                rows.push(
                    "All manuscript buffers must save successfully before a power request.".into(),
                );
                rows.push(
                    if *yes {
                        "  Cancel     [YES, proceed]"
                    } else {
                        "[Cancel]      Yes, proceed"
                    }
                    .into(),
                );
                hint = "←→ select  Enter confirm  Esc cancel";
            }
        }
        Page::Tutorial { sel } => {
            title = format!(
                "Interactive tutorial · {} / {} completed{}",
                app.deck.progress.completed.len(),
                tutorial::lessons().len(),
                if tutorial::is_demo(&app.project.root) {
                    " · SAFE DEMO"
                } else {
                    " · press d for a safe demo"
                }
            );
            selected = Some(*sel);
            rows = tutorial::lessons()
                .iter()
                .map(|l| {
                    format!(
                        "{:02} [{}] {}{}",
                        l.id,
                        if app.deck.progress.completed.contains(&l.id) {
                            "done"
                        } else if app.deck.progress.skipped.contains(&l.id) {
                            "skip"
                        } else {
                            "    "
                        },
                        l.title,
                        if l.event.is_none() { " (planned)" } else { "" }
                    )
                })
                .collect();
            hint = "↑↓ choose  Enter lesson  d new safe demo  s skip  Esc home";
        }
        Page::Lesson { id } => {
            let l = &tutorial::lessons()[*id as usize - 1];
            title = format!("Lesson {} · {}", id, l.title);
            rows = crate::ui::wrap_words(&l.instructions, inner.width.saturating_sub(4) as usize);
            rows.push(String::new());
            rows.push(
                if l.event.is_some() {
                    "Enter launches the actual feature; progress is event-verified."
                } else {
                    "PLANNED: cannot start or complete this lesson yet."
                }
                .into(),
            );
            hint = "Enter start/replay  h offline help  Esc lessons";
        }
        Page::Shortcuts { sel, capture } => {
            title = if *capture {
                "Press Ctrl+Alt+A-Z or F10-F12 · Esc cancels"
            } else {
                "Custom shortcut aliases · legacy bindings protected"
            }
            .into();
            selected = Some(*sel);
            rows = SHORTCUT_ACTIONS
                .iter()
                .map(|(action, name)| {
                    format!(
                        "{name:<25} {}",
                        app.deck
                            .shortcuts
                            .bindings
                            .iter()
                            .filter(|b| b.action == *action)
                            .map(|b| b.chord.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect();
            hint = "↑↓ action  Enter add/replace alias  Esc system";
        }
    }
    let inner = if matches!(page, Page::Home { .. })
        && app.pin_count(true) > 0
        && app
            .deck
            .state
            .dashboard
            .iter()
            .any(|w| w.widget == Widget::Pins && w.visible)
        && inner.width >= 70
    {
        let [main, cards] = ratatui::layout::Layout::horizontal([
            ratatui::layout::Constraint::Min(36),
            ratatui::layout::Constraint::Length(34),
        ])
        .areas(inner);
        super::pins::dashboard(f, app, cards, t);
        main
    } else {
        inner
    };
    let width = inner.width.saturating_sub(2) as usize;
    let content_height = inner.height.saturating_sub(4) as usize;
    let start = selected.map_or(0, |sel| {
        sel.saturating_sub(content_height.saturating_sub(1))
    });
    let mut lines = vec![Line::from(Span::styled(
        crate::ui::truncate(&title, width),
        Style::default().fg(t.accent),
    ))];
    for (i, row) in rows.iter().enumerate().skip(start).take(content_height) {
        let prefix = if selected == Some(i) { "▸ " } else { "  " };
        let style = Style::default().fg(if selected == Some(i) {
            t.accent
        } else {
            t.text
        });
        lines.push(Line::from(Span::styled(
            crate::ui::truncate(&format!("{prefix}{row}"), width),
            style,
        )));
    }
    while lines.len() < inner.height.saturating_sub(2) as usize {
        lines.push(Line::from(""));
    }
    let note = if app.deck.busy {
        "System request pending; writing input remains responsive."
    } else {
        &app.deck.note
    };
    lines.push(Line::from(Span::styled(
        crate::ui::truncate(note, width),
        Style::default().fg(t.dim),
    )));
    lines.push(Line::from(Span::styled(
        crate::ui::truncate(hint, width),
        Style::default().fg(t.accent),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

impl App {
    fn save_workspace_state(&mut self, next: workspaces::State) -> Result<()> {
        next.validate()?;
        let store = self
            .deck
            .layout_store
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Original layout unavailable; not overwritten"))?;
        let old = std::mem::replace(&mut store.state, next.clone());
        if let Err(e) = store.save() {
            store.state = old;
            return Err(e);
        }
        self.deck.state = next;
        self.deck.dirty = false;
        Ok(())
    }
    pub fn save_workspace_pin_set(&mut self, slot: usize, ids: Option<Vec<u64>>) -> Result<()> {
        self.remember_deck();
        let mut next = self.deck.state.clone();
        next.workspaces
            .get_mut(slot)
            .ok_or_else(|| anyhow::anyhow!("Workspace missing"))?
            .pin_ids = ids;
        self.save_workspace_state(next)
    }
    pub fn starter_preset_picker(&mut self, slot: usize) {
        let entries=workspaces::Preset::ALL.into_iter().map(|preset|palette::Entry {label:format!("Apply {} starter",preset.name()),key:String::new(),detail:"Explicit empty original pane note + layout; existing prose is never templated/replaced".into(),action:Action::ApplyPreset(slot,preset)}).collect();
        self.overlay = Overlay::Palette {
            query: String::new(),
            sel: 0,
            entries,
        };
    }
    pub fn apply_starter_preset(&mut self, slot: usize, preset: workspaces::Preset) {
        let result = (|| -> Result<()> {
            self.workflow_prepare()?;
            let store = self.deck.layout_store.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Original layout unavailable; no starter sources created")
            })?;
            store.verify_current()?;
            if slot >= self.deck.state.workspaces.len() {
                anyhow::bail!("Workspace missing");
            }
            let valid = |app: &Self, i: usize| {
                let n = &app.project.nodes[i];
                n.in_manuscript
                    && n.kind == Kind::Scene
                    && !n.read_only
                    && !n.parked
                    && !n.clash
                    && n.disk.unavailable.is_none()
            };
            let primary = [
                grimoire_core::documents::Pane::Primary,
                grimoire_core::documents::Pane::Secondary,
            ]
            .into_iter()
            .find(|&p| self.view_open(p).is_some_and(|i| valid(self, i)))
            .and_then(|p| self.view_position(p))
            .or_else(|| {
                self.project
                    .nodes
                    .iter()
                    .enumerate()
                    .find(|(i, _)| valid(self, *i))
                    .map(|(_, n)| workspaces::Position {
                        document: n
                            .path
                            .strip_prefix(&self.project.root)
                            .ok()
                            .map(Path::to_path_buf),
                        ..Default::default()
                    })
            })
            .ok_or_else(|| anyhow::anyhow!("Choose a readable manuscript scene first"))?;
            self.remember_deck();
            let retained_secondary = self.view_position(grimoire_core::documents::Pane::Secondary);
            let secondary = if let Some((relative, role)) = preset.source() {
                grimoire_core::workflow::ensure_workspace_note(&self.project.root, preset)?;
                self.reload_tree()?;
                self.workflow_original_node(Path::new(relative), role)?;
                let position = [
                    grimoire_core::documents::Pane::Primary,
                    grimoire_core::documents::Pane::Secondary,
                ]
                .into_iter()
                .filter_map(|p| self.view_position(p))
                .find(|p| p.document.as_deref() == Some(Path::new(relative)))
                .unwrap_or(workspaces::Position {
                    document: Some(relative.into()),
                    ..Default::default()
                });
                Some(position)
            } else {
                retained_secondary
            };
            let mut next = self.deck.state.clone();
            next.selected = slot;
            let w = &mut next.workspaces[slot];
            w.primary = primary;
            w.secondary = secondary;
            w.secondary_visible = preset != workspaces::Preset::Focus && w.secondary.is_some();
            w.focus = preset == workspaces::Preset::Focus;
            w.outline = !w.focus;
            w.outline_width = if preset == workspaces::Preset::Planning {
                36
            } else {
                30
            };
            w.secondary_percent = if preset == workspaces::Preset::Research {
                50
            } else {
                40
            };
            w.active_editor = grimoire_core::documents::Pane::Primary;
            self.save_workspace_state(next)?;
            self.restore_deck_position();
            self.focus_mode = self.deck.state.current().focus && self.open.is_some();
            self.activate_editor(grimoire_core::documents::Pane::Primary);
            self.overlay = Overlay::None;
            self.observe_deck(tutorial::Event::WorkspaceSelected);
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.msg = format!(
                    "{} starter applied; independent original pane source, no existing prose replaced",
                    preset.name()
                )
            }
            Err(e) => {
                self.overlay = Overlay::Deck(Page::Workspaces {
                    sel: slot.min(self.deck.state.workspaces.len() - 1),
                });
                self.deck.note = format!(
                    "Starter not applied: {e:#}; sources/any newly created empty original kept"
                );
            }
        }
    }
}
