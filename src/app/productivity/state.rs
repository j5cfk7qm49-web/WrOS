use anyhow::{Context, Result, bail};
use chrono::{Duration as CivilDuration, NaiveDate};
use grimoire_core::{
    accounting::{ActiveClock, Clock, Ledger, Role, Source, SystemClock},
    challenges::{self, Day, Profile},
    measure::{Counts, Mutation},
    productivity::{self as model, Metric, Snapshot, Sprint, SprintKind, SprintStatus},
    statefile::{self, Store},
};
use std::collections::BTreeSet;
use std::path::Path;

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
    pub(super) render_fit: std::cell::Cell<(usize, usize)>,
    pub store: Option<Store<model::Settings>>,
    pub config: model::Settings,
    pub ledger: Ledger,
    pub clock: Box<dyn Clock>,
    pub activity: ActiveClock,
    pub sprint: Option<Sprint>,
    pub snapshot: Snapshot,
    pub day: Option<Store<Day>>,
    pub date: String,
    pub(super) calendar_date: NaiveDate,
    pub warning: String,
    pub(crate) rows: Vec<String>,
    pub(super) row_selection: Option<usize>,
    pub(super) draft_profile: Option<Profile>,
    pub(super) last_flush: u64,
    pub(super) last_tick: Option<u64>,
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
            calendar_date: now.wall.date_naive(),
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
            calendar_summary: "No calendar schedule · n creates the November preset".into(),
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
    pub(super) fn record(&mut self, source: Source, mutation: Mutation) {
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
    pub(super) fn recent(&self, root: &Path, date: &str) -> Result<BTreeSet<String>> {
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

    pub(super) fn load_challenge_day(&mut self, root: &Path) -> Result<()> {
        self.day = None;
        if !self.config.challenges {
            bail!("Daily challenges disabled; no assignment generated");
        }
        let now = self.clock.now();
        let date = self.date.clone();
        let path = challenges::path(&date)?;
        let existing = statefile::read(root, &path)?;
        if existing.is_none() && date != now.day() {
            bail!("No assignment recorded for this date; no historical work/assignment invented");
        }
        let default = match existing {
            Some(bytes) => serde_json::from_slice::<Day>(&bytes)?,
            None => Day::assign(
                &now,
                self.config.challenge_profile.clone(),
                self.config.challenge_seed,
                &self.recent(root, &date)?,
            )?,
        };
        let mut store = Store::load(root, &path, default)?;
        if store.value.date != date {
            bail!("Challenge date mismatch; original preserved");
        }
        if !store.exists() {
            store.update(|_| Ok(()))?;
        }
        self.day = Some(store);
        Ok(())
    }
    pub(super) fn row_count(&self, page: Page) -> usize {
        match page {
            Page::Goals => self.config.goals.len(),
            Page::GoalTypes => GOALS.len(),
            Page::Sprints => 0,
            Page::SprintTypes => SPRINTS.len(),
            Page::Challenges => self.day.as_ref().map_or(0, |s| s.value.assignments.len()),
            Page::ChallengeFilters => 13,
            Page::Achievements => self.config.awards.len(),
            _ => 0,
        }
    }
}

pub(super) const GOALS: [(&str, Metric, u64); 9] = [
    ("Daily input additions", Metric::InputAdded, 500),
    ("Weekly keyboard additions", Metric::TypedAdded, 3500),
    ("Monthly recorded net change", Metric::NetInput, 10000),
    (
        "Native current project words",
        Metric::ManuscriptWords,
        80000,
    ),
    ("Native current scene words", Metric::SceneWords, 1000),
    ("Daily observed active minutes", Metric::ActiveMinutes, 30),
    ("Daily revision operations", Metric::RevisionOperations, 10),
    ("Daily footnote additions", Metric::FootnoteAdded, 100),
    ("Scheduled deadline net change", Metric::NetInput, 50000),
];
pub(super) const SPRINTS: [(SprintKind, &str, u64); 5] = [
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
