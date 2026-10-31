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
    ReviewPassage,
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

#[derive(Clone)]
struct Comparison {
    document_id: u64,
    revision: (u64, u64),
    saved_id: u64,
    variant: bool,
    label: String,
    saved: String,
    metadata: (
        grimoire_core::sync::Fingerprint,
        grimoire_core::sync::Fingerprint,
    ),
}

pub struct Milestones {
    pub store: Option<Store<MilestoneStore>>,
    pub variants: Option<Store<VariantStore>>,
    pub nav: wros_shell::Navigation<Page>,
    pub notice: String,
    pub scroll: u16,
    pub milestone_draft: Option<MilestoneDraft>,
    pub variant_draft: Option<VariantDraft>,
    comparison: Option<Comparison>,
    passages: Vec<grimoire_core::history::Passage>,
    passage: usize,
    diff: Vec<grimoire_core::history::Piece>,
    max_scroll: std::cell::Cell<u16>,
}

impl Milestones {
    fn metadata_revision(
        &self,
    ) -> Result<(
        grimoire_core::sync::Fingerprint,
        grimoire_core::sync::Fingerprint,
    )> {
        let milestones = self
            .store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("milestone store unavailable"))?;
        let variants = self
            .variants
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("variant store unavailable"))?;
        Ok((
            grimoire_core::sync::Fingerprint::of_bytes(&serde_json::to_vec(&milestones.value)?),
            grimoire_core::sync::Fingerprint::of_bytes(&serde_json::to_vec(&variants.value)?),
        ))
    }
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
            passages: Vec::new(),
            passage: 0,
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
        let selected = self.milestones.nav.selected();
        let variant = self.milestones.nav.current() == Page::Variants;
        let (saved_id, label, saved) = if !variant {
            self.milestones
                .store
                .as_ref()
                .and_then(|s| s.value.milestones.get(selected))
                .and_then(|m| {
                    m.scene_snapshots.get(&id).map(|body| {
                        (
                            m.id,
                            format!("Milestone '{}' ({})", m.name, m.timestamp),
                            body.clone(),
                        )
                    })
                })
        } else {
            self.milestones.variants.as_ref().and_then(|s| {
                s.value.variants_for_scene(id).get(selected).map(|v| {
                    (
                        v.id,
                        format!("Variant '{}' ({})", v.name, v.created_at),
                        v.content.clone(),
                    )
                })
            })
        }
        .ok_or_else(|| anyhow::anyhow!("selected snapshot has no version for this document"))?;
        let current = self.editor.text();
        ensure!(
            saved.len() <= 2 * 1024 * 1024 && current.len() <= 2 * 1024 * 1024,
            "draft comparison exceeds 2 MiB; original preserved"
        );
        let diff = grimoire_core::history::diff(&saved, &current);
        let passages = grimoire_core::history::passages(&diff, &current)?;
        self.milestones.comparison = Some(Comparison {
            document_id: id,
            revision: self.editor.revision_stamp(),
            saved_id,
            variant,
            label,
            saved,
            metadata: self.milestones.metadata_revision()?,
        });
        self.milestones.diff = diff;
        self.milestones.passages = passages;
        self.milestones.passage = 0;
        Ok(())
    }
    fn verify_story_comparison(&mut self) -> Result<u64> {
        let id = self.active_story_document_id()?;
        ensure!(
            self.milestones
                .comparison
                .as_ref()
                .is_some_and(|c| c.document_id == id && c.revision == self.editor.revision_stamp()),
            "source revision changed; reopen the comparison before restoring"
        );
        self.milestones.store_mut()?.verify()?;
        self.milestones.variants_mut()?.verify()?;
        ensure!(
            self.milestones
                .comparison
                .as_ref()
                .is_some_and(|c| Some(c.metadata) == self.milestones.metadata_revision().ok()),
            "draft metadata changed since review; reopen the comparison"
        );
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

    fn restore_reviewed_body(
        &mut self,
        body: &str,
        recovery_name: &str,
        message: String,
    ) -> Result<()> {
        let scene_id = self.verify_story_comparison()?;
        let current = self.editor.text();
        ensure!(
            body != current,
            "the reviewed wording already matches the current scene"
        );
        self.milestones.variants_mut()?.update(|v| {
            v.save_variant(scene_id, recovery_name, current)?;
            Ok(())
        })?;
        self.snapshot_before_story_restore()?;
        self.editor.set_text(body);
        self.flush();
        self.overlay = Overlay::None;
        self.focus = self.editor_focus();
        self.msg = message;
        Ok(())
    }

    fn restore_selected_milestone_scene(&mut self) -> Result<()> {
        self.verify_story_comparison()?;
        let comparison = self
            .milestones
            .comparison
            .clone()
            .ok_or_else(|| anyhow::anyhow!("reopen the comparison"))?;
        ensure!(!comparison.variant, "reopen the milestone comparison");
        self.restore_reviewed_body(
            &comparison.saved,
            "Before milestone restore",
            "Restored scene from reviewed milestone (previous version saved in Variants & Undo)."
                .into(),
        )
    }

    fn restore_selected_variant(&mut self) -> Result<()> {
        self.verify_story_comparison()?;
        let comparison = self
            .milestones
            .comparison
            .clone()
            .ok_or_else(|| anyhow::anyhow!("reopen the comparison"))?;
        ensure!(comparison.variant, "reopen the variant comparison");
        self.restore_reviewed_body(
            &comparison.saved,
            "Before variant switch",
            format!(
                "Restored reviewed {} · Ctrl-Z or Variants reverts it.",
                plain(&comparison.label)
            ),
        )
    }

    fn open_passage_review(&mut self) -> Result<()> {
        self.verify_story_comparison()?;
        ensure!(
            !self.milestones.passages.is_empty(),
            "no changed passage to restore"
        );
        self.milestones.nav.open(Page::ReviewPassage);
        self.milestones.nav.select(0, 2); // Always default to Cancel.
        self.milestones.scroll = 0;
        Ok(())
    }

    fn restore_reviewed_passage(&mut self) -> Result<()> {
        self.verify_story_comparison()?;
        let restored = grimoire_core::history::restore_passage(
            &self.milestones.diff,
            &self.editor.text(),
            self.milestones.passage,
        )?;
        self.restore_reviewed_body(&restored, "Before passage restore",
            "Restored one reviewed passage · other writing retained · Variants, dated history and Ctrl-Z keep the previous scene.".into())
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
            (Page::DiffMilestone | Page::DiffVariant, Key::Left | Key::Char('[')) => {
                self.milestones.passage = self.milestones.passage.saturating_sub(1);
                Ok(())
            }
            (Page::DiffMilestone | Page::DiffVariant, Key::Right | Key::Char(']')) => {
                self.milestones.passage = self
                    .milestones
                    .passage
                    .saturating_add(1)
                    .min(self.milestones.passages.len().saturating_sub(1));
                Ok(())
            }
            (Page::DiffMilestone | Page::DiffVariant, Key::Char('p')) => self.open_passage_review(),
            (Page::ReviewPassage, Key::Up | Key::Left) => {
                self.milestones.nav.select(0, 2);
                Ok(())
            }
            (Page::ReviewPassage, Key::Down | Key::Right) => {
                self.milestones.nav.select(1, 2);
                Ok(())
            }
            (Page::ReviewPassage, Key::Tab | Key::BackTab) => {
                self.milestones
                    .nav
                    .select(1 - self.milestones.nav.selected().min(1), 2);
                Ok(())
            }
            (Page::ReviewPassage, Key::Enter) => {
                if self.milestones.nav.selected() == 0 {
                    self.milestones.nav.back();
                    self.milestones.scroll = 0;
                    Ok(())
                } else {
                    self.restore_reviewed_passage()
                }
            }
            (Page::DiffMilestone, Key::Char('r')) => self.restore_selected_milestone_scene(),
            (Page::DiffVariant, Key::Char('r')) => self.restore_selected_variant(),
            (Page::DiffMilestone | Page::DiffVariant | Page::ReviewPassage, Key::PageDown) => {
                self.milestones.scroll = self
                    .milestones
                    .scroll
                    .saturating_add(8)
                    .min(self.milestones.max_scroll.get());
                Ok(())
            }
            (Page::DiffMilestone | Page::DiffVariant | Page::ReviewPassage, Key::PageUp) => {
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
        Page::ReviewPassage => {
            let [choices, preview] = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Length(6),
                ratatui::layout::Constraint::Min(0),
            ])
            .areas(area);
            let rows = vec![
                "Cancel — keep current writing".into(),
                "Restore this saved passage".into(),
            ];
            wros_shell::render_menu(
                f,
                choices,
                wros_shell::Menu {
                    title: "Restore one reviewed passage",
                    rows: &rows,
                    selected: m.nav.selected(),
                    notice: "A full recovery variant and dated history are kept before an actual change.",
                    hint: "↑↓/Tab choose  Enter confirm  Esc back  F1 help",
                },
                theme.into(),
            );
            let body = wros_shell::render_panel(
                f,
                preview,
                "Current and saved wording",
                &m.notice,
                "PgUp/PgDn scroll both wordings · Cancel is selected when this review opens",
                theme.into(),
            );
            let text = m.passages.get(m.passage).map_or_else(|| "No changed passage.".into(), |p|
                format!("Passage {} of {} · current UTF-8 bytes {}..{}\n\nCURRENT WORDING:\n{}\n\nSAVED WORDING:\n{}",
                    m.passage + 1, m.passages.len(), p.current_range.start, p.current_range.end,
                    if p.current.is_empty() { "(no text — insertion point)".into() } else { plain(&p.current) },
                    if p.saved.is_empty() { "(no text — removes this current passage)".into() } else { plain(&p.saved) }));
            let paragraph = Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme.text));
            let max = paragraph
                .line_count(body.width)
                .saturating_sub(body.height as usize)
                .min(u16::MAX as usize) as u16;
            m.max_scroll.set(max);
            f.render_widget(paragraph.scroll((m.scroll.min(max), 0)), body);
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
                "←→ passage  p review one  r restore scene  PgUp/PgDn scroll  Esc back",
                theme.into(),
            );
            let current = app.editor.text();
            let summary = m.comparison.as_ref().map_or_else(
                || "Reopen the comparison.".into(),
                |c| {
                    format!(
                        "{} · saved #{} vs active scene:\nSaved words: {} · Current words: {}",
                        plain(&c.label),
                        c.saved_id,
                        grimoire_core::notes::count_words(&c.saved),
                        grimoire_core::notes::count_words(&current)
                    )
                },
            );
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
            lines.push(ratatui::text::Line::from(if m.passages.is_empty() {
                "No changed passages.".into()
            } else {
                format!(
                    "Passage {} of {} · ←→ choose · p reviews current and saved wording",
                    m.passage + 1,
                    m.passages.len()
                )
            }));
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
