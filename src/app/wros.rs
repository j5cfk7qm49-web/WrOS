//! WritingDeck layouts, tutorial and narrow system settings.
//! Render/input paths never run a system command. A single bounded worker does.
#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::{App, BackupMark, Overlay, shell};
use grimoire_core::{
    resume, tutorial,
    workspaces::{self, State},
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use wros_sys::{PowerAction, Snapshot, keyboard::Layout as KeyboardLayout, shortcuts::Shortcuts};

mod draw;
mod input;
mod lessons;
mod power;
mod session;
mod shortcuts;
mod system;
mod wifi;
mod workspace;

pub(super) use draw::draw;
use system::Worker;

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

#[derive(Clone, Copy, Debug)]
pub struct PendingPower {
    pub kind: PowerWait,
    /// What is being waited for: "the sync", "the final sync", "the backup", "the system request".
    pub what: &'static str,
    pub since: std::time::Instant,
}

pub struct Deck {
    pub shell: shell::Shell,
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
    widget_wifi_scan_pending: bool,
    /// When a brightness key last changed the light: it shows for a moment.
    pub light_flash: Option<std::time::Instant>,
    /// "14:07", worked out on the tick so drawing never reads the time zone.
    pub clock: String,
    /// The book's backup, for the bar's line, also worked out on the tick.
    pub backup_mark: Option<BackupMark>,
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
            shell: shell::Shell::load(root, background),
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
            widget_wifi_scan_pending: false,
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
}

impl App {
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
}
