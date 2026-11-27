use super::{Deck, Page};
use crate::app::{App, Focus, Overlay};
use crate::palette::{self, Action};
use anyhow::Result;
use grimoire_core::{project::Kind, tutorial, workspaces};
use std::path::Path;
use wros_sys::shortcuts::Action as ShortcutAction;

impl Deck {
    pub(super) fn save_layout(&mut self) {
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
}

impl App {
    pub(super) fn remember_deck(&mut self) {
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
    pub(crate) fn save_deck(&mut self) {
        self.remember_deck();
        if self.deck.dirty {
            self.deck.save_layout();
        }
    }
    pub(super) fn restore_deck_position(&mut self) {
        self.restore_workspace_views(true);
    }

    pub(crate) fn toggle_outline(&mut self) {
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
    pub(crate) fn save_workspace_pin_set(
        &mut self,
        slot: usize,
        ids: Option<Vec<u64>>,
    ) -> Result<()> {
        self.remember_deck();
        let mut next = self.deck.state.clone();
        next.workspaces
            .get_mut(slot)
            .ok_or_else(|| anyhow::anyhow!("Workspace missing"))?
            .pin_ids = ids;
        self.save_workspace_state(next)
    }
    pub(super) fn starter_preset_picker(&mut self, slot: usize) {
        let entries=workspaces::Preset::ALL.into_iter().map(|preset|palette::Entry {label:format!("Apply {} starter",preset.name()),key:String::new(),detail:"Explicit empty original pane note + layout; existing prose is never templated/replaced".into(),action:Action::ApplyPreset(slot,preset)}).collect();
        self.overlay = Overlay::Palette {
            query: String::new(),
            sel: 0,
            entries,
            under_bar: false,
        };
    }
    pub(crate) fn apply_starter_preset(&mut self, slot: usize, preset: workspaces::Preset) {
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
