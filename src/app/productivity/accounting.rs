use super::Page;
use crate::app::{App, Overlay};
use anyhow::Context;
use grimoire_core::{
    accounting::{Role, Source, Totals},
    editor::Editor,
    measure::{Counts, Mutation, Origin},
    productivity::{self as model, Metric, Snapshot, SprintKind, SprintStatus},
    project::Kind,
    statefile::Store,
};
use std::collections::BTreeSet;
use std::path::Path;

/// Every real source editor opts in; App applies disabled/unavailable policy
/// before routing another input. Programmatic loads never enqueue input.
pub fn source_editor(text: &str) -> Editor {
    let mut e = Editor::from_text(text);
    e.measure_operations(true);
    e
}
impl App {
    pub(super) fn metric_source(&self, i: usize) -> Option<Source> {
        let n = self.project.nodes.get(i)?;
        if n.kind != Kind::Scene || n.read_only || n.never_read() {
            return None;
        }
        let path = n.path.strip_prefix(&self.project.root).ok()?.to_path_buf();
        let role = if n.in_manuscript && !self.project.in_trash(i) {
            Role::Manuscript
        } else if path.starts_with("notes")
            || path.starts_with("characters")
            || path.starts_with("places")
        {
            Role::Notes
        } else if path.starts_with("research") {
            Role::Research
        } else {
            Role::Other
        };
        Some(Source { path, role })
    }
    /// Drain before any source editor is adopted/detached; moving two view
    /// contexts itself stays pure and can still be used while drawing.
    pub fn capture_productivity(&mut self) {
        let mut batches = vec![(self.open, self.editor.take_mutations())];
        if let Some(other) = &mut self.views.other {
            batches.push((other.open, other.editor.take_mutations()));
        }
        for (i, mutations) in batches {
            if let Some(source) = i.and_then(|i| self.metric_source(i)) {
                for mutation in mutations {
                    self.productivity.record(source.clone(), mutation);
                }
            }
        }
        let on = self.productivity.tracking();
        self.editor.measure_operations(on);
        if let Some(other) = &mut self.views.other {
            other.editor.measure_operations(on);
        }
    }
    pub fn record_source_operation(&mut self, i: usize, origin: Origin, counts: Counts) {
        if let Some(source) = self.metric_source(i) {
            self.productivity
                .record(source, Mutation { origin, counts });
        }
    }
    pub fn adopt_measured_source(&mut self, i: usize, text: &str, origin: Origin, counts: Counts) {
        if self.open == Some(i) {
            self.editor
                .set_text_from_measured_operation(text, origin, counts);
        } else if let Some(other) = &mut self.views.other
            && other.open == Some(i)
        {
            other
                .editor
                .set_text_from_measured_operation(text, origin, counts);
        } else {
            self.record_source_operation(i, origin, counts);
        }
    }
    pub fn refresh_productivity_snapshot(&mut self) {
        let complete_native = !self.project.nodes.iter().any(|n| {
            n.in_manuscript
                && n.kind == Kind::Scene
                && (n.read_only || n.disk.unavailable.is_some())
        });
        let scenes = self
            .project
            .nodes
            .iter()
            .filter(|n| {
                n.kind == Kind::Scene
                    && n.in_manuscript
                    && !n.read_only
                    && n.disk.unavailable.is_none()
            })
            .filter_map(|n| {
                Some((
                    n.path.strip_prefix(&self.project.root).ok()?.to_path_buf(),
                    n.words() as u64,
                ))
            })
            .collect();
        self.productivity.snapshot = Snapshot {
            complete: self.productivity.ledger.complete
                && self.productivity.unrecorded == Counts::default(),
            entries: self.productivity.ledger.known.clone(),
            manuscript_words: complete_native.then(|| self.project.total_words() as u64),
            scenes,
        };
        let now = self.productivity.clock.now();
        let totals = Totals::collect(
            self.productivity.ledger.known.iter(),
            &now.day(),
            &now.day(),
            Role::Manuscript,
        );
        let footnotes = self.productivity.config.include_footnotes;
        self.productivity.summary = if !self.productivity.snapshot.complete {
            "Recorded metrics unavailable/incomplete; no totals guessed".into()
        } else if totals.input.gaps + totals.replay.gaps > 0 {
            format!(
                "{} word totals unavailable ({} gaps) · chars +{} / −{}",
                now.day(),
                totals.input.gaps + totals.replay.gaps,
                totals.input.inserted_chars,
                totals.input.deleted_chars
            )
        } else {
            format!(
                "{} +{} / −{} · net {}",
                now.day(),
                totals.input.added_words(footnotes),
                totals.input.removed_words(footnotes),
                totals.net(footnotes)
            )
        };
        self.productivity.stats_date = now.day();
        let p = &self.productivity;
        self.productivity.goal_summary = p.config.goals.iter().find(|g| g.enabled).map_or(
            "No enabled advanced goals · Enter adds/reviews".into(),
            |g| {
                format!(
                    "{}: {} / {} (checkpoint)",
                    g.label,
                    p.snapshot
                        .goal(g, &now.day(), p.config.include_footnotes)
                        .map_or("unavailable".into(), |n| n.to_string()),
                    g.target
                )
            },
        );
        let p = &self.productivity;
        self.productivity.calendar_summary = p.config.schedule.as_ref().map_or(
            "No calendar schedule · n creates the November preset".into(),
            |s| {
                let net = p.snapshot.value(
                    Metric::NetInput,
                    &s.start,
                    &s.end,
                    None,
                    None,
                    p.config.include_footnotes,
                );
                format!(
                    "{}→{}: {} / {} recorded (checkpoint)",
                    s.start,
                    s.end,
                    net.map_or("unavailable".into(), |n| n.to_string()),
                    s.target
                )
            },
        );
    }
    /// Debounced metadata checkpoints are independent of prose saves. A normal
    /// quit/book switch refuses to discard pending records; force quit remains
    /// an explicit, separately labelled user choice.
    pub fn checkpoint_productivity(&mut self, force: bool) -> bool {
        self.capture_productivity();
        let now = self.productivity.clock.now();
        if !force && now.mono_ms.saturating_sub(self.productivity.last_flush) < 30_000 {
            return true;
        }
        self.productivity.last_flush = now.mono_ms;
        if let Err(e) = self.productivity.ledger.flush() {
            self.productivity.warning =
                format!("Edit ledger not recorded: {e:#}; pending groups retained; r reload/retry");
            return false;
        }
        self.refresh_productivity_snapshot();
        let reading = self.productivity.sprint.as_ref().and_then(|s| {
            self.productivity
                .snapshot
                .lifetime(s.metric(), s.mode, s.include_footnotes)
        });
        if let Some(s) = &mut self.productivity.sprint {
            s.advance(&now, reading);
        }
        if self.productivity.store.is_some() {
            let mut next = self.productivity.config.clone();
            next.sprint = self.productivity.sprint.clone();
            let mut awards = match next.observe_awards(&self.productivity.snapshot, &now.stamp()) {
                Ok(a) => a,
                Err(e) => {
                    self.productivity.warning =
                        format!("Optional milestones not recorded: {e:#}; ledger/prose preserved");
                    Vec::new()
                }
            };
            for id in self.deck.progress.completed_lessons() {
                let label = format!("Observed offline learning exercise {id}");
                if next
                    .award(&format!("learning-{id}"), &label, false, &now.stamp())
                    .unwrap_or(false)
                {
                    awards.push(label);
                }
            }
            if let Some(s) = &next.sprint
                && s.status == SprintStatus::Completed
                && s.kind != SprintKind::Break
                && !s.metrics_incomplete
                && (s.kind != SprintKind::NoEditing || !s.deletion_observed)
            {
                let id = format!("sprint-{}", s.id);
                if next
                    .award(&id, "Completed an optional sprint", true, &now.stamp())
                    .unwrap_or(false)
                {
                    awards.push("Completed an optional sprint".into());
                }
            }
            if next != self.productivity.config {
                let value = next.clone();
                let critical = serde_json::to_value(&next.sprint).ok()
                    != serde_json::to_value(&self.productivity.config.sprint).ok();
                if let Err(e) = self.productivity.update(|s| {
                    *s = value;
                    Ok(())
                }) {
                    self.productivity.warning = if critical {
                        format!("Sprint checkpoint not recorded: {e:#}; live runtime retained")
                    } else {
                        format!(
                            "Optional milestone checkpoint not recorded: {e:#}; operation ledger and saved prose retained"
                        )
                    };
                    if critical {
                        return false;
                    }
                    return self.productivity.unrecorded == Counts::default();
                }
            }
            if !awards.is_empty() && self.productivity.config.notifications && !self.focus_mode {
                self.msg = format!("Optional milestone: {}", awards.join("; "));
            }
        }
        if self.productivity.unrecorded != Counts::default() {
            return false;
        }
        if self.productivity.store.is_some() {
            self.productivity.warning.clear();
        }
        if let Overlay::Productivity { page, sel, .. } = self.overlay {
            self.productivity.build_rows(page, sel);
        }
        true
    }
    pub fn tick_productivity(&mut self) {
        let now = self.productivity.clock.now();
        if self
            .productivity
            .last_tick
            .is_some_and(|t| now.mono_ms >= t && now.mono_ms - t < 1000)
            && self.productivity.stats_date == now.day()
        {
            return;
        }
        self.productivity.last_tick = Some(now.mono_ms);
        if now.mono_ms < self.productivity.last_flush {
            self.productivity.activity.pause();
            self.productivity.last_flush = now.mono_ms;
        }
        if self
            .productivity
            .sprint
            .as_ref()
            .is_some_and(|s| s.time_due(&now))
        {
            self.checkpoint_productivity(true);
            return;
        }
        let reading = self.productivity.sprint.as_ref().and_then(|s| {
            self.productivity
                .snapshot
                .lifetime(s.metric(), s.mode, s.include_footnotes)
        });
        let finished = self
            .productivity
            .sprint
            .as_mut()
            .is_some_and(|s| s.advance(&now, reading));
        if finished
            || self.productivity.ledger.pending_count() > 0
            || self.productivity.sprint.as_ref().is_some_and(|s| {
                (s.status == SprintStatus::Running && s.anchor_ms.is_some())
                    || s.break_timer
                        .as_ref()
                        .is_some_and(|b| b.status == SprintStatus::Running && b.anchor_ms.is_some())
            })
        {
            self.checkpoint_productivity(finished);
        }
        if let Overlay::Productivity {
            page: Page::Sprints,
            sel,
            ..
        } = self.overlay
        {
            self.productivity.build_rows(Page::Sprints, sel);
        }
        if self.productivity.stats_date != now.day() {
            self.refresh_productivity_snapshot();
        }
        if self.productivity.date != now.day() && self.productivity.day.is_none() {
            self.productivity.date = now.day();
        }
    }
    pub fn pause_productivity_activity(&mut self) {
        self.productivity.activity.pause();
    }
    pub fn reload_productivity(&mut self) {
        self.capture_productivity();
        // Materialize the live monotonic segment before comparing serialized
        // state: an un-ticked running timer is still unsaved local work.
        let now = self.productivity.clock.now();
        let reading = self.productivity.sprint.as_ref().and_then(|s| {
            self.productivity
                .snapshot
                .lifetime(s.metric(), s.mode, s.include_footnotes)
        });
        if let Some(s) = &mut self.productivity.sprint {
            s.advance(&now, reading);
        }
        let root = self.project.root.clone();
        match Store::load(
            &root,
            Path::new(".grimoire/writingdeck/productivity.json"),
            model::Settings::default(),
        ) {
            Ok(s) => {
                let before = serde_json::to_value(&self.productivity.config.sprint).ok();
                let incoming = serde_json::to_value(&s.value.sprint).ok();
                let live = serde_json::to_value(&self.productivity.sprint).ok();
                let pending_sprint_work = self.productivity.sprint.is_some()
                    && (self.productivity.ledger.pending_count() > 0
                        || self.productivity.unrecorded != Counts::default());
                if incoming != before && (live != before || pending_sprint_work) {
                    self.productivity.warning="Conflicting external and uncheckpointed local sprint states; both originals retained, no automatic merge/overwrite".into();
                    self.msg = self.productivity.warning.clone();
                    return;
                }
                if incoming != before {
                    self.productivity.sprint = s.value.sprint.clone();
                }
                self.productivity.config = s.value.clone();
                self.productivity.store = Some(s);
            }
            Err(e) => {
                self.productivity.warning =
                    format!("Settings reload refused: {e:#}; original retained");
                return;
            }
        }
        self.productivity.ledger.reload();
        self.productivity.day = None;
        self.productivity.activity.pause();
        self.capture_productivity();
        if self.checkpoint_productivity(true) {
            self.msg = "Recorded metadata explicitly reloaded/retried; prose never replaced".into();
        } else {
            self.msg = self.productivity.warning.clone();
        }
    }
    pub fn productivity_saved_sources(&mut self, saved: &[usize]) {
        if !self.productivity.config.achievements || self.productivity.store.is_none() {
            return;
        }
        let finished = |n: &grimoire_core::project::Node| {
            n.meta("status").is_some_and(|s| {
                matches!(
                    s.to_lowercase().as_str(),
                    "done" | "finished" | "complete" | "completed" | "final"
                )
            })
        };
        let mut sources = Vec::new();
        let mut parents = BTreeSet::new();
        for &i in saved {
            if let Some(source) = self.metric_source(i).filter(|s| s.role == Role::Manuscript)
                && finished(&self.project.nodes[i])
            {
                sources.push((source.path, "Saved a scene marked finished (author status)"));
                if let Some(parent) = self.parents[i] {
                    parents.insert(parent);
                }
            }
        }
        for parent in parents {
            let scenes: Vec<_> = self
                .project
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| {
                    n.kind == Kind::Scene && n.in_manuscript && self.parents[*i] == Some(parent)
                })
                .collect();
            if !scenes.is_empty()
                && scenes.iter().all(|(_, n)| {
                    !n.dirty && !n.read_only && n.disk.unavailable.is_none() && finished(n)
                })
                && let Ok(path) = self.project.nodes[parent]
                    .path
                    .strip_prefix(&self.project.root)
            {
                sources.push((
                    path.to_path_buf(),
                    "Chapter's scenes all marked finished (author statuses)",
                ));
            }
        }
        if sources.is_empty() {
            return;
        }
        let now = self.productivity.clock.now();
        let result = self.productivity.update(|settings| {
            for (path, label) in sources {
                let id = format!(
                    "finished-source-{:x}",
                    grimoire_core::sync::Fingerprint::of(&path.to_string_lossy()).hash
                );
                if settings.award(&id, label, false, &now.stamp())? {
                    settings
                        .awards
                        .last_mut()
                        .context("New award unavailable")?
                        .source = Some(path);
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            self.productivity.warning = format!(
                "Optional finished-source status milestone not recorded: {e:#}; Markdown status retained"
            );
        }
    }
}
