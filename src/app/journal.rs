//! Journal UI is a reader of immutable saved observations and the ordinary
//! Project reflection source. No file access, commands or saves during draw.
use super::*;
use anyhow::Context;
use chrono::{DateTime, FixedOffset, Local};
use grimoire_core::{
    documents::Pane,
    journal::{self, Day, Kind as FactKind, Observation, Store},
    sync::Fingerprint,
};
use ratatui::{
    Frame,
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};

pub struct Journal {
    pub date: String,
    pub day: Day,
    pub reflection: bool,
    pub scroll: usize,
    reader_width: std::cell::Cell<usize>,
    pub warning: String,
    pub source_error: Option<String>,
    source_nodes: Vec<usize>,
    writer: Option<Store>,
    writer_date: String,
    blocked: bool,
    pub pending: Vec<(FactKind, Observation)>,
    opened: DateTime<FixedOffset>,
    initial_words: Option<u64>,
    last_checkpoint: Option<Instant>,
    had_manuscript_save: bool,
    exit_queued: bool,
    reviewed: Option<String>,
    lesson_before: Option<(PathBuf, Fingerprint)>,
}
impl Journal {
    pub fn load(root: &Path, initial: Option<usize>) -> Self {
        let opened = Local::now().fixed_offset();
        let date = opened.format("%Y-%m-%d").to_string();
        let (writer, warning) = match Store::load(root, &date) {
            Ok(s) => (Some(s), String::new()),
            Err(e) => (None, format!("Journal original preserved: {e:#}")),
        };
        let day = writer
            .as_ref()
            .map(|s| s.day().clone())
            .unwrap_or_else(|| Day::empty(&date).unwrap());
        Self {
            writer,
            writer_date: date.clone(),
            blocked: !warning.is_empty(),
            date,
            day,
            warning,
            reflection: false,
            scroll: 0,
            reader_width: std::cell::Cell::new(40),
            source_nodes: Vec::new(),
            source_error: None,
            pending: Vec::new(),
            opened,
            initial_words: initial.map(|n| n as u64),
            last_checkpoint: None,
            had_manuscript_save: false,
            exit_queued: false,
            reviewed: None,
            lesson_before: None,
        }
    }
    fn read_view(&mut self, root: &Path, date: &str) {
        match Store::load(root, date) {
            Ok(s) => {
                self.day = s.day().clone();
                self.warning.clear();
            }
            Err(e) => {
                self.day = Day::empty(date).unwrap();
                self.warning = format!("Journal original preserved: {e:#}");
            }
        }
        self.date = date.into();
        self.scroll = 0;
    }
    fn append_pending(&mut self, root: &Path) -> Result<()> {
        if self.blocked {
            anyhow::bail!(
                "journal recording paused; r explicitly reloads/retries; pending observations remain in memory"
            );
        }
        while let Some((kind, o)) = self.pending.first().cloned() {
            let day = o.day()?;
            if self.writer_date != day || self.writer.is_none() {
                self.writer = Some(Store::load(root, &day)?);
                self.writer_date = day.clone();
            }
            self.writer.as_mut().unwrap().append(kind, o)?;
            self.pending.remove(0);
            if self.date == day {
                self.day = self.writer.as_ref().unwrap().day().clone();
                self.warning.clear();
            }
        }
        Ok(())
    }
    fn flush(&mut self, root: &Path) -> bool {
        match self.append_pending(root) {
            Ok(()) => true,
            Err(e) => {
                self.blocked = true;
                self.warning = format!(
                    "Journal not recorded: {e:#}; {} pending in memory",
                    self.pending.len()
                );
                false
            }
        }
    }
    pub fn review_lesson_start(&mut self) {
        self.reviewed = None;
        self.lesson_before = None;
    }
    #[cfg(test)]
    pub fn expire_checkpoint_for_test(&mut self) {
        self.last_checkpoint = Some(Instant::now() - Duration::from_secs(601));
    }
    pub fn fact_count(&self) -> usize {
        self.day.facts.len()
    }
}
impl App {
    fn journal_observation(&self) -> Result<Observation> {
        if self.project.nodes.iter().any(|n| {
            n.in_manuscript
                && n.kind == Kind::Scene
                && (n.read_only || n.disk.unavailable.is_some())
        }) {
            anyhow::bail!(
                "manuscript total unavailable: a source is unreadable/unsettled; no complete-count fact invented"
            );
        }
        let at = Local::now().fixed_offset();
        let daily = (self.journal.initial_words.is_some()
            && at.format("%Y-%m-%d").to_string() == self.baseline_date)
            .then(|| self.today_words());
        Observation::new(
            at,
            self.journal.opened,
            self.journal.initial_words,
            self.project.total_words() as u64,
            daily,
        )
    }
    /// Called only after a completely clean, successful manuscript save.
    pub fn journal_saved(&mut self, saved: &[usize]) {
        self.productivity_saved_sources(saved);
        let manuscript = saved.iter().any(|&i| self.project.nodes[i].in_manuscript);
        if manuscript {
            self.journal.had_manuscript_save = true;
            self.journal.exit_queued = false;
            // A previously refused exit request was cancelled by resumed work.
            self.journal
                .pending
                .retain(|(k, _)| *k != FactKind::ExitRequested);
            if self.journal.pending.is_empty()
                && self
                    .journal
                    .last_checkpoint
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(600))
            {
                match self.journal_observation() {
                    Ok(o) => {
                        self.journal.pending.push((FactKind::SavedCheckpoint, o));
                        self.journal.last_checkpoint = Some(Instant::now());
                        self.flush_journal();
                    }
                    Err(e) => {
                        self.journal.warning = format!("Journal observation unavailable: {e:#}")
                    }
                }
            }
        }
        let completed = self
            .journal
            .lesson_before
            .as_ref()
            .is_some_and(|(path, before)| {
                saved.iter().any(|&i| {
                    let n = &self.project.nodes[i];
                    n.path == *path
                        && !n.in_manuscript
                        && !n.body.trim().is_empty()
                        && Fingerprint::of(&n.body) != *before
                })
            });
        if completed
            && self.journal.reviewed.as_deref() == Some(self.journal.date.as_str())
            && self.deck.progress.active == Some(14)
        {
            self.observe_deck(grimoire_core::tutorial::Event::JournalReflectionSaved);
            self.journal.lesson_before = None;
        }
    }
    fn flush_journal(&mut self) -> bool {
        let ok = self.journal.flush(&self.project.root);
        if !ok {
            self.msg = self.journal.warning.clone();
        }
        ok
    }
    /// Accurate request boundary, not an assertion that the process exited.
    pub fn journal_exit_checkpoint(&mut self) -> bool {
        if !self.journal.had_manuscript_save {
            return true;
        }
        if !self.journal.exit_queued {
            match self.journal_observation() {
                Ok(o) => {
                    self.journal.pending.push((FactKind::ExitRequested, o));
                    self.journal.exit_queued = true;
                }
                Err(e) => {
                    self.msg = format!("Journal exit observation unavailable: {e:#}");
                    return false;
                }
            }
        }
        self.flush_journal()
    }
    pub fn refresh_journal_source(&mut self) {
        self.journal.source_nodes.clear();
        self.journal.source_error = None;
        let result = (|| -> Result<()> {
            let relative = journal::reflection_path(Path::new(""), &self.journal.date)?;
            let id = grimoire_core::pins::identify(&self.project.root, &relative)?;
            let mut nodes = Vec::new();
            for (i, n) in self
                .project
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == Kind::Scene)
            {
                if let Ok(path) = n.path.strip_prefix(&self.project.root)
                    && let Ok(other) = grimoire_core::pins::identify(&self.project.root, path)
                    && id.same_file(&other)
                {
                    if n.in_manuscript {
                        anyhow::bail!(
                            "Reflection path aliases a manuscript source; repair the path; no version guessed"
                        );
                    }
                    nodes.push(i);
                }
            }
            if nodes.is_empty() {
                anyhow::bail!(
                    "Reflection is not loaded; Enter explicitly loads its original after save guards"
                );
            }
            self.journal.source_nodes = nodes;
            Ok(())
        })();
        if let Err(e) = result {
            self.journal.source_error = Some(format!(
                "Reflection unavailable: {e:#}. Enter explicitly creates/opens its original, never a copied body."
            ));
        }
    }
    pub fn journal_source_node(&self) -> Option<usize> {
        [Pane::Primary, Pane::Secondary]
            .into_iter()
            .filter_map(|p| self.view_open(p))
            .find(|i| self.journal.source_nodes.contains(i))
            .or_else(|| self.journal.source_nodes.first().copied())
    }
    pub fn journal_body(&self) -> std::result::Result<&str, String> {
        if let Some(e) = &self.journal.source_error {
            return Err(e.clone());
        }
        if let (Some(a), Some(b)) = (
            self.view_open(Pane::Primary),
            self.view_open(Pane::Secondary),
        ) && a != b
            && self.journal.source_nodes.contains(&a)
            && self.journal.source_nodes.contains(&b)
        {
            return Err("Reflection has two retained editor contexts; repair external aliases first; no version guessed".into());
        }
        let n = self
            .journal_source_node()
            .and_then(|i| self.project.nodes.get(i))
            .ok_or("Reflection not loaded; Enter explicitly opens/creates the source")?;
        if n.read_only || n.parked || n.disk.unavailable.is_some() {
            return Err(
                "Reflection source unavailable/unsettled; no editable copy invented".into(),
            );
        }
        Ok(&n.body)
    }
    pub fn open_journal(&mut self) {
        self.flush();
        self.tree_drag = None;
        self.journal.reflection = false;
        let date = Local::now().format("%Y-%m-%d").to_string();
        self.journal.read_view(&self.project.root, &date);
        self.refresh_journal_source();
        self.overlay = Overlay::Journal;
        self.msg="Read-only saved observations; Tab views a separate original reflection; no active-time or gross-word estimates".into();
    }
    pub fn select_journal_date(&mut self, date: &str) -> Result<()> {
        journal::date(date)?;
        self.journal.read_view(&self.project.root, date);
        self.refresh_journal_source();
        Ok(())
    }
    pub fn retry_journal(&mut self) {
        // Deliberate reload; ordinary autosave never adopts an external log
        // before retrying a conflicting append. Exact observations deduplicate.
        self.journal.writer = None;
        self.journal.blocked = false;
        let ok = self.flush_journal();
        let date = self.journal.date.clone();
        self.journal.read_view(&self.project.root, &date);
        self.refresh_journal_source();
        if ok {
            self.msg =
                "Journal reloaded; pending observations appended without replacing existing facts"
                    .into();
        }
    }
    fn journal_track_reflection(&mut self, i: usize) {
        if self.deck.progress.active == Some(14) {
            self.journal.lesson_before = Some((
                self.project.nodes[i].path.clone(),
                Fingerprint::of(&self.project.nodes[i].body),
            ));
        }
    }
    fn focus_journal_owner(&mut self) -> bool {
        for pane in [Pane::Primary, Pane::Secondary] {
            if self
                .view_open(pane)
                .is_some_and(|i| self.journal.source_nodes.contains(&i))
            {
                if pane == Pane::Secondary {
                    if self.focus_mode {
                        self.msg="Esc, then Ctrl-D leaves strict focus before focusing a reflection already owned by Secondary".into();
                        return true;
                    }
                    self.views.secondary_visible = true;
                }
                let i = self.view_open(pane).unwrap();
                self.journal_track_reflection(i);
                self.activate_editor(pane);
                self.overlay = Overlay::None;
                self.msg =
                    "Reflection already owned; focused its original editor without a duplicate"
                        .into();
                return true;
            }
        }
        false
    }
    pub fn edit_journal_reflection(&mut self, primary: bool) {
        self.overlay = Overlay::Journal;
        if self.focus_mode && !primary {
            self.msg="Esc, then Ctrl-D leaves strict focus before a reflection can open beside the manuscript; e uses Primary".into();
            return;
        }
        self.flush();
        if let Err(e) = self.ensure_document_claims() {
            self.rescue();
            self.save_state = SaveState::Failed(e.to_string());
            self.last_attempt = Some(Instant::now());
            self.msg =
                format!("Reflection not opened: {e}; all source buffers kept; recovery attempted");
            return;
        }
        self.refresh_journal_source();
        if self.journal_body().is_ok() && self.focus_journal_owner() {
            return;
        }
        if !self.commit_saves() || self.project.dirty_count() != 0 {
            return;
        }
        let result = (|| -> Result<usize> {
            let path = journal::ensure_reflection(&self.project.root, &self.journal.date)?;
            if !self.project.nodes.iter().any(|n| n.path == path) {
                self.reload_tree()?;
            }
            self.refresh_journal_source();
            self.journal_body().map_err(anyhow::Error::msg)?;
            self.journal_source_node().context("reflection not loaded")
        })();
        let i = match result {
            Ok(i) => i,
            Err(e) => {
                self.msg = format!("Reflection not opened: {e:#}; all source buffers kept");
                return;
            }
        };
        self.journal_track_reflection(i);
        if primary {
            self.open_scene(i);
            if self.open == Some(i) {
                self.overlay = Overlay::None;
            }
        } else {
            self.open_secondary_node(i);
        }
    }
    pub fn capture_journal_checkpoint(&mut self) {
        self.flush();
        if self.project.dirty_count() != 0 {
            self.msg =
                "Save both source buffers with Ctrl-S first; no unsaved snapshot recorded".into();
            return;
        }
        if !self.journal.had_manuscript_save {
            self.msg="No successful manuscript save in this app/book context; viewing or note editing is not a writing fact".into();
            return;
        }
        if let Err(e) = self.ensure_document_claims() {
            self.rescue();
            self.msg = format!("Snapshot not recorded: {e}; source buffers kept");
            return;
        }
        if !self.journal.pending.is_empty() {
            self.msg =
                "Pending observations remain unrecorded; r explicitly reloads/retries".into();
            return;
        }
        let o = match self.journal_observation() {
            Ok(o) => o,
            Err(e) => {
                self.msg = format!("Snapshot unavailable: {e:#}");
                return;
            }
        };
        let same = self
            .journal
            .writer
            .as_ref()
            .and_then(|s| {
                s.day()
                    .facts
                    .iter()
                    .rev()
                    .find(|f| f.observation.app_opened_at == o.app_opened_at)
            })
            .is_some_and(|f| {
                f.observation.manuscript_words == o.manuscript_words
                    && f.observation.initial_manuscript_words == o.initial_manuscript_words
                    && f.observation.daily_net_words == o.daily_net_words
                    && f.observation.day().ok() == o.day().ok()
            });
        if same {
            self.msg =
                "These measured counts are already recorded; no duplicate snapshot added".into();
            return;
        }
        self.journal.pending.push((FactKind::SavedCheckpoint, o));
        self.journal.last_checkpoint = Some(Instant::now());
        if self.flush_journal() {
            self.msg =
                "Current clean saved Project counts recorded; no gross-word or time estimate"
                    .into();
        }
    }
    pub fn on_journal_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.overlay = Overlay::None;
            }
            Key::Left | Key::Right => {
                let d = journal::date(&self.journal.date).unwrap();
                let days = chrono::Days::new(1);
                let next = if key == Key::Right {
                    d.checked_add_days(days)
                } else {
                    d.checked_sub_days(days)
                };
                if let Some(d) = next {
                    let _ = self.select_journal_date(&d.format("%Y-%m-%d").to_string());
                }
            }
            Key::Char('t') => {
                let date = Local::now().format("%Y-%m-%d").to_string();
                let _ = self.select_journal_date(&date);
            }
            Key::Tab | Key::BackTab => {
                if !self.journal.reflection && !self.journal.day.facts.is_empty() {
                    self.journal.reviewed = Some(self.journal.date.clone());
                }
                self.journal.reflection = !self.journal.reflection;
                self.journal.scroll = 0;
            }
            Key::Char('r') => self.retry_journal(),
            Key::Char('c') => self.capture_journal_checkpoint(),
            Key::Enter => self.edit_journal_reflection(false),
            Key::Char('e') => self.edit_journal_reflection(true),
            Key::Home => self.journal.scroll = 0,
            Key::Up | Key::PageUp => {
                self.journal.scroll =
                    self.journal
                        .scroll
                        .saturating_sub(if key == Key::PageUp { 10 } else { 1 })
            }
            Key::Down | Key::PageDown | Key::End => {
                let max = if self.journal.reflection {
                    self.journal_body()
                        .ok()
                        .map(|b| {
                            grimoire_core::pins::excerpt(
                                b,
                                self.journal.reader_width.get().max(1),
                                None,
                                0,
                                0,
                            )
                            .total_rows
                            .saturating_sub(1)
                        })
                        .unwrap_or(0)
                } else {
                    self.journal
                        .fact_count()
                        .saturating_mul(3)
                        .saturating_sub(1)
                };
                self.journal.scroll = if key == Key::End {
                    max
                } else {
                    (self.journal.scroll + if key == Key::PageDown { 10 } else { 1 }).min(max)
                };
            }
            _ => {}
        }
    }
}
fn fact_rows(day: &Day, start: usize, height: usize) -> Vec<Line<'static>> {
    if day.facts.is_empty() {
        return vec![
            Line::from("No recorded manuscript save checkpoints for this local date."),
            Line::from(
                "Write/save manuscript text first; opening this page or editing notes is not a writing fact.",
            ),
        ];
    }
    let mut rows = Vec::new();
    for f in day.facts.iter().rev().skip(start / 3).take(height / 3 + 2) {
        let o = &f.observation;
        rows.push(Line::from(format!(
            "#{} {} · {}",
            f.id,
            f.kind.name(),
            DateTime::parse_from_rfc3339(&o.at)
                .unwrap()
                .format("%H:%M:%S %:z")
        )));
        rows.push(Line::from(format!(
            "Book {} · daily net {} · since-open delta {}",
            o.manuscript_words,
            o.daily_net_words
                .map(|n| format!("{n:+}"))
                .unwrap_or_else(|| "unavailable".into()),
            o.delta_since_open()
                .map(|n| format!("{n:+}"))
                .unwrap_or_else(|| "unavailable".into())
        )));
        rows.push(Line::from(format!(
            "App opened {} · initial {} words",
            DateTime::parse_from_rfc3339(&o.app_opened_at)
                .unwrap()
                .format("%Y-%m-%d %H:%M:%S %:z"),
            o.initial_manuscript_words
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unavailable".into())
        )));
    }
    rows.into_iter().skip(start % 3).take(height).collect()
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    f.render_widget(Clear, area);
    let title = format!(
        " Writing journal · {} · {} ",
        app.journal.date,
        if app.journal.reflection {
            "Reflection source"
        } else {
            "Recorded facts"
        }
    );
    let block = ratatui::widgets::Block::bordered()
        .title(title)
        .border_style(Style::default().fg(t.accent));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 8 || inner.height < 3 {
        return;
    }
    let info = if app.journal.reflection {
        "MANUAL REFLECTION — separate ordinary Markdown; Enter/e explicitly edits its original."
    } else {
        "AUTOMATIC — sampled saved Project counts (≥10min); c captures current clean counts; no gross/time estimates."
    };
    f.render_widget(
        Paragraph::new(info).style(Style::default().fg(t.dim)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let content = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(3),
    );
    app.journal.reader_width.set(content.width as usize);
    let rows = if app.journal.reflection {
        match app.journal_body() {
            Ok(body) => {
                let e = grimoire_core::pins::excerpt(
                    body,
                    content.width as usize,
                    None,
                    app.journal.scroll,
                    content.height as usize,
                );
                if body.trim().is_empty() {
                    vec![Line::from(
                        "Empty reflection. Enter/e opens the original editor; preview typing is ignored.",
                    )]
                } else {
                    e.lines.into_iter().map(Line::from).collect()
                }
            }
            Err(e) => vec![Line::from(e)],
        }
    } else if app.journal.day.facts.is_empty() && !app.journal.warning.is_empty() {
        vec![Line::from(
            "Recorded observations unavailable; original preserved, not assumed empty.",
        )]
    } else {
        fact_rows(
            &app.journal.day,
            app.journal.scroll,
            content.height as usize,
        )
    };
    f.render_widget(
        Paragraph::new(rows).style(Style::default().fg(t.text)),
        content,
    );
    // Main's safety instruction must outrank a long metadata/count warning,
    // even when unavailable totals prevented constructing any pending fact.
    let note = if app.msg.contains("Q again to quit anyway") {
        app.msg.clone()
    } else if !app.journal.pending.is_empty() {
        format!(
            "{} pending, NOT recorded · r reload/retry · {}",
            app.journal.pending.len(),
            app.msg
        )
    } else if !app.journal.warning.is_empty() {
        app.journal.warning.clone()
    } else {
        app.msg.clone()
    };
    f.render_widget(
        Paragraph::new(note).style(Style::default().fg(t.accent)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
    f.render_widget(
        Paragraph::new(if inner.width >= 76 {
            "←→ date  Tab view  ↑↓/Pg scroll  c snapshot  Enter/e edit  r retry  Esc"
        } else {
            "Tab view  Enter edit  r retry  Esc back"
        })
        .style(Style::default().fg(t.dim)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
