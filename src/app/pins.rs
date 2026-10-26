//! Sticky cards are cached readers of Project nodes. Only explicit source
//! editing enters the existing guarded editor/registry; rendering does no I/O.
use super::*;
use anyhow::{Context, bail};
use grimoire_core::pins::{
    self, Anchor, Color, Life, Mode as ScrollMode, Pin, Resolution, Scope, State, Store,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color as Ink, Style},
    text::Line,
    widgets::{Clear, Paragraph},
};

#[derive(Default)]
pub struct Source {
    pub nodes: Vec<usize>,
    pub error: Option<String>,
}
pub struct Pins {
    store: Option<Store>,
    fallback: State,
    pub warning: String,
    pub sources: HashMap<u64, Source>,
    pub rect: Rect,
    pub hits: Vec<(u64, Rect)>,
    pub list_page: usize,
    pub board_life: Life,
    pub dashboard_hits: std::cell::RefCell<Vec<(u64, Rect)>>,
    pub board_hits: std::cell::RefCell<Vec<(u64, Rect)>>,
    pub board_preview: std::cell::Cell<Rect>,
    pub board_page: std::cell::Cell<usize>,
}
impl Pins {
    pub fn load(root: &Path) -> Self {
        let (store, warning) = match Store::load(root) {
            Ok(s) => (Some(s), String::new()),
            Err(e) => (
                None,
                format!("Pins metadata preserved; editing disabled: {e:#}"),
            ),
        };
        Self {
            store,
            fallback: State::default(),
            warning,
            sources: HashMap::new(),
            rect: Rect::default(),
            hits: vec![],
            list_page: 2,
            board_life: Life::Active,
            dashboard_hits: std::cell::RefCell::new(vec![]),
            board_hits: std::cell::RefCell::new(vec![]),
            board_preview: std::cell::Cell::new(Rect::default()),
            board_page: std::cell::Cell::new(1),
        }
    }
    pub fn state(&self) -> &State {
        self.store.as_ref().map_or(&self.fallback, |s| &s.state)
    }
    pub fn update<T>(&mut self, edit: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let Some(store) = &mut self.store else {
            bail!("{}", self.warning);
        };
        match store.update(edit) {
            Ok(value) => {
                self.warning.clear();
                Ok(value)
            }
            Err(e) => {
                self.warning =
                    format!("Pins NOT saved: {e:#}. Previous cards kept; R reloads metadata.");
                Err(e)
            }
        }
    }
}
impl App {
    /// Input/tick/reload boundary only. Validate containment and aliases once;
    /// the draw path reads this map and the current in-memory Project bodies.
    pub fn refresh_pin_sources(&mut self) {
        self.pins.sources.clear();
        if self.pins.state().pins.is_empty() {
            return;
        }
        let identities: Vec<_> = self
            .project
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Scene)
            .filter_map(|(i, n)| {
                let path = n.path.strip_prefix(&self.project.root).ok()?;
                Some((i, pins::identify(&self.project.root, path).ok()?))
            })
            .collect();
        let mut checked = HashMap::new();
        for pin in &self.pins.state().pins {
            let source = match pins::identify(&self.project.root, &pin.source) {
                Ok(identity) => {
                    let nodes: Vec<_> = identities
                        .iter()
                        .filter_map(|(i, other)| identity.same_file(other).then_some(*i))
                        .collect();
                    let error = nodes.is_empty().then(|| {
                        "Source is not a loaded project document; reload/relink explicitly".into()
                    });
                    Source { nodes, error }
                }
                Err(e) => Source {
                    nodes: vec![],
                    error: Some(format!("Source unavailable: {e:#}")),
                },
            };
            // Keep collection separate from state borrow.
            checked.insert(pin.id, source);
        }
        self.pins.sources = checked;
    }
    pub fn pin_node(&self, id: u64) -> Option<usize> {
        let source = self.pins.sources.get(&id)?;
        [Pane::Primary, Pane::Secondary]
            .into_iter()
            .filter_map(|p| self.view_open(p))
            .find(|i| source.nodes.contains(i))
            .or_else(|| source.nodes.first().copied())
    }
    pub fn pin_body(&self, id: u64) -> std::result::Result<&str, String> {
        let source = self
            .pins
            .sources
            .get(&id)
            .ok_or("Source not checked; R reloads the board")?;
        if let Some(error) = &source.error {
            return Err(error.clone());
        }
        if let (Some(a), Some(b)) = (
            self.view_open(Pane::Primary),
            self.view_open(Pane::Secondary),
        ) && a != b
            && source.nodes.contains(&a)
            && source.nodes.contains(&b)
        {
            return Err("Source identity has two retained editor contexts; repair external aliases first; no version guessed".into());
        }
        let n = self
            .pin_node(id)
            .and_then(|i| self.project.nodes.get(i))
            .ok_or("Source missing from project")?;
        if n.read_only || n.parked || n.disk.unavailable.is_some() {
            return Err(
                "Source unreadable, non-UTF-8 or a conflict copy; resolve it in the editor first"
                    .into(),
            );
        }
        Ok(&n.body)
    }
    fn pin_context(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        let Some(i) = self.view_open(Pane::Primary) else {
            return (None, None);
        };
        let node = &self.project.nodes[i];
        if !node.in_manuscript {
            return (None, None);
        }
        let scene = node
            .path
            .strip_prefix(&self.project.root)
            .ok()
            .map(Path::to_path_buf);
        let mut parent = self.parents.get(i).copied().flatten();
        while let Some(i) = parent {
            if self.project.nodes[i].kind == Kind::Container
                && manuscript::section_of(&self.project, i) == manuscript::Section::Chapter
            {
                return (
                    scene,
                    self.project.nodes[i]
                        .path
                        .strip_prefix(&self.project.root)
                        .ok()
                        .map(Path::to_path_buf),
                );
            }
            parent = self.parents.get(i).copied().flatten();
        }
        (scene, None)
    }
    pub fn pin_ids(&self, dashboard: bool) -> Vec<u64> {
        let (scene, chapter) = self.pin_context();
        self.pins
            .state()
            .pins
            .iter()
            .filter(|p| p.visible(scene.as_deref(), chapter.as_deref(), dashboard))
            .filter(|p| {
                self.deck
                    .state
                    .current()
                    .pin_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&p.id))
            })
            .map(|p| p.id)
            .collect()
    }
    fn board_ids(&self) -> Vec<u64> {
        self.pins
            .state()
            .pins
            .iter()
            .filter(|p| p.life == self.pins.board_life)
            .map(|p| p.id)
            .collect()
    }
    pub fn pin_count(&self, dashboard: bool) -> usize {
        self.pin_ids(dashboard).len()
    }
    pub fn open_pinboard(&mut self) {
        self.flush();
        self.refresh_pin_sources();
        self.overlay = Overlay::Pins;
        self.tree_drag = None;
        self.msg =
            "Pinboard · n pin document · Enter edit source · ? help · no source text is copied"
                .into();
        self.pin_select_valid(true);
    }
    pub fn pin_picker(&mut self, relink: Option<u64>) {
        let entries = self
            .project
            .nodes
            .iter()
            .filter(|n| {
                n.kind == Kind::Scene && !n.read_only && !n.parked && n.disk.unavailable.is_none()
            })
            .filter(|n| {
                n.path
                    .strip_prefix(&self.project.root)
                    .is_ok_and(|p| pins::source_relative(p).is_ok())
            })
            .map(|n| palette::Entry {
                label: format!(
                    "{} · {}",
                    n.title,
                    n.path.strip_prefix(&self.project.root).unwrap().display()
                ),
                key: String::new(),
                detail: if relink.is_some() {
                    "Relink reference"
                } else {
                    "Pin reference"
                }
                .into(),
                action: match relink {
                    Some(id) => Action::RelinkPin(id, n.path.clone()),
                    None => Action::PinDocument(n.path.clone()),
                },
            })
            .collect();
        self.overlay = Overlay::Palette {
            query: String::new(),
            sel: 0,
            entries,
        };
        self.msg =
            "Choose an existing project Markdown document; no copied sticky-note body".into();
    }
    pub fn pin_current(&mut self) {
        let Some(i) = self.open else {
            self.msg = "Open a document to pin it".into();
            return;
        };
        let path = self.project.nodes[i].path.clone();
        self.pin_document(&path, None);
    }
    pub fn pin_document(&mut self, path: &Path, relink: Option<u64>) {
        let _ = self.try_pin_document(path, relink);
    }
    pub(super) fn try_pin_document(&mut self, path: &Path, relink: Option<u64>) -> Result<u64> {
        self.flush();
        let result = (|| -> Result<u64> {
            let source = path.strip_prefix(&self.project.root)?.to_path_buf();
            pins::identify(&self.project.root, &source)?;
            let n = self
                .project
                .nodes
                .iter()
                .find(|n| n.path == path)
                .context("source not loaded; reload the project")?;
            if n.kind != Kind::Scene || n.read_only || n.parked || n.disk.unavailable.is_some() {
                bail!("choose a readable, settled Markdown document");
            }
            self.pins.update(|s| {
                let id = if let Some(id) = relink {
                    let p = s.pin_mut(id)?;
                    p.source = source;
                    p.scroll = 0;
                    id
                } else {
                    s.add(source)?
                };
                s.selected = Some(id);
                s.sidebar_visible = true;
                Ok(id)
            })
        })();
        match result {
            Ok(id) => {
                self.pins.board_life = relink
                    .and_then(|id| {
                        self.pins
                            .state()
                            .pins
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| p.life)
                    })
                    .unwrap_or(Life::Active);
                self.refresh_pin_sources();
                self.overlay = Overlay::Pins;
                self.tree_drag = None;
                self.msg = if relink.is_some() {
                    "Reference relinked; anchor retained and rechecked"
                } else {
                    "Pinned reference saved · Enter edits its original source"
                }
                .into();
                if relink.is_none() {
                    self.observe_deck(grimoire_core::tutorial::Event::PinCreated);
                }
                Ok(id)
            }
            Err(e) => {
                // A failed pin/relink must not unexpectedly return typing to
                // the manuscript underneath the source picker.
                self.overlay = Overlay::Pins;
                self.tree_drag = None;
                self.msg = format!("Pin not saved: {e:#}; sources untouched");
                Err(e)
            }
        }
    }
    pub fn toggle_pins(&mut self) {
        let visible = !self.pins.state().sidebar_visible;
        match self.pins.update(|s| {
            s.sidebar_visible = visible;
            Ok(())
        }) {
            Ok(()) => {
                if !visible && self.focus == Focus::Pins {
                    self.focus = self.editor_focus();
                }
                self.msg = if visible {
                    "Pins sidebar requested · narrow/strict-focus layouts retain cards in Pinboard"
                } else {
                    "Pins sidebar hidden; cards and their sources retained"
                }
                .into();
            }
            Err(e) => self.msg = e.to_string(),
        }
    }
    fn pin_selected(&self) -> Option<Pin> {
        self.pins
            .state()
            .pins
            .iter()
            .find(|p| Some(p.id) == self.pins.state().selected)
            .cloned()
    }
    fn pin_edit(&mut self, primary: bool) {
        let Some(pin) = self.pin_selected() else {
            return;
        };
        self.refresh_pin_sources();
        if let Err(e) = self.ensure_document_claims() {
            self.rescue();
            self.save_state = SaveState::Failed(e.to_string());
            self.last_attempt = Some(Instant::now());
            self.msg =
                format!("Cannot edit pin source: {e}; all buffers kept; recovery copies attempted");
            return;
        }
        if let Err(e) = self.pin_body(pin.id) {
            self.msg = format!("{e}; b explicitly relinks this card");
            return;
        }
        let Some(i) = self.pin_node(pin.id) else {
            return;
        };
        if self.focus_mode && !primary {
            self.msg="Ctrl-D leaves strict focus before opening a source beside the manuscript; e uses primary".into();
            return;
        }
        // Existing owners win, including aliases. Never open a second writer.
        for pane in [Pane::Primary, Pane::Secondary] {
            if self.view_open(pane).is_some_and(|i| {
                self.pins
                    .sources
                    .get(&pin.id)
                    .is_some_and(|s| s.nodes.contains(&i))
            }) {
                if pane == Pane::Secondary {
                    if self.focus_mode {
                        self.msg = "Leave strict focus with Ctrl-D before focusing the source already owned by Secondary".into();
                        return;
                    }
                    self.views.secondary_visible = true;
                }
                self.activate_editor(pane);
                self.overlay = Overlay::None;
                self.msg = "Pin source already owned; focused the existing editor (no copy)".into();
                return;
            }
        }
        if primary {
            if !self.commit_saves() {
                return;
            }
            self.open_scene(i);
            if self.open == Some(i) {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
            }
        } else {
            self.open_secondary_node(i);
        }
    }
    fn pin_select_valid(&mut self, board: bool) {
        let ids = if board {
            self.board_ids()
        } else {
            self.pin_ids(false)
        };
        if self
            .pins
            .state()
            .selected
            .is_none_or(|id| !ids.contains(&id))
            && let Some(&id) = ids.first()
        {
            let _ = self.pins.update(|s| {
                s.selected = Some(id);
                Ok(())
            });
        }
    }
    pub fn prepare_pin_focus(&mut self) {
        let ids = self.pin_ids(false);
        if ids.is_empty() {
            return;
        }
        let at = self
            .pins
            .state()
            .selected
            .and_then(|id| ids.iter().position(|i| *i == id))
            .unwrap_or(0);
        let start = sidebar_start(
            self.pins.state(),
            &ids,
            at,
            self.pins.rect.height.saturating_sub(2) as usize,
        );
        if self.pins.state().selected == Some(ids[at]) && self.pins.state().sidebar_scroll == start
        {
            return;
        }
        if let Err(e) = self.pins.update(|s| {
            s.selected = Some(ids[at]);
            s.sidebar_scroll = start;
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    fn pin_move_selection(&mut self, down: bool, board: bool) {
        let ids = if board {
            self.board_ids()
        } else {
            self.pin_ids(false)
        };
        if ids.is_empty() {
            return;
        }
        let at = self
            .pins
            .state()
            .selected
            .and_then(|id| ids.iter().position(|&n| n == id))
            .unwrap_or(0);
        let next = if down {
            (at + 1).min(ids.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        let id = ids[next];
        let page = if board {
            self.pins.board_page.get()
        } else {
            self.pins.list_page
        }
        .max(1);
        let available = self.pins.rect.height.saturating_sub(2) as usize;
        if let Err(e) = self.pins.update(|s| {
            s.selected = Some(id);
            if board {
                if next < s.board_scroll {
                    s.board_scroll = next;
                } else if next >= s.board_scroll + page {
                    s.board_scroll = next + 1 - page;
                }
            } else {
                s.sidebar_scroll = sidebar_start(s, &ids, next, available);
            }
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    fn pin_scroll(&mut self, id: u64, down: bool, step: usize) {
        let Some(pin) = self.pins.state().pins.iter().find(|p| p.id == id) else {
            return;
        };
        let body = match self.pin_body(id) {
            Ok(b) => b,
            Err(e) => {
                self.msg = e;
                return;
            }
        };
        let width = if matches!(
            self.overlay,
            Overlay::Deck(super::writingdeck::Page::Home { .. })
        ) {
            self.pins
                .dashboard_hits
                .borrow()
                .iter()
                .find(|(i, _)| *i == id)
                .map_or(24, |(_, r)| r.width.saturating_sub(2))
        } else if self.overlay == Overlay::Pins {
            self.pins.board_preview.get().width.saturating_sub(2)
        } else {
            self.pins.rect.width.saturating_sub(4)
        }
        .max(1) as usize;
        let anchor = if pin.mode == ScrollMode::Anchored {
            pin.anchor.as_ref()
        } else {
            None
        };
        let preview = pins::excerpt(body, width, anchor, 0, 0);
        if matches!(
            preview.resolution,
            Resolution::Missing | Resolution::Ambiguous
        ) {
            self.msg="Anchor missing or ambiguous; use f for a free preview, or h/q to capture a new anchor".into();
            return;
        }
        let next = if down {
            pin.scroll
                .saturating_add(step)
                .min(preview.total_rows.saturating_sub(1))
                .min(1_000_000)
        } else {
            pin.scroll
                .saturating_sub(step)
                .min(preview.total_rows.saturating_sub(1))
        };
        if let Err(e) = self.pins.update(|s| {
            s.pin_mut(id)?.scroll = next;
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    fn pin_restore_anchor(&mut self) {
        let Some(pin) = self.pin_selected() else {
            return;
        };
        let resolved = self
            .pin_body(pin.id)
            .map(|body| pins::resolve(body, pin.anchor.as_ref()));
        if !matches!(resolved, Ok(Resolution::Found(_))) {
            self.msg="No unique semantic anchor to restore; h captures an ATX heading, q captures a source selection".into();
            return;
        }
        if let Err(e) = self.pins.update(|s| {
            let p = s.pin_mut(pin.id)?;
            p.mode = ScrollMode::Anchored;
            p.scroll = 0;
            Ok(())
        }) {
            self.msg = e.to_string();
            return;
        }
        self.msg = "Returned to the unique source anchor (not a stored line number)".into();
        if pin.scroll > 0 && pin.mode == ScrollMode::Anchored {
            self.observe_deck(grimoire_core::tutorial::Event::PinAnchorRestored);
        }
    }
    fn pin_capture_heading(&mut self) {
        let Some(pin) = self.pin_selected() else {
            return;
        };
        let body = match self.pin_body(pin.id) {
            Ok(b) => b,
            Err(e) => {
                self.msg = e;
                return;
            }
        };
        let headings = pins::headings(body);
        if headings.is_empty() {
            self.msg="No ATX heading in source. Enter edits it; add a ## Heading, then return here and press h".into();
            return;
        }
        let old = pin.anchor.as_ref().and_then(|a| {
            if let Anchor::Heading { text } = a {
                Some(text)
            } else {
                None
            }
        });
        let at = old
            .and_then(|old| headings.iter().position(|h| h == old))
            .map_or(0, |i| (i + 1) % headings.len());
        let text = headings[at].clone();
        if let Err(e) = self.pins.update(|s| {
            let p = s.pin_mut(pin.id)?;
            p.anchor = Some(Anchor::Heading { text });
            p.mode = ScrollMode::Anchored;
            p.scroll = 0;
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    fn pin_capture_quote(&mut self) {
        let Some(pin) = self.pin_selected() else {
            return;
        };
        if !self.open.is_some_and(|i| {
            self.pins
                .sources
                .get(&pin.id)
                .is_some_and(|s| s.nodes.contains(&i))
        }) {
            self.msg="Select a passage in this pin's source editor first, then reopen Pinboard and press q".into();
            return;
        }
        let Some(text) = self.editor.selected_text().filter(|t| !t.trim().is_empty()) else {
            self.msg="Select a source passage in the editor first; quote anchors are exact, case-sensitive text".into();
            return;
        };
        if let Err(e) = self.pins.update(|s| {
            let p = s.pin_mut(pin.id)?;
            p.anchor = Some(Anchor::Quote { text });
            p.mode = ScrollMode::Anchored;
            p.scroll = 0;
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    fn pin_cycle_scope(&mut self) {
        let Some(pin) = self.pin_selected() else {
            return;
        };
        let (scene, chapter) = self.pin_context();
        let next = match pin.scope {
            Scope::Always => Scope::Novel,
            Scope::Novel => Scope::Chapter,
            Scope::Chapter => Scope::Scene,
            Scope::Scene => Scope::Dashboard,
            Scope::Dashboard => Scope::Hidden,
            Scope::Hidden => Scope::Always,
        };
        let context = match next {
            Scope::Scene => scene,
            Scope::Chapter => chapter,
            _ => None,
        };
        if matches!(next, Scope::Scene | Scope::Chapter) && context.is_none() {
            self.msg =
                "Open a manuscript scene in Primary before capturing scene/chapter scope".into();
            return;
        }
        if let Err(e) = self.pins.update(|s| {
            let p = s.pin_mut(pin.id)?;
            p.scope = next;
            p.context = context;
            Ok(())
        }) {
            self.msg = e.to_string();
        }
    }
    pub fn on_pin_key(&mut self, key: Key) {
        if let Overlay::PinCategory { id, ref buf } = self.overlay.clone() {
            let mut buf = buf.clone();
            match key {
                Key::Esc => self.overlay = Overlay::Pins,
                Key::Enter => {
                    match self.pins.update(|s| {
                        s.pin_mut(id)?.category = buf;
                        Ok(())
                    }) {
                        Ok(()) => self.overlay = Overlay::Pins,
                        Err(e) => self.msg = e.to_string(),
                    }
                }
                Key::Backspace => {
                    buf.pop();
                    self.overlay = Overlay::PinCategory { id, buf };
                }
                Key::Char(c) if !c.is_control() && buf.chars().count() < 40 => {
                    buf.push(c);
                    self.overlay = Overlay::PinCategory { id, buf };
                }
                _ => {}
            }
            return;
        }
        let board = self.overlay == Overlay::Pins;
        if !board && self.focus != Focus::Pins {
            return;
        }
        if matches!(key, Key::Up | Key::Down) {
            self.pin_move_selection(key == Key::Down, board);
            return;
        }
        if key == Key::Char('R') {
            let life = self.pins.board_life;
            self.pins = Pins::load(&self.project.root);
            self.pins.board_life = life;
            self.refresh_pin_sources();
            self.msg = if self.pins.warning.is_empty() {
                "Pin metadata reloaded; source reload/conflict handling remains guarded".into()
            } else {
                self.pins.warning.clone()
            };
            return;
        }
        match key {
            Key::Esc => {
                self.overlay = Overlay::None;
                self.focus = self.editor_focus();
            }
            Key::Char('n') => self.pin_picker(None),
            Key::Char('v') => self.toggle_pins(),
            Key::Char('[' | ']') => {
                if let Err(e) = self.pins.update(|s| {
                    s.sidebar_width = if key == Key::Char(']') {
                        (s.sidebar_width + 2).min(60)
                    } else {
                        s.sidebar_width.saturating_sub(2).max(24)
                    };
                    Ok(())
                }) {
                    self.msg = e.to_string();
                }
            }
            Key::Tab | Key::BackTab if board => {
                self.pins.board_life = match self.pins.board_life {
                    Life::Active => Life::Archived,
                    Life::Archived => Life::Trashed,
                    Life::Trashed => Life::Active,
                };
                self.pin_select_valid(true);
            }
            _ => {
                self.pin_select_valid(board);
                let Some(pin) = self.pin_selected() else {
                    return;
                };
                // An empty scope/filter may not act on a hidden selection.
                let ids = if board {
                    self.board_ids()
                } else {
                    self.pin_ids(false)
                };
                if !ids.contains(&pin.id) {
                    return;
                }
                match key {
                    Key::Enter => self.pin_edit(false),
                    Key::Char('e') => self.pin_edit(true),
                    Key::PageDown | Key::PageUp => self.pin_scroll(pin.id, key == Key::PageDown, 3),
                    Key::Home => self.pin_restore_anchor(),
                    Key::Char('h') => self.pin_capture_heading(),
                    Key::Char('q') => self.pin_capture_quote(),
                    Key::Char('s') => self.pin_cycle_scope(),
                    Key::Char('b') => self.pin_picker(Some(pin.id)),
                    Key::Char('g') => {
                        self.overlay = Overlay::PinCategory {
                            id: pin.id,
                            buf: pin.category.clone(),
                        }
                    }
                    Key::Char('f') => {
                        if let Err(e) = self.pins.update(|s| {
                            let p = s.pin_mut(pin.id)?;
                            p.mode = if p.mode == ScrollMode::Free {
                                ScrollMode::Anchored
                            } else {
                                ScrollMode::Free
                            };
                            p.scroll = 0;
                            Ok(())
                        }) {
                            self.msg = e.to_string();
                        }
                    }
                    Key::Char('c') => {
                        if let Err(e) = self.pins.update(|s| {
                            s.pin_mut(pin.id)?.color = pin.color.next();
                            Ok(())
                        }) {
                            self.msg = e.to_string();
                        }
                    }
                    Key::Char(' ') => {
                        if let Err(e) = self.pins.update(|s| {
                            s.pin_mut(pin.id)?.collapsed = !pin.collapsed;
                            Ok(())
                        }) {
                            self.msg = e.to_string();
                        }
                    }
                    Key::Char('+' | '-') => {
                        if let Err(e) = self.pins.update(|s| {
                            s.pin_mut(pin.id)?.height = if key == Key::Char('+') {
                                (pin.height + 1).min(20)
                            } else {
                                pin.height.saturating_sub(1).max(4)
                            };
                            Ok(())
                        }) {
                            self.msg = e.to_string();
                        }
                    }
                    Key::Char('J' | 'K') => {
                        if let Err(e) = self.pins.update(|s| {
                            let at = ids.iter().position(|&id| id == pin.id).unwrap();
                            let other = if key == Key::Char('J') {
                                at + 1
                            } else {
                                at.saturating_sub(1)
                            };
                            if let Some(id) = ids.get(other) {
                                let a = s.pins.iter().position(|p| p.id == pin.id).unwrap();
                                let b = s.pins.iter().position(|p| p.id == *id).unwrap();
                                s.pins.swap(a, b);
                            }
                            Ok(())
                        }) {
                            self.msg = e.to_string();
                        }
                        if !board {
                            self.prepare_pin_focus();
                        }
                    }
                    Key::Char('a' | 'd' | 'u') => {
                        let life = match key {
                            Key::Char('a') => Life::Archived,
                            Key::Char('d') => Life::Trashed,
                            _ => Life::Active,
                        };
                        if let Err(e) = self.pins.update(|s| s.set_life(pin.id, life)) {
                            self.msg = e.to_string();
                        } else {
                            self.msg="Card metadata updated; source never moved or deleted · Tab visits Archive/Trash, u restores".into();
                            self.pin_select_valid(board);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    pub fn pin_follow_many(&mut self, renames: &[(PathBuf, PathBuf)]) {
        let map = |path: &Path| {
            renames
                .iter()
                .find_map(|(from, to)| {
                    self.project
                        .root
                        .join(path)
                        .strip_prefix(from)
                        .ok()
                        .map(|rest| to.join(rest))
                })
                .and_then(|p| {
                    p.strip_prefix(&self.project.root)
                        .ok()
                        .map(Path::to_path_buf)
                })
                .filter(|p| !p.components().any(|c| c.as_os_str() == ".grimoire"))
        };
        let changes: Vec<_> = self
            .pins
            .state()
            .pins
            .iter()
            .filter_map(|p| {
                let source = map(&p.source);
                let context = p.context.as_deref().and_then(map);
                (source.is_some() || context.is_some()).then_some((p.id, source, context))
            })
            .collect();
        if !changes.is_empty()
            && let Err(e) = self.pins.update(|s| {
                for (id, source, context) in changes {
                    let p = s.pin_mut(id)?;
                    if let Some(source) = source {
                        p.source = source;
                    }
                    if context.is_some() {
                        p.context = context;
                    }
                }
                Ok(())
            })
        {
            self.msg = format!(
                "Document moved, but pin references NOT saved: {e}; original metadata retained; b relinks explicitly"
            );
        }
    }
    pub fn pin_click(&mut self, x: u16, y: u16) -> bool {
        let dashboard = matches!(
            self.overlay,
            Overlay::Deck(super::writingdeck::Page::Home { .. })
        );
        let chosen = if self.overlay == Overlay::Pins {
            self.pins
                .board_hits
                .borrow()
                .iter()
                .find(|(_, r)| super::hit(*r, x, y))
                .copied()
        } else if dashboard {
            self.pins
                .dashboard_hits
                .borrow()
                .iter()
                .find(|(_, r)| super::hit(*r, x, y))
                .copied()
        } else if self.overlay == Overlay::None {
            self.pins
                .hits
                .iter()
                .find(|(_, r)| super::hit(*r, x, y))
                .copied()
        } else {
            None
        };
        if let Some((id, _)) = chosen {
            self.flush();
            match self.pins.update(|s| {
                s.selected = Some(id);
                Ok(())
            }) {
                Ok(()) => {
                    if dashboard {
                        self.open_pinboard();
                    } else if self.overlay == Overlay::None {
                        self.focus = Focus::Pins;
                    }
                }
                Err(e) => self.msg = e.to_string(),
            }
            return true;
        }
        false
    }
    pub fn pin_wheel(&mut self, x: u16, y: u16, down: bool) -> bool {
        if self.overlay == Overlay::Pins {
            if super::hit(self.pins.board_preview.get(), x, y) {
                if let Some(id) = self
                    .pins
                    .state()
                    .selected
                    .filter(|id| self.board_ids().contains(id))
                {
                    self.pin_scroll(id, down, 3);
                }
            } else {
                self.pin_move_selection(down, true);
            }
            return true;
        }
        if matches!(
            self.overlay,
            Overlay::Deck(super::writingdeck::Page::Home { .. })
        ) {
            let id = self
                .pins
                .dashboard_hits
                .borrow()
                .iter()
                .find(|(_, r)| super::hit(*r, x, y))
                .map(|(id, _)| *id);
            if let Some(id) = id {
                self.pin_scroll(id, down, 3);
                return true;
            }
        }
        if self.overlay != Overlay::None {
            return false;
        }
        if let Some((id, _)) = self
            .pins
            .hits
            .iter()
            .find(|(_, r)| super::hit(*r, x, y))
            .copied()
        {
            self.pin_scroll(id, down, 3);
            return true;
        }
        if super::hit(self.pins.rect, x, y) {
            self.pin_move_selection(down, false);
            return true;
        }
        false
    }
}

fn sidebar_start(state: &State, ids: &[u64], at: usize, height: usize) -> usize {
    let height = height.max(2);
    let size = |id: u64| {
        state
            .pins
            .iter()
            .find(|p| p.id == id)
            .map_or(2, |p| if p.collapsed { 2 } else { p.height as usize })
            .min(height)
    };
    let mut start = state.sidebar_scroll.min(at);
    let mut used: usize = ids[start..=at].iter().map(|&id| size(id)).sum();
    while used > height && start < at {
        used -= size(ids[start]);
        start += 1;
    }
    start
}

fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect()
}
fn color(c: Color, t: &Theme) -> Ink {
    match c {
        Color::Neutral => t.dim,
        Color::Amber => Ink::Yellow,
        Color::Green => Ink::Green,
        Color::Blue => Ink::Cyan,
        Color::Rose => Ink::Magenta,
    }
}
fn draw_card(f: &mut Frame, app: &App, pin: &Pin, area: Rect, t: &Theme, focused: bool) {
    let title = app
        .pin_node(pin.id)
        .and_then(|i| app.project.nodes.get(i))
        .map_or_else(|| pin.source.display().to_string(), |n| n.title.clone());
    let title = format!(
        "{} {} · {}{}",
        if Some(pin.id) == app.pins.state().selected {
            "▶"
        } else {
            "·"
        },
        safe(&title),
        if pin.category.is_empty() {
            format!("{:?}", pin.scope)
        } else {
            pin.category.clone()
        },
        if pin.collapsed { " [+]" } else { "" }
    );
    let mut block = crate::ui::pane_block(&title, focused, t);
    block = block.border_style(Style::default().fg(if focused {
        t.accent
    } else {
        color(pin.color, t)
    }));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if pin.collapsed || inner.height == 0 {
        return;
    }
    let mut rows: Vec<Line> = vec![];
    match app.pin_body(pin.id) {
        Err(error) => rows.push(Line::from(error).style(Style::default().fg(t.warn))),
        Ok(body) => {
            let anchor = if pin.mode == ScrollMode::Anchored {
                pin.anchor.as_ref()
            } else {
                None
            };
            let preview = pins::excerpt(
                body,
                inner.width as usize,
                anchor,
                pin.scroll,
                inner.height.saturating_sub(1) as usize,
            );
            if matches!(
                preview.resolution,
                Resolution::Missing | Resolution::Ambiguous
            ) {
                rows.push(
                    Line::from(format!(
                        "ANCHOR {:?} · f free / h,q recapture",
                        preview.resolution
                    ))
                    .style(Style::default().fg(t.warn)),
                );
            } else {
                if preview.total_rows > 0 && pin.scroll >= preview.total_rows {
                    rows.push(
                        Line::from("Source shortened; PgUp/PgDn clamps saved offset")
                            .style(Style::default().fg(t.warn)),
                    );
                } else {
                    rows.extend(preview.lines.into_iter().map(Line::from));
                }
            }
            let status = format!(
                "{:?} · row {} / {} · Enter source",
                pin.mode,
                pin.scroll + 1,
                preview.total_rows
            );
            let footer = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
            f.render_widget(
                Paragraph::new(status).style(Style::default().fg(t.dim)),
                footer,
            );
        }
    }
    f.render_widget(
        Paragraph::new(rows).style(Style::default().fg(t.text)),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        ),
    );
}
/// Reserve sidebar space only if there remains a usable independent editor.
/// Narrow and strict-focus terminals retain everything in the full Pinboard.
pub fn sidebar(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) -> Rect {
    app.pins.rect = Rect::default();
    app.pins.hits.clear();
    let width = app.pins.state().sidebar_width;
    let minimum = if app.views.secondary_visible && app.view_open(Pane::Secondary).is_some() {
        72
    } else {
        34
    };
    if app.focus_mode
        || !app.pins.state().sidebar_visible
        || area.width < width + minimum
        || area.height < 10
    {
        if app.focus == Focus::Pins {
            app.focus = app.editor_focus();
        }
        return area;
    }
    let [edit, side] =
        Layout::horizontal([Constraint::Min(minimum), Constraint::Length(width)]).areas(area);
    app.pins.rect = side;
    let ids = app.pin_ids(false);
    let count = ids.len();
    let title = format!("PINS · {count}");
    let block = crate::ui::pane_block(&title, app.focus == Focus::Pins, t);
    let inner = block.inner(side);
    f.render_widget(block, side);
    if count == 0 {
        if app.focus == Focus::Pins {
            app.focus = app.editor_focus();
        }
        f.render_widget(
            Paragraph::new(
                "No cards in this scope\nCtrl-K → Pinboard\nCards are references, not copies",
            )
            .style(Style::default().fg(t.dim)),
            inner,
        );
        return edit;
    }
    let scroll = app.pins.state().sidebar_scroll.min(count.saturating_sub(1));
    let mut y = inner.y;
    let mut hits = vec![];
    for id in ids.into_iter().skip(scroll) {
        let pin = app.pins.state().pins.iter().find(|p| p.id == id).unwrap();
        let h = if pin.collapsed { 2 } else { pin.height }.min(inner.bottom().saturating_sub(y));
        if h < 2 {
            break;
        }
        let rect = Rect::new(inner.x, y, inner.width, h);
        draw_card(
            f,
            app,
            pin,
            rect,
            t,
            app.focus == Focus::Pins && Some(id) == app.pins.state().selected,
        );
        hits.push((id, rect));
        y += h;
    }
    app.pins.list_page = hits.len().max(1);
    app.pins.hits = hits;
    edit
}
pub fn draw_board(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    f.render_widget(Clear, area);
    let title = format!("PINBOARD · {:?} · metadata-only", app.pins.board_life);
    let block = crate::ui::pane_block(&title, true, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 5 || inner.width < 8 {
        return;
    }
    if let Overlay::PinCategory { buf, .. } = &app.overlay {
        f.render_widget(
            Paragraph::new(format!(
                "Category (40 characters): {buf}_\nEnter save · Esc cancel"
            ))
            .style(Style::default().fg(t.text)),
            inner,
        );
        return;
    }
    let hint = "↑↓ card · PgUp/PgDn scroll · Home anchor · h heading · q quote · f free/anchor\nEnter source beside · e primary · n pin · b relink · v sidebar · s scope · g category\nspace fold · +/- height · [] sidebar width · c color · J/K order · a archive · d trash · u restore · Tab collection · R reload · Esc close";
    let help_height = if inner.height >= 15 { 3 } else { 1 };
    let status_height = if app.pins.warning.is_empty() { 1 } else { 2 };
    let usable = inner.height.saturating_sub(help_height + status_height);
    let list_width = (inner.width / 3)
        .clamp(14, 38)
        .min(inner.width.saturating_sub(8));
    let [list, preview] = Layout::horizontal([Constraint::Length(list_width), Constraint::Min(8)])
        .areas(Rect::new(inner.x, inner.y, inner.width, usable));
    app.pins.board_preview.set(preview);
    app.pins.board_page.set(usable as usize);
    let ids = app.board_ids();
    let selected = app
        .pins
        .state()
        .selected
        .filter(|id| ids.contains(id))
        .or_else(|| ids.first().copied());
    let selected_at = selected
        .and_then(|id| ids.iter().position(|&i| i == id))
        .unwrap_or(0);
    let scroll = app
        .pins
        .state()
        .board_scroll
        .min(selected_at)
        .max(selected_at.saturating_sub(usable.saturating_sub(1) as usize));
    let rows: Vec<Line> = ids
        .iter()
        .skip(scroll)
        .take(usable as usize)
        .filter_map(|id| app.pins.state().pins.iter().find(|p| p.id == *id))
        .map(|p| {
            Line::from(format!(
                "{} #{} {}",
                if Some(p.id) == selected { "▶" } else { " " },
                p.id,
                safe(&p.source.display().to_string())
            ))
            .style(Style::default().fg(if Some(p.id) == selected {
                t.accent
            } else {
                t.dim
            }))
        })
        .collect();
    *app.pins.board_hits.borrow_mut() = ids
        .iter()
        .skip(scroll)
        .take(usable as usize)
        .enumerate()
        .map(|(row, &id)| (id, Rect::new(list.x, list.y + row as u16, list.width, 1)))
        .collect();
    f.render_widget(Paragraph::new(rows), list);
    if let Some(pin) = selected.and_then(|id| app.pins.state().pins.iter().find(|p| p.id == id)) {
        draw_card(f, app, pin, preview, t, true);
    } else {
        f.render_widget(Paragraph::new("No cards in this collection.\nn: pin an existing document\nTab: Active / Archive / Trash\nu: restore card; never deletes source").style(Style::default().fg(t.dim)),preview);
    }
    let y = inner.y + usable;
    let status = if app.pins.warning.is_empty() {
        app.msg.as_str()
    } else {
        app.pins.warning.as_str()
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(if app.pins.warning.is_empty() {
            t.dim
        } else {
            t.warn
        })),
        Rect::new(inner.x, y, inner.width, status_height),
    );
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(t.dim)),
        Rect::new(inner.x, y + status_height, inner.width, help_height),
    );
}

/// Dashboard-only and always-visible cards. Overflow remains accessible through
/// the widget/Pinboard; mouse wheel inside a card scrolls only that reference.
pub fn dashboard(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let ids = app.pin_ids(true);
    let mut y = area.y;
    let mut hits = vec![];
    for id in ids {
        let pin = app.pins.state().pins.iter().find(|p| p.id == id).unwrap();
        let height =
            if pin.collapsed { 2 } else { pin.height }.min(area.bottom().saturating_sub(y));
        if height < 2 {
            break;
        }
        let rect = Rect::new(area.x, y, area.width, height);
        draw_card(f, app, pin, rect, t, false);
        hits.push((id, rect));
        y += height;
    }
    *app.pins.dashboard_hits.borrow_mut() = hits;
}
