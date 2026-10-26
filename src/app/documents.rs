//! Two editable views without two competing save paths.
//!
//! App.editor/open remain the ACTIVE editor context for the existing commands.
//! The inactive context owns the other Editor, undo/selection and geometry.
//! Switching is a move, not a clone; drawing temporarily borrows each context.
//! Both flush into the existing Project nodes and use its guarded save/history/
//! conflict/recovery machinery. The portable registry enforces file identity.
use super::*;
use grimoire_core::documents::{Assignment, Pane, Registry};
use grimoire_core::workspaces::Position;

pub struct Inactive {
    pub open: Option<usize>,
    pub editor: Editor,
    width: usize,
    height: usize,
    rect: Rect,
    prose: Rect,
    caret: Option<(usize, usize, usize, usize, usize, bool)>,
    manual_scroll: bool,
    click: Option<(Instant, u16, u16, u8)>,
}
impl Inactive {
    fn new(i: usize, body: &str) -> Self {
        Self {
            open: Some(i),
            editor: productivity::source_editor(body),
            width: 60,
            height: 20,
            rect: Rect::default(),
            prose: Rect::default(),
            caret: None,
            manual_scroll: false,
            click: None,
        }
    }
}
pub struct Views {
    pub active: Pane,
    pub other: Option<Inactive>,
    pub secondary_visible: bool,
    pub secondary_room: bool,
    pub registry: Registry,
}
impl Views {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            active: Pane::Primary,
            other: None,
            secondary_visible: false,
            secondary_room: false,
            registry: Registry::new(root)?,
        })
    }
}
impl Focus {
    pub fn pane(self) -> Option<Pane> {
        match self {
            Self::Editor => Some(Pane::Primary),
            Self::Secondary => Some(Pane::Secondary),
            _ => None,
        }
    }
    pub fn is_editor(self) -> bool {
        self.pane().is_some()
    }
}
impl App {
    pub fn editor_focus(&self) -> Focus {
        if self.views.active == Pane::Primary {
            Focus::Editor
        } else {
            Focus::Secondary
        }
    }
    pub fn view_is_focused(&self) -> bool {
        self.focus.pane() == Some(self.views.active)
    }
    pub fn view_open(&self, pane: Pane) -> Option<usize> {
        if self.views.active == pane {
            self.open
        } else {
            self.views.other.as_ref()?.open
        }
    }
    pub fn view_editor(&self, pane: Pane) -> Option<&Editor> {
        if self.views.active == pane {
            Some(&self.editor)
        } else {
            Some(&self.views.other.as_ref()?.editor)
        }
    }
    pub fn view_rect(&self, pane: Pane) -> Rect {
        if self.views.active == pane {
            self.rect_prose
        } else {
            self.views
                .other
                .as_ref()
                .map_or(Rect::default(), |v| v.prose)
        }
    }
    pub fn editor_at(&self, x: u16, y: u16) -> Option<Pane> {
        [Pane::Primary, Pane::Secondary]
            .into_iter()
            .find(|&pane| self.view_open(pane).is_some() && hit(self.view_rect(pane), x, y))
    }
    pub fn view_position(&self, pane: Pane) -> Option<Position> {
        let node = self.project.nodes.get(self.view_open(pane)?)?;
        let ed = self.view_editor(pane)?;
        Some(Position {
            document: Some(
                node.path
                    .strip_prefix(&self.project.root)
                    .ok()?
                    .to_path_buf(),
            ),
            line: ed.cy,
            column: ed.cx,
            scroll: ed.scroll,
            anchor: ed.selection_anchor(),
        })
    }
    pub fn clear_view_rects(&mut self) {
        self.rect_prose = Rect::default();
        self.rect_editor = Rect::default();
        if let Some(v) = &mut self.views.other {
            v.prose = Rect::default();
            v.rect = Rect::default();
        }
    }
    /// Pure memory operation. Never save or issue system calls while rendering.
    pub(super) fn swap_editor_context(&mut self, pane: Pane) -> bool {
        if self.views.active == pane {
            return true;
        }
        let Some(other) = &mut self.views.other else {
            return false;
        };
        std::mem::swap(&mut self.open, &mut other.open);
        std::mem::swap(&mut self.editor, &mut other.editor);
        std::mem::swap(&mut self.edit_width, &mut other.width);
        std::mem::swap(&mut self.edit_height, &mut other.height);
        std::mem::swap(&mut self.rect_editor, &mut other.rect);
        std::mem::swap(&mut self.rect_prose, &mut other.prose);
        std::mem::swap(&mut self.last_caret, &mut other.caret);
        std::mem::swap(&mut self.manual_scroll, &mut other.manual_scroll);
        std::mem::swap(&mut self.last_click, &mut other.click);
        self.views.active = pane;
        true
    }
    pub fn with_editor_view<T>(&mut self, pane: Pane, f: impl FnOnce(&mut Self) -> T) -> T {
        let original = self.views.active;
        self.swap_editor_context(pane);
        let result = f(self);
        self.swap_editor_context(original);
        result
    }
    pub fn activate_editor(&mut self, pane: Pane) {
        self.flush();
        if self.swap_editor_context(pane) {
            self.focus = self.editor_focus();
        }
    }
    pub fn ensure_document_claims(&mut self) -> Result<()> {
        for pane in [Pane::Primary, Pane::Secondary] {
            let path = self
                .view_open(pane)
                .and_then(|i| self.project.nodes.get(i))
                .map(|n| (n.path.clone(), n.read_only, n.never_read()));
            let Some((path, read_only, never_read)) = path else {
                self.views.registry.release(pane);
                continue;
            };
            let relative = path.strip_prefix(&self.project.root)?;
            let was_owned = self.views.registry.path(pane).is_some();
            match self.views.registry.assign(pane, relative) {
                Ok(Assignment::Assigned) => {}
                Ok(Assignment::OwnedBy(owner)) => anyhow::bail!(
                    "document already belongs to {owner:?}; both buffers retained, saving paused"
                ),
                // A never-readable/outside read-only viewer must not prevent
                // unrelated saves. Claim its identity whenever it CAN be
                // verified, including while its bytes are still read-only.
                Err(_) if (read_only || never_read) && !was_owned => {}
                Err(e) => return Err(e),
            }
        }
        self.views.registry.validate()
    }
    pub fn update_document_editor(&mut self, i: usize) {
        self.capture_productivity();
        let body = &self.project.nodes[i].body;
        if self.open == Some(i) && self.editor.text() != *body {
            self.editor.set_text(body);
        }
        if let Some(other) = &mut self.views.other
            && other.open == Some(i)
            && other.editor.text() != *body
        {
            other.editor.set_text(body);
        }
    }
    /// Detach only after the caller has successfully committed/kept changes.
    /// F11 does NOT detach: a hidden secondary retains selection and undo.
    pub fn detach_secondary(&mut self) {
        self.flush();
        self.swap_editor_context(Pane::Primary);
        if let Some(mut other) = self.views.other.take()
            && let Some(i) = other.open
        {
            self.undo_stash.insert(
                self.project.nodes[i].path.clone(),
                other.editor.take_history(),
            );
        }
        self.views.registry.release(Pane::Secondary);
        self.views.secondary_visible = false;
        self.views.secondary_room = false;
        if self.focus == Focus::Secondary {
            self.focus = Focus::Editor;
        }
    }
    pub fn close_document_views_under(&mut self, path: &Path) {
        self.flush();
        for pane in [Pane::Primary, Pane::Secondary] {
            let affected = self
                .view_open(pane)
                .is_some_and(|i| self.project.nodes[i].path.starts_with(path));
            if affected {
                self.with_editor_view(pane, |app| {
                    if let Some(i) = app.open {
                        app.undo_stash
                            .insert(app.project.nodes[i].path.clone(), app.editor.take_history());
                    }
                    app.open = None;
                    app.editor = productivity::source_editor("");
                });
                self.views.registry.release(pane);
                if pane == Pane::Secondary {
                    self.views.secondary_visible = false;
                }
                if self.focus.pane() == Some(pane) {
                    self.focus = Focus::Tree;
                }
            }
        }
        if self.view_open(Pane::Secondary).is_none() {
            self.swap_editor_context(Pane::Primary);
        }
    }
    pub fn hide_secondary(&mut self) {
        if self.views.active == Pane::Secondary {
            self.activate_editor(Pane::Primary);
        }
        self.views.secondary_visible = false;
        self.views.secondary_room = false;
        self.save_deck();
    }
    pub fn toggle_secondary(&mut self) {
        if self.focus_mode {
            self.msg = "strict focus hides other panes · Ctrl-D leaves focus first".into();
            return;
        }
        if self.view_open(Pane::Secondary).is_none() {
            self.open_secondary_picker();
        } else if self.views.secondary_visible {
            self.hide_secondary();
        } else {
            self.views.secondary_visible = true;
            self.activate_editor(Pane::Secondary);
            self.save_deck();
        }
    }
    pub fn open_secondary_picker(&mut self) {
        self.flush();
        if let Err(e) = self.ensure_document_claims() {
            self.msg = format!("cannot change editor ownership: {e}");
            return;
        }
        let entries = palette::entries(self)
            .into_iter()
            .filter_map(|mut e| {
                let Action::Open(path) = &e.action else {
                    return None;
                };
                let relative = path.strip_prefix(&self.project.root).ok()?;
                // No duplicate owner, outside symlink or unreadable document.
                let owner = self.views.registry.owner_for(relative).ok()?;
                if owner == Some(Pane::Primary) {
                    return None;
                }
                let n = self.project.nodes.iter().find(|n| n.path == *path)?;
                if n.read_only || n.disk.unavailable.is_some() {
                    return None;
                }
                e.action = Action::EditSecondary(path.clone());
                Some(e)
            })
            .collect();
        self.overlay = Overlay::Palette {
            query: String::new(),
            sel: 0,
            entries,
        };
        self.msg =
            "Choose a scene, character, note or research document for the independent editor"
                .into();
    }
    /// Load, but do not persist/activate yet (also used by workspace restore).
    fn load_secondary_node(&mut self, i: usize) -> bool {
        let Some(node) = self.project.nodes.get(i) else {
            return false;
        };
        if node.kind != Kind::Scene || node.read_only || node.disk.unavailable.is_some() {
            self.msg = "choose a readable UTF-8 document; no secondary editor created".into();
            return false;
        }
        let path = node.path.clone();
        let Some(relative) = path
            .strip_prefix(&self.project.root)
            .ok()
            .map(Path::to_path_buf)
        else {
            return false;
        };
        if let Err(e) = self.ensure_document_claims() {
            self.msg = format!("cannot change editor ownership: {e}");
            return false;
        }
        match self.views.registry.owner_for(&relative) {
            Ok(Some(Pane::Primary)) => {
                self.activate_editor(Pane::Primary);
                self.msg = "already open in the primary editor; no second buffer created".into();
                return false;
            }
            Ok(Some(Pane::Secondary)) => return true,
            Ok(None) => {}
            Err(e) => {
                self.msg = format!("can't open second editor: {e}");
                return false;
            }
        }
        if !self.commit_saves() {
            return false;
        }
        match self.project.check_disk(i, true) {
            Ok(change @ (DiskChange::Gone | DiskChange::GoneParked(_))) => {
                self.take_disk_change(i, change);
                return false;
            }
            Ok(change) => self.take_disk_change(i, change),
            Err(e) => {
                self.msg = format!("can't check second document: {e}");
                return false;
            }
        }
        if self.project.nodes[i].read_only || self.project.nodes[i].disk.unavailable.is_some() {
            return false;
        }
        match self.views.registry.assign(Pane::Secondary, &relative) {
            Ok(Assignment::Assigned) => {}
            Ok(Assignment::OwnedBy(_)) => {
                self.msg = "document already has an editor; no duplicate created".into();
                return false;
            }
            Err(e) => {
                self.msg = format!("can't claim second document: {e}");
                return false;
            }
        }
        self.swap_editor_context(Pane::Primary);
        if let Some(mut old) = self.views.other.take()
            && let Some(j) = old.open
        {
            self.undo_stash.insert(
                self.project.nodes[j].path.clone(),
                old.editor.take_history(),
            );
        }
        let mut view = Inactive::new(i, &self.project.nodes[i].body);
        if let Some(h) = self.undo_stash.remove(&path) {
            view.editor.set_history(h);
        }
        self.views.other = Some(view);
        true
    }
    pub fn open_secondary_node(&mut self, i: usize) {
        if self.focus_mode {
            self.msg = "Ctrl-D leaves strict focus before opening another pane".into();
            return;
        }
        if self.load_secondary_node(i) {
            self.views.secondary_visible = true;
            self.overlay = Overlay::None;
            self.activate_editor(Pane::Secondary);
            self.msg = "independent editor · Tab changes pane · F11 hides without losing selection or undo".into();
            self.save_deck();
        }
    }
    pub fn restore_workspace_views(&mut self, primary: bool) {
        let w = self.deck.state.current().clone();
        let newer_resume = !primary
            && self.view_position(Pane::Primary).is_some_and(|p| {
                p.document != w.primary.document
                    || (p.line, p.column) != (w.primary.line, w.primary.column)
            });
        self.detach_secondary();
        if primary && let Some(p) = &w.primary.document {
            let path = self.project.root.join(p);
            if let Some(i) = self
                .project
                .nodes
                .iter()
                .position(|n| n.path == path && n.kind == Kind::Scene)
            {
                self.open_scene(i);
                if self.view_open(Pane::Primary) == Some(i) {
                    self.reveal(i);
                    self.restore_view_position(Pane::Primary, &w.primary);
                }
            }
        } else if !primary && let Some(pos) = self.view_position(Pane::Primary) {
            // A newer upstream resume position wins. Apply old selection/scroll
            // only when it names the exact same cursor and file.
            if pos.document == w.primary.document
                && (pos.line, pos.column) == (w.primary.line, w.primary.column)
            {
                self.restore_view_position(Pane::Primary, &w.primary);
            }
        }
        if let Some(p) = &w.secondary
            && let Some(path) = &p.document
        {
            let path = self.project.root.join(path);
            if let Some(i) = self
                .project
                .nodes
                .iter()
                .position(|n| n.path == path && n.kind == Kind::Scene)
                && self.load_secondary_node(i)
            {
                self.restore_view_position(Pane::Secondary, p);
                self.views.secondary_visible = w.secondary_visible;
            }
        }
        if w.active_editor == Pane::Secondary
            && !newer_resume
            && w.secondary_visible
            && !w.focus
            && self.view_open(Pane::Secondary).is_some()
        {
            self.activate_editor(Pane::Secondary);
        } else if newer_resume {
            self.activate_editor(Pane::Primary);
        }
    }
    pub(super) fn restore_view_position(&mut self, pane: Pane, p: &Position) {
        self.with_editor_view(pane, |app| {
            if let Some(anchor) = p.anchor {
                app.editor.select(anchor, (p.line, p.column));
            } else {
                app.editor.place(p.line, p.column);
            }
            app.editor.scroll = p.scroll;
            app.manual_scroll = true;
            app.last_caret = Some((
                app.editor.cy,
                app.editor.cx,
                app.editor.lines.len(),
                app.edit_width,
                app.edit_height,
                app.focus_mode,
            ));
        });
    }
    #[cfg(test)]
    pub fn check_documents_now(&mut self) {
        self.last_disk_check = None;
        self.last_attempt = None;
        self.reload_failed = None;
        self.sync_tick();
    }
    /// Wheel motion over a nonfocused editable pane never steals focus.
    pub fn scroll_editor_view(&mut self, pane: Pane, down: bool, step: usize) {
        self.with_editor_view(pane, |app| {
            let rows = app.editor.layout(app.edit_width);
            app.manual_scroll = true;
            app.last_caret = Some((
                app.editor.cy,
                app.editor.cx,
                app.editor.lines.len(),
                app.edit_width,
                app.edit_height,
                app.focus_mode,
            ));
            app.editor.scroll = if down {
                app.editor
                    .scroll
                    .saturating_add(step)
                    .min(rows.len().saturating_sub(1))
            } else {
                app.editor.scroll.saturating_sub(step)
            };
        });
    }
}
