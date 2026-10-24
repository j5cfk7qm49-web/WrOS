//! WritingDeck home, layouts, tutorial and narrow system settings.
//! Render/input paths never run a system command. A single bounded worker does.
use super::*;
use grimoire_core::{
    tutorial,
    workspaces::{self, Position, State, Widget},
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
    Home { sel: usize },
    Widgets { sel: usize },
    Workspaces { sel: usize },
    System { sel: usize },
    Keyboard { sel: usize },
    KeyboardWait,
    KeyboardTest { text: String },
    Power { action: PowerAction, yes: bool },
    Tutorial { sel: usize },
    Lesson { id: u8 },
    Shortcuts { sel: usize, capture: bool },
}
struct Worker {
    tx: SyncSender<Request>,
    rx: Receiver<(Request, Result<Reply, String>)>,
}
impl Worker {
    fn spawn(mut backend: Box<dyn Backend>) -> Self {
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
        Self { tx, rx }
    }
}
const SHORTCUT_ACTIONS: [(ShortcutAction, &str); 8] = [
    (ShortcutAction::Dashboard, "WritingDeck dashboard"),
    (ShortcutAction::SystemSettings, "System settings"),
    (ShortcutAction::Tutorial, "Interactive tutorial"),
    (ShortcutAction::Outline, "Toggle outline"),
    (ShortcutAction::Focus, "Focus mode"),
    (ShortcutAction::Save, "Save"),
    (ShortcutAction::Find, "Find in scene"),
    (ShortcutAction::Palette, "Find anything"),
];

pub struct Deck {
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
        let layout_store = workspaces::Store::load(root)
            .map_err(|e| warnings.push(e.to_string()))
            .ok();
        let mut state = layout_store
            .as_ref()
            .map(|s| s.state.clone())
            .unwrap_or_default();
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
            worker: None,
            enable_system: background,
            preview: None,
            recent: String::new(),
            weekly: 0,
            dirty: false,
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
}

impl App {
    pub fn init_writingdeck(&mut self, launch: bool) {
        if launch {
            if resume::read(&self.project.root).is_none() {
                self.restore_deck_position();
            }
            self.focus_mode = self.deck.state.current().focus && self.open.is_some();
            if self.focus_mode || !self.deck.state.current().outline {
                self.focus = Focus::Editor;
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
        let document = self.open.and_then(|i| {
            self.project.nodes[i]
                .path
                .strip_prefix(&self.project.root)
                .ok()
                .map(Path::to_path_buf)
        });
        let pos = Position {
            document,
            line: self.editor.cy,
            column: self.editor.cx,
            scroll: self.editor.scroll,
        };
        let w = self.deck.state.current_mut();
        if w.primary != pos || w.focus != self.focus_mode {
            w.primary = pos;
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
        let p = self.deck.state.current().primary.clone();
        if let Some(path) = p.document {
            let path = self.project.root.join(path);
            if let Some(i) = self
                .project
                .nodes
                .iter()
                .position(|n| n.path == path && n.kind == Kind::Scene)
            {
                self.open_scene(i);
                self.reveal(i);
                self.editor.place(p.line, p.column);
                self.editor.scroll = p.scroll;
            }
        }
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
        let w = self.deck.state.current_mut();
        w.outline = !w.outline;
        if !w.outline && self.focus == Focus::Tree {
            self.focus = Focus::Editor;
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
            ShortcutAction::Focus => self.toggle_focus_mode(),
            ShortcutAction::Save => self.save(),
            ShortcutAction::Find => self.open_find(),
            ShortcutAction::Palette => self.open_palette(),
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
                    self.deck.note = format!("Unavailable: {e}");
                    if matches!(self.overlay, Overlay::Deck(Page::KeyboardWait)) {
                        self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                    }
                }
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
                            self.focus = Focus::Editor;
                        }
                    }
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
                                4 => self.open_system_settings(),
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
                        self.remember_deck();
                        self.deck.state.selected = sel;
                        self.restore_deck_position();
                        self.focus_mode = self.deck.state.current().focus && self.open.is_some();
                        if self.focus_mode || !self.deck.state.current().outline {
                            self.focus = Focus::Editor;
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
                    8..=10 => {
                        self.overlay = Overlay::Deck(Page::Power {
                            action: match sel {
                                8 => PowerAction::Suspend,
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
                    if yes && self.commit_saves() {
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
                    self.focus = Focus::Editor;
                }
            }
            Widget::DailyGoal | Widget::NovelGoal => self.run_action(Action::Goals),
            Widget::WeeklyProgress => self.run_action(Action::Progress),
            Widget::Sprint => self.run_action(Action::StartSprint),
            Widget::RecentScenes => self.open_palette(),
            Widget::Battery | Widget::System => self.open_system_settings(),
            Widget::Backup => self.open_help(Some("sync")),
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
                self.focus = Focus::Editor;
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
            17 => self.run_action(Action::Goals),
            19 => self.run_action(Action::StartSprint),
            26 => self.spelling(),
            33 => self.open_history(),
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
    f.render_widget(Clear, area);
    let block = crate::ui::pane_block("WRITINGDECK OS · offline first", true, t);
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
                    Widget::RecentScenes => app.deck.recent.clone(),
                    Widget::Battery => app
                        .deck
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.battery_percent)
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "unavailable; refresh in System settings".into()),
                    Widget::Backup => app
                        .backup_note
                        .clone()
                        .unwrap_or_else(|| "not configured; local save is not backup".into()),
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
                    "System settings",
                    "Interactive tutorial",
                    "Configure widgets",
                ]
                .map(str::to_string),
            );
            hint = "↑↓ choose  Enter open  h hide widget  J/K reorder  w widgets  Esc resume";
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
            title = "Saved workspaces · primary editor + outline foundation".into();
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
            hint = "↑↓ choose  Enter restore  +/- outline width  n clone custom  Esc home";
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
                "Suspend (confirmation required)".into(),
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
        Page::Tutorial { sel } => {
            title = format!(
                "Interactive tutorial · {} / 39 completed{}",
                app.deck.progress.completed.len(),
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
