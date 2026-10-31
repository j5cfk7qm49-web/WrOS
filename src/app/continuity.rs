//! Continuity tracker TUI workflow: explicit facts, chronology events, plot threads,
//! and contradiction review without hallucinating contradictions.
use super::*;
mod knowledge_ui;
use anyhow::{Result, ensure};
use grimoire_core::{
    continuity::{Certainty, ContinuityStore, Contradiction, Fact, FactCategory, ThreadStatus},
    statefile::Store,
};
use ratatui::{
    Frame,
    style::Style,
    widgets::{Clear, Paragraph, Wrap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Facts,
    Timeline,
    Threads,
    Conflicts,
    AddFact,
    AddEvent,
    AddThread,
    Knowledge,
    EditKnowledge,
    KnowledgeDetail,
    FilterKnowledge,
}

#[derive(Debug, Clone, Default)]
pub struct FactDraft {
    pub category: Option<FactCategory>,
    pub certainty: Option<Certainty>,
    pub field: usize, // 0: subject, 1: property, 2: value, 3: scope, 4: notes
    pub subject: String,
    pub property: String,
    pub value: String,
    pub scope: String,
    pub notes: String,
}

#[derive(Debug, Clone, Default)]
pub struct EventDraft {
    pub field: usize, // 0: order, 1: title, 2: story_date, 3: participants, 4: description
    pub order: String,
    pub title: String,
    pub story_date: String,
    pub participants: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadDraft {
    pub status: Option<ThreadStatus>,
    pub field: usize, // 0: title, 1: introduced_in, 2: resolved_in, 3: notes
    pub title: String,
    pub introduced_in: String,
    pub resolved_in: String,
    pub notes: String,
}

pub struct Continuity {
    pub store: Option<Store<ContinuityStore>>,
    knowledge: Option<Store<grimoire_core::knowledge::Knowledge>>,
    knowledge_problem: String,
    knowledge_conflicts: std::collections::BTreeSet<u64>,
    knowledge_draft: Option<knowledge_ui::Draft>,
    knowledge_all: bool,
    knowledge_archived: bool,
    knowledge_max_scroll: std::cell::Cell<u16>,
    knowledge_filter: String,
    knowledge_detail: Option<String>,
    pub nav: wros_shell::Navigation<Page>,
    pub notice: String,
    pub scroll: u16,
    pub conflicts: Vec<Contradiction>,
    pub fact_draft: Option<FactDraft>,
    pub event_draft: Option<EventDraft>,
    pub thread_draft: Option<ThreadDraft>,
}

impl Continuity {
    pub fn load(root: &Path) -> Self {
        let (store, notice, conflicts) = match ContinuityStore::load(root) {
            Ok(s) => {
                let (c, notice) = match s.value.check_contradictions() {
                    Ok(c) => (
                        c,
                        "1 Facts  2 Timeline  3 Threads  4 Conflicts · n adds to active view."
                            .into(),
                    ),
                    Err(error) => (
                        Vec::new(),
                        format!("Conflict review unavailable: {error:#}"),
                    ),
                };
                (Some(s), notice, c)
            }
            Err(e) => (
                None,
                format!("Continuity store unavailable: {e:#}"),
                Vec::new(),
            ),
        };
        let (knowledge, knowledge_problem) = match grimoire_core::knowledge::Knowledge::load(root) {
            Ok(s) => (Some(s), String::new()),
            Err(e) => (
                None,
                format!("Character knowledge unavailable; originals preserved: {e:#}"),
            ),
        };
        let mut output = Self {
            store,
            knowledge,
            knowledge_problem,
            knowledge_conflicts: std::collections::BTreeSet::new(),
            knowledge_draft: None,
            knowledge_all: false,
            knowledge_archived: false,
            knowledge_max_scroll: std::cell::Cell::new(0),
            knowledge_filter: String::new(),
            knowledge_detail: None,
            nav: wros_shell::Navigation::new(Page::Facts),
            notice,
            scroll: 0,
            conflicts,
            fact_draft: None,
            event_draft: None,
            thread_draft: None,
        };
        output.refresh_knowledge();
        output
    }

    pub fn typing(&self) -> bool {
        matches!(
            self.nav.current(),
            Page::AddFact
                | Page::AddEvent
                | Page::AddThread
                | Page::EditKnowledge
                | Page::FilterKnowledge
        )
    }

    fn store_mut(&mut self) -> Result<&mut Store<ContinuityStore>> {
        self.store
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("continuity store unavailable; original preserved"))
    }

    fn refresh_conflicts(&mut self) -> Result<()> {
        if let Some(s) = &self.store {
            self.conflicts = s.value.check_contradictions()?;
        }
        Ok(())
    }
}

fn plain(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

impl App {
    pub fn open_continuity(&mut self) {
        self.overlay = Overlay::Continuity;
        self.continuity.nav.reset(Page::Facts);
        self.continuity.knowledge_draft = None;
        self.continuity.fact_draft = None;
        self.continuity.event_draft = None;
        self.continuity.thread_draft = None;
        self.continuity.scroll = 0;
        if let Err(error) = self.continuity.refresh_conflicts() {
            self.continuity.notice = format!("Conflict review unavailable: {error:#}");
        }
    }

    pub fn paste_continuity(&mut self, text: &str) {
        if matches!(
            self.continuity.nav.current(),
            Page::EditKnowledge | Page::FilterKnowledge
        ) {
            self.paste_knowledge(text);
            return;
        }
        let clean: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        match self.continuity.nav.current() {
            Page::AddFact => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    let target = match d.field {
                        0 => &mut d.subject,
                        1 => &mut d.property,
                        2 => &mut d.value,
                        3 => &mut d.scope,
                        _ => &mut d.notes,
                    };
                    let limit = if d.field == 3 { 256 } else { 4096 };
                    if target.len().saturating_add(clean.len()) <= limit {
                        target.push_str(&clean);
                    }
                }
            }
            Page::AddEvent => {
                if let Some(d) = &mut self.continuity.event_draft {
                    if d.field == 0 {
                        for c in clean.chars() {
                            if (c.is_ascii_digit() || (c == '-' && d.order.is_empty()))
                                && d.order.len() < 9
                            {
                                d.order.push(c);
                            }
                        }
                    } else {
                        let target = match d.field {
                            1 => &mut d.title,
                            2 => &mut d.story_date,
                            3 => &mut d.participants,
                            _ => &mut d.description,
                        };
                        if target.len().saturating_add(clean.len()) <= 4096 {
                            target.push_str(&clean);
                        }
                    }
                }
            }
            Page::AddThread => {
                if let Some(d) = &mut self.continuity.thread_draft {
                    let target = match d.field {
                        0 => &mut d.title,
                        1 => &mut d.introduced_in,
                        2 => &mut d.resolved_in,
                        _ => &mut d.notes,
                    };
                    if target.len().saturating_add(clean.len()) <= 4096 {
                        target.push_str(&clean);
                    }
                }
            }
            _ => {}
        }
    }

    fn save_continuity_fact(&mut self) -> Result<()> {
        let d = self
            .continuity
            .fact_draft
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no fact draft"))?;
        ensure!(
            !d.subject.trim().is_empty()
                && !d.property.trim().is_empty()
                && !d.value.trim().is_empty(),
            "subject, property and value are required"
        );
        let scene_id = if self.open.is_some() {
            Some(self.active_story_document_id()?)
        } else {
            None
        };
        let scene_name = self.open.map(|i| self.project.nodes[i].title.clone());
        let mut fact = Fact::new(
            d.category.unwrap_or(FactCategory::Character),
            d.subject.trim(),
            d.property.trim(),
            d.value.trim(),
            d.certainty.unwrap_or(Certainty::Definite),
        );
        fact.scene_id = scene_id;
        fact.scene_name = scene_name;
        if !d.scope.trim().is_empty() {
            fact.exclusive_scope = Some(d.scope.trim().to_string());
        }
        fact.notes = d.notes.trim().to_string();
        self.continuity.store_mut()?.update(|s| {
            s.add_fact(fact)?;
            Ok(())
        })?;
        self.continuity.fact_draft = None;
        self.continuity.nav.reset(Page::Facts);
        self.continuity.refresh_conflicts()?;
        self.continuity.notice = "Fact saved to continuity store.".into();
        Ok(())
    }

    fn save_continuity_event(&mut self) -> Result<()> {
        let d = self
            .continuity
            .event_draft
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no event draft"))?;
        ensure!(!d.title.trim().is_empty(), "event title is required");
        let order: i32 = if d.order.trim().is_empty() {
            (self
                .continuity
                .store
                .as_ref()
                .map_or(0, |s| s.value.events.len()) as i32)
                + 1
        } else {
            d.order
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("event order must be an integer"))?
        };
        let scene_id = if self.open.is_some() {
            Some(self.active_story_document_id()?)
        } else {
            None
        };
        let participants: Vec<String> = d
            .participants
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        self.continuity.store_mut()?.update(|s| {
            s.add_event(
                order,
                d.title.trim(),
                d.story_date.trim(),
                scene_id,
                participants,
                d.description.trim(),
            )?;
            Ok(())
        })?;
        self.continuity.event_draft = None;
        self.continuity.nav.reset(Page::Timeline);
        self.continuity.notice = "Timeline event saved.".into();
        Ok(())
    }

    fn save_continuity_thread(&mut self) -> Result<()> {
        let d = self
            .continuity
            .thread_draft
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no thread draft"))?;
        ensure!(!d.title.trim().is_empty(), "plot thread title is required");
        let intro =
            (!d.introduced_in.trim().is_empty()).then(|| d.introduced_in.trim().to_string());
        let res = (!d.resolved_in.trim().is_empty()).then(|| d.resolved_in.trim().to_string());
        self.continuity.store_mut()?.update(|s| {
            s.add_thread(
                d.title.trim(),
                d.status.unwrap_or(ThreadStatus::Open),
                intro,
                res,
                d.notes.trim(),
            )?;
            Ok(())
        })?;
        self.continuity.thread_draft = None;
        self.continuity.nav.reset(Page::Threads);
        self.continuity.notice = "Plot thread saved.".into();
        Ok(())
    }

    pub fn on_continuity_key(&mut self, key: Key) {
        let page = self.continuity.nav.current();
        if key == Key::Esc {
            if page == Page::FilterKnowledge {
                self.continuity.nav.back();
                let count = self
                    .continuity
                    .knowledge_visible(self.current_story_document_id())
                    .len();
                self.continuity.nav.select(0, count);
                return;
            }
            self.continuity.knowledge_draft = None;
            self.continuity.fact_draft = None;
            self.continuity.event_draft = None;
            self.continuity.thread_draft = None;
            self.continuity.scroll = 0;
            if !self.continuity.nav.back() {
                self.overlay = Overlay::None;
            }
            return;
        }
        if self.on_knowledge_key(page, key) {
            return;
        }
        let result: Result<()> = (|| match (page, key) {
            (Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts, Key::Char('1')) => {
                self.continuity.nav.reset(Page::Facts);
                Ok(())
            }
            (Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts, Key::Char('2')) => {
                self.continuity.nav.reset(Page::Timeline);
                Ok(())
            }
            (Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts, Key::Char('3')) => {
                self.continuity.nav.reset(Page::Threads);
                Ok(())
            }
            (
                Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts,
                Key::Char('4' | 'c'),
            ) => {
                self.continuity.refresh_conflicts()?;
                self.continuity.nav.reset(Page::Conflicts);
                let has_facts = self
                    .continuity
                    .store
                    .as_ref()
                    .is_some_and(|s| !s.value.facts.is_empty());
                if has_facts {
                    self.observe_deck(grimoire_core::tutorial::Event::ContinuityReviewed);
                }
                Ok(())
            }
            (
                Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts,
                Key::Down | Key::Char('j'),
            ) => {
                let len = self.continuity_list_len(page);
                self.continuity.nav.move_by(1, len);
                Ok(())
            }
            (
                Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts,
                Key::Up | Key::Char('k'),
            ) => {
                let len = self.continuity_list_len(page);
                self.continuity.nav.move_by(-1, len);
                Ok(())
            }
            (Page::Facts, Key::Char('n')) => {
                self.continuity.fact_draft = Some(FactDraft {
                    category: Some(FactCategory::Character),
                    certainty: Some(Certainty::Definite),
                    ..FactDraft::default()
                });
                self.continuity.nav.open(Page::AddFact);
                Ok(())
            }
            (Page::Timeline, Key::Char('n')) => {
                self.continuity.event_draft = Some(EventDraft::default());
                self.continuity.nav.open(Page::AddEvent);
                Ok(())
            }
            (Page::Threads, Key::Char('n')) => {
                self.continuity.thread_draft = Some(ThreadDraft {
                    status: Some(ThreadStatus::Open),
                    ..ThreadDraft::default()
                });
                self.continuity.nav.open(Page::AddThread);
                Ok(())
            }
            (Page::Facts, Key::Char('t')) => {
                let sel = self.continuity.nav.selected();
                let id = self
                    .continuity
                    .store
                    .as_ref()
                    .and_then(|s| s.value.facts.get(sel).map(|f| f.id))
                    .ok_or_else(|| anyhow::anyhow!("select a fact first"))?;
                let next = self
                    .continuity
                    .store_mut()?
                    .update(|s| s.cycle_fact_certainty(id))?;
                self.continuity.refresh_conflicts()?;
                self.continuity.notice = format!("Fact #{id} certainty set to {}.", next.label());
                Ok(())
            }
            (Page::Threads, Key::Char('t')) => {
                let sel = self.continuity.nav.selected();
                let id = self
                    .continuity
                    .store
                    .as_ref()
                    .and_then(|s| s.value.threads.get(sel).map(|t| t.id))
                    .ok_or_else(|| anyhow::anyhow!("select a thread first"))?;
                let next = self
                    .continuity
                    .store_mut()?
                    .update(|s| s.cycle_thread_status(id))?;
                self.continuity.notice = format!("Thread #{id} status set to {}.", next.label());
                Ok(())
            }
            (Page::Facts, Key::Char('d')) => {
                let sel = self.continuity.nav.selected();
                let id = self
                    .continuity
                    .store
                    .as_ref()
                    .and_then(|s| s.value.facts.get(sel).map(|f| f.id))
                    .ok_or_else(|| anyhow::anyhow!("select a fact to delete"))?;
                self.continuity.store_mut()?.update(|s| {
                    s.remove_fact(id);
                    Ok(())
                })?;
                self.continuity.refresh_conflicts()?;
                self.continuity.notice = format!("Removed fact #{id}.");
                Ok(())
            }
            (Page::Timeline, Key::Char('d')) => {
                let sel = self.continuity.nav.selected();
                let id = self
                    .continuity
                    .store
                    .as_ref()
                    .and_then(|s| s.value.events.get(sel).map(|e| e.id))
                    .ok_or_else(|| anyhow::anyhow!("select an event to delete"))?;
                self.continuity.store_mut()?.update(|s| {
                    s.remove_event(id);
                    Ok(())
                })?;
                self.continuity.notice = format!("Removed timeline event #{id}.");
                Ok(())
            }
            (Page::Threads, Key::Char('d')) => {
                let sel = self.continuity.nav.selected();
                let id = self
                    .continuity
                    .store
                    .as_ref()
                    .and_then(|s| s.value.threads.get(sel).map(|t| t.id))
                    .ok_or_else(|| anyhow::anyhow!("select a thread to delete"))?;
                self.continuity.store_mut()?.update(|s| {
                    s.remove_thread(id);
                    Ok(())
                })?;
                self.continuity.notice = format!("Removed plot thread #{id}.");
                Ok(())
            }
            (Page::Conflicts, Key::Enter | Key::Char('r')) => {
                self.continuity.refresh_conflicts()?;
                if self
                    .continuity
                    .store
                    .as_ref()
                    .is_some_and(|s| !s.value.facts.is_empty())
                {
                    self.observe_deck(grimoire_core::tutorial::Event::ContinuityReviewed);
                }
                self.continuity.notice = format!(
                    "Reviewed {} continuity contradiction(s)/suggestion(s).",
                    self.continuity.conflicts.len()
                );
                Ok(())
            }
            (Page::AddFact, Key::Tab | Key::Down) => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    d.field = (d.field + 1) % 5;
                }
                Ok(())
            }
            (Page::AddFact, Key::BackTab | Key::Up) => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    d.field = (d.field + 4) % 5;
                }
                Ok(())
            }
            (Page::AddFact, Key::Left) => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    d.category = Some(d.category.unwrap_or(FactCategory::Character).next());
                }
                Ok(())
            }
            (Page::AddFact, Key::Right) => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    d.certainty = Some(d.certainty.unwrap_or(Certainty::Definite).next());
                }
                Ok(())
            }
            (Page::AddFact, Key::Backspace) => {
                if let Some(d) = &mut self.continuity.fact_draft {
                    match d.field {
                        0 => {
                            d.subject.pop();
                        }
                        1 => {
                            d.property.pop();
                        }
                        2 => {
                            d.value.pop();
                        }
                        3 => {
                            d.scope.pop();
                        }
                        _ => {
                            d.notes.pop();
                        }
                    }
                }
                Ok(())
            }
            (Page::AddFact, Key::Enter) => self.save_continuity_fact(),
            (Page::AddFact, Key::Char(c)) => {
                self.paste_continuity(&c.to_string());
                Ok(())
            }
            (Page::AddEvent, Key::Tab | Key::Down) => {
                if let Some(d) = &mut self.continuity.event_draft {
                    d.field = (d.field + 1) % 5;
                }
                Ok(())
            }
            (Page::AddEvent, Key::BackTab | Key::Up) => {
                if let Some(d) = &mut self.continuity.event_draft {
                    d.field = (d.field + 4) % 5;
                }
                Ok(())
            }
            (Page::AddEvent, Key::Backspace) => {
                if let Some(d) = &mut self.continuity.event_draft {
                    match d.field {
                        0 => {
                            d.order.pop();
                        }
                        1 => {
                            d.title.pop();
                        }
                        2 => {
                            d.story_date.pop();
                        }
                        3 => {
                            d.participants.pop();
                        }
                        _ => {
                            d.description.pop();
                        }
                    }
                }
                Ok(())
            }
            (Page::AddEvent, Key::Enter) => self.save_continuity_event(),
            (Page::AddEvent, Key::Char(c)) => {
                self.paste_continuity(&c.to_string());
                Ok(())
            }
            (Page::AddThread, Key::Tab | Key::Down) => {
                if let Some(d) = &mut self.continuity.thread_draft {
                    d.field = (d.field + 1) % 4;
                }
                Ok(())
            }
            (Page::AddThread, Key::BackTab | Key::Up) => {
                if let Some(d) = &mut self.continuity.thread_draft {
                    d.field = (d.field + 3) % 4;
                }
                Ok(())
            }
            (Page::AddThread, Key::Left | Key::Right) => {
                if let Some(d) = &mut self.continuity.thread_draft {
                    d.status = Some(d.status.unwrap_or(ThreadStatus::Open).next());
                }
                Ok(())
            }
            (Page::AddThread, Key::Backspace) => {
                if let Some(d) = &mut self.continuity.thread_draft {
                    match d.field {
                        0 => {
                            d.title.pop();
                        }
                        1 => {
                            d.introduced_in.pop();
                        }
                        2 => {
                            d.resolved_in.pop();
                        }
                        _ => {
                            d.notes.pop();
                        }
                    }
                }
                Ok(())
            }
            (Page::AddThread, Key::Enter) => self.save_continuity_thread(),
            (Page::AddThread, Key::Char(c)) => {
                self.paste_continuity(&c.to_string());
                Ok(())
            }
            _ => Ok(()),
        })();
        if let Err(e) = result {
            self.continuity.notice = format!("{e:#}");
        }
    }

    fn continuity_list_len(&self, page: Page) -> usize {
        let Some(s) = &self.continuity.store else {
            return 0;
        };
        match page {
            Page::Facts => s.value.facts.len(),
            Page::Timeline => s.value.events.len(),
            Page::Threads => s.value.threads.len(),
            Page::Conflicts => self.continuity.conflicts.len(),
            _ => 0,
        }
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let c = &app.continuity;
    f.render_widget(Clear, area);
    let page = c.nav.current();
    match page {
        Page::Knowledge | Page::EditKnowledge | Page::KnowledgeDetail | Page::FilterKnowledge => {
            knowledge_ui::draw(f, app, area, theme)
        }
        Page::Facts | Page::Timeline | Page::Threads | Page::Conflicts => {
            let rows: Vec<String> = match (&c.store, page) {
                (Some(s), Page::Facts) => s
                    .value
                    .facts
                    .iter()
                    .map(|fact| {
                        let scope = fact
                            .exclusive_scope
                            .as_deref()
                            .map_or(String::new(), |sc| format!(" [scope: {}]", plain(sc)));
                        format!(
                            "#{} · {} · {}: {} = {}{}\n  {} · {}",
                            fact.id,
                            fact.category.label(),
                            plain(&fact.subject),
                            plain(&fact.property),
                            plain(&fact.value),
                            scope,
                            fact.certainty.label(),
                            fact.scene_name.as_deref().unwrap_or("Book-wide")
                        )
                    })
                    .collect(),
                (Some(s), Page::Timeline) => s
                    .value
                    .events
                    .iter()
                    .map(|ev| {
                        format!(
                            "#{} · [Order {}] {} ({})\n  Participants: {} · {}",
                            ev.id,
                            ev.order,
                            plain(&ev.title),
                            plain(&ev.story_date),
                            plain(&ev.participants.join(", ")),
                            plain(&ev.description)
                        )
                    })
                    .collect(),
                (Some(s), Page::Threads) => s
                    .value
                    .threads
                    .iter()
                    .map(|th| {
                        format!(
                            "#{} · [{}] {}\n  Introduced: {} · Resolved: {} · {}",
                            th.id,
                            th.status.label(),
                            plain(&th.title),
                            plain(th.introduced_in.as_deref().unwrap_or("—")),
                            plain(th.resolved_in.as_deref().unwrap_or("—")),
                            plain(&th.notes)
                        )
                    })
                    .collect(),
                (_, Page::Conflicts) => c
                    .conflicts
                    .iter()
                    .map(|conf| {
                        format!(
                            "[{}] {} · {} (facts #{} vs #{})\n  {}",
                            conf.severity.label(),
                            plain(&conf.subject),
                            plain(&conf.property),
                            conf.fact_a_id,
                            conf.fact_b_id,
                            plain(&conf.explanation)
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let rows = if rows.is_empty() {
                vec![match page {
                    Page::Facts => {
                        "No continuity facts recorded yet. Press n to add an explicit fact.".into()
                    }
                    Page::Timeline => {
                        "No timeline events recorded yet. Press n to add an event.".into()
                    }
                    Page::Threads => {
                        "No plot threads recorded yet. Press n to add a plot thread.".into()
                    }
                    Page::Conflicts => {
                        "No contradictions or inconsistencies detected across active facts.".into()
                    }
                    _ => String::new(),
                }]
            } else {
                rows
            };
            let title = match page {
                Page::Facts => "Continuity Tracker · 1 Facts",
                Page::Timeline => "Continuity Tracker · 2 Timeline",
                Page::Threads => "Continuity Tracker · 3 Plot Threads",
                Page::Conflicts => "Continuity Tracker · 4 Contradictions & Suggestions",
                _ => unreachable!(),
            };
            wros_shell::render_menu(
                f,
                area,
                wros_shell::Menu {
                    title,
                    rows: &rows,
                    selected: c.nav.selected(),
                    notice: &c.notice,
                    hint: if page == Page::Facts {
                        "1-5 tabs  n new  g record knowledge  t certainty  d delete  F1 help"
                    } else {
                        "1-5 tabs  n new  t status  d delete  c conflicts  Esc close  F1 help"
                    },
                },
                theme.into(),
            );
        }
        Page::AddFact => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Continuity Tracker · New Fact",
                &c.notice,
                "Tab/↑↓ field  ← category  → certainty  Enter save  Esc cancel",
                theme.into(),
            );
            if let Some(d) = &c.fact_draft {
                let mark = |i: usize| if d.field == i { "▸ " } else { "  " };
                let text = format!(
                    "Category (←): {}\nCertainty (→): {}\n\n{}Subject:  {}\n{}Property: {}\n{}Value:    {}\n{}Scope:    {} (set matching scope on two Definite facts for Definite Conflict)\n{}Notes:    {}",
                    d.category.unwrap_or(FactCategory::Character).label(),
                    d.certainty.unwrap_or(Certainty::Definite).label(),
                    mark(0),
                    plain(&d.subject),
                    mark(1),
                    plain(&d.property),
                    mark(2),
                    plain(&d.value),
                    mark(3),
                    plain(&d.scope),
                    mark(4),
                    plain(&d.notes),
                );
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
        Page::AddEvent => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Continuity Tracker · New Timeline Event",
                &c.notice,
                "Tab/↑↓ field  Enter save  Esc cancel",
                theme.into(),
            );
            if let Some(d) = &c.event_draft {
                let mark = |i: usize| if d.field == i { "▸ " } else { "  " };
                let text = format!(
                    "{}Order:        {}\n{}Title:        {}\n{}Story date:   {}\n{}Participants: {}\n{}Description:  {}",
                    mark(0),
                    plain(&d.order),
                    mark(1),
                    plain(&d.title),
                    mark(2),
                    plain(&d.story_date),
                    mark(3),
                    plain(&d.participants),
                    mark(4),
                    plain(&d.description),
                );
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
        Page::AddThread => {
            let body = wros_shell::render_panel(
                f,
                area,
                "Continuity Tracker · New Plot Thread",
                &c.notice,
                "Tab/↑↓ field  ←/→ status  Enter save  Esc cancel",
                theme.into(),
            );
            if let Some(d) = &c.thread_draft {
                let mark = |i: usize| if d.field == i { "▸ " } else { "  " };
                let text = format!(
                    "Status (←/→): {}\n\n{}Title:         {}\n{}Introduced in: {}\n{}Resolved in:   {}\n{}Notes:         {}",
                    d.status.unwrap_or(ThreadStatus::Open).label(),
                    mark(0),
                    plain(&d.title),
                    mark(1),
                    plain(&d.introduced_in),
                    mark(2),
                    plain(&d.resolved_in),
                    mark(3),
                    plain(&d.notes),
                );
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(theme.text)),
                    body,
                );
            }
        }
    }
}
