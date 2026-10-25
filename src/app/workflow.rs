//! Cached scratchpad/handoff readers. All mutations are explicit and use the
//! original Project/editor ownership/save/recovery path; never draw-time I/O.
use super::*;
use anyhow::{Context, bail};
use grimoire_core::{
    documents::Pane,
    pins::{Life, SourceIdentity},
    workflow::{self, Store},
};
use ratatui::{
    Frame,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Source {
    pub nodes: Vec<usize>,
    pub error: Option<String>,
}
pub struct Workflow {
    pub store: Option<Store>,
    pub warning: String,
    pub selected: Option<u64>,
    pub life: Life,
    pub scroll: usize,
    width: std::cell::Cell<usize>,
    pub sources: BTreeMap<u64, Source>,
    pub handoff: Source,
    pub context_id: String,
}
impl Workflow {
    pub fn load(root: &Path) -> Self {
        let (store, warning) = match Store::load(root) {
            Ok(s) => (Some(s), String::new()),
            Err(e) => (None, format!("Workflow original preserved: {e:#}")),
        };
        let selected = store.as_ref().and_then(|s| s.state.selected);
        Self {
            store,
            warning,
            selected,
            life: Life::Active,
            scroll: 0,
            width: std::cell::Cell::new(40),
            sources: BTreeMap::new(),
            handoff: Source::default(),
            context_id: format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
        }
    }
    pub fn state(&self) -> Option<&workflow::State> {
        self.store.as_ref().map(|s| &s.state)
    }
    fn update<T>(&mut self, f: impl FnOnce(&mut workflow::State) -> Result<T>) -> Result<T> {
        let result = self
            .store
            .as_mut()
            .context("workflow metadata unavailable; r explicitly reloads; original preserved")?
            .update(f);
        match &result {
            Ok(_) => self.warning.clear(),
            Err(e) => self.warning = format!("Workflow not saved: {e:#}"),
        };
        result
    }
}
impl App {
    fn workflow_cache(
        &self,
        relative: &Path,
        role: &str,
        identities: &[(usize, SourceIdentity)],
    ) -> Source {
        let result = (|| -> Result<Vec<usize>> {
            workflow::source_path(&self.project.root, relative, role)?;
            let id = grimoire_core::pins::identify(&self.project.root, relative)?;
            let nodes: Vec<usize> = identities
                .iter()
                .filter(|(_, other)| id.same_file(other))
                .map(|(i, _)| *i)
                .collect();
            if nodes.is_empty() {
                bail!("Original not loaded/missing; restore its path, never a competing copy");
            }
            if nodes
                .iter()
                .any(|&i| self.project.nodes[i].in_manuscript != (role == "manuscript"))
            {
                bail!("Original aliases a different document role; repair aliases first");
            }
            Ok(nodes)
        })();
        match result {
            Ok(nodes) => Source { nodes, error: None },
            Err(e) => Source {
                nodes: vec![],
                error: Some(format!("Original unavailable: {e:#}")),
            },
        }
    }
    pub fn refresh_workflow_sources(&mut self) {
        let identities: Vec<_> = self
            .project
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Scene)
            .filter_map(|(i, n)| {
                n.path
                    .strip_prefix(&self.project.root)
                    .ok()
                    .and_then(|p| grimoire_core::pins::identify(&self.project.root, p).ok())
                    .map(|id| (i, id))
            })
            .collect();
        let sources = self
            .workflow
            .state()
            .map(|s| {
                s.notes
                    .iter()
                    .map(|n| (n.id, self.workflow_cache(&n.source, "notes", &identities)))
                    .collect()
            })
            .unwrap_or_default();
        self.workflow.sources = sources;
        self.workflow.handoff =
            self.workflow_cache(Path::new(workflow::HANDOFF), "notes", &identities);
    }
    fn workflow_node(&self, source: &Source) -> Result<usize> {
        if let Some(e) = &source.error {
            bail!("{e}");
        }
        if let (Some(a), Some(b)) = (
            self.view_open(Pane::Primary),
            self.view_open(Pane::Secondary),
        ) && a != b
            && source.nodes.contains(&a)
            && source.nodes.contains(&b)
        {
            bail!("Two retained editor aliases; no version guessed; repair aliases before editing");
        }
        let i = [Pane::Primary, Pane::Secondary]
            .into_iter()
            .filter_map(|p| self.view_open(p))
            .find(|i| source.nodes.contains(i))
            .or_else(|| source.nodes.first().copied())
            .context("Original unavailable")?;
        let n = &self.project.nodes[i];
        if n.read_only || n.parked || n.clash || n.disk.unavailable.is_some() {
            bail!("Original is non-UTF-8/unsettled/conflicted; no editable copy invented");
        }
        Ok(i)
    }
    pub fn scratch_ids(&self) -> Vec<u64> {
        self.workflow
            .state()
            .map(|s| {
                s.notes
                    .iter()
                    .filter(|n| n.life == self.workflow.life)
                    .map(|n| n.id)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn scratch_body(&self, id: u64) -> Result<&str> {
        let source = self
            .workflow
            .sources
            .get(&id)
            .context("Original unavailable")?;
        let i = self.workflow_node(source)?;
        Ok(&self.project.nodes[i].body)
    }
    pub fn handoff_body(&self) -> Result<&str> {
        let i = self.workflow_node(&self.workflow.handoff)?;
        Ok(&self.project.nodes[i].body)
    }
    pub(super) fn workflow_prepare(&mut self) -> Result<()> {
        self.flush();
        if let Err(e) = self.ensure_document_claims() {
            self.rescue();
            bail!("{e}; all buffers retained; recovery attempted");
        }
        if !self.commit_saves() || self.project.dirty_count() != 0 {
            bail!("Save both source buffers with Ctrl-S first; Waiting/failed sources retained");
        }
        Ok(())
    }
    fn workflow_focus(&mut self, i: usize, primary: bool) -> Result<()> {
        if self.focus_mode && !primary {
            bail!("Esc then Ctrl-D leaves strict focus; e explicitly uses Primary");
        }
        // The registry, not a copied preview, determines the existing owner.
        for pane in [Pane::Primary, Pane::Secondary] {
            if self.view_open(pane) == Some(i) {
                if pane == Pane::Secondary && self.focus_mode {
                    bail!("Strict focus hides the original Secondary; leave focus before editing");
                }
                if pane == Pane::Secondary {
                    self.views.secondary_visible = true;
                }
                self.activate_editor(pane);
                self.overlay = Overlay::None;
                return Ok(());
            }
        }
        if primary {
            self.open_scene(i);
            if self.view_open(Pane::Primary) == Some(i) {
                self.overlay = Overlay::None;
            }
        } else {
            self.open_secondary_node(i);
        }
        Ok(())
    }
    pub fn open_scratchpad(&mut self) {
        self.flush();
        self.tree_drag = None;
        self.refresh_workflow_sources();
        self.overlay = Overlay::Scratchpad;
        let ids = self.scratch_ids();
        if !self.workflow.selected.is_some_and(|id| ids.contains(&id)) {
            self.workflow.selected = ids.first().copied();
        }
        self.workflow.scroll = 0;
        self.msg="Scratchpad · original Notes sources · n new · l link scene · p promote saved snapshot; original always kept".into();
    }
    pub fn new_scratchpad(&mut self, primary: bool) {
        self.overlay = Overlay::Scratchpad;
        let mut touched = false;
        let result = (|| -> Result<u64> {
            if self.focus_mode && !primary {
                bail!("Strict focus: N creates in Primary; leave focus for Secondary");
            }
            self.workflow_prepare()?;
            let store = self
                .workflow
                .store
                .as_mut()
                .context("Workflow metadata unavailable; r reloads without replacing it")?;
            touched = true;
            let id = store.create_note()?;
            Ok(id)
        })();
        // A failed multi-file creation can still leave an honest reserved reference.
        if touched {
            let _ = self.reload_tree();
        }
        self.refresh_workflow_sources();
        match result {
            Ok(id) => {
                self.workflow.selected = Some(id);
                self.workflow.life = Life::Active;
                self.edit_scratchpad(primary);
            }
            Err(e) => self.msg = format!("Scratchpad not created: {e:#}; originals kept"),
        }
    }
    pub fn edit_scratchpad(&mut self, primary: bool) {
        self.overlay = Overlay::Scratchpad;
        let result = (|| -> Result<()> {
            self.workflow_prepare()?;
            self.refresh_workflow_sources();
            let id = self
                .workflow
                .selected
                .context("n creates a new original note")?;
            let source = self
                .workflow
                .sources
                .get(&id)
                .context("Original missing; never recreated from a preview")?;
            let i = self.workflow_node(source)?;
            self.workflow_focus(i, primary)
        })();
        if let Err(e) = result {
            self.msg = format!("Original not opened: {e:#}");
        }
    }
    pub fn scratch_link_picker(&mut self) {
        let Some(id) = self.workflow.selected else {
            return;
        };
        let entries = self
            .project
            .nodes
            .iter()
            .filter(|n| {
                n.kind == Kind::Scene
                    && n.in_manuscript
                    && !n.read_only
                    && !n.parked
                    && !n.clash
                    && n.disk.unavailable.is_none()
            })
            .map(|n| palette::Entry {
                label: format!(
                    "{} · {}",
                    n.title,
                    n.path
                        .strip_prefix(&self.project.root)
                        .unwrap_or(&n.path)
                        .display()
                ),
                key: String::new(),
                detail: "Link scene; source not moved/copied".into(),
                action: Action::LinkScratchpad(id, n.path.clone()),
            })
            .collect();
        self.overlay = Overlay::Palette {
            query: String::new(),
            sel: 0,
            entries,
        };
    }
    pub fn link_scratchpad(&mut self, id: u64, path: &Path) {
        self.overlay = Overlay::Scratchpad;
        let result = (|| -> Result<()> {
            self.workflow_prepare()?;
            let relative = path.strip_prefix(&self.project.root)?.to_path_buf();
            workflow::source_path(&self.project.root, &relative, "manuscript")?;
            let n = self
                .project
                .nodes
                .iter()
                .find(|n| n.path == path && n.in_manuscript && n.kind == Kind::Scene)
                .context("Manuscript scene not loaded")?;
            if n.read_only || n.parked || n.clash || n.disk.unavailable.is_some() {
                bail!("Scene unavailable/unsettled");
            }
            self.workflow.update(|s| {
                s.note_mut(id)?.scene = Some(relative);
                s.selected = Some(id);
                Ok(())
            })
        })();
        self.msg = match result {
            Ok(()) => "Scene link saved; both originals untouched".into(),
            Err(e) => format!("Scene link not saved: {e:#}"),
        };
    }
    pub fn promote_scratchpad(&mut self) {
        self.overlay = Overlay::Scratchpad;
        let mut touched = false;
        let mut fresh = false;
        let result = (|| -> Result<PathBuf> {
            self.workflow_prepare()?;
            self.refresh_workflow_sources();
            let id = self
                .workflow
                .selected
                .context("Choose a scratchpad first")?;
            let i = self.workflow_node(
                self.workflow
                    .sources
                    .get(&id)
                    .context("Original unavailable")?,
            )?;
            let seen = self.project.nodes[i]
                .seen
                .context("No saved original fingerprint")?;
            let store = self
                .workflow
                .store
                .as_mut()
                .context("Workflow metadata unavailable")?;
            fresh = store
                .state
                .note(id)?
                .promotion
                .as_ref()
                .is_none_or(|p| p.phase != workflow::Phase::Ready);
            touched = true;
            store.promote(id, seen)
        })();
        // Reload after either outcome: a prepared receipt may own an already-created
        // scene. Never delete that scene or guess at a competing source version.
        if touched {
            let _ = self.reload_tree();
        }
        self.refresh_workflow_sources();
        match result {
            Ok(path) => {
                self.msg = format!(
                    "Independent scene snapshot: {}; original scratchpad kept",
                    path.strip_prefix(&self.project.root)
                        .unwrap_or(&path)
                        .display()
                );
                if fresh {
                    self.observe_deck(grimoire_core::tutorial::Event::ScratchpadPromoted);
                }
            }
            Err(e) => {
                self.msg =
                    format!("Promotion incomplete: {e:#}; original and any prepared target kept")
            }
        }
    }
    pub(super) fn workflow_original_node(&self, relative: &Path, role: &str) -> Result<usize> {
        let ids: Vec<_> = self
            .project
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Scene)
            .filter_map(|(i, n)| {
                n.path
                    .strip_prefix(&self.project.root)
                    .ok()
                    .and_then(|p| grimoire_core::pins::identify(&self.project.root, p).ok())
                    .map(|id| (i, id))
            })
            .collect();
        let source = self.workflow_cache(relative, role, &ids);
        self.workflow_node(&source)
    }
    fn workflow_open_path(&mut self, relative: &Path, role: &str) -> Result<()> {
        self.workflow_prepare()?;
        let i = self.workflow_original_node(relative, role)?;
        self.workflow_focus(i, true)
    }
    pub fn open_handoff(&mut self) {
        self.flush();
        self.tree_drag = None;
        self.refresh_workflow_sources();
        self.workflow.scroll = 0;
        self.overlay = Overlay::Handoff;
        self.msg="Original next-session intention · Enter/e edits · s saves/arms next-open display · g resume context · o toggle startup".into();
    }
    pub fn edit_handoff(&mut self, primary: bool) {
        self.overlay = Overlay::Handoff;
        let result = (|| -> Result<()> {
            if self.focus_mode && !primary {
                bail!("Strict focus: e uses Primary; leave focus for Secondary");
            }
            self.workflow_prepare()?;
            workflow::ensure_handoff(&self.project.root)?;
            self.reload_tree()?;
            self.refresh_workflow_sources();
            let i = self.workflow_node(&self.workflow.handoff)?;
            self.workflow_focus(i, primary)
        })();
        if let Err(e) = result {
            self.msg = format!("Handoff original not opened: {e:#}");
        }
    }
    pub fn leave_handoff(&mut self) {
        self.overlay = Overlay::Handoff;
        let result = (|| -> Result<()> {
            self.workflow_prepare()?;
            self.refresh_workflow_sources();
            let i = self.workflow_node(&self.workflow.handoff)?;
            let seen = self.project.nodes[i]
                .seen
                .context("No saved handoff fingerprint")?;
            let position = [Pane::Primary, Pane::Secondary]
                .into_iter()
                .find(|&p| {
                    self.view_open(p)
                        .is_some_and(|i| self.project.nodes[i].in_manuscript)
                })
                .and_then(|p| self.view_position(p));
            let context_id = self.workflow.context_id.clone();
            self.workflow
                .store
                .as_mut()
                .context("Workflow metadata unavailable")?
                .leave_handoff(
                    seen,
                    chrono::Local::now().fixed_offset().to_rfc3339(),
                    context_id,
                    position,
                )
        })();
        self.msg=match result {Ok(())=>"Original intention saved and armed for next book open; no session duration or automatic prose invented".into(),Err(e)=>format!("Handoff not armed: {e:#}; original prose kept")};
    }
    pub fn apply_handoff_startup(&mut self) {
        let ready = self
            .workflow
            .state()
            .filter(|s| s.handoff.show_on_open)
            .and_then(|s| s.handoff.intention.as_ref())
            .filter(|h| h.context_id != self.workflow.context_id)
            .cloned();
        if let Some(h) = ready
            && (self.overlay == Overlay::None
                || matches!(self.overlay, Overlay::Deck(writingdeck::Page::Home { .. })))
        {
            self.open_handoff();
            if self
                .workflow_node(&self.workflow.handoff)
                .ok()
                .and_then(|i| self.project.nodes[i].seen)
                .is_some_and(|f| workflow::Stamp::from(f) == h.source_stamp)
            {
                self.observe_deck(grimoire_core::tutorial::Event::HandoffReopened);
            }
        }
    }
    pub fn reload_workflow(&mut self) {
        let overlay = self.overlay.clone();
        match self
            .workflow_prepare()
            .and_then(|()| Store::load(&self.project.root))
        {
            Ok(store) => {
                self.workflow.selected = store.state.selected;
                self.workflow.store = Some(store);
                self.workflow.warning.clear();
                self.msg="Workflow reloaded explicitly; originals and prepared promotion targets retained".into();
            }
            Err(e) => self.msg = format!("Workflow not reloaded: {e:#}; original retained"),
        };
        self.refresh_workflow_sources();
        self.overlay = overlay;
    }
    pub fn on_workflow_key(&mut self, key: Key) {
        let handoff = self.overlay == Overlay::Handoff;
        match key {
            Key::Esc => self.overlay = Overlay::None,
            Key::Enter => {
                if handoff {
                    self.edit_handoff(false)
                } else {
                    self.edit_scratchpad(false)
                }
            }
            Key::Char('e') => {
                if handoff {
                    self.edit_handoff(true)
                } else {
                    self.edit_scratchpad(true)
                }
            }
            Key::Char('r') => self.reload_workflow(),
            Key::Char('s') if handoff => self.leave_handoff(),
            Key::Char('o') if handoff => {
                let result = self.workflow.update(|s| {
                    s.handoff.show_on_open = !s.handoff.show_on_open;
                    Ok(s.handoff.show_on_open)
                });
                self.msg = match result {
                    Ok(v) => format!("Show original handoff on next open: {v}"),
                    Err(e) => format!("Setting not saved: {e:#}"),
                };
            }
            Key::Char('g') if handoff => {
                let pos = self
                    .workflow
                    .state()
                    .and_then(|s| s.handoff.intention.as_ref())
                    .and_then(|h| h.position.clone());
                let result = (|| -> Result<()> {
                    let p = pos.context(
                        "No saved manuscript context; intention prose remains available",
                    )?;
                    let relative = p.document.as_deref().context("No context document")?;
                    self.workflow_open_path(relative, "manuscript")?;
                    let pane = self.views.active;
                    self.restore_view_position(pane, &p);
                    self.with_editor_view(pane, |app| {
                        app.editor.scroll = app
                            .editor
                            .scroll
                            .min(app.editor.layout(app.edit_width).len().saturating_sub(1));
                    });
                    Ok(())
                })();
                if let Err(e) = result {
                    self.msg = format!("Context not resumed: {e:#}");
                }
            }
            Key::Char('n' | 'N') if !handoff => self.new_scratchpad(key == Key::Char('N')),
            Key::Char('l') if !handoff => self.scratch_link_picker(),
            Key::Char('v') if !handoff => {
                let path = self.workflow.selected.and_then(|id| {
                    self.workflow
                        .state()?
                        .note(id)
                        .ok()?
                        .promotion
                        .as_ref()
                        .map(|p| p.target.clone())
                });
                if let Some(path) = path
                    && let Err(e) = self.workflow_open_path(&path, "manuscript")
                {
                    self.msg = format!("Promoted original not opened: {e:#}");
                }
            }
            Key::Char('x') if !handoff => {
                let result = (|| -> Result<()> {
                    let id = self.workflow.selected.context("Choose a note")?;
                    self.workflow
                        .store
                        .as_mut()
                        .context("Workflow unavailable")?
                        .cancel_prepared(id)
                })();
                self.msg = match result {
                    Ok(()) => "Unwritten preparation cancelled; no file deleted or replaced".into(),
                    Err(e) => format!("Preparation not cancelled: {e:#}"),
                };
            }
            Key::Char('p') if !handoff => self.promote_scratchpad(),
            Key::Char('g') if !handoff => {
                let relative = self
                    .workflow
                    .selected
                    .and_then(|id| self.workflow.state()?.note(id).ok()?.scene.clone());
                if let Some(path) = relative
                    && let Err(e) = self.workflow_open_path(&path, "manuscript")
                {
                    self.msg = format!("Linked original not opened: {e:#}");
                }
            }
            Key::Char('a') | Key::Delete if !handoff => {
                if let Some(id) = self.workflow.selected {
                    let life = if key == Key::Delete {
                        Life::Trashed
                    } else if self.workflow.life == Life::Active {
                        Life::Archived
                    } else {
                        Life::Active
                    };
                    let result = self.workflow.update(|s| {
                        s.note_mut(id)?.life = life;
                        Ok(())
                    });
                    self.msg = match result {
                        Ok(()) => {
                            "Reference archived/trashed/restored; original Markdown never deleted"
                                .into()
                        }
                        Err(e) => format!("Lifecycle not saved: {e:#}"),
                    };
                    self.workflow.selected = self.scratch_ids().first().copied();
                }
            }
            Key::Tab if !handoff => {
                self.workflow.life = match self.workflow.life {
                    Life::Active => Life::Archived,
                    Life::Archived => Life::Trashed,
                    Life::Trashed => Life::Active,
                };
                self.workflow.selected = self.scratch_ids().first().copied();
                self.workflow.scroll = 0;
            }
            Key::Up | Key::Down if !handoff => {
                let ids = self.scratch_ids();
                if !ids.is_empty() {
                    let index = ids
                        .iter()
                        .position(|id| Some(*id) == self.workflow.selected)
                        .unwrap_or(0);
                    let next = if key == Key::Up {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(ids.len() - 1)
                    };
                    self.workflow.selected = Some(ids[next]);
                    self.workflow.scroll = 0;
                }
            }
            Key::Home => self.workflow.scroll = 0,
            Key::PageUp => self.workflow.scroll = self.workflow.scroll.saturating_sub(10),
            Key::PageDown | Key::End | Key::Down
                if handoff || matches!(key, Key::PageDown | Key::End) =>
            {
                let body = if handoff {
                    self.handoff_body().ok()
                } else {
                    self.workflow
                        .selected
                        .and_then(|id| self.scratch_body(id).ok())
                };
                let max = body
                    .map(|b| {
                        grimoire_core::pins::excerpt(
                            b,
                            self.workflow.width.get().max(1),
                            None,
                            0,
                            0,
                        )
                        .total_rows
                        .saturating_sub(1)
                    })
                    .unwrap_or(0);
                self.workflow.scroll = if key == Key::End {
                    max
                } else {
                    self.workflow
                        .scroll
                        .saturating_add(if key == Key::Down { 1 } else { 10 })
                        .min(max)
                };
            }
            Key::Up if handoff => self.workflow.scroll = self.workflow.scroll.saturating_sub(1),
            _ => {}
        }
    }
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    f.render_widget(Clear, area);
    let handoff = app.overlay == Overlay::Handoff;
    let block = Block::bordered()
        .title(if handoff {
            " Session handoff · original intention "
        } else {
            " Scratchpad · original Notes "
        })
        .border_style(Style::default().fg(t.accent));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 8 || inner.height < 5 {
        return;
    }
    let mut top = Vec::new();
    if app.workflow.state().is_none() {
        top.push(Line::from(
            "Workflow metadata unavailable; originals preserved, not assumed absent.",
        ));
    }
    if handoff {
        if let Some(state) = app.workflow.state() {
            top.push(Line::from(format!(
                "Next-open display: {} · no generated prose/time metrics",
                state.handoff.show_on_open
            )));
            if let Some(h) = &state.handoff.intention {
                top.push(Line::from(format!("Left {}", h.left_at)));
                top.push(Line::from(format!(
                    "Context: {}",
                    h.position
                        .as_ref()
                        .and_then(|p| p.document.as_ref())
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "not captured".into())
                )));
            }
        }
    } else {
        top.push(Line::from(format!(
            "{:?} references · originals stay in notes/scratchpad",
            app.workflow.life
        )));
        let ids = app.scratch_ids();
        let chosen = ids
            .iter()
            .position(|id| Some(*id) == app.workflow.selected)
            .unwrap_or(0);
        let available = (inner.height as usize / 3).clamp(1, 8);
        let start = chosen.saturating_sub(available - 1);
        for id in ids.iter().skip(start).take(available) {
            let title = app
                .workflow
                .sources
                .get(id)
                .and_then(|s| app.workflow_node(s).ok())
                .map(|i| app.project.nodes[i].title.as_str())
                .unwrap_or("original unavailable");
            top.push(Line::from(format!(
                "{} #{id} {title}",
                if Some(*id) == app.workflow.selected {
                    "›"
                } else {
                    " "
                }
            )));
        }
        if ids.is_empty() && app.workflow.state().is_some() {
            top.push(Line::from(
                "No references in this view; n explicitly creates an original note.",
            ));
        }
        if let Some(n) = app
            .workflow
            .selected
            .and_then(|id| app.workflow.state()?.note(id).ok())
        {
            top.push(Line::from(format!(
                "Scene: {}",
                n.scene
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not linked (l)".into())
            )));
            top.push(Line::from(format!(
                "Promotion: {}",
                n.promotion
                    .as_ref()
                    .map(|p| format!("{:?} → {}", p.phase, p.target.display()))
                    .unwrap_or_else(|| "not requested".into())
            )));
        }
    }
    let top_height = (top.len() as u16).min(inner.height.saturating_sub(3));
    f.render_widget(
        Paragraph::new(top).style(Style::default().fg(t.dim)),
        Rect::new(inner.x, inner.y, inner.width, top_height),
    );
    let content = Rect::new(
        inner.x,
        inner.y + top_height,
        inner.width,
        inner.height.saturating_sub(top_height + 2),
    );
    app.workflow.width.set(content.width as usize);
    let body = if handoff {
        app.handoff_body()
    } else if app.workflow.state().is_none() {
        Err(anyhow::anyhow!(
            "r explicitly reloads metadata; no missing source guessed or recreated"
        ))
    } else {
        app.workflow
            .selected
            .context("n creates an original note")
            .and_then(|id| app.scratch_body(id))
    };
    let rows: Vec<Line> = match body {
        Ok(b) if b.trim().is_empty() => vec![Line::from(
            "Empty original; Enter/e explicitly edits. This reader is not a writer.",
        )],
        Ok(b) => {
            let excerpt = grimoire_core::pins::excerpt(
                b,
                content.width as usize,
                None,
                app.workflow.scroll,
                content.height as usize,
            );
            excerpt.lines.into_iter().map(Line::from).collect()
        }
        Err(e) => vec![Line::from(format!("{e:#}"))],
    };
    f.render_widget(
        Paragraph::new(rows).style(Style::default().fg(t.text)),
        content,
    );
    let note = if app.msg.contains("Q again to quit anyway") || app.workflow.warning.is_empty() {
        app.msg.clone()
    } else {
        app.workflow.warning.clone()
    };
    f.render_widget(
        Paragraph::new(note).style(Style::default().fg(t.accent)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
    f.render_widget(
        Paragraph::new(if handoff {
            "Enter/e edit  s arm next open  g resume  o startup  r reload  Esc"
        } else {
            "↑↓ note  n/N new  Enter/e edit  l link  p promote  Tab archive  r reload  Esc"
        })
        .style(Style::default().fg(t.dim)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
