//! Author-entered knowledge, separate from prose and the canonical fact store.
use super::*;
use grimoire_core::knowledge::{self as model, Entry, Knowledge, Moment, State, Version};

pub(super) struct Draft {
    entry: Option<u64>,
    previous: Option<Version>,
    document: u64,
    epoch: (u64, u64),
    source_hash: String,
    scene: String,
    fact: Fact,
    field: usize,
    character: String,
    state: State,
    moment: Moment,
    notes: String,
}
impl Continuity {
    fn knowledge_store(&mut self) -> Result<&mut Store<Knowledge>> {
        self.knowledge.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "character knowledge unavailable; original metadata preserved: {}",
                self.knowledge_problem
            )
        })
    }
    pub(super) fn knowledge_visible(&self, document: Option<u64>) -> Vec<&Entry> {
        let filter = self.knowledge_filter.to_lowercase();
        self.knowledge.as_ref().map_or_else(Vec::new, |store| {
            store
                .value
                .entries
                .iter()
                .filter(|e| {
                    (self.knowledge_all || Some(e.current.document_id) == document)
                        && (self.knowledge_archived || !e.current.archived)
                        && e.current.character.to_lowercase().contains(&filter)
                })
                .collect()
        })
    }
    pub(super) fn refresh_knowledge(&mut self) {
        let Some(store) = &self.knowledge else {
            return;
        };
        self.knowledge_conflicts.clear();
        match store.value.conflicts() {
            Ok(pairs) => {
                self.knowledge_problem.clear();
                self.knowledge_conflicts
                    .extend(pairs.iter().flat_map(|(a, b)| [*a, *b]));
            }
            Err(e) => {
                self.knowledge_problem = format!("Knowledge conflict review unavailable: {e:#}")
            }
        }
    }
}
impl App {
    fn selected_knowledge(&self) -> Result<Entry> {
        self.continuity
            .knowledge_visible(self.current_story_document_id())
            .get(self.continuity.nav.selected())
            .map(|e| (*e).clone())
            .ok_or_else(|| anyhow::anyhow!("select a knowledge record first"))
    }
    fn begin_knowledge(&mut self, edit: bool) -> Result<()> {
        self.continuity.knowledge_store()?.verify()?;
        self.continuity.store_mut()?.verify()?;
        let existing = if edit {
            Some(self.selected_knowledge()?)
        } else {
            None
        };
        let fact = if let Some(e) = &existing {
            self.continuity
                .store
                .as_ref()
                .and_then(|s| s.value.facts.iter().find(|f| f.id == e.current.fact.id))
                .cloned()
        } else {
            self.continuity
                .store
                .as_ref()
                .and_then(|s| s.value.facts.get(self.continuity.nav.selected()))
                .cloned()
        }
        .ok_or_else(|| anyhow::anyhow!("select an existing explicit fact first"))?;
        if let Some(e) = &existing {
            ensure!(
                fact == e.current.fact,
                "canonical fact changed; original knowledge retained. Select the current fact and g to create a newly reviewed record"
            );
        }
        let document = self.active_story_document_id()?;
        if let Some(e) = &existing {
            ensure!(
                e.current.document_id == document,
                "open the recorded scene before editing its knowledge"
            );
        }
        let epoch = self.editor.revision_stamp();
        let source_hash = model::source_hash(&self.editor.text());
        let scene = self
            .open
            .map(|i| self.project.nodes[i].title.clone())
            .unwrap_or_default();
        let previous = existing.as_ref().map(|e| e.current.clone());
        self.continuity.knowledge_draft = Some(Draft {
            entry: existing.as_ref().map(|e| e.id),
            previous: previous.clone(),
            document,
            epoch,
            source_hash,
            scene,
            fact,
            field: 0,
            character: previous
                .as_ref()
                .map_or_else(String::new, |v| v.character.clone()),
            state: previous.as_ref().map_or(State::Knows, |v| v.state),
            moment: previous.as_ref().map_or(Moment::After, |v| v.moment),
            notes: previous
                .as_ref()
                .map_or_else(String::new, |v| v.notes.clone()),
        });
        self.continuity.nav.open(Page::EditKnowledge);
        self.continuity.notice =
            "Explicit author assertion for this scene point; no knowledge is inferred from prose."
                .into();
        Ok(())
    }
    fn save_knowledge(&mut self) -> Result<()> {
        let document = self.active_story_document_id()?;
        let draft = self
            .continuity
            .knowledge_draft
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no knowledge draft"))?;
        ensure!(
            draft.document == document
                && draft.epoch == self.editor.revision_stamp()
                && draft.source_hash == model::source_hash(&self.editor.text()),
            "source revision changed; cancel and review again"
        );
        let (entry, previous, fact, version) = (
            draft.entry,
            draft.previous.clone(),
            draft.fact.clone(),
            Version {
                character: draft.character.trim().into(),
                document_id: draft.document,
                scene_name: draft.scene.clone(),
                source_sha256: draft.source_hash.clone(),
                fact: draft.fact.clone(),
                state: draft.state,
                moment: draft.moment,
                notes: draft.notes.clone(),
                archived: draft.previous.as_ref().is_some_and(|v| v.archived),
                recorded_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        self.continuity.store_mut()?.verify()?;
        ensure!(
            self.continuity.store.as_ref().is_some_and(|s| s
                .value
                .facts
                .iter()
                .any(|f| f == &fact)),
            "fact changed since review; cancel and select the current fact"
        );
        let id = self.continuity.knowledge_store()?.update(|store| {
            if let Some(id) = entry {
                ensure!(
                    store
                        .entries
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| &e.current)
                        == previous.as_ref(),
                    "knowledge changed since review; cancel and reload"
                );
                store.revise(id, version)?;
                Ok(id)
            } else {
                store.add(version)
            }
        })?;
        self.continuity.knowledge_draft = None;
        self.continuity.refresh_knowledge();
        self.continuity.nav.reset(Page::Knowledge);
        let entries = self.continuity.knowledge_visible(Some(document));
        let count = entries.len();
        let selected = entries.iter().position(|e| e.id == id).unwrap_or(0);
        self.continuity.nav.select(selected, count);
        self.continuity.notice = if self.continuity.knowledge_problem.is_empty() {
            format!(
                "Knowledge saved · {} conflicting explicit assertions; records and earlier versions retained.",
                self.continuity.knowledge_conflicts.len()
            )
        } else {
            format!("Knowledge saved. {}", self.continuity.knowledge_problem)
        };
        Ok(())
    }
    pub(super) fn paste_knowledge(&mut self, text: &str) {
        if self.continuity.nav.current() == Page::FilterKnowledge {
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            if self
                .continuity
                .knowledge_filter
                .len()
                .saturating_add(clean.len())
                <= 256
            {
                self.continuity.knowledge_filter.push_str(&clean);
            }
            return;
        }
        if let Some(draft) = &mut self.continuity.knowledge_draft {
            let clean: String = text
                .chars()
                .map(|c| if matches!(c, '\n' | '\t') { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            let (target, limit) = match draft.field {
                0 => (&mut draft.character, 256),
                3 => (&mut draft.notes, 4096),
                _ => return,
            };
            if target.len().saturating_add(clean.len()) <= limit {
                target.push_str(&clean);
            }
        }
    }
    fn archive_knowledge(&mut self) -> Result<()> {
        let entry = self.selected_knowledge()?;
        let mut next = entry.current.clone();
        next.archived = !next.archived;
        next.recorded_at = chrono::Utc::now().to_rfc3339();
        self.continuity.knowledge_store()?.update(|s| {
            ensure!(
                s.entries
                    .iter()
                    .find(|e| e.id == entry.id)
                    .map(|e| &e.current)
                    == Some(&entry.current),
                "record changed; select it again"
            );
            s.revise(entry.id, next)
        })?;
        self.continuity.refresh_knowledge();
        self.continuity.notice = "Knowledge archive state changed; all versions retained.".into();
        Ok(())
    }
    fn open_knowledge_source(&mut self) -> Result<()> {
        let entry = self.selected_knowledge()?;
        let doc = self
            .review_notes
            .store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("document registry unavailable"))?
            .value
            .document(entry.current.document_id)?;
        ensure!(
            !doc.needs_reconnect,
            "recorded source needs explicit document reconnection"
        );
        let path = self.project.root.join(&doc.path);
        ensure!(
            self.project
                .nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.path == path && n.kind == Kind::Scene && !self.project.in_trash(i)),
            "recorded source is unavailable; originals retained"
        );
        self.run_action(palette::Action::Open(path));
        Ok(())
    }
    pub(super) fn on_knowledge_key(&mut self, page: Page, key: Key) -> bool {
        let routed = matches!(
            page,
            Page::Knowledge | Page::EditKnowledge | Page::KnowledgeDetail | Page::FilterKnowledge
        ) || (matches!(
            page,
            Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts
        ) && key == Key::Char('5'))
            || (page == Page::Facts && key == Key::Char('g'));
        if !routed {
            return false;
        }
        let result: Result<()> = (|| match (page, key) {
            (_, Key::Char('5')) if !matches!(page, Page::EditKnowledge | Page::FilterKnowledge) => {
                self.continuity.knowledge_store()?.verify()?;
                self.continuity.refresh_knowledge();
                self.continuity.nav.reset(Page::Knowledge);
                self.continuity.notice = if self.continuity.knowledge_problem.is_empty() {
                    "Author-entered knowledge · a all scenes · h archived · v retained versions"
                        .into()
                } else {
                    self.continuity.knowledge_problem.clone()
                };
                Ok(())
            }
            (Page::Facts, Key::Char('g')) => self.begin_knowledge(false),
            (Page::Knowledge, Key::Char('1')) => {
                self.continuity.nav.reset(Page::Facts);
                Ok(())
            }
            (Page::Knowledge, Key::Char('2')) => {
                self.continuity.nav.reset(Page::Timeline);
                Ok(())
            }
            (Page::Knowledge, Key::Char('3')) => {
                self.continuity.nav.reset(Page::Threads);
                Ok(())
            }
            (Page::Knowledge, Key::Char('4')) => {
                self.continuity.refresh_conflicts()?;
                self.continuity.nav.reset(Page::Conflicts);
                Ok(())
            }
            (Page::Knowledge, Key::Down | Key::Char('j')) => {
                let len = self
                    .continuity
                    .knowledge_visible(self.current_story_document_id())
                    .len();
                self.continuity.nav.move_by(1, len);
                Ok(())
            }
            (Page::Knowledge, Key::Up | Key::Char('k')) => {
                let len = self
                    .continuity
                    .knowledge_visible(self.current_story_document_id())
                    .len();
                self.continuity.nav.move_by(-1, len);
                Ok(())
            }
            (Page::Knowledge, Key::Char('a')) => {
                self.continuity.knowledge_all = !self.continuity.knowledge_all;
                self.continuity.nav.select(0, 1);
                Ok(())
            }
            (Page::Knowledge, Key::Char('h')) => {
                self.continuity.knowledge_archived = !self.continuity.knowledge_archived;
                self.continuity.nav.select(0, 1);
                Ok(())
            }
            (Page::Knowledge, Key::Char('e') | Key::Enter) => self.begin_knowledge(true),
            (Page::Knowledge, Key::Char('d')) => self.archive_knowledge(),
            (Page::Knowledge, Key::Char('g')) => self.open_knowledge_source(),
            (Page::Knowledge, Key::Char('v')) => {
                let entry = self.selected_knowledge()?;
                let mut text = format!("CURRENT RECORD\n{}\n", description(&entry.current));
                for (index, version) in entry.previous.iter().enumerate().rev() {
                    text.push_str(&format!(
                        "\nEARLIER VERSION {}\n{}\n",
                        index + 1,
                        description(version)
                    ));
                }
                ensure!(
                    text.len() <= 256 * 1024 && text.lines().count() <= 4096,
                    "knowledge history view exceeds 256 KiB or 4096 lines; no partial view; original metadata retained"
                );
                self.continuity.knowledge_detail = Some(text);
                self.continuity.scroll = 0;
                self.continuity.nav.open(Page::KnowledgeDetail);
                Ok(())
            }
            (Page::Knowledge, Key::Char('/')) => {
                self.continuity.nav.open(Page::FilterKnowledge);
                Ok(())
            }
            (Page::Knowledge, Key::Home | Key::End | Key::PageDown | Key::PageUp) => {
                let count = self
                    .continuity
                    .knowledge_visible(self.current_story_document_id())
                    .len();
                match key {
                    Key::Home => self.continuity.nav.select(0, count),
                    Key::End => self.continuity.nav.select(count.saturating_sub(1), count),
                    Key::PageDown => self.continuity.nav.move_by(8, count),
                    _ => self.continuity.nav.move_by(-8, count),
                }
                Ok(())
            }
            (Page::FilterKnowledge, Key::Char(c)) => {
                self.paste_knowledge(&c.to_string());
                Ok(())
            }
            (Page::FilterKnowledge, Key::Backspace) => {
                self.continuity.knowledge_filter.pop();
                Ok(())
            }
            (Page::FilterKnowledge, Key::Enter) => {
                self.continuity.nav.back();
                let count = self
                    .continuity
                    .knowledge_visible(self.current_story_document_id())
                    .len();
                self.continuity.nav.select(0, count);
                Ok(())
            }
            (Page::Knowledge, Key::Char('n')) => {
                self.continuity.nav.reset(Page::Facts);
                self.continuity.notice =
                    "Select an explicit fact, then g to record who knows it at this scene point."
                        .into();
                Ok(())
            }
            (Page::EditKnowledge, Key::Tab | Key::Down) => {
                if let Some(d) = &mut self.continuity.knowledge_draft {
                    d.field = (d.field + 1) % 4;
                }
                Ok(())
            }
            (Page::EditKnowledge, Key::BackTab | Key::Up) => {
                if let Some(d) = &mut self.continuity.knowledge_draft {
                    d.field = (d.field + 3) % 4;
                }
                Ok(())
            }
            (Page::EditKnowledge, Key::Left | Key::Right) => {
                if let Some(d) = &mut self.continuity.knowledge_draft {
                    if d.field == 1 {
                        d.state = d.state.next();
                    } else if d.field == 2 {
                        d.moment = d.moment.next();
                    }
                }
                Ok(())
            }
            (Page::EditKnowledge, Key::Backspace) => {
                if let Some(d) = &mut self.continuity.knowledge_draft {
                    match d.field {
                        0 => {
                            d.character.pop();
                        }
                        3 => {
                            d.notes.pop();
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            (Page::EditKnowledge, Key::Enter) => self.save_knowledge(),
            (Page::EditKnowledge, Key::Char(c)) => {
                self.paste_knowledge(&c.to_string());
                Ok(())
            }
            (Page::KnowledgeDetail, Key::PageDown) => {
                self.continuity.scroll = self
                    .continuity
                    .scroll
                    .saturating_add(8)
                    .min(self.continuity.knowledge_max_scroll.get());
                Ok(())
            }
            (Page::KnowledgeDetail, Key::PageUp) => {
                self.continuity.scroll = self.continuity.scroll.saturating_sub(8);
                Ok(())
            }
            (Page::KnowledgeDetail, Key::Home) => {
                self.continuity.scroll = 0;
                Ok(())
            }
            (Page::KnowledgeDetail, Key::End) => {
                self.continuity.scroll = self.continuity.knowledge_max_scroll.get();
                Ok(())
            }
            _ => Ok(()),
        })();
        if let Err(e) = result {
            self.continuity.notice = format!("{e:#}");
        }
        true
    }
}
fn description(v: &Version) -> String {
    let fact_source = match (v.fact.scene_id, v.fact.scene_name.as_deref()) {
        (Some(id), Some(name)) => format!("document #{id} · {}", plain(name)),
        (Some(id), None) => format!("document #{id}"),
        (None, _) => "Book-wide".into(),
    };
    format!(
        "{} · {} · {}\nScene: {} · document #{}\nSource revision SHA-256: {}\nFact #{} · {} · {}\nProperty: {} = {}\nCertainty: {}\nSingle-value scope: {}\nFact source: {}\nFact notes: {}\nKnowledge notes: {}\nRecorded: {} · {}",
        plain(&v.character),
        v.state.label(),
        v.moment.label(),
        plain(&v.scene_name),
        v.document_id,
        plain(&v.source_sha256),
        v.fact.id,
        v.fact.category.label(),
        plain(&v.fact.subject),
        plain(&v.fact.property),
        plain(&v.fact.value),
        v.fact.certainty.label(),
        v.fact
            .exclusive_scope
            .as_deref()
            .map_or_else(|| "Not declared".into(), plain),
        fact_source,
        plain(&v.fact.notes),
        plain(&v.notes),
        plain(&v.recorded_at),
        if v.archived { "Archived" } else { "Active" }
    )
}

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let c = &app.continuity;
    match c.nav.current() {
        Page::Knowledge => {
            let active = app.current_story_document_id();
            let title = format!(
                "Continuity Tracker · 5 Character Knowledge · {}{}{}",
                if c.knowledge_all {
                    "All scenes"
                } else {
                    "This scene"
                },
                if c.knowledge_archived {
                    " + archived"
                } else {
                    ""
                },
                if c.knowledge_filter.is_empty() {
                    ""
                } else {
                    " · Filtered"
                }
            );
            let hash = model::source_hash(&app.editor.text());
            let facts = c.store.as_ref().map(|s| {
                s.value
                    .facts
                    .iter()
                    .map(|f| (f.id, f))
                    .collect::<std::collections::BTreeMap<_, _>>()
            });
            let mut rows: Vec<String> = c
                .knowledge_visible(active)
                .into_iter()
                .map(|e| {
                    let v = &e.current;
                    let changed = facts
                        .as_ref()
                        .is_none_or(|facts| facts.get(&v.fact.id).copied() != Some(&v.fact));
                    let stale = Some(v.document_id) == active && v.source_sha256 != hash;
                    let mut flags = Vec::new();
                    if c.knowledge_conflicts.contains(&e.id) {
                        flags.push("Conflicting assertion");
                    }
                    if stale {
                        flags.push("Source revised");
                    }
                    if changed {
                        flags.push("Fact changed/missing");
                    }
                    if v.archived {
                        flags.push("Archived");
                    }
                    let status = if flags.is_empty() {
                        String::new()
                    } else {
                        format!("\n  {}", flags.join(" · "))
                    };
                    format!(
                        "{} · {} · {}\n  {} · {}: {} = {}{}",
                        plain(&v.character),
                        v.state.label(),
                        v.moment.label(),
                        plain(&v.scene_name),
                        plain(&v.fact.subject),
                        plain(&v.fact.property),
                        plain(&v.fact.value),
                        status
                    )
                })
                .collect();
            if rows.is_empty() {
                rows.push(if c.knowledge.is_none(){"Knowledge unavailable; original metadata preserved.".into()}else{"No matching knowledge records. In Facts select a fact and press g to record who knows it.".into()});
            }
            wros_shell::render_menu(
                f,
                area,
                wros_shell::Menu {
                    title: &title,
                    rows: &rows,
                    selected: c.nav.selected(),
                    notice: &c.notice,
                    hint: "e edit  d archive  v history  g source  / filter  a all  h archived  F1 help",
                },
                theme.into(),
            );
        }
        Page::EditKnowledge => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Record character knowledge",
                &c.notice,
                "Tab/↑↓ field  ←→ state/point  Enter save  Esc cancel  F1 help",
                theme.into(),
            );
            if let Some(d) = &c.knowledge_draft {
                let values = [
                    format!("Character: {}", plain(&d.character)),
                    format!("Knowledge: {}", d.state.label()),
                    format!("Point: {}", d.moment.label()),
                    format!("Notes: {}", plain(&d.notes)),
                ];
                let mut text = format!(
                    "Scene: {}\nReviewed fact: {} · {} = {}\n\n",
                    plain(&d.scene),
                    plain(&d.fact.subject),
                    plain(&d.fact.property),
                    plain(&d.fact.value)
                );
                for (index, value) in values.iter().enumerate() {
                    text.push_str(if index == d.field { "▸ " } else { "  " });
                    text.push_str(value);
                    text.push('\n');
                }
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
        Page::KnowledgeDetail => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Character knowledge · retained versions",
                &c.notice,
                "PgUp/PgDn Home/End scroll  Esc back  F1 help",
                theme.into(),
            );
            let text = c
                .knowledge_detail
                .as_deref()
                .unwrap_or("Record unavailable; close and select again.");
            let paragraph = Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme.text));
            let max = paragraph
                .line_count(body.width)
                .saturating_sub(body.height as usize)
                .min(u16::MAX as usize) as u16;
            c.knowledge_max_scroll.set(max);
            f.render_widget(paragraph.scroll((c.scroll.min(max), 0)), body);
        }
        Page::FilterKnowledge => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Filter character knowledge",
                &c.notice,
                "Type character name  Backspace edit  Enter/Esc return  F1 help",
                theme.into(),
            );
            f.render_widget(
                Paragraph::new(format!(
                    "Character contains: {}\n\nFiltering changes this view only.",
                    plain(&c.knowledge_filter)
                ))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme.text)),
                body,
            );
        }
        _ => {}
    }
}
