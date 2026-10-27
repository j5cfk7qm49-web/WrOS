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
}

pub struct Shell {
    pub nav: Navigation<Route>,
    pub notices: Notices,
    pub external: bool,
    origin: Option<Overlay>,
    path: PathBuf,
    seen: Option<grimoire_core::sync::Fingerprint>,
    writable: bool,
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
            Entry::info("Local accounts · native management not available yet"),
            Entry::info("Google · account service not configured"),
            Entry::info("Google Drive · native browsing not available yet"),
            Entry::info("Synchronization · conflict-safe service not available yet"),
            Entry::info("Backups · verified remote restore not available yet"),
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
                    Entry::new("Scan Wi-Fi networks", Choice::System(3)),
                    Entry::info("Connect / saved connections · native enrollment still required"),
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
                    Entry::info(
                        "Warm screen · very warm 2200K preference; display service pending",
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
                    Entry::new("Suspend · requires secure lock support", Choice::System(8)),
                    Entry::new("Shutdown · confirmation required", Choice::System(9)),
                    Entry::new("Restart · confirmation required", Choice::System(10)),
                ]),
                Panel::Storage => rows.push(Entry::info(format!(
                    "Available local storage: {}",
                    status
                        .and_then(|s| s.available_bytes)
                        .map_or("unknown".into(), |n| format!("{} MiB", n / (1024 * 1024)))
                ))),
                Panel::DateTime => rows.extend([
                    Entry::info(format!(
                        "Timezone: {}",
                        status
                            .and_then(|s| s.timezone.as_deref())
                            .unwrap_or("unknown")
                    )),
                    Entry::info("Date/time editing · narrow authorized service pending"),
                ]),
                Panel::Updates => rows.push(Entry::info(
                    "Signed WrOS updates and rollback are not available yet",
                )),
                Panel::Recovery => rows.push(Entry::info(
                    "Native recovery is not qualified; use documented maintenance media",
                )),
            }
            rows
        }
    }
}

impl App {
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
                    Choice::Open(route) => {
                        self.deck.shell.nav.open(route);
                        if matches!(route, Route::Panel(_))
                            && self.deck.snapshot.is_none()
                            && !self.deck.busy
                        {
                            self.deck.request(writingdeck_sys::Request::Refresh);
                        }
                    }
                    Choice::Information => self.deck.shell.notices.push(&row.label),
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
    };
    if app
        .deck
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.simulated)
    {
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
        wros_shell::Theme {
            text: theme.text,
            accent: theme.accent,
            dim: theme.dim,
            border: theme.border,
        },
    );
}
