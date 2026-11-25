//! WritingDeck layouts, tutorial and narrow system settings.
//! Render/input paths never run a system command. A single bounded worker does.
use super::*;
use grimoire_core::{
    tutorial,
    workspaces::{self, State},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::sync::mpsc::{self, Receiver, SyncSender};
use wros_sys::{
    Backend, PowerAction, Reply, Request, Snapshot,
    keyboard::Layout as KeyboardLayout,
    shortcuts::{self, Action as ShortcutAction, Binding, Shortcuts},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Settings,
    LiveInstall {
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
    fn system() -> Self {
        #[cfg(all(target_os = "linux", feature = "compositor-warmth"))]
        if crate::session::composited() {
            return Self::spawn(crate::compositor_warmth::backend());
        }
        Self::spawn(Box::new(wros_sys::linux::Linux::default()))
    }
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
const LIVE_INSTALL_CHOICES: [&str; 2] = ["Install WrOS to disk", "Try it first"];

const SHORTCUT_ACTIONS: [(ShortcutAction, &str); 22] = [
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
    (ShortcutAction::MenuBar, "Open the menu bar"),
];

/// A restart or shutdown the writer asked for, kept until what runs has finished (#168).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerWait {
    Restart,
    Shutdown,
    /// Restart to install a staged update; checks the system still reports it staged.
    UpdateRestart,
    /// Lock and sleep from the power menu; waits only for a system request or a backup.
    Sleep,
}

impl PowerWait {
    fn action(self) -> PowerAction {
        match self {
            PowerWait::Shutdown => PowerAction::Shutdown,
            PowerWait::Sleep => PowerAction::Suspend,
            PowerWait::Restart | PowerWait::UpdateRestart => PowerAction::Restart,
        }
    }
    fn noun(self) -> &'static str {
        match self {
            PowerWait::Shutdown => "Shutting down",
            PowerWait::Sleep => "Going to sleep",
            PowerWait::Restart => "Restarting",
            PowerWait::UpdateRestart => "Restarting to install the update",
        }
    }
    fn refused(self) -> &'static str {
        match self {
            PowerWait::Restart | PowerWait::Shutdown | PowerWait::Sleep => "Power unchanged:",
            PowerWait::UpdateRestart => "Not restarting:",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PendingPower {
    pub kind: PowerWait,
    /// What is being waited for: "the sync", "the final sync", "the backup", "the system request".
    pub what: &'static str,
    pub since: std::time::Instant,
}

/// Give up on a waiting restart or shutdown after this long (an offline sync that hangs).
pub const POWER_WAIT_LIMIT: Duration = Duration::from_secs(10 * 60);

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

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
    /// What the running system request is doing and when it started (#167).
    pub running: Option<(String, std::time::Instant)>,
    /// The update engine's download progress while an online update downloads.
    pub fetch_progress: Option<wros_sys::FetchProgress>,
    fetch_read: Option<std::time::Instant>,
    /// A restart or shutdown waiting for a sync, backup or system request (#168).
    pub power_wait: Option<PendingPower>,
    pub lock_available: bool,
    pub installer_available: bool,
    /// Running with native pixels or the cage/foot trial.
    pub graphical_session: bool,
    /// Where the writer's session choice is kept, when the choice can be offered:
    /// an installed, non-live WrOS with a graphical renderer available.
    pub session_settings: Option<std::path::PathBuf>,
    pub graphical_chosen: bool,
    pub boot_settings_available: bool,
    pub boot_status: Option<wros_sys::boot::Status>,
    pending_boot_theme: Option<String>,
    pub live_session: bool,
    live_install_prompt_shown: bool,
    /// An installed appliance session checks the WrOS update server after login and daily.
    pub online_updates: bool,
    pub lock_requested: bool,
    pub suspend_available: bool,
    pub suspend_requested: bool,
    /// What closing the lid does, and where the writer's choice is kept.
    pub lid_close: crate::lid::LidClose,
    pub lid_settings: Option<PathBuf>,
    /// Battery, screen light and volume for the bottom bar.
    pub gauges: crate::gauges::Gauges,
    /// When a brightness key last changed the light: it shows for a moment.
    pub light_flash: Option<std::time::Instant>,
    /// "14:07", worked out on the tick so drawing never reads the time zone.
    pub clock: String,
    /// The book's backup, for the bar's line, also worked out on the tick.
    pub backup_mark: Option<super::BackupMark>,
    /// Battery warnings and the countdown at the reserve.
    pub battery: crate::battery::Watch,
    /// Quit is offered only outside WrOS: in a WrOS session, leaving the app just ends
    /// the login and goes back to the login screen.
    pub quit_offered: bool,
    pub power_pending: bool,
    pub installer_requested: bool,
    /// An app-only update is installed: save, then start the new writing app in place.
    pub app_restart_requested: bool,
    worker: Option<Worker>,
    enable_system: bool,
    pub preview: Option<(String, KeyboardLayout, Instant)>,
    system_parent: Option<usize>,
    pub dirty: bool,
}
impl Deck {
    pub fn load(root: &Path, background: bool) -> Self {
        let mut warnings = Vec::new();
        let live_session = background
            && (std::env::var_os("WROS_LIVE_SESSION").is_some()
                || Path::new(wros_sys::paths::LIVE_MEDIUM_SQUASHFS).is_file());
        if live_session {
            warnings
                .push("Live session · save work somewhere persistent before restarting.".into());
        }
        if background {
            warnings.extend(crate::session::notice());
        }
        let session_settings = (background
            && !live_session
            && crate::installed_session()
            && crate::session::graphical_installed())
        .then(crate::session::settings_path)
        .flatten();
        let graphical_chosen = session_settings
            .as_deref()
            .is_some_and(crate::session::graphical_chosen);
        let layout_store = workspaces::Store::load(root)
            .map_err(|e| warnings.push(e.to_string()))
            .ok();
        // Keep the legacy dashboard layout and startup setting for older WrOS versions.
        let state = layout_store
            .as_ref()
            .map(|s| s.state.clone())
            .unwrap_or_default();
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
        let lid_settings = background.then(crate::lid::settings_path);
        let lid_close = lid_settings
            .as_deref()
            .map(crate::lid::load)
            .unwrap_or_default();
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
            online_updates: background && !live_session && crate::installed_session(),
            live_session,
            live_install_prompt_shown: false,
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
            graphical_session: background && crate::session::graphical(),
            session_settings,
            graphical_chosen,
            boot_settings_available: background
                && !live_session
                && crate::installed_session()
                && Path::new(wros_sys::paths::WROS_BOOT_SETTINGS).is_file(),
            boot_status: None,
            pending_boot_theme: None,
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
            suspend_available: {
                #[cfg(target_os = "linux")]
                {
                    background && crate::console_lock::suspend_available()
                }
                #[cfg(not(target_os = "linux"))]
                {
                    false
                }
            },
            suspend_requested: false,
            lid_close,
            lid_settings,
            gauges: crate::gauges::Gauges::default(),
            light_flash: None,
            clock: String::new(),
            backup_mark: None,
            battery: crate::battery::Watch::default(),
            quit_offered: !crate::installed_session(),
            power_pending: false,
            running: None,
            fetch_progress: None,
            fetch_read: None,
            power_wait: None,
            installer_requested: false,
            app_restart_requested: false,
            // Colour ownership starts at login, including when no Settings page
            // is opened. Both system requests and Wayland work stay off-thread.
            worker: (background && crate::session::composited()).then(Worker::system),
            enable_system: background,
            preview: None,
            system_parent: None,
            dirty: false,
        }
    }
    fn take_live_install_prompt(&mut self) -> bool {
        if !self.live_session || self.live_install_prompt_shown {
            return false;
        }
        self.live_install_prompt_shown = true;
        // Remember the prompt across app restarts while the runtime directory exists.
        // Without a writable runtime directory, remember it in this app session.
        let already_shown = self.enable_system
            && std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|dir| !dir.is_empty())
                .is_some_and(|dir| {
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(PathBuf::from(dir).join("wros-live-install-prompt"))
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists)
                });
        !already_shown
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
    /// While a system request runs: what it is doing, and for how long once it takes a
    /// while (#167). Shown in place of the last notice until the reply arrives.
    pub fn busy_line(&self) -> String {
        let Some((what, since)) = &self.running else {
            return "Waiting for the system…".into();
        };
        let what = match &self.fetch_progress {
            Some(progress) => progress.line(),
            None => what.clone(),
        };
        match wros_sys::elapsed_label(since.elapsed()) {
            Some(time) => format!("{what} · {time}"),
            None => what,
        }
    }
    /// While an online update downloads, read the engine's progress about once a second.
    pub(crate) fn tick_fetch_progress(&mut self) {
        let fetching = self.busy
            && self
                .running
                .as_ref()
                .is_some_and(|(what, _)| *what == Request::FetchOnlineUpdate.activity());
        if !fetching
            || self
                .fetch_read
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.fetch_read = Some(std::time::Instant::now());
        self.fetch_progress = wros_sys::FetchProgress::read();
    }
    /// "Restarting when the sync finishes · 0:12 · Esc cancels" while a power action waits.
    pub fn power_wait_line(&self) -> Option<String> {
        let pending = self.power_wait?;
        let time = wros_sys::elapsed_label(pending.since.elapsed())
            .map(|time| format!(" · {time}"))
            .unwrap_or_default();
        Some(format!(
            "{} when {} finishes{time} · Esc cancels",
            pending.kind.noun(),
            pending.what
        ))
    }
    pub fn request(&mut self, request: Request) {
        if self.busy {
            self.note = format!("Still busy, try again in a moment: {}", self.busy_line());
            return;
        }
        if self.worker.is_none() {
            if !self.enable_system {
                self.note = "System operations disabled in this test session.".into();
                return;
            }
            self.worker = Some(Worker::system());
        }
        let powering_off = matches!(
            request,
            Request::Power(PowerAction::Shutdown | PowerAction::Restart)
        );
        let activity = request.activity();
        match self.worker.as_ref().unwrap().tx.try_send(request) {
            Ok(()) => {
                self.power_pending |= powering_off;
                self.busy = true;
                self.running = Some((activity, std::time::Instant::now()));
                self.note = self.busy_line();
            }
            Err(e) => self.note = format!("System worker unavailable: {e}"),
        }
    }
    #[cfg(test)]
    pub fn simulate(&mut self) {
        self.worker = Some(Worker::spawn(Box::new(wros_sys::Simulated::laptop())));
    }
    #[cfg(test)]
    pub fn use_test_backend(&mut self, backend: Box<dyn Backend>) {
        self.worker = Some(Worker::spawn(backend));
    }

    pub(super) fn carry_import_worker(&mut self, previous: &mut Self) {
        std::mem::swap(&mut self.worker, &mut previous.worker);
        std::mem::swap(&mut self.busy, &mut previous.busy);
    }
}

impl App {
    pub(crate) fn sync_boot_theme_if_changed(&mut self, path: &Path) {
        if self.deck.boot_settings_available
            && fs::read_to_string(path).ok().as_deref() != Some(&theme::to_config(&self.theme))
        {
            self.sync_boot_theme();
        }
    }

    pub(crate) fn sync_boot_theme(&mut self) {
        if self.deck.boot_settings_available {
            self.deck.pending_boot_theme = Some(theme::to_config(&self.theme));
        }
    }

    pub(crate) fn request_suspend(&mut self) -> bool {
        if !self.deck.suspend_available || !self.deck.lock_available || self.deck.live_session {
            self.deck.note = wros_sys::SUSPEND_UNAVAILABLE.into();
            self.deck.shell.notices.push(&self.deck.note);
            return true;
        }
        // A sync never holds sleep up: the lock never waits for the network (#168).
        if let Some(what) = self.sleep_blocker() {
            self.deck.note = format!(
                "{} is running. Wait for it to finish, then try again.",
                capitalise(what)
            );
            self.deck.shell.notices.push(&self.deck.note);
            return false;
        }
        self.request_console_lock();
        if self.deck.lock_requested {
            // Locking never waits for the network: start the final sync and lock now. If
            // sleep interrupts it, it finishes after wake.
            self.start_final_sync();
            self.deck.suspend_requested = true;
        }
        true
    }

    /// What a closed lid or a nearly empty battery asks for right now: nothing,
    /// a lock (`Some(false)`), or a lock and sleep (`Some(true)`). The battery
    /// warns first and counts down before it asks for sleep, whatever the lid choice.
    pub(crate) fn power_event(
        &mut self,
        lid_closed: bool,
        battery: crate::battery::Reading,
        now: std::time::Instant,
    ) -> Option<bool> {
        use crate::battery::Step;
        let shown = self.deck.battery.showing(now).map(str::to_string);
        let step = self
            .deck
            .battery
            .read(battery, self.deck.suspend_available, now);
        // The notices keep each warning, and a countdown once, not every second of it.
        if let Some(said) = self.deck.battery.showing(now)
            && shown.as_deref() != Some(said)
            && (step == Step::Save || !said.contains(" seconds"))
        {
            self.deck.shell.notices.push(said);
            if battery.discharging {
                self.observe_deck(tutorial::Event::BatteryWarningShown);
            }
        }
        match step {
            Step::Sleep => return Some(true),
            Step::Save => {
                self.commit_saves();
            }
            Step::Nothing => {}
        }
        if !lid_closed || !self.deck.lock_available {
            return None;
        }
        match self.deck.lid_close {
            crate::lid::LidClose::Nothing => None,
            crate::lid::LidClose::Lock => Some(false),
            crate::lid::LidClose::LockAndSleep => Some(self.deck.suspend_available),
        }
    }

    /// Acts on `power_event`. False means the event must be retried after
    /// the current system or backup operation finishes.
    pub(crate) fn request_power_lock(&mut self, sleep: bool) -> bool {
        if sleep {
            return self.request_suspend();
        }
        if self.deck.busy {
            return false;
        }
        self.request_console_lock();
        true
    }

    pub fn cycle_lid_close(&mut self) {
        let Some(settings) = self.deck.lid_settings.clone() else {
            return;
        };
        let next = self.deck.lid_close.next();
        let saved = crate::lid::save(&settings, next);
        self.deck.note = match &saved {
            Ok(()) => {
                self.deck.lid_close = next;
                match next {
                    crate::lid::LidClose::LockAndSleep => {
                        "Closing the lid now saves, locks and sleeps."
                    }
                    crate::lid::LidClose::Lock => {
                        "Closing the lid now saves and locks. The laptop stays awake."
                    }
                    crate::lid::LidClose::Nothing => {
                        "Closing the lid now does nothing. Your project stays open."
                    }
                }
                .to_string()
            }
            Err(error) => format!("Couldn't save the lid choice: {error}"),
        };
        self.deck.shell.notices.push(&self.deck.note);
        if saved.is_ok() {
            self.observe_deck(tutorial::Event::LidChoiceSaved);
        }
    }

    /// What sleep waits for: a system request or a backup, never a sync.
    fn sleep_blocker(&mut self) -> Option<&'static str> {
        self.deck.shell.backup.tick();
        if self.deck.busy {
            Some("the system request")
        } else if self.deck.shell.backup.job.is_some() {
            Some("the backup")
        } else {
            None
        }
    }

    /// What has to finish before WrOS can restart or shut down, if anything.
    fn power_blocker(&mut self) -> Option<&'static str> {
        self.deck.shell.backup.tick();
        if self.deck.busy {
            Some("the system request")
        } else if self.deck.shell.backup.job.is_some() {
            Some("the backup")
        } else if self.deck.shell.backup.sync.job.is_some() {
            Some("the sync")
        } else if self.deck.shell.backup.shelved_sync_running().is_some() {
            Some("the previous project's sync")
        } else {
            None
        }
    }

    /// Restart or shut down (#168): save, close this writing session, sync once, then go.
    /// Anything still running is waited for, not refused: the action is kept and tried
    /// again, every check included, once it finishes. Esc cancels the wait.
    /// False when refused (nothing will happen); true when sent or waiting.
    pub(crate) fn power_off(&mut self, kind: PowerWait) -> bool {
        self.deck.power_wait = None;
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.power_notice(format!(
                "{} unsaved buffers or source conflicts need attention. Return to writing to review them.",
                kind.refused()
            ));
            return false;
        }
        if kind == PowerWait::UpdateRestart && !self.update_staged_confirmed() {
            self.power_notice(
                "Not restarting: the system has not confirmed a staged update. Refresh Recovery status first.",
            );
            return false;
        }
        if let Some(what) = self.power_blocker() {
            self.power_hold(kind, what);
            return true;
        }
        self.save_deck();
        self.end_timeline_session(chrono::Local::now());
        if !self.final_sync_done() {
            // The final sync started; it is what we wait for now.
            self.power_hold(kind, "the final sync");
            return true;
        }
        self.deck.shell.update_flow.last_result = None;
        self.deck.request(Request::Power(kind.action()));
        true
    }

    fn power_hold(&mut self, kind: PowerWait, what: &'static str) {
        let since = match self.deck.power_wait {
            Some(PendingPower { since, .. }) => since,
            None => std::time::Instant::now(),
        };
        self.deck.power_wait = Some(PendingPower { kind, what, since });
        let line = self.deck.power_wait_line().unwrap_or_default();
        self.power_notice(line);
    }

    /// Said in Settings and in the writing status line.
    fn power_notice(&mut self, text: impl Into<String>) {
        self.deck.note = text.into();
        self.deck.shell.notices.push(&self.deck.note);
        self.msg = self.deck.note.clone();
    }

    /// Esc while a restart or shutdown waits: keep WrOS running.
    pub(crate) fn cancel_power_wait(&mut self) -> bool {
        let Some(pending) = self.deck.power_wait.take() else {
            return false;
        };
        self.power_notice(format!(
            "{} cancelled. {} carries on.",
            pending.kind.noun(),
            capitalise(pending.what)
        ));
        true
    }

    /// Go ahead with a waiting restart or shutdown once nothing runs, or give up after a while.
    pub(crate) fn tick_power_wait(&mut self) {
        let Some(pending) = self.deck.power_wait else {
            return;
        };
        if pending.since.elapsed() >= POWER_WAIT_LIMIT {
            self.deck.power_wait = None;
            self.power_notice(format!(
                "{} cancelled: {} did not finish in {} minutes. Try again, or check Backups and accounts.",
                pending.kind.noun(),
                pending.what,
                POWER_WAIT_LIMIT.as_secs() / 60
            ));
            return;
        }
        if pending.kind == PowerWait::Sleep {
            if self.sleep_blocker().is_none() {
                self.deck.power_wait = None;
                self.request_suspend();
            }
            return;
        }
        if self.project.dirty_count() != 0 {
            // The writer went back to writing: their words come first.
            self.deck.power_wait = None;
            self.power_notice(format!(
                "{} cancelled: you went back to writing.",
                pending.kind.noun()
            ));
            return;
        }
        // A final sync that brought files from Drive leaves the book to be re-read first
        // (sync_tick does it); going ahead before then would refuse as 'unsaved'.
        if self.power_blocker().is_none() && !self.tree_stale {
            self.power_off(pending.kind);
            // Still waiting (a final sync started): keep the original start time.
            if let Some(waiting) = &mut self.deck.power_wait {
                waiting.since = pending.since;
            }
        }
    }

    pub fn toggle_graphical_session(&mut self) {
        let Some(settings) = self.deck.session_settings.clone() else {
            return;
        };
        let on = !self.deck.graphical_chosen;
        let message = match crate::session::choose(&settings, on) {
            Ok(()) => {
                self.deck.graphical_chosen = on;
                if on {
                    "Graphical session on. It starts at your next login."
                } else {
                    "Graphical session off. The console session starts at your next login."
                }
                .to_string()
            }
            Err(error) => format!("Couldn't save the session choice: {error}"),
        };
        self.deck.note = message;
        self.deck.shell.notices.push(&self.deck.note);
    }

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
        self.end_timeline_session(chrono::Local::now());
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
    pub fn init_wros(&mut self, launch: bool) {
        if launch {
            self.restore_workspace_views(resume::read(&self.project.root).is_none());
            self.focus_mode = self.deck.state.current().focus && self.open.is_some();
            if self.focus_mode {
                self.activate_editor(grimoire_core::documents::Pane::Primary);
            } else if !self.deck.state.current().outline {
                self.focus = self.editor_focus();
            }
            if self.overlay == Overlay::None && self.deck.take_live_install_prompt() {
                self.overlay = Overlay::Deck(Page::LiveInstall { sel: 1 });
            }
        }
    }
    /// The console lock handed the session back after the writer's password.
    pub fn on_unlocked(&mut self) {
        self.deck.note = self
            .deck
            .battery
            .unlocked()
            .unwrap_or("Writing session unlocked.")
            .into();
        if self.deck.note != "Writing session unlocked." {
            self.msg = self.deck.note.clone();
        }
        self.deck.shell.notices.push(&self.deck.note);
        self.observe_deck(tutorial::Event::SessionUnlocked);
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
    pub fn open_system_settings(&mut self) {
        self.deck.system_parent = None;
        self.overlay = Overlay::Deck(Page::System { sel: 0 });
        self.observe_deck(tutorial::Event::SystemOpened);
        if self.deck.snapshot.is_none() {
            self.deck.request(Request::Refresh);
        }
    }
    fn close_system_subpage(&mut self) {
        self.overlay = self
            .deck
            .system_parent
            .take()
            .map_or(Overlay::None, |sel| Overlay::Deck(Page::System { sel }));
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
            "outline hidden"
        }
        .to_string();
        if !self.deck.state.current().outline {
            self.msg = match self.deck.shortcuts.chord_for(ShortcutAction::Outline) {
                Some(key) => format!("outline hidden · {key} restores it"),
                None => "outline hidden · View › Outline restores it".into(),
            };
        }
        self.deck.dirty = true;
        self.save_deck();
        self.observe_deck(tutorial::Event::OutlineToggled);
    }
    /// New readings from the gauges' thread; a change of light (its key) shows
    /// the light for a moment.
    pub fn set_gauges(&mut self, gauges: crate::gauges::Gauges, now: std::time::Instant) {
        if self.deck.gauges.light.is_some() && gauges.light != self.deck.gauges.light {
            self.deck.light_flash = Some(now);
        }
        self.deck.gauges = gauges;
    }
    pub fn deck_shortcut(&mut self, action: ShortcutAction) {
        match action {
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
            ShortcutAction::MenuBar => self.open_menu(),
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
            if action == ShortcutAction::MenuBar {
                // One key for the bar: the new one replaces F10 (Esc still opens it).
                shortcuts
                    .bindings
                    .retain(|b| b.action != ShortcutAction::MenuBar);
            }
            shortcuts.bindings.push(Binding { chord, action });
            shortcuts.validate()?;
            let text = serde_json::to_string_pretty(&shortcuts.to_saved())? + "\n";
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
        self.tick_source_catalogue();
        self.deck.shell.backup.tick();
        self.tick_music_setup();
        let now = chrono::Local::now();
        self.deck.clock = now.format("%H:%M").to_string();
        self.tick_timeline(Instant::now(), now);
        self.deck.backup_mark = self.read_backup_mark(now);
        match self.deck.shell.backup.finished.take() {
            Some(super::backup::Finished::Restore) => {
                self.observe_deck(tutorial::Event::BackupRestored)
            }
            Some(super::backup::Finished::LinkPhone) => {
                self.observe_deck(tutorial::Event::PhoneLinked)
            }
            Some(super::backup::Finished::DriveList)
                if self.overlay == Overlay::Deck(Page::Settings)
                    && self.deck.shell.nav.current() == super::shell::Route::GoogleDrive =>
            {
                self.observe_deck(tutorial::Event::DriveFolderListed)
            }
            Some(super::backup::Finished::DriveList) => {}
            None => {}
        }
        self.tick_update_chain();
        self.tick_power_wait();
        self.deck.tick_fetch_progress();
        self.tick_productivity();
        self.tick_online_updates();
        self.tick_update_health();
        if !self.deck.power_pending {
            self.tick_read();
            self.tick_sync();
            self.tick_commonplace_backup();
            self.tick_auto_backup();
        }
        let result = self.deck.worker.as_ref().and_then(|w| w.rx.try_recv().ok());
        if let Some((request, result)) = result {
            self.deck.busy = false;
            self.deck.running = None;
            self.deck.fetch_progress = None;
            self.deck.fetch_read = None;
            if matches!(request, Request::Power(_)) && (result.is_err() || self.deck.simulated()) {
                self.deck.power_pending = false;
            }
            match result {
                Ok(Reply::Snapshot(s)) => {
                    self.deck.note = if s.simulated {
                        "SIMULATED system status; not hardware verified"
                    } else {
                        "System status refreshed locally."
                    }
                    .into();
                    // Settings just changed or read these: the bar shows them now.
                    if s.battery_percent.is_some() {
                        self.deck.gauges.battery = s.battery_percent;
                        self.deck.gauges.charging = s.charging.unwrap_or(false);
                    }
                    if s.brightness_percent.is_some() {
                        self.deck.gauges.light = s.brightness_percent;
                    }
                    self.deck.snapshot = Some(s);
                    if self.deck.boot_settings_available
                        && self.deck.shell.nav.current()
                            == super::shell::Route::Panel(super::shell::Panel::Display)
                    {
                        self.deck.request(Request::BootStatus);
                    }
                }
                Ok(Reply::BootStatus(status)) => {
                    self.deck.note = status.preparation().into();
                    let saved = matches!(request, Request::BootScreen { enabled, .. } if enabled == status.enabled)
                        && !status.failed;
                    self.deck.boot_status = Some(status);
                    if saved {
                        self.observe_deck(tutorial::Event::StartScreenSaved);
                    }
                }
                Ok(Reply::Message(message)) => {
                    self.on_update_message(&request, &message);
                    if request != Request::ReportUpdateReady {
                        self.deck.note = message;
                    }
                    if matches!(request, Request::KeyboardConfirm(_)) {
                        self.observe_deck(tutorial::Event::LayoutConfirmed);
                    }
                    if matches!(request, Request::DisplayConfirm(_)) {
                        self.observe_deck(tutorial::Event::WarmthKept);
                    }
                    if matches!(
                        request,
                        Request::DisplayConfirm(_) | Request::DisplayCancel(_)
                    ) {
                        self.deck.request(Request::DisplayStatus);
                    }
                    if matches!(request, Request::ChangePassword { .. }) {
                        self.deck.shell.password_form = None;
                        if matches!(self.overlay, Overlay::Deck(Page::Settings))
                            && self.deck.shell.nav.current() == super::shell::Route::ChangePassword
                        {
                            self.deck.shell.nav.back();
                        }
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
                }
                Ok(Reply::UpdateReview(review)) => {
                    self.on_update_review(&request, review);
                }
                Ok(Reply::UpdateStaged(staged)) => {
                    self.on_update_staged(staged);
                }
                Ok(Reply::UpdateApplied(applied)) => {
                    self.on_update_applied(applied);
                }
                Ok(Reply::ImportMedia(sticks)) => {
                    self.on_import_media(&request, sticks);
                }
                Ok(Reply::UpdateMedia(media)) => {
                    self.on_update_media(media);
                }
                Ok(Reply::OnlineUpdate(online)) => {
                    self.on_online_update(online);
                }
                Ok(Reply::OnlineUpdateFetched(path)) => {
                    self.on_online_update_fetched(path);
                }
                Ok(Reply::WifiScan(networks)) => {
                    self.deck.note = format!(
                        "{} named Wi-Fi networks returned. Select a network to connect.",
                        networks.len()
                    );
                    self.deck.shell.wifi_networks = Some(networks);
                    self.observe_deck(tutorial::Event::WifiScanned);
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
                    // A recent helper result stays on screen; the status read
                    // back after it is shown in the Recovery and Updates views.
                    self.deck.note = self
                        .deck
                        .shell
                        .update_flow
                        .last_result
                        .clone()
                        .unwrap_or_else(|| info.status.clone());
                    self.deck.shell.recovery = Some(info);
                    self.on_recovery_for_chain();
                    self.on_recovery_for_health();
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
                Ok(Reply::LocalClock(time)) => {
                    self.deck.note = format!(
                        "Clock set to {time}; network time is off until you switch it back on{}.",
                        if self.deck.snapshot.as_ref().is_some_and(|s| s.simulated) {
                            " (simulated)"
                        } else {
                            ""
                        }
                    );
                    self.deck
                        .snapshot
                        .get_or_insert_with(Snapshot::default)
                        .network_time = Some(false);
                    if matches!(self.overlay, Overlay::Deck(Page::Settings))
                        && self.deck.shell.nav.current() == super::shell::Route::ManualClockConfirm
                    {
                        self.deck.shell.clock_choice = None;
                        self.deck.shell.clock_form = None;
                        self.deck.shell.nav.back();
                        self.deck.shell.nav.back();
                    }
                }
                Ok(Reply::NetworkTime) => {
                    self.deck.note =
                        "Network time is on; the clock sets itself whenever WrOS is online.".into();
                    let snapshot = self.deck.snapshot.get_or_insert_with(Snapshot::default);
                    snapshot.network_time = Some(true);
                    snapshot.time_synchronized = None;
                }
                Ok(Reply::Audio(audio)) => {
                    if audio.available {
                        self.deck.gauges.volume = audio.volume_percent;
                        self.deck.gauges.muted = audio.muted;
                    }
                    self.deck.note = audio.message.clone();
                    self.deck.shell.audio_status = Some(audio);
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
                    self.on_import_media_error(&request, &e);
                    self.deck.shell.warm_wait = false;
                    if matches!(request, Request::DisplayStatus) {
                        self.deck.shell.display = None;
                    }
                    if matches!(request, Request::ConnectWifi { .. })
                        && let Some(form) = &mut self.deck.shell.wifi_form
                    {
                        form.password.clear();
                    }
                    if matches!(
                        request,
                        Request::BootStatus | Request::BootTheme(_) | Request::BootScreen { .. }
                    ) {
                        self.deck.note =
                            "Couldn't change the start screen. Your project is safe. Try again."
                                .into();
                    } else if matches!(request, Request::ScanWifi) {
                        self.deck.note = super::shell::WIFI_SCAN_FAILED.into();
                    } else if !self.on_update_error(&request, &e) {
                        self.deck.note = format!("Unavailable: {e}");
                    }
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
        if !self.deck.busy
            && let Some(theme) = self.deck.pending_boot_theme.take()
        {
            self.deck.request(Request::BootTheme(theme));
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
        self.run_pending_settings_row();
    }
    pub fn on_deck_key(&mut self, key: Key) {
        let Overlay::Deck(page) = self.overlay.clone() else {
            return;
        };
        if key == Key::Esc && self.cancel_power_wait() {
            return;
        }
        match page {
            Page::Settings => self.on_settings_key(key),
            Page::LiveInstall { mut sel } => {
                if nav(key, &mut sel, LIVE_INSTALL_CHOICES.len()) {
                    self.overlay = Overlay::Deck(Page::LiveInstall { sel });
                } else if matches!(key, Key::Enter | Key::Esc) {
                    self.overlay = Overlay::None;
                    if key == Key::Enter && sel == 0 {
                        self.request_live_installer();
                    }
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
                    Key::Esc => self.overlay = Overlay::None,
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
                if nav(key, &mut sel, 12) {
                    self.overlay = Overlay::Deck(Page::System { sel });
                    return;
                }
                if key == Key::Esc {
                    self.overlay = Overlay::None;
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
                    4 => {
                        self.deck.system_parent = Some(sel);
                        self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                    }
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
                        if self.deck.suspend_available {
                            self.overlay = Overlay::Deck(Page::Power {
                                action: PowerAction::Suspend,
                                yes: false,
                            });
                        } else {
                            self.deck.note = wros_sys::SUSPEND_UNAVAILABLE.into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
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
                        self.deck.system_parent = Some(sel);
                        self.overlay = Overlay::Deck(Page::Shortcuts {
                            sel: 0,
                            capture: false,
                        })
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
                    Key::Esc => self.close_system_subpage(),
                    _ => {}
                }
            }
            Page::KeyboardWait => {
                if key == Key::Esc {
                    self.overlay = if self.deck.shell.external {
                        Overlay::None
                    } else {
                        Overlay::Deck(Page::Keyboard { sel: 0 })
                    };
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
                    self.overlay = if self.deck.shell.external {
                        Overlay::None
                    } else {
                        Overlay::Deck(Page::Keyboard { sel: 0 })
                    };
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
                        if let Some(what) = self.sleep_blocker() {
                            // Wait for it, said on screen; Esc cancels (#168).
                            self.power_hold(PowerWait::Sleep, what);
                            self.open_system_settings();
                            return;
                        }
                        if !self.request_suspend() {
                            return;
                        }
                    } else if yes {
                        let kind = if action == PowerAction::Restart {
                            PowerWait::Restart
                        } else {
                            PowerWait::Shutdown
                        };
                        if !self.power_off(kind) {
                            // Refused: the confirmation stays for an explicit retry.
                            return;
                        }
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
                    Key::Esc => self.overlay = Overlay::None,
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
                    Key::Esc => self.close_system_subpage(),
                    _ => {}
                }
            }
        }
    }
    /// `d` on the tutorial page: open the safe demo book, making it only the first time.
    /// Lessons already done or skipped carry into it, so the count does not start over.
    fn start_tutorial_demo(&mut self) {
        if tutorial::is_demo(&self.project.root) {
            self.deck.note = format!(
                "You're in the demo project {} ({}). Choose a lesson and press Enter.",
                crate::books::title_of_folder(&self.project.root),
                crate::books::pretty(&self.project.root)
            );
            return;
        }
        let parent = if self.background {
            grimoire_core::paths::data_dir().join("writingdeck")
        } else {
            self.project.root.parent().unwrap().to_path_buf()
        };
        if let Err(e) = fs::create_dir_all(&parent) {
            self.deck.note = e.to_string();
            return;
        }
        let carried = self.deck.progress.clone();
        let (root, made) = match existing_demo(&parent) {
            Some(root) => (root, false),
            None => {
                let mut n = 1;
                let mut root = parent.join("Tutorial-1");
                while root.exists() {
                    n += 1;
                    root = parent.join(format!("Tutorial-{n}"));
                }
                if let Err(e) = tutorial::create_demo(&root) {
                    self.deck.note = format!("Demo project not created: {e:#}");
                    return;
                }
                (root, true)
            }
        };
        if !self.open_book(&root) {
            return;
        }
        let progress = &mut self.deck.progress;
        let before = (progress.completed.len(), progress.skipped.len());
        progress.completed.extend(carried.completed_lessons());
        for id in carried.skipped {
            if !progress.completed.contains(&id) {
                progress.skipped.insert(id);
            }
        }
        if before != (progress.completed.len(), progress.skipped.len()) {
            self.deck.save_progress();
        }
        self.overlay = Overlay::Deck(Page::Tutorial { sel: 0 });
        self.deck.note = format!(
            "{} the demo project {} ({}). Your project was saved and left untouched.",
            if made { "Made and opened" } else { "Opened" },
            crate::books::title_of_folder(&root),
            crate::books::pretty(&root)
        );
    }
    fn start_deck_lesson(&mut self, id: u8) {
        use super::shell::{Panel, Route};
        if tutorial::lessons()
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.event)
            .is_some_and(|e| !e.safe_outside_demo())
            && !tutorial::is_demo(&self.project.root)
        {
            self.deck.note = "This exercise needs the safe demo project: press d in Interactive \
                              tutorial first. Your project is unchanged."
                .into();
            return;
        }
        if let Err(e) = self.deck.progress.start(id) {
            self.deck.note = e.to_string();
            return;
        }
        self.deck.save_progress();
        match id {
            1 => self.open_menu(),
            2 => self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 }),
            3 => {
                self.overlay = Overlay::NewBook {
                    buf: String::new(),
                    shape: project::Template::Novel,
                };
            }
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
            22..=24 => self.open_spark(),
            25 => self.open_lexicon(),
            27..=29 => self.open_prose_dashboard(),
            30 => {
                self.open_prose_dashboard();
                self.prose_dashboard.page = super::prose_dashboard::Page::Voice;
            }
            26 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.spelling();
            }
            40 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.msg = "Optional exercise: type an article mistake in this demo manuscript, then open Writing checks and read its model disclosure. No permission or text has been inserted.".into();
            }
            31 => self.open_continuity(),
            32 => self.open_review_notes(),
            33 => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
                self.open_history();
            }
            34 | 35 => self.run_action(Action::Export),
            // Pages by name; the Settings menu map knows where each one sits.
            36 | 37 => self.open_settings_page(Route::Backups),
            41 => self.open_settings_page(Route::GoogleAccount),
            42 => {
                let mut path = super::shell::path_to(Route::Panel(Panel::Display));
                path.push(Route::WarmScreen);
                self.open_settings_at(&path);
            }
            43 => self.open_settings_page(Route::Panel(Panel::Updates)),
            44 => self.open_settings_page(Route::Panel(Panel::Network)),
            45..=47 => self.open_settings_page(Route::Panel(Panel::Power)),
            48 => self.open_settings_page(Route::GoogleAccount),
            49 => self.open_merge_practice(),
            50 => self.open_settings_page(Route::Panel(Panel::Display)),
            38 => self.open_system_settings(),
            39 => {
                self.open_help(Some("history"));
                self.observe_deck(tutorial::Event::RecoveryHelpOpened);
            }
            _ => {}
        }
    }
}
/// The newest safe demo book in `parent` (Tutorial-1, Tutorial-2, …), if one is there.
fn existing_demo(parent: &std::path::Path) -> Option<std::path::PathBuf> {
    fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let n: u32 = path
                .file_name()?
                .to_str()?
                .strip_prefix("Tutorial-")?
                .parse()
                .ok()?;
            let real = !path.symlink_metadata().ok()?.file_type().is_symlink();
            (real && tutorial::is_demo(&path)).then_some((n, path))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, path)| path)
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
    let area = if matches!(page, Page::LiveInstall { .. }) {
        crate::ui::centred(area, 52, 8)
    } else {
        area
    };
    f.render_widget(Clear, area);
    let block = crate::ui::pane_block("WrOS", true, t);
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
        Page::LiveInstall { sel } => {
            title = "Live session".into();
            selected = Some(*sel);
            rows = LIVE_INSTALL_CHOICES.into_iter().map(String::from).collect();
            hint = "↑↓ choose  Enter select  Esc close";
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
            hint = "↑↓ choose  Enter restore  +/- outline  < > secondary size  \
                    n clone  b starter  p pin set  Esc close";
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
                if app.deck.suspend_available {
                    "Lock and suspend".into()
                } else {
                    wros_sys::SUSPEND_UNAVAILABLE.into()
                },
                "Shutdown (confirmation required)".into(),
                "Restart (confirmation required)".into(),
                "Keyboard shortcuts".into(),
            ];
            hint = "↑↓ choose  Enter request  Esc close · unavailable services never imply success";
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
            hint = "↑↓ choose  Enter preview  Esc back · no character remapping in WrOS";
        }
        Page::KeyboardWait => {
            title = "Applying a temporary keyboard layout…".into();
            rows.push("An independent systemd timer owns rollback even if WrOS crashes.".into());
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
            if *action == PowerAction::Suspend && !app.deck.suspend_available {
                title = "Suspend unavailable".into();
                rows.push(wros_sys::SUSPEND_UNAVAILABLE.into());
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
                app.deck.progress.completed_lessons().count(),
                tutorial::lessons().len(),
                if tutorial::is_demo(&app.project.root) {
                    format!(
                        " · demo project {}",
                        crate::books::title_of_folder(&app.project.root)
                    )
                } else {
                    " · d opens the safe demo project".into()
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
            hint = "↑↓ choose  Enter lesson  d demo project  s skip  Esc close";
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
            hint = "↑↓ action  Enter add/replace alias  Esc back";
        }
    }
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
    let busy_line;
    let note = if matches!(page, Page::LiveInstall { .. }) {
        "Save work somewhere persistent before reboot."
    } else if let Some(line) = app.deck.power_wait_line() {
        busy_line = line;
        &busy_line
    } else if app.deck.busy {
        busy_line = app.deck.busy_line();
        &busy_line
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
            under_bar: false,
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
