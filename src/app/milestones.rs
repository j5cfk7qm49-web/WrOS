//! Named draft milestones and reversible scene variants TUI workflow.
//! Integrates with existing editor undo and `history::diff` without replacing
//! per-scene dated history.
use super::*;
use anyhow::{Result, ensure};
use grimoire_core::{
    milestones::{MilestoneStore, VariantStore},
    statefile::Store,
};
use ratatui::{
    Frame,
    style::Style,
    widgets::{Clear, Paragraph, Wrap},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Milestones,
    Variants,
    NewMilestone,
    NewVariant,
    DiffMilestone,
    DiffVariant,
}

#[derive(Debug, Clone, Default)]
pub struct MilestoneDraft {
    pub field: usize, // 0: name, 1: description
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct VariantDraft {
    pub name: String,
}

pub struct Milestones {
    pub store: Option<Store<MilestoneStore>>,
    pub variants: Option<Store<VariantStore>>,
    pub nav: wros_shell::Navigation<Page>,
    pub notice: String,
    pub scroll: u16,
    pub milestone_draft: Option<MilestoneDraft>,
    pub variant_draft: Option<VariantDraft>,
    comparison: Option<(u64, (u64, u64))>,
    diff: Vec<grimoire_core::history::Piece>,
    max_scroll: std::cell::Cell<u16>,
}

impl Milestones {
    pub fn load(root: &Path) -> Self {
        let (store, m_err) = match MilestoneStore::load(root) {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(format!("Milestones unavailable: {e:#}"))),
        };
        let (variants, v_err) = match VariantStore::load(root) {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(format!("Variants unavailable: {e:#}"))),
        };
        let notice = m_err.or(v_err).unwrap_or_else(|| {
            "1 Milestones  2 Scene Variants · n creates snapshot/variant, Enter diffs/restores."
                .into()
        });
        Self {
            store,
            variants,
            nav: wros_shell::Navigation::new(Page::Milestones),
            notice,
            scroll: 0,
            milestone_draft: None,
            variant_draft: None,
            comparison: None,
            diff: Vec::new(),
            max_scroll: std::cell::Cell::new(0),
        }
    }

    pub fn typing(&self) -> bool {
        matches!(self.nav.current(), Page::NewMilestone | Page::NewVariant)
    }

    fn store_mut(&mut self) -> Result<&mut Store<MilestoneStore>> {
        self.store
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("milestone store unavailable; original preserved"))
    }

    fn variants_mut(&mut self) -> Result<&mut Store<VariantStore>> {
        self.variants
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("variant store unavailable; original preserved"))
    }
}

fn plain(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

impl App {
    pub fn open_milestones(&mut self) {
        self.overlay = Overlay::Milestones;
        self.milestones.nav.reset(Page::Milestones);
        self.milestones.milestone_draft = None;
        self.milestones.variant_draft = None;
        self.milestones.scroll = 0;
        self.milestones.comparison = None;
    }

    fn prepare_story_comparison(&mut self) -> Result<()> {
        let id = self.active_story_document_id()?;
        self.milestones.store_mut()?.verify()?;
        self.milestones.variants_mut()?.verify()?;
        self.milestones.comparison = Some((id, self.editor.revision_stamp()));
        let target = if self.milestones.nav.current() == Page::Milestones {
            self.milestones
                .store
                .as_ref()
                .and_then(|s| s.value.milestones.get(self.milestones.nav.selected()))
                .and_then(|m| m.scene_snapshots.get(&id))
                .cloned()
        } else {
            self.milestones.variants.as_ref().and_then(|s| {
                s.value
                    .variants_for_scene(id)
                    .get(self.milestones.nav.selected())
                    .map(|v| v.content.clone())
            })
        }
        .ok_or_else(|| anyhow::anyhow!("selected snapshot has no version for this document"))?;
        self.milestones.diff = grimoire_core::history::diff(&target, &self.editor.text());
        Ok(())
    }
    fn verify_story_comparison(&mut self) -> Result<u64> {
        let id = self.active_story_document_id()?;
        ensure!(
            self.milestones.comparison == Some((id, self.editor.revision_stamp())),
            "source revision changed; reopen the comparison before restoring"
        );
        self.milestones.store_mut()?.verify()?;
        self.milestones.variants_mut()?.verify()?;
        Ok(id)
    }

    pub fn paste_milestones(&mut self, text: &str) {
        let clean: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        match self.milestones.nav.current() {
            Page::NewMilestone => {
                if let Some(d) = &mut self.milestones.milestone_draft {
                    let (target, limit) = if d.field == 0 {
                        (&mut d.name, 256)
                    } else {
                        (&mut d.description, 4096)
                    };
                    if target.len().saturating_add(clean.len()) <= limit {
                        target.push_str(&clean);
                    }
                }
            }
            Page::NewVariant => {
                if let Some(d) = &mut self.milestones.variant_draft
                    && d.name.len().saturating_add(clean.len()) <= 256
                {
                    d.name.push_str(&clean);
                }
            }
            _ => {}
        }
    }

    fn save_new_milestone(&mut self) -> Result<()> {
        self.flush();
        let d = self
            .milestones
            .milestone_draft
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no milestone draft"))?;
        ensure!(!d.name.trim().is_empty(), "milestone name is required");
        let mut scenes = HashMap::new();
        let indices: Vec<_> = self
            .project
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                n.in_manuscript && n.compile && n.kind == Kind::Scene && !self.project.in_trash(*i)
            })
            .map(|(i, _)| i)
            .collect();
        self.project.ensure_whole()?;
        for i in indices {
            let id = self.story_document_id(i)?;
            scenes.insert(id, self.project.nodes[i].body.clone());
        }
        let id = self
            .milestones
            .store_mut()?
            .update(|s| s.create_milestone(d.name.trim(), d.description.trim(), scenes))?;
        self.milestones.milestone_draft = None;
        self.milestones.nav.reset(Page::Milestones);
        self.milestones.notice = format!("Saved named milestone #{id}.");
        self.observe_deck(grimoire_core::tutorial::Event::HistoryOpened);
        Ok(())
    }

    fn save_new_variant(&mut self) -> Result<()> {
        self.flush();
        let scene_id = self.active_story_document_id()?;
        let d = self
            .milestones
            .variant_draft
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no variant draft"))?;
        ensure!(!d.name.trim().is_empty(), "variant name is required");
        let body = self.editor.text();
        let id = self
            .milestones
            .variants_mut()?
            .update(|s| s.save_variant(scene_id, d.name.trim(), body))?;
        self.milestones.variant_draft = None;
        self.milestones.nav.reset(Page::Variants);
        self.milestones.notice = format!("Saved scene variant #{id}; reversible at any time.");
        self.observe_deck(grimoire_core::tutorial::Event::HistoryOpened);
        Ok(())
    }

    fn restore_selected_milestone_scene(&mut self) -> Result<()> {
        let scene_id = self.verify_story_comparison()?;
        let sel = self.milestones.nav.selected();
        let snapshot = self
            .milestones
            .store
            .as_ref()
            .and_then(|s| s.value.milestones.get(sel))
            .and_then(|m| m.scene_snapshots.get(&scene_id).cloned())
            .ok_or_else(|| anyhow::anyhow!("this milestone has no snapshot for the open scene"))?;
        let current = self.editor.text();
        self.milestones.store_mut()?.verify()?;
        self.milestones.variants_mut()?.update(|v| {
            v.save_variant(scene_id, "Before milestone restore", current)?;
            Ok(())
        })?;
        self.snapshot_before_story_restore()?;
        self.editor.set_text(&snapshot);
        self.flush();
        self.overlay = Overlay::None;
        self.focus = self.editor_focus();
        self.msg =
            "Restored scene from milestone (previous version saved in Variants & Undo).".into();
        Ok(())
    }

    fn restore_selected_variant(&mut self) -> Result<()> {
        let scene_id = self.verify_story_comparison()?;
        let sel = self.milestones.nav.selected();
        let variant = self
            .milestones
            .variants
            .as_ref()
            .map(|s| s.value.variants_for_scene(scene_id))
            .and_then(|list| list.get(sel).copied().cloned())
            .ok_or_else(|| anyhow::anyhow!("select a scene variant first"))?;
        let current = self.editor.text();
        if current != variant.content {
            self.milestones.variants_mut()?.update(|v| {
                v.save_variant(scene_id, "Before variant switch", current)?;
                Ok(())
            })?;
        }
        self.snapshot_before_story_restore()?;
        self.editor.set_text(&variant.content);
        self.flush();
        self.overlay = Overlay::None;
        self.focus = self.editor_focus();
        self.msg = format!(
            "Switched to variant '{}' · Ctrl-Z or Variants reverts it.",
            variant.name
        );
        Ok(())
    }

    pub fn on_milestones_key(&mut self, key: Key) {
        let page = self.milestones.nav.current();
        if key == Key::Esc {
            self.milestones.milestone_draft = None;
            self.milestones.variant_draft = None;
            self.milestones.scroll = 0;
            if !self.milestones.nav.back() {
                self.overlay = Overlay::None;
            }
            return;
        }
        let result: Result<()> = (|| match (page, key) {
            (Page::Milestones | Page::Variants, Key::Char('1')) => {
                self.milestones.nav.reset(Page::Milestones);
                Ok(())
            }
            (Page::Milestones | Page::Variants, Key::Char('2')) => {
                self.milestones.nav.reset(Page::Variants);
                Ok(())
            }
            (Page::Milestones, Key::Down | Key::Char('j')) => {
                let len = self
                    .milestones
                    .store
                    .as_ref()
                    .map_or(0, |s| s.value.milestones.len());
                self.milestones.nav.move_by(1, len);
                Ok(())
            }
            (Page::Milestones, Key::Up | Key::Char('k')) => {
                let len = self
                    .milestones
                    .store
                    .as_ref()
                    .map_or(0, |s| s.value.milestones.len());
                self.milestones.nav.move_by(-1, len);
                Ok(())
            }
            (Page::Variants, Key::Down | Key::Char('j')) => {
                let len = self
                    .current_story_document_id()
                    .and_then(|id| {
                        self.milestones
                            .variants
                            .as_ref()
                            .map(|s| s.value.variants_for_scene(id).len())
                    })
                    .unwrap_or(0);
                self.milestones.nav.move_by(1, len);
                Ok(())
            }
            (Page::Variants, Key::Up | Key::Char('k')) => {
                let len = self
                    .current_story_document_id()
                    .and_then(|id| {
                        self.milestones
                            .variants
                            .as_ref()
                            .map(|s| s.value.variants_for_scene(id).len())
                    })
                    .unwrap_or(0);
                self.milestones.nav.move_by(-1, len);
                Ok(())
            }
            (Page::Milestones, Key::Char('n')) => {
                self.milestones.milestone_draft = Some(MilestoneDraft::default());
                self.milestones.nav.open(Page::NewMilestone);
                Ok(())
            }
            (Page::Variants, Key::Char('n')) => {
                ensure!(self.open.is_some(), "open a scene in the editor first");
                self.milestones.variant_draft = Some(VariantDraft::default());
                self.milestones.nav.open(Page::NewVariant);
                Ok(())
            }
            (Page::Milestones, Key::Enter) => {
                let has = self
                    .milestones
                    .store
                    .as_ref()
                    .is_some_and(|s| !s.value.milestones.is_empty());
                ensure!(has, "no milestones saved yet; press n to create one");
                self.prepare_story_comparison()?;
                self.milestones.scroll = 0;
                self.milestones.nav.open(Page::DiffMilestone);
                self.observe_deck(grimoire_core::tutorial::Event::HistoryOpened);
                Ok(())
            }
            (Page::Variants, Key::Enter) => {
                let has = self
                    .current_story_document_id()
                    .and_then(|id| {
                        self.milestones
                            .variants
                            .as_ref()
                            .map(|s| !s.value.variants_for_scene(id).is_empty())
                    })
                    .unwrap_or(false);
                ensure!(has, "no variants for this scene yet; press n to save one");
                self.prepare_story_comparison()?;
                self.milestones.scroll = 0;
                self.milestones.nav.open(Page::DiffVariant);
                self.observe_deck(grimoire_core::tutorial::Event::HistoryOpened);
                Ok(())
            }
            (Page::DiffMilestone, Key::Char('r')) => self.restore_selected_milestone_scene(),
            (Page::DiffVariant, Key::Char('r')) => self.restore_selected_variant(),
            (Page::DiffMilestone | Page::DiffVariant, Key::PageDown) => {
                self.milestones.scroll = self
                    .milestones
                    .scroll
                    .saturating_add(8)
                    .min(self.milestones.max_scroll.get());
                Ok(())
            }
            (Page::DiffMilestone | Page::DiffVariant, Key::PageUp) => {
                self.milestones.scroll = self.milestones.scroll.saturating_sub(8);
                Ok(())
            }
            (Page::NewMilestone, Key::Tab | Key::Down | Key::Up | Key::BackTab) => {
                if let Some(d) = &mut self.milestones.milestone_draft {
                    d.field = 1 - d.field;
                }
                Ok(())
            }
            (Page::NewMilestone, Key::Backspace) => {
                if let Some(d) = &mut self.milestones.milestone_draft {
                    if d.field == 0 {
                        d.name.pop();
                    } else {
                        d.description.pop();
                    }
                }
                Ok(())
            }
            (Page::NewMilestone, Key::Enter) => self.save_new_milestone(),
            (Page::NewMilestone, Key::Char(c)) => {
                self.paste_milestones(&c.to_string());
                Ok(())
            }
            (Page::NewVariant, Key::Backspace) => {
                if let Some(d) = &mut self.milestones.variant_draft {
                    d.name.pop();
                }
                Ok(())
            }
            (Page::NewVariant, Key::Enter) => self.save_new_variant(),
            (Page::NewVariant, Key::Char(c)) => {
                self.paste_milestones(&c.to_string());
                Ok(())
            }
            _ => Ok(()),
        })();
        if let Err(e) = result {
            self.milestones.notice = format!("{e:#}");
        }
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let m = &app.milestones;
    f.render_widget(Clear, area);
    let page = m.nav.current();
    match page {
        Page::Milestones | Page::Variants => {
            let rows: Vec<String> = if page == Page::Milestones {
                m.store
                    .as_ref()
                    .map(|s| {
                        s.value
                            .milestones
                            .iter()
                            .map(|ms| {
                                format!(
                                    "#{} · {} ({}) · {} words across {} scenes\n  {}",
                                    ms.id,
                                    plain(&ms.name),
                                    plain(&ms.timestamp),
                                    ms.total_words,
                                    ms.scene_snapshots.len(),
                                    plain(&ms.description)
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else if let Some(scene_id) = app.current_story_document_id() {
                m.variants
                    .as_ref()
                    .map(|s| {
                        s.value
                            .variants_for_scene(scene_id)
                            .into_iter()
                            .map(|v| {
                                format!(
                                    "#{} · {} ({}) · {} words",
                                    v.id,
                                    plain(&v.name),
                                    plain(&v.created_at),
                                    grimoire_core::notes::count_words(&v.content)
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let rows = if rows.is_empty() {
                vec![if page == Page::Milestones {
                    "No draft milestones saved yet. Press n to snapshot all manuscript scenes."
                        .into()
                } else {
                    "No scene variants saved for the active scene. Press n to save an alternative version.".into()
                }]
            } else {
                rows
            };
            wros_shell::render_menu(
                f,
                area,
                wros_shell::Menu {
                    title: if page == Page::Milestones {
                        "Draft Milestones & Variants · 1 Named Milestones"
                    } else {
                        "Draft Milestones & Variants · 2 Active Scene Variants"
                    },
                    rows: &rows,
                    selected: m.nav.selected(),
                    notice: &m.notice,
                    hint: "1 Milestones  2 Scene Variants  n new  Enter diff/restore  Esc close  F1 help",
                },
                theme.into(),
            );
        }
        Page::NewMilestone => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Draft Milestones · Create Named Milestone",
                &m.notice,
                "Tab/↑↓ field  Enter save milestone  Esc cancel",
                theme.into(),
            );
            if let Some(d) = &m.milestone_draft {
                let text = format!(
                    "{}Name:        {}\n{}Description: {}",
                    if d.field == 0 { "▸ " } else { "  " },
                    plain(&d.name),
                    if d.field == 1 { "▸ " } else { "  " },
                    plain(&d.description)
                );
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
        Page::NewVariant => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Scene Variants · Save Alternative Version",
                &m.notice,
                "Type name  Enter save variant  Esc cancel",
                theme.into(),
            );
            if let Some(d) = &m.variant_draft {
                let text = format!("▸ Variant name: {}", plain(&d.name));
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
        Page::DiffMilestone | Page::DiffVariant => {
            let body = wros_shell::render_panel(
                f,
                area,
                if page == Page::DiffMilestone {
                    "Draft Milestones · Scene Comparison"
                } else {
                    "Scene Variants · Alternative Comparison"
                },
                &m.notice,
                "r restore into editor (reversible)  PgUp/PgDn scroll  Esc back",
                theme.into(),
            );
            let current = app.editor.text();
            let summary = if page == Page::DiffMilestone {
                if let (Some(scene_id), Some(ms)) = (
                    app.current_story_document_id(),
                    m.store
                        .as_ref()
                        .and_then(|s| s.value.milestones.get(m.nav.selected())),
                ) {
                    if let Some(snap) = ms.scene_snapshots.get(&scene_id) {
                        format!(
                            "Milestone '{}' ({}) vs active scene:\nSnapshot words: {} · Current words: {}\n\nSnapshot preview:\n{}",
                            plain(&ms.name),
                            plain(&ms.timestamp),
                            grimoire_core::notes::count_words(snap),
                            grimoire_core::notes::count_words(&current),
                            plain(snap)
                        )
                    } else {
                        "Active scene was not present in this milestone.".into()
                    }
                } else {
                    "Open a scene to compare against this milestone.".into()
                }
            } else if let Some(scene_id) = app.current_story_document_id() {
                if let Some(v) = m.variants.as_ref().and_then(|s| {
                    s.value
                        .variants_for_scene(scene_id)
                        .get(m.nav.selected())
                        .copied()
                }) {
                    format!(
                        "Variant '{}' ({}) vs active scene:\nVariant words: {} · Current words: {}\n\nVariant text:\n{}",
                        plain(&v.name),
                        plain(&v.created_at),
                        grimoire_core::notes::count_words(&v.content),
                        grimoire_core::notes::count_words(&current),
                        plain(&v.content)
                    )
                } else {
                    "Select a variant to compare.".into()
                }
            } else {
                "Open a scene first.".into()
            };
            let (diff, _) =
                super::overlays::diff_lines(&m.diff, body.width.saturating_sub(1) as usize, theme);
            let mut lines: Vec<ratatui::text::Line> = summary
                .lines()
                .take(2)
                .map(|s| ratatui::text::Line::from(s.to_owned()))
                .collect();
            lines.push(ratatui::text::Line::from(
                "Struck text is saved wording; underlined text is current wording.",
            ));
            lines.extend(diff);
            let paragraph = Paragraph::new(lines).style(Style::default().fg(theme.text));
            let max = paragraph
                .line_count(body.width)
                .saturating_sub(body.height as usize)
                .min(u16::MAX as usize) as u16;
            m.max_scroll.set(max);
            f.render_widget(paragraph.scroll((m.scroll.min(max), 0)), body);
        }
    }
}
