//! Real operation accounting with ordinary Markdown still the only prose.
//! Draw reads cached rows only. No timer infers writing from an open window.
use super::*;
use anyhow::{Context, bail};
use chrono::{Datelike, Duration as CivilDuration};
use grimoire_core::{
    accounting::{ActiveClock, Clock, Ledger, Mode as WorkMode, Role, Source, SystemClock, Totals},
    challenges::{self, Category, Day, Evidence, Profile, Status},
    measure::{Counts, Mutation, Origin},
    productivity::{
        self as model, Goal, Metric, Period, Schedule, Snapshot, Sprint, SprintKind, SprintStatus,
    },
    statefile::{self, Store},
};
use ratatui::{
    Frame,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Goals,
    Calendar,
    Sprints,
    Achievements,
    Challenges,
    GoalTypes,
    SprintTypes,
    ChallengeFilters,
}
impl Page {
    pub const TABS: [Self; 6] = [
        Self::Overview,
        Self::Goals,
        Self::Calendar,
        Self::Sprints,
        Self::Achievements,
        Self::Challenges,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "Productivity · recorded edits",
            Self::Goals => "Measured and native-source goals",
            Self::Calendar => "Calendar · rest days and pace",
            Self::Sprints => "Advanced sprints · recorded results",
            Self::Achievements => "Optional milestone collection",
            Self::Challenges => "Daily challenges · local civil date",
            Self::GoalTypes => "New goal · choose a metric",
            Self::SprintTypes => "New sprint · choose a variant",
            Self::ChallengeFilters => "Future challenge filters · explicit save",
        }
    }
    fn hints(self) -> &'static str {
        match self {
            Self::Overview => {
                "t tracking · m mode · f footnotes · i idle · a milestones · n notices · d challenges"
            }
            Self::Goals => {
                "n new goal · Space enable/disable selected · Delete remove goal metadata"
            }
            Self::Calendar => {
                "n November 50k preset · Space rest day · b start · e end · g target · ↑↓ date"
            }
            Self::Sprints => {
                "n new · p pause/resume · x stop · b timed break · k skip break (retained)"
            }
            Self::Achievements => {
                "a enable/disable · n quiet notices · no streak penalties or quality scores"
            }
            Self::Challenges => {
                "a accept · c check measured · x self-report · R reroll · f filters · S optional Spark · ←→ date"
            }
            Self::GoalTypes | Self::SprintTypes => {
                "↑↓ choose · Enter set target · Esc back · no manuscript changed"
            }
            Self::ChallengeFilters => {
                "↑↓ row · Space category · ←→ mode/level/time · Enter save · Esc cancel"
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Goal(usize),
    ScheduleStart,
    ScheduleEnd,
    ScheduleTarget,
    Idle,
    Sprint(SprintKind),
}
pub struct Productivity {
    pub render_fit: std::cell::Cell<(usize, usize)>,
    pub store: Option<Store<model::Settings>>,
    pub config: model::Settings,
    pub ledger: Ledger,
    pub clock: Box<dyn Clock>,
    pub activity: ActiveClock,
    pub sprint: Option<Sprint>,
    pub snapshot: Snapshot,
    pub day: Option<Store<Day>>,
    pub date: String,
    pub warning: String,
    pub rows: Vec<String>,
    pub row_selection: Option<usize>,
    pub draft_profile: Option<Profile>,
    last_flush: u64,
    last_tick: Option<u64>,
    pub unrecorded: Counts,
    pub summary: String,
    pub stats_date: String,
    pub goal_summary: String,
    pub calendar_summary: String,
}
impl Productivity {
    pub fn load(root: &Path) -> Self {
        let clock: Box<dyn Clock> = Box::<SystemClock>::default();
        let now = clock.now();
        let (store, config, warning) = match Store::load(
            root,
            Path::new(".grimoire/writingdeck/productivity.json"),
            model::Settings::default(),
        ) {
            Ok(s) => {
                let c = s.value.clone();
                (Some(s), c, String::new())
            }
            Err(e) => {
                let c = model::Settings {
                    tracking: false,
                    achievements: false,
                    challenges: false,
                    ..model::Settings::default()
                };
                (
                    None,
                    c,
                    format!("Productivity settings unavailable: {e:#}; originals preserved"),
                )
            }
        };
        let sprint = config.sprint.clone();
        Self {
            render_fit: std::cell::Cell::new((0, 0)),
            store,
            config,
            ledger: Ledger::load(root),
            clock,
            activity: ActiveClock::default(),
            sprint,
            snapshot: Snapshot::default(),
            day: None,
            date: now.day(),
            warning,
            rows: vec![],
            row_selection: None,
            draft_profile: None,
            last_flush: now.mono_ms,
            last_tick: None,
            unrecorded: Counts::default(),
            summary: "Recorded metrics starting; no baseline-derived gross/time".into(),
            stats_date: now.day(),
            goal_summary: "No advanced goals · Enter adds one".into(),
            calendar_summary: "No calendar schedule · Enter configures".into(),
        }
    }
    pub fn tracking(&self) -> bool {
        self.store.is_some() && self.config.tracking
    }
    pub fn update<R>(&mut self, f: impl FnOnce(&mut model::Settings) -> Result<R>) -> Result<R> {
        let runtime = self.sprint.clone();
        let store = self
            .store
            .as_mut()
            .context("productivity metadata unavailable; original preserved; reload explicitly")?;
        let result = store.update(|s| {
            s.sprint = runtime;
            f(s)
        })?;
        self.config = store.value.clone();
        self.sprint = self.config.sprint.clone();
        Ok(result)
    }
    fn record(&mut self, source: Source, mutation: Mutation) {
        if !self.tracking() {
            self.activity.pause();
            return;
        }
        let now = self.clock.now();
        let activity = self.activity.edit(
            now.clone(),
            &source,
            self.config.mode,
            mutation.origin,
            self.config.idle_seconds as u64 * 1000,
        );
        if source.role == Role::Manuscript
            && mutation.counts.deleted_chars > 0
            && let Some(s) = &mut self.sprint
            && s.kind == SprintKind::NoEditing
            && s.status == SprintStatus::Running
            && s.anchor_ms.is_some()
        {
            s.deletion_observed = true;
        }
        if let Err(e) = self
            .ledger
            .observe(&now, source, self.config.mode, mutation, activity)
        {
            self.unrecorded.plus(mutation.counts);
            self.warning = format!("Metrics incomplete: {e:#}; manuscript buffers retained");
        }
    }
    fn recent(&self, root: &Path, date: &str) -> Result<BTreeSet<String>> {
        let d = grimoire_core::journal::date(date)?;
        let mut ids = BTreeSet::new();
        for n in 1..=7 {
            let Some(previous) = d.checked_sub_signed(CivilDuration::days(n)) else {
                break;
            };
            let p = challenges::path(&previous.to_string())?;
            if grimoire_core::journal::date(&previous.to_string()).is_err() {
                break;
            }
            if let Some(bytes) = statefile::read(root, &p)? {
                let day: Day = serde_json::from_slice(&bytes)?;
                grimoire_core::statefile::Validate::validate(&day)?;
                if day.date != previous.to_string() {
                    bail!("recent challenge date mismatch; original preserved");
                }
                ids.extend(day.assignments.into_iter().map(|a| a.record_id));
                ids.extend(
                    day.rerolls
                        .into_iter()
                        .flat_map(|r| [r.old_record, r.replacement]),
                );
            }
        }
        Ok(ids)
    }
}
/// Every real source editor opts in; App applies disabled/unavailable policy
/// before routing another input. Programmatic loads never enqueue input.
pub fn source_editor(text: &str) -> Editor {
    let mut e = Editor::from_text(text);
    e.measure_operations(true);
    e
}
impl App {
    fn metric_source(&self, i: usize) -> Option<Source> {
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
                totals.input.added,
                totals.input.removed,
                totals.net(self.productivity.config.include_footnotes)
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
            "No calendar schedule · Enter configures".into(),
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
            for id in &self.deck.progress.completed {
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
            self.build_productivity_rows(page, sel);
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
            self.build_productivity_rows(Page::Sprints, sel);
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
                    settings.awards.last_mut().unwrap().source = Some(path);
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
    fn load_challenge_day(&mut self) -> Result<()> {
        self.productivity.day = None;
        if !self.productivity.config.challenges {
            bail!("Daily challenges disabled; no assignment generated");
        }
        let now = self.productivity.clock.now();
        let date = self.productivity.date.clone();
        let path = challenges::path(&date)?;
        let existing = statefile::read(&self.project.root, &path)?;
        if existing.is_none() && date != now.day() {
            bail!("No assignment recorded for this date; no historical work/assignment invented");
        }
        let default = match existing {
            Some(bytes) => serde_json::from_slice::<Day>(&bytes)?,
            None => Day::assign(
                &now,
                self.productivity.config.challenge_profile.clone(),
                self.productivity.config.challenge_seed,
                &self.productivity.recent(&self.project.root, &date)?,
            )?,
        };
        let mut store = Store::load(&self.project.root, &path, default)?;
        if store.value.date != date {
            bail!("Challenge date mismatch; original preserved");
        }
        if !store.exists() {
            store.update(|_| Ok(()))?;
        }
        self.productivity.day = Some(store);
        Ok(())
    }
    pub fn open_productivity(&mut self, page: Page) {
        self.flush();
        self.productivity.activity.pause();
        self.checkpoint_productivity(true);
        if page == Page::Challenges {
            self.productivity.date = self.productivity.clock.now().day();
            if let Err(e) = self.load_challenge_day() {
                self.msg = format!("Challenges unavailable: {e:#}");
            }
        }
        if page == Page::ChallengeFilters {
            self.productivity.draft_profile =
                Some(self.productivity.config.challenge_profile.clone());
        }
        self.overlay = Overlay::Productivity {
            page,
            sel: 0,
            scroll: None,
        };
        self.build_productivity_rows(page, 0);
        if page == Page::Achievements && !self.productivity.config.awards.is_empty() {
            self.observe_deck(grimoire_core::tutorial::Event::AchievementReviewed);
        }
    }
}

const GOALS: [(&str, Metric, u64); 9] = [
    ("Daily input additions", Metric::InputAdded, 500),
    ("Weekly keyboard additions", Metric::TypedAdded, 3500),
    ("Monthly recorded net change", Metric::NetInput, 10000),
    ("Native current novel words", Metric::ManuscriptWords, 80000),
    ("Native current scene words", Metric::SceneWords, 1000),
    ("Daily observed active minutes", Metric::ActiveMinutes, 30),
    ("Daily revision operations", Metric::RevisionOperations, 10),
    ("Daily footnote additions", Metric::FootnoteAdded, 100),
    ("Scheduled deadline net change", Metric::NetInput, 50000),
];
const SPRINTS: [(SprintKind, &str, u64); 5] = [
    (
        SprintKind::Timed,
        "Timed (elapsed minutes, not active-writing proof)",
        15,
    ),
    (
        SprintKind::Words,
        "Words (recorded net, including replay; paste allowed)",
        250,
    ),
    (
        SprintKind::Revision,
        "Revision (operations in Revision mode)",
        20,
    ),
    (
        SprintKind::NoEditing,
        "No-editing experiment (deletions retained and reported)",
        10,
    ),
    (
        SprintKind::Challenge,
        "Link selected accepted measured challenge",
        50,
    ),
];
impl App {
    fn productivity_count(&self, page: Page) -> usize {
        match page {
            Page::Goals => self.productivity.config.goals.len(),
            Page::GoalTypes => GOALS.len(),
            Page::Calendar => self
                .productivity
                .config
                .schedule
                .as_ref()
                .and_then(|s| s.days().ok())
                .map_or(0, |d| d.len()),
            Page::Sprints => 0,
            Page::SprintTypes => SPRINTS.len(),
            Page::Challenges => self
                .productivity
                .day
                .as_ref()
                .map_or(0, |s| s.value.assignments.len()),
            Page::ChallengeFilters => 13,
            Page::Achievements => self.productivity.config.awards.len(),
            _ => 0,
        }
    }
    pub fn build_productivity_rows(&mut self, page: Page, sel: usize) {
        let p = &self.productivity;
        let mut rows = Vec::new();
        let mut selected = None;
        if !p.warning.is_empty() {
            rows.push(p.warning.clone());
        }
        if !p.ledger.warning.is_empty() {
            rows.push(p.ledger.warning.clone());
        }
        match page {
            Page::Overview => {
                rows.push(format!(
                    "Tracking {} · {:?} · idle cutoff {}s · footnotes {}",
                    if p.tracking() {
                        "on"
                    } else {
                        "off/unavailable"
                    },
                    p.config.mode,
                    p.config.idle_seconds,
                    if p.config.include_footnotes {
                        "included"
                    } else {
                        "separate"
                    }
                ));
                rows.push("Measured replacements, not baselines/Git; words use source policy v1, not rendered/native export counts.".into());
                let now = p.clock.now(); // Called by input/checkpoint, never by draw.
                rows.push(format!(
                    "Local {} · {} · persisted {} groups / pending {}",
                    now.day(),
                    now.zone
                        .as_deref()
                        .unwrap_or("IANA zone unavailable; fixed offset recorded"),
                    p.ledger.known.len(),
                    p.ledger.pending_count()
                ));
                if !p.snapshot.complete {
                    rows.push("History incomplete: following values are known cached subsets, not complete totals; goals do not guess missing values.".into());
                }
                for (role, name) in [
                    (Role::Manuscript, "Manuscript"),
                    (Role::Notes, "Notes/characters/places"),
                    (Role::Research, "Research"),
                    (Role::Other, "Other"),
                ] {
                    let t = Totals::collect(p.ledger.known.iter(), &now.day(), &now.day(), role);
                    rows.push(format!(
                        "{name}: input +{} / −{} · net {} · {} word gaps",
                        t.input.added,
                        t.input.removed,
                        t.net(p.config.include_footnotes),
                        t.input.gaps + t.replay.gaps
                    ));
                    rows.push(format!("  keyboard +{} · paste +{} · undo/redo/restore net {} · footnotes +{} / −{}",t.typed.added,t.pasted.added,t.replay.net(true),t.input.foot_added,t.input.foot_removed));
                    rows.push(format!(
                        "  inter-edit active {:.1}s · keyboard-pair {:.1}s · unassigned {:.1}s",
                        t.active_ms as f64 / 1000.,
                        t.typed_pair_ms as f64 / 1000.,
                        t.unassigned_ms as f64 / 1000.
                    ));
                }
                rows.push("No startup, idle tail or offline time. Keyboard-pair intervals are NOT a claim of proven exclusive typing.".into());
                rows.push("Large-source word gaps remain unknown; raw inserted/deleted characters still record. No automatic manuscript rewriting.".into());
                rows.push(format!(
                    "Milestones {} · notices {} · challenges {} · {} unrecorded operations",
                    p.config.achievements,
                    p.config.notifications,
                    p.config.challenges,
                    p.unrecorded.operations
                ));
            }
            Page::Goals => {
                rows.push("Recorded-operation goals differ from native existing-source totals; no legacy baseline becomes gross input.".into());
                selected = Some(rows.len() + sel);
                for g in &p.config.goals {
                    let value =
                        p.snapshot
                            .goal(g, &p.clock.now().day(), p.config.include_footnotes);
                    rows.push(format!(
                        "{} {} · {} / {} · {} · {:?}",
                        if g.enabled { "[on]" } else { "[off]" },
                        g.label,
                        value.map_or("unavailable".into(), |n| n.to_string()),
                        g.target,
                        g.metric.label(),
                        g.period
                    ));
                }
                if p.config.goals.is_empty() {
                    rows.push("No advanced goals yet. n chooses a configurable daily/weekly/monthly/novel/scene/time/revision/deadline target.".into());
                }
                rows.push("Native source counts retain Grimoire's whitespace/private-note policy; the footnote switch changes only ledger goals.".into());
            }
            Page::GoalTypes => {
                selected = Some(rows.len() + sel);
                for (label, metric, target) in GOALS {
                    rows.push(format!("{label} · suggested {target} · {}", metric.label()));
                }
            }
            Page::Calendar => {
                if let Some(schedule) = &p.config.schedule {
                    let now = p.clock.now();
                    let net = p.snapshot.value(
                        Metric::NetInput,
                        &schedule.start,
                        &schedule.end,
                        None,
                        None,
                        p.config.include_footnotes,
                    );
                    rows.push(format!(
                        "{} → {} · target {} recorded net words · current {}",
                        schedule.start,
                        schedule.end,
                        schedule.target,
                        net.map_or("unavailable".into(), |n| n.to_string())
                    ));
                    if let Some(net) = net
                        && let Ok(pace) = schedule.pace(&now.day(), net)
                    {
                        rows.push(format!("Working days left {} · required/day {} · ahead/behind {} (end-of-day plan)",pace.working_days_left,pace.required_per_day.map_or("unavailable".into(),|n|n.to_string()),pace.ahead_behind.map_or("no workdays".into(),|n|format!("{n:+}"))));
                        let dates: BTreeSet<_> = p
                            .ledger
                            .known
                            .iter()
                            .filter(|e| e.source.role == Role::Manuscript)
                            .map(|e| e.date.clone())
                            .collect();
                        let history: BTreeMap<_, _> = dates
                            .into_iter()
                            .filter_map(|d| {
                                p.snapshot
                                    .value(
                                        Metric::NetInput,
                                        &d,
                                        &d,
                                        None,
                                        None,
                                        p.config.include_footnotes,
                                    )
                                    .map(|n| (d, n))
                            })
                            .collect();
                        rows.push(format!("Forecast {} · assumes mean of last 3–7 recorded work dates; missing days are not zero",schedule.forecast(&now.day(),net,&history).ok().flatten().unwrap_or_else(||"unavailable".into())));
                    }
                    selected = Some(rows.len() + sel);
                    if let Ok(days) = schedule.days() {
                        for day in days {
                            let d = day.to_string();
                            let recorded = p
                                .ledger
                                .known
                                .iter()
                                .any(|e| e.source.role == Role::Manuscript && e.date == d);
                            let value = if recorded {
                                p.snapshot
                                    .value(
                                        Metric::NetInput,
                                        &d,
                                        &d,
                                        None,
                                        None,
                                        p.config.include_footnotes,
                                    )
                                    .map_or("unavailable".into(), |n| format!("{n:+}"))
                            } else {
                                "not recorded".into()
                            };
                            rows.push(format!(
                                "{} {:3} {} · {value}",
                                d,
                                day.format("%a"),
                                if schedule.rest_days.contains(&d) {
                                    "rest"
                                } else {
                                    "work"
                                }
                            ));
                        }
                    }
                } else {
                    rows.push("No calendar schedule yet. n explicitly creates this year's offline November 50,000-word preset.".into());
                    rows.push("Start/end, target and optional rest days are configurable. No external account, endorsement or inferred past writing.".into());
                }
            }
            Page::Sprints => {
                rows.push("Elapsed sprint time is monotonic, not active-writing proof. Restart requires explicit resume; offline time is never added.".into());
                selected = None;
                if let Some(s) = &p.sprint {
                    rows.push(format!(
                        "Current #{} {:?} · {} · {:.1} min · progress {} / {}{}{}",
                        s.id,
                        s.kind,
                        if s.effective_paused() {
                            "paused (resume explicitly)"
                        } else {
                            match s.status {
                                SprintStatus::Running => "running",
                                SprintStatus::Completed => "completed",
                                SprintStatus::Stopped => "stopped",
                                SprintStatus::Paused => "paused",
                            }
                        },
                        s.elapsed_ms as f64 / 60_000.,
                        if s.kind == SprintKind::Timed {
                            format!("{:.1} min elapsed", s.elapsed_ms as f64 / 60_000.)
                        } else if s.metrics_incomplete {
                            "unavailable".into()
                        } else {
                            s.progress.to_string()
                        },
                        if s.kind == SprintKind::Timed {
                            format!("{} min timer", s.target)
                        } else {
                            s.target.to_string()
                        },
                        if s.deletion_observed {
                            " · deletion observed; text retained"
                        } else {
                            ""
                        },
                        if s.challenge.is_some() {
                            " · challenge linked"
                        } else {
                            ""
                        }
                    ));
                    if let Some(spark) = &s.spark {
                        rows.push(format!("Spark source {} · {} · exact component references; timed completion is not creative completion", spark.id, spark.corpus_version));
                    }
                    rows.push(format!(
                        "Policy: {} · mode {:?} · footnotes {} · checkpoint {}",
                        s.metric().label(),
                        s.mode,
                        s.include_footnotes,
                        s.last_checkpoint
                    ));
                }
                if let Some(s) = &p.sprint {
                    if let Some(b) = &s.break_timer {
                        rows.push(format!("Break #{} {} · {:.1} / {} min; work paused · p toggles break · k skips/records",b.id,if b.effective_paused(){"paused; explicit resume required"}else{"running"},b.elapsed_ms as f64/60000.,b.target));
                    }
                    if !s.breaks.is_empty() {
                        rows.push(format!(
                            "{} completed/stopped breaks retained; no forced work resume",
                            s.breaks.len()
                        ));
                    }
                }
                for s in p.config.sprint_history.iter().rev() {
                    rows.push(format!(
                        "#{} {:?} {:?} · {:.1} min · {} / {}{}",
                        s.id,
                        s.kind,
                        s.status,
                        s.elapsed_ms as f64 / 60_000.,
                        if s.kind == SprintKind::Timed {
                            format!("{:.1} min elapsed", s.elapsed_ms as f64 / 60_000.)
                        } else if s.metrics_incomplete {
                            "unavailable".into()
                        } else {
                            s.progress.to_string()
                        },
                        if s.kind == SprintKind::Timed {
                            format!("{} min timer", s.target)
                        } else {
                            s.target.to_string()
                        },
                        if s.deletion_observed {
                            " · deletions retained"
                        } else {
                            ""
                        }
                    ));
                }
                if p.sprint.is_none() && p.config.sprint_history.is_empty() {
                    rows.push("No advanced sprints yet; existing F3/Pomodoro and basic sprint controls remain separate.".into());
                }
                rows.push("p pauses/resumes only this sprint; edits while paused are excluded. x stops without deleting prose. b starts an optional timed break.".into());
            }
            Page::SprintTypes => {
                selected = Some(rows.len() + sel);
                for (_, label, target) in SPRINTS {
                    rows.push(format!("{label} · suggested target {target}"));
                }
            }
            Page::Achievements => {
                rows.push(format!(
                    "Milestones {} · notices {} · no punitive streak resets",
                    p.config.achievements, p.config.notifications
                ));
                selected = Some(rows.len() + sel);
                for a in &p.config.awards {
                    rows.push(format!(
                        "{} · {} · {}",
                        a.label,
                        if a.measured {
                            "recorded measurement"
                        } else {
                            "explicit action / self-report, not text-quality verification"
                        },
                        a.earned_at
                    ));
                }
                if p.config.awards.is_empty() {
                    rows.push("No milestones recorded yet. Actual keyboard word additions, Revision-mode edits, optional sprints, saved finished sources and learning can count.".into());
                }
                rows.push("Disabling retains your collection but awards nothing new. Nothing forces a notification into focus writing.".into());
            }
            Page::Challenges => {
                rows.push(format!(
                    "{} · creation zone {} · {} original exercises across 10 categories",
                    p.date,
                    p.day
                        .as_ref()
                        .and_then(|d| d.value.zone.as_deref())
                        .unwrap_or("unavailable"),
                    challenges::catalog().len()
                ));
                if let Some(day) = &p.day {
                    rows.push(format!(
                        "Saved profile: {:?}, difficulty ≤{}, ≤{} min · {} explicit rerolls",
                        day.value.profile.mode,
                        day.value.profile.difficulty,
                        day.value.profile.max_minutes,
                        day.value.rerolls.len()
                    ));
                    selected = Some(rows.len() + sel);
                    for a in &day.value.assignments {
                        if let Some(e) = challenges::exercise(&a.record_id) {
                            rows.push(format!(
                                "[{:?}] {} · {} · {} min · level {}{}",
                                a.status,
                                e.title,
                                e.category.label(),
                                e.minutes,
                                e.difficulty,
                                a.evidence.map_or("".into(), |k| format!(" · {k:?}"))
                            ));
                        }
                    }
                    if let Some(a) = day.value.assignments.get(sel)
                        && let Some(e) = challenges::exercise(&a.record_id)
                    {
                        rows.push(String::new());
                        rows.push(format!("{:?} mode · {}", e.mode, e.instructions));
                        if let Some(c) = &e.measured {
                            let value = p
                                .snapshot
                                .lifetime(c.metric, Some(e.mode), a.include_footnotes)
                                .zip(a.baseline)
                                .map(|(a, b)| a - b);
                            rows.push(format!("Measured criterion: {} / {} {} since acceptance (Ctrl-S checkpoints pending edits)",value.map_or("not accepted/unavailable".into(),|n|n.to_string()),c.target,c.metric.label()));
                        } else {
                            rows.push("Subjective exercise: x explicitly self-reports completion; no automatic judgment of your prose or reading.".into());
                        }
                    }
                    if day.value.assignments.is_empty() {
                        rows.push("No exercise matches saved filters. Widen future filters; no hidden repeated or invented assignment.".into());
                    }
                } else {
                    rows.push(if p.config.challenges{"No readable assignment for this date; originals retained. r reloads; → returns toward today."}else{"Challenges disabled. Enable from 1 Edits; no assignment or notification is generated."}.into());
                }
            }
            Page::ChallengeFilters => {
                if let Some(profile) = &p.draft_profile {
                    selected = Some(rows.len() + sel);
                    for category in Category::ALL {
                        rows.push(format!(
                            "[{}] {}",
                            if profile.categories.contains(&category) {
                                'x'
                            } else {
                                ' '
                            },
                            category.label()
                        ));
                    }
                    rows.push(format!("Mode {:?} (None = mixed)", profile.mode));
                    rows.push(format!("Maximum difficulty {}", profile.difficulty));
                    rows.push(format!("Maximum duration {} min", profile.max_minutes));
                    rows.push("Enter saves for future dates and explicit rerolls only; today's existing tasks do not silently regenerate.".into());
                }
            }
        }
        self.productivity.rows = rows;
        self.productivity.row_selection = selected;
    }
    fn productivity_number(&mut self, kind: Input, back: Page, value: String) {
        self.overlay = Overlay::ProductivityInput {
            kind,
            buf: value,
            back,
        };
    }
    fn new_productivity_goal(&mut self, index: usize, target: u64) -> Result<()> {
        let (label, metric, _) = *GOALS.get(index).context("choose goal metric")?;
        let now = self.productivity.clock.now();
        let source = if metric == Metric::SceneWords {
            Some(
                self.open
                    .and_then(|i| self.metric_source(i))
                    .filter(|s| s.role == Role::Manuscript)
                    .context("open a manuscript scene before creating its goal")?
                    .path,
            )
        } else {
            None
        };
        let period = match index {
            0 | 5 | 6 | 7 => Period::Daily,
            1 => Period::Weekly,
            2 => Period::Monthly,
            8 => {
                let s = self
                    .productivity
                    .config
                    .schedule
                    .as_ref()
                    .context("configure a Calendar schedule first")?;
                Period::Deadline {
                    start: s.start.clone(),
                    end: s.end.clone(),
                }
            }
            _ => Period::SinceCreated,
        };
        self.productivity.update(|s| {
            let id = s.allocate()?;
            s.goals.push(Goal {
                id,
                label: label.into(),
                metric,
                target,
                period,
                created: now.day(),
                mode: if metric == Metric::RevisionOperations {
                    Some(WorkMode::Revision)
                } else {
                    None
                },
                source,
                enabled: true,
            });
            Ok(())
        })?;
        Ok(())
    }
    fn new_advanced_sprint(&mut self, kind: SprintKind, target: u64) -> Result<()> {
        self.new_advanced_sprint_with_spark(kind, target, None)
    }
    pub(super) fn new_advanced_sprint_with_spark(
        &mut self,
        kind: SprintKind,
        target: u64,
        spark: Option<grimoire_core::spark::ResultRef>,
    ) -> Result<()> {
        if kind == SprintKind::Break
            && self.productivity.sprint.as_ref().is_some_and(|s| {
                !matches!(s.status, SprintStatus::Completed | SprintStatus::Stopped)
            })
        {
            let now = self.productivity.clock.now();
            let s = self.productivity.sprint.as_ref().unwrap();
            let reading =
                self.productivity
                    .snapshot
                    .lifetime(s.metric(), s.mode, s.include_footnotes);
            self.productivity.update(|settings| {
                let id = settings.allocate()?;
                settings
                    .sprint
                    .as_mut()
                    .unwrap()
                    .start_break(id, &now, reading, target)
            })?;
            return Ok(());
        }
        if let Some(s) = &self.productivity.sprint
            && !matches!(s.status, SprintStatus::Completed | SprintStatus::Stopped)
        {
            bail!("Pause/resume or stop the current sprint before starting another");
        }
        if kind == SprintKind::NoEditing && !self.productivity.tracking() {
            bail!(
                "No-editing experiment requires operation tracking; no restriction is claimed while unobserved"
            );
        }
        let now = self.productivity.clock.now();
        let mut criterion = None;
        let mut mode = if kind == SprintKind::Revision {
            Some(WorkMode::Revision)
        } else {
            None
        };
        let mut link = None;
        let mut chosen_target = target;
        let mut footnotes = self.productivity.config.include_footnotes;
        if kind == SprintKind::Challenge {
            if self.productivity.day.is_none() {
                self.productivity.date = now.day();
                self.load_challenge_day()?;
            }
            let day = &self
                .productivity
                .day
                .as_ref()
                .context("open/accept a daily challenge first")?
                .value;
            let a=day.assignments.iter().find(|a|a.status==Status::Accepted&&challenges::exercise(&a.record_id).is_some_and(|e|e.measured.is_some())).context("accept a measured daily challenge first; subjective tasks use explicit self-report, not an invented counter")?;
            let e = challenges::exercise(&a.record_id).unwrap();
            criterion = e.measured.clone();
            mode = Some(e.mode);
            let c = criterion.as_ref().unwrap();
            footnotes = a.include_footnotes;
            let current = self
                .productivity
                .snapshot
                .lifetime(c.metric, Some(e.mode), footnotes)
                .context("measured challenge metric unavailable")?;
            let delta = current
                - a.baseline
                    .context("measured challenge baseline unavailable")?;
            if delta >= c.target as i128 {
                bail!("challenge criterion already reached; check/record its completion instead");
            }
            chosen_target = (c.target as i128 - delta.max(0)) as u64;
            link = Some((day.date.clone(), a.record_id.clone()));
        }
        let metric = criterion.as_ref().map_or(
            if kind == SprintKind::Revision {
                Metric::RevisionOperations
            } else {
                Metric::NetInput
            },
            |c| c.metric,
        );
        let reading = self.productivity.snapshot.lifetime(metric, mode, footnotes);
        self.productivity.update(|s| {
            if let Some(old) = s.sprint.take() {
                if s.sprint_history.len() >= 128 {
                    bail!("128 sprint results retained; archive explicitly before adding another");
                }
                s.sprint_history.push(old);
            }
            let id = s.allocate()?;
            let mut sprint = Sprint::new(id, kind, chosen_target, &now, reading, footnotes)?;
            sprint.mode = mode;
            sprint.criterion = criterion;
            sprint.challenge = link;
            sprint.spark = spark;
            s.sprint = Some(sprint);
            Ok(())
        })?;
        Ok(())
    }
}
impl App {
    pub fn on_productivity_key(&mut self, key: Key) {
        if matches!(
            self.overlay,
            Overlay::Productivity {
                page: Page::Challenges,
                ..
            }
        ) && key == Key::Char('S')
        {
            self.open_spark();
            self.refresh_spark_assignment();
            self.spark_page(super::spark::Page::Assignment, 0);
            return;
        }
        if matches!(self.overlay, Overlay::ProductivityInput { .. }) {
            self.on_productivity_input(key);
            return;
        }
        let Overlay::Productivity {
            page,
            mut sel,
            mut scroll,
        } = self.overlay
        else {
            return;
        };
        if key == Key::Esc || key == Key::Char('q') {
            if matches!(
                page,
                Page::GoalTypes | Page::SprintTypes | Page::ChallengeFilters
            ) {
                self.open_productivity(match page {
                    Page::GoalTypes => Page::Goals,
                    Page::SprintTypes => Page::Sprints,
                    _ => Page::Challenges,
                });
            } else {
                self.overlay = Overlay::None;
            }
            return;
        }
        if let Key::Char(c @ '1'..='6') = key {
            self.open_productivity(Page::TABS[c as usize - '1' as usize]);
            return;
        }
        if key == Key::Tab || key == Key::BackTab {
            let i = Page::TABS.iter().position(|p| *p == page).unwrap_or(0);
            let next = if key == Key::Tab {
                (i + 1) % 6
            } else {
                (i + 5) % 6
            };
            self.open_productivity(Page::TABS[next]);
            return;
        }
        if key == Key::Char('r') {
            self.reload_productivity();
            if page == Page::Challenges
                && let Err(e) = self.load_challenge_day()
            {
                self.msg = format!("Challenge reload refused: {e:#}");
            }
            self.build_productivity_rows(page, sel);
            return;
        }
        let count = self.productivity_count(page);
        if matches!(key, Key::Up | Key::Down | Key::Home | Key::End) && count > 0 {
            sel = match key {
                Key::Up => sel.saturating_sub(1),
                Key::Down => (sel + 1).min(count - 1),
                Key::Home => 0,
                _ => count - 1,
            };
            scroll = None;
            self.overlay = Overlay::Productivity { page, sel, scroll };
            self.build_productivity_rows(page, sel);
            return;
        }
        if matches!(key, Key::PageUp | Key::PageDown)
            || count == 0 && matches!(key, Key::Up | Key::Down)
        {
            let (offset, max) = self.productivity.render_fit.get();
            let step = if matches!(key, Key::Up | Key::Down) {
                1
            } else {
                10
            };
            scroll = Some(if matches!(key, Key::Down | Key::PageDown) {
                (offset + step).min(max)
            } else {
                offset.saturating_sub(step)
            });
            self.overlay = Overlay::Productivity { page, sel, scroll };
            return;
        }
        let result = (|| -> Result<()> {
            match page {
                Page::Overview => {
                    match key {
                        Key::Char('t' | 'm' | 'f' | 'a' | 'n' | 'd') => {
                            self.flush();
                            if !self.checkpoint_productivity(true) {
                                bail!(
                                    "pending measurements/sprint could not be checkpointed; settings unchanged; r explicitly reloads/retries"
                                );
                            }
                            self.productivity.update(|s| {
                                match key {
                                    Key::Char('t') => {
                                        s.tracking = !s.tracking;
                                        if !s.tracking
                                            && let Some(sprint) = &mut s.sprint
                                            && sprint.kind == SprintKind::NoEditing
                                        {
                                            sprint.metrics_incomplete = true;
                                        }
                                    }
                                    Key::Char('m') => s.mode = s.mode.next(),
                                    Key::Char('f') => s.include_footnotes = !s.include_footnotes,
                                    Key::Char('a') => s.achievements = !s.achievements,
                                    Key::Char('n') => s.notifications = !s.notifications,
                                    _ => s.challenges = !s.challenges,
                                }
                                Ok(())
                            })?;
                            self.productivity.activity.pause();
                            self.capture_productivity();
                            self.msg="Productivity preference recorded locally; original Markdown untouched".into();
                        }
                        Key::Char('i') => {
                            self.productivity_number(
                                Input::Idle,
                                page,
                                self.productivity.config.idle_seconds.to_string(),
                            );
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                Page::Goals => match key {
                    Key::Char('n') => {
                        self.open_productivity(Page::GoalTypes);
                        return Ok(());
                    }
                    Key::Char(' ') => {
                        self.productivity.update(|s| {
                            let g = s.goals.get_mut(sel).context("choose a goal")?;
                            g.enabled = !g.enabled;
                            Ok(())
                        })?;
                    }
                    Key::Delete => {
                        self.productivity.update(|s| {
                            if sel >= s.goals.len() {
                                bail!("choose a goal");
                            }
                            s.goals.remove(sel);
                            Ok(())
                        })?;
                        sel = sel.min(self.productivity.config.goals.len().saturating_sub(1));
                    }
                    _ => {}
                },
                Page::GoalTypes => {
                    if key == Key::Enter || key == Key::Char(' ') {
                        self.productivity_number(
                            Input::Goal(sel),
                            Page::Goals,
                            GOALS[sel.min(GOALS.len() - 1)].2.to_string(),
                        );
                        return Ok(());
                    }
                }
                Page::Calendar => {
                    match key {
                        Key::Char('n') => {
                            let year = self.productivity.clock.now().wall.year();
                            let schedule = Schedule::november(year)?;
                            self.productivity.update(|s| {
                                if let Some(old)=s.schedule.take(){if s.schedule_history.len()>=120{bail!("120 earlier calendars retained; archive explicitly before replacing");}s.schedule_history.push(old);}
                                s.schedule = Some(schedule);
                                Ok(())
                            })?;
                            sel = 0;
                            self.msg = format!(
                                "November {year} 50k schedule recorded; choose optional rest days. No past words inferred."
                            );
                        }
                        Key::Char(' ') => {
                            let schedule = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?;
                            let day = schedule
                                .days()?
                                .get(sel)
                                .context("choose a calendar date")?
                                .to_string();
                            self.productivity
                                .update(|s| s.schedule.as_mut().unwrap().toggle_rest(&day))?;
                            self.observe_deck(grimoire_core::tutorial::Event::CalendarConfigured);
                            self.msg="Explicit rest-day choice recorded; remaining-day pace recalculated".into();
                        }
                        Key::Char('b') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .start
                                .clone();
                            self.productivity_number(Input::ScheduleStart, page, value);
                            return Ok(());
                        }
                        Key::Char('e') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .end
                                .clone();
                            self.productivity_number(Input::ScheduleEnd, page, value);
                            return Ok(());
                        }
                        Key::Char('g') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .target
                                .to_string();
                            self.productivity_number(Input::ScheduleTarget, page, value);
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                Page::Sprints => match key {
                    Key::Char('n') => {
                        self.open_productivity(Page::SprintTypes);
                        return Ok(());
                    }
                    Key::Char('b') => {
                        self.productivity_number(
                            Input::Sprint(SprintKind::Break),
                            page,
                            "5".into(),
                        );
                        return Ok(());
                    }
                    Key::Char('p' | 'x' | 'k') => {
                        self.flush();
                        if !self.checkpoint_productivity(true) {
                            bail!("checkpoint pending records before changing sprint state");
                        }
                        let now = self.productivity.clock.now();
                        let sprint = self
                            .productivity
                            .sprint
                            .as_ref()
                            .context("start a sprint first")?;
                        let reading = self.productivity.snapshot.lifetime(
                            sprint.metric(),
                            sprint.mode,
                            sprint.include_footnotes,
                        );
                        if key == Key::Char('p')
                            && self
                                .productivity
                                .sprint
                                .as_ref()
                                .is_some_and(|s| s.kind == SprintKind::NoEditing)
                            && !self.productivity.tracking()
                        {
                            bail!(
                                "enable operation tracking before resuming a no-editing experiment"
                            );
                        }
                        let sprint = self.productivity.sprint.as_mut().unwrap();
                        if key == Key::Char('x') {
                            sprint.stop(&now, reading);
                        } else if key == Key::Char('k') {
                            sprint.skip_break(&now)?;
                        } else if sprint.effective_paused() {
                            sprint.resume(&now, reading)?;
                        } else {
                            sprint.pause(&now, reading);
                        }
                        self.productivity.activity.pause();
                        self.productivity.update(|_| Ok(()))?;
                        self.msg =
                            "Sprint state recorded; no text deleted or edits suppressed".into();
                    }
                    _ => {}
                },
                Page::SprintTypes => {
                    if key == Key::Enter || key == Key::Char(' ') {
                        let (kind, _, value) = SPRINTS[sel.min(SPRINTS.len() - 1)];
                        self.productivity_number(
                            Input::Sprint(kind),
                            Page::Sprints,
                            value.to_string(),
                        );
                        return Ok(());
                    }
                }
                Page::Achievements => match key {
                    Key::Char('a') => {
                        self.productivity.update(|s| {
                            s.achievements = !s.achievements;
                            Ok(())
                        })?;
                    }
                    Key::Char('n') => {
                        self.productivity.update(|s| {
                            s.notifications = !s.notifications;
                            Ok(())
                        })?;
                    }
                    _ => {}
                },
                Page::Challenges => {
                    if matches!(key, Key::Left | Key::Right) {
                        let date = grimoire_core::journal::date(&self.productivity.date)?;
                        let next = date
                            .checked_add_signed(CivilDuration::days(if key == Key::Left {
                                -1
                            } else {
                                1
                            }))
                            .context("date outside supported calendar")?
                            .to_string();
                        grimoire_core::journal::date(&next)?;
                        self.productivity.date = next;
                        if let Err(e) = self.load_challenge_day() {
                            self.msg = format!("Assignment unavailable: {e:#}");
                        }
                    } else if key == Key::Char('f') {
                        self.open_productivity(Page::ChallengeFilters);
                        return Ok(());
                    } else if matches!(key, Key::Char('a' | 'c' | 'x' | 'R')) {
                        if !self.productivity.config.challenges {
                            bail!("challenges disabled; no state changed");
                        }
                        self.flush();
                        if !self.checkpoint_productivity(true) && key != Key::Char('x') {
                            bail!(
                                "recorded metrics checkpoint unavailable; no measured acceptance/completion invented"
                            );
                        }
                        let now = self.productivity.clock.now();
                        let foot = self.productivity.config.include_footnotes;
                        let recent = if key == Key::Char('R') {
                            self.productivity
                                .recent(&self.project.root, &self.productivity.date)?
                        } else {
                            BTreeSet::new()
                        };
                        let profile = self.productivity.config.challenge_profile.clone();
                        let snapshot = &self.productivity.snapshot;
                        let store = self
                            .productivity
                            .day
                            .as_mut()
                            .context("no readable assignment; originals preserved")?;
                        let mut completed = None;
                        match key {
                            Key::Char('a') => {
                                store.update(|d| d.accept(sel, &now, snapshot, foot))?;
                                self.msg="Accepted; only subsequent recorded work in the exercise's mode counts".into();
                            }
                            Key::Char('R') => {
                                store.update(|d| {
                                    if d.assignments.is_empty() {
                                        d.new_selection(&now, &profile, &recent)
                                    } else {
                                        d.reroll_with_profile(sel, &now, &profile, &recent)
                                    }
                                })?;
                                self.msg="Explicit reroll recorded; original reference retained in history".into();
                            }
                            _ => {
                                let evidence = store.update(|d| {
                                    d.complete(sel, &now, snapshot, key == Key::Char('x'))
                                })?;
                                let a = &store.value.assignments[sel];
                                completed =
                                    Some((a.record_id.clone(), store.value.date.clone(), evidence));
                            }
                        }
                        if let Some((id, date, evidence)) = completed {
                            let award_id = format!("challenge-{date}-{id}");
                            self.productivity.update(|s| {
                                s.award(
                                    &award_id,
                                    if evidence == Evidence::Measured {
                                        "Completed a measured challenge"
                                    } else {
                                        "Self-reported a creative/reading exercise"
                                    },
                                    evidence == Evidence::Measured,
                                    &now.stamp(),
                                )?;
                                Ok(())
                            })?;
                            self.msg = format!(
                                "Challenge completion recorded: {evidence:?}; no quality/reading proof inferred"
                            );
                            if evidence == Evidence::Measured {
                                self.observe_deck(
                                    grimoire_core::tutorial::Event::DailyChallengeCompleted,
                                );
                            }
                        }
                    }
                }
                Page::ChallengeFilters => {
                    let profile = self
                        .productivity
                        .draft_profile
                        .as_mut()
                        .context("open filters again")?;
                    if key == Key::Enter {
                        let next = profile.clone();
                        self.productivity.update(|s| {
                            s.challenge_profile = next;
                            Ok(())
                        })?;
                        self.open_productivity(Page::Challenges);
                        self.msg =
                            "Future challenge filters saved; today's existing assignment retained"
                                .into();
                        return Ok(());
                    }
                    match key {
                        Key::Char(' ') if sel < 10 => {
                            let c = Category::ALL[sel];
                            if !profile.categories.remove(&c) {
                                profile.categories.insert(c);
                            }
                        }
                        Key::Left | Key::Right if sel == 10 => {
                            profile.mode = match (profile.mode, key) {
                                (None, Key::Right) | (Some(WorkMode::Revision), Key::Left) => {
                                    Some(WorkMode::Drafting)
                                }
                                (Some(WorkMode::Drafting), Key::Right)
                                | (Some(WorkMode::ReadThrough), Key::Left) => {
                                    Some(WorkMode::Revision)
                                }
                                (Some(WorkMode::Revision), Key::Right) | (None, Key::Left) => {
                                    Some(WorkMode::ReadThrough)
                                }
                                _ => None,
                            };
                        }
                        Key::Left | Key::Right if sel == 11 => {
                            profile.difficulty = if key == Key::Left {
                                profile.difficulty.saturating_sub(1).max(1)
                            } else {
                                (profile.difficulty + 1).min(3)
                            };
                        }
                        Key::Left | Key::Right if sel == 12 => {
                            let options = [5, 15, 30, 60, 120];
                            let at = options
                                .iter()
                                .position(|n| *n == profile.max_minutes)
                                .unwrap_or(2);
                            profile.max_minutes = options[if key == Key::Left {
                                at.saturating_sub(1)
                            } else {
                                (at + 1).min(4)
                            }];
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.msg = format!("Productivity action not recorded: {e:#}");
        }
        if matches!(self.overlay,Overlay::Productivity{page:p,..}if p==page) {
            sel = sel.min(self.productivity_count(page).saturating_sub(1));
            self.overlay = Overlay::Productivity { page, sel, scroll };
            self.refresh_productivity_snapshot();
            self.build_productivity_rows(page, sel);
        }
    }
    fn on_productivity_input(&mut self, key: Key) {
        let Overlay::ProductivityInput { kind, buf, back } = &mut self.overlay else {
            return;
        };
        match key {
            Key::Esc => {
                let back = *back;
                self.open_productivity(back);
            }
            Key::Backspace => {
                buf.pop();
            }
            Key::Char(c) if (c.is_ascii_digit() || c == '-') && buf.len() < 16 => buf.push(c),
            Key::Enter => {
                let (kind, text, back) = (*kind, buf.clone(), *back);
                self.flush();
                let result = (|| -> Result<()> {
                    match kind {
                        Input::ScheduleStart | Input::ScheduleEnd => {
                            grimoire_core::journal::date(&text)?;
                            self.productivity.update(|s| {
                                let schedule =
                                    s.schedule.as_mut().context("create schedule first")?;
                                if kind == Input::ScheduleStart {
                                    schedule.start = text;
                                } else {
                                    schedule.end = text;
                                }
                                Ok(())
                            })?;
                        }
                        Input::Goal(i) => {
                            self.new_productivity_goal(
                                i,
                                text.parse().context("enter a positive integer target")?,
                            )?;
                        }
                        Input::ScheduleTarget => {
                            let n = text.parse().context("enter a positive word target")?;
                            self.productivity.update(|s| {
                                s.schedule.as_mut().context("create schedule first")?.target = n;
                                Ok(())
                            })?;
                        }
                        Input::Idle => {
                            let n = text.parse().context("enter 1–900 seconds")?;
                            if !self.checkpoint_productivity(true) {
                                bail!("checkpoint pending operations first");
                            }
                            self.productivity.update(|s| {
                                s.idle_seconds = n;
                                Ok(())
                            })?;
                            self.productivity.activity.pause();
                        }
                        Input::Sprint(kind) => {
                            if !self.checkpoint_productivity(true) {
                                bail!("recorded metrics checkpoint unavailable");
                            }
                            let n: u64 = text.parse().context("enter a positive sprint target")?;
                            if n == 0
                                || n > 1_000_000
                                || matches!(
                                    kind,
                                    SprintKind::Timed | SprintKind::NoEditing | SprintKind::Break
                                ) && n > 240
                            {
                                bail!(
                                    "sprint target out of range (time 1–240 minutes; count 1–1,000,000)"
                                );
                            }
                            self.new_advanced_sprint(kind, n)?;
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        self.open_productivity(back);
                        self.msg =
                            "Local productivity setting recorded; manuscript originals untouched"
                                .into();
                    }
                    Err(e) => {
                        self.msg = format!("Not recorded: {e:#}; input and originals retained")
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    f.render_widget(Clear, area);
    if let Overlay::ProductivityInput { kind, buf, .. } = &app.overlay {
        let block = Block::bordered()
            .title(" Productivity · explicit target/date ")
            .border_style(Style::default().fg(t.accent));
        let inside = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(format!("{kind:?}")),
                Line::from(format!("Value: {buf}▏")),
                Line::from("Backspace edits · Enter validates/saves · Esc cancels"),
                Line::from("ISO date YYYY-MM-DD; numeric targets only. No original prose edited."),
            ])
            .wrap(Wrap { trim: false }),
            inside,
        );
        if inside.height >= 8 {
            f.render_widget(
                Paragraph::new(app.msg.as_str())
                    .style(Style::default().fg(t.accent))
                    .wrap(Wrap { trim: false }),
                Rect::new(inside.x, inside.y + inside.height - 2, inside.width, 2),
            );
        }
        return;
    }
    let Overlay::Productivity { page, scroll, .. } = app.overlay else {
        return;
    };
    let block = Block::bordered()
        .title(format!(" {} ", page.title()))
        .border_style(Style::default().fg(t.accent));
    let inside = block.inner(area);
    f.render_widget(block, area);
    if inside.height < 5 || inside.width < 8 {
        return;
    }
    let header = Rect::new(inside.x, inside.y, inside.width, 1);
    f.render_widget(
        Paragraph::new("1 edits  2 goals  3 calendar  4 sprints  5 collection  6 challenges")
            .style(Style::default().fg(t.dim)),
        header,
    );
    let footer_h = 5.min(inside.height.saturating_sub(2));
    let body = Rect::new(
        inside.x,
        inside.y + 1,
        inside.width,
        inside.height - 1 - footer_h,
    );
    let footer = Rect::new(inside.x, body.y + body.height, inside.width, footer_h);
    let mut lines = Vec::new();
    let mut selected_at = None;
    for (i, row) in app.productivity.rows.iter().enumerate() {
        let chosen = app.productivity.row_selection == Some(i);
        if chosen {
            selected_at = Some(lines.len());
        }
        for text in crate::ui::wrap_words(row, body.width.saturating_sub(2) as usize) {
            lines.push(Line::styled(
                format!("{}{text}", if chosen { "› " } else { "  " }),
                Style::default().fg(if chosen { t.accent } else { t.text }),
            ));
        }
    }
    let max = lines.len().saturating_sub(body.height as usize);
    let offset = scroll
        .unwrap_or_else(|| {
            selected_at
                .unwrap_or(0)
                .saturating_sub(body.height as usize / 2)
        })
        .min(max);
    app.productivity.render_fit.set((offset, max));
    f.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(offset)
                .take(body.height as usize)
                .collect::<Vec<_>>(),
        ),
        body,
    );
    f.render_widget(
        Paragraph::new(format!(
            "{}\nr reload/retry · Tab page · PgUp/PgDn scroll · Esc back · F1 policy",
            page.hints()
        ))
        .style(Style::default().fg(t.dim))
        .wrap(Wrap { trim: false }),
        Rect::new(
            footer.x,
            footer.y,
            footer.width,
            footer.height.saturating_sub(2),
        ),
    );
    f.render_widget(
        Paragraph::new(app.msg.as_str())
            .style(Style::default().fg(t.accent))
            .wrap(Wrap { trim: false }),
        Rect::new(
            footer.x,
            footer.y + footer.height.saturating_sub(2),
            footer.width,
            2,
        ),
    );
}
