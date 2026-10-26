//! Offline goals, civil-date schedules, optional milestones and sprint state.
//! Values come from recorded operations or explicitly identified native source
//! counts. Self-report is not silently upgraded to measured completion.
use crate::{
    accounting::{self, Entry, Mode, Role, Totals},
    statefile::Validate,
};
use anyhow::{Result, bail};
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Metric {
    InputAdded,
    TypedAdded,
    NetInput,
    RevisionOperations,
    ActiveMinutes,
    FootnoteAdded,
    ManuscriptWords,
    SceneWords,
}
impl Metric {
    pub fn label(self) -> &'static str {
        match self {
            Self::InputAdded => "recorded input word additions",
            Self::TypedAdded => "keyboard word additions (no paste/replay)",
            Self::NetInput => "recorded editor net change",
            Self::RevisionOperations => "recorded revision operations",
            Self::ActiveMinutes => "observed inter-edit minutes",
            Self::FootnoteAdded => "recorded footnote word additions",
            Self::ManuscriptWords => "native current manuscript words",
            Self::SceneWords => "native current scene words",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Period {
    Daily,
    Weekly,
    Monthly,
    SinceCreated,
    Deadline { start: String, end: String },
}
impl Period {
    pub fn window(&self, day: &str, created: &str) -> Result<(String, String)> {
        let d = crate::journal::date(day)?;
        let (a, b) = match self {
            Self::Daily => (d, d),
            Self::Weekly => {
                let a = d - Duration::days(d.weekday().num_days_from_monday() as i64);
                (a, a + Duration::days(6))
            }
            Self::Monthly => {
                let a = d.with_day(1).unwrap();
                let b = a
                    .checked_add_months(Months::new(1))
                    .ok_or_else(|| anyhow::anyhow!("calendar boundary unavailable"))?
                    - Duration::days(1);
                (a, b)
            }
            Self::SinceCreated => (crate::journal::date(created)?, d),
            Self::Deadline { start, end } => {
                (crate::journal::date(start)?, crate::journal::date(end)?)
            }
        };
        Ok((a.to_string(), b.to_string()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub id: u64,
    pub label: String,
    pub metric: Metric,
    pub target: u64,
    pub period: Period,
    pub created: String,
    pub mode: Option<Mode>,
    pub source: Option<PathBuf>,
    pub enabled: bool,
}
impl Validate for Goal {
    fn validate(&self) -> Result<()> {
        if self.id == 0
            || self.label.is_empty()
            || self.label.chars().count() > 80
            || self.label.chars().any(char::is_control)
            || self.target == 0
            || self.target > 1_000_000_000
        {
            bail!("invalid goal identity/target/label");
        }
        crate::journal::date(&self.created)?;
        if let Some(p) = &self.source {
            crate::pins::source_relative(p)?;
            if !p.starts_with("manuscript") {
                bail!("goal source is not manuscript");
            }
        }
        if self.metric == Metric::SceneWords && self.source.is_none() {
            bail!("scene goal requires its source");
        }
        if let Period::Deadline { start, end } = &self.period {
            let (a, b) = (crate::journal::date(start)?, crate::journal::date(end)?);
            if b < a || b.signed_duration_since(a).num_days() > 3660 {
                bail!("deadline range exceeds ten years");
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Snapshot {
    pub complete: bool,
    pub entries: Vec<Entry>,
    pub manuscript_words: Option<u64>,
    pub scenes: BTreeMap<PathBuf, u64>,
}
impl Snapshot {
    pub fn value(
        &self,
        metric: Metric,
        from: &str,
        to: &str,
        mode: Option<Mode>,
        source: Option<&PathBuf>,
        footnotes: bool,
    ) -> Option<i128> {
        if metric == Metric::ManuscriptWords {
            return self.manuscript_words.map(i128::from);
        }
        if metric == Metric::SceneWords {
            return source
                .and_then(|p| self.scenes.get(p))
                .copied()
                .map(i128::from);
        }
        if !self.complete {
            return None;
        }
        let entries = self.entries.iter().filter(|e| {
            mode.is_none_or(|m| m == e.mode) && source.is_none_or(|p| *p == e.source.path)
        });
        let t = Totals::collect(entries, from, to, Role::Manuscript);
        if (matches!(
            metric,
            Metric::InputAdded | Metric::TypedAdded | Metric::FootnoteAdded | Metric::NetInput
        ) && t.input.gaps > 0)
            || (metric == Metric::NetInput && t.replay.gaps > 0)
        {
            return None;
        }
        Some(match metric {
            Metric::InputAdded => {
                t.input.added as i128
                    + if footnotes {
                        t.input.foot_added as i128
                    } else {
                        0
                    }
            }
            Metric::TypedAdded => {
                t.typed.added as i128
                    + if footnotes {
                        t.typed.foot_added as i128
                    } else {
                        0
                    }
            }
            Metric::NetInput => t.net(footnotes),
            Metric::RevisionOperations => t.revision_operations as i128,
            Metric::ActiveMinutes => {
                if t.unassigned_ms > 0 {
                    return None;
                }
                t.active_ms as i128 / 60_000
            }
            Metric::FootnoteAdded => t.input.foot_added as i128,
            _ => unreachable!(),
        })
    }
    pub fn goal(&self, goal: &Goal, day: &str, footnotes: bool) -> Option<i128> {
        let (a, b) = goal.period.window(day, &goal.created).ok()?;
        self.value(
            goal.metric,
            &a,
            &b,
            goal.mode,
            goal.source.as_ref(),
            footnotes,
        )
    }
    pub fn lifetime(&self, metric: Metric, mode: Option<Mode>, footnotes: bool) -> Option<i128> {
        self.value(metric, "0001-01-01", "9999-12-31", mode, None, footnotes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub start: String,
    pub end: String,
    pub target: u64,
    pub rest_days: BTreeSet<String>,
}
impl Schedule {
    pub fn november(year: i32) -> Result<Self> {
        let start =
            NaiveDate::from_ymd_opt(year, 11, 1).ok_or_else(|| anyhow::anyhow!("invalid year"))?;
        let s = Self {
            start: start.to_string(),
            end: NaiveDate::from_ymd_opt(year, 11, 30).unwrap().to_string(),
            target: 50_000,
            rest_days: BTreeSet::new(),
        };
        s.validate()?;
        Ok(s)
    }
    pub fn days(&self) -> Result<Vec<NaiveDate>> {
        self.validate()?;
        let (a, b) = (
            crate::journal::date(&self.start)?,
            crate::journal::date(&self.end)?,
        );
        Ok((0..=b.signed_duration_since(a).num_days())
            .map(|i| a + Duration::days(i))
            .collect())
    }
    pub fn toggle_rest(&mut self, day: &str) -> Result<()> {
        let d = crate::journal::date(day)?;
        if d < crate::journal::date(&self.start)? || d > crate::journal::date(&self.end)? {
            bail!("rest day outside schedule");
        }
        if !self.rest_days.remove(day) {
            self.rest_days.insert(day.into());
        }
        Ok(())
    }
    pub fn pace(&self, day: &str, recorded_net: i128) -> Result<Pace> {
        let today = crate::journal::date(day)?;
        let days = self.days()?;
        let working: Vec<_> = days
            .into_iter()
            .filter(|d| !self.rest_days.contains(&d.to_string()))
            .collect();
        let left = working.iter().filter(|&&d| d >= today).count() as u64;
        let remaining = (self.target as i128 - recorded_net).max(0) as u128;
        let due = working.iter().filter(|&&d| d <= today).count() as i128;
        let expected = if working.is_empty() {
            None
        } else {
            Some(self.target as i128 * due / working.len() as i128)
        };
        Ok(Pace {
            working_days_left: left,
            required_per_day: if remaining == 0 {
                Some(0)
            } else if left == 0 {
                None
            } else {
                Some(remaining.div_ceil(left as u128).min(u64::MAX as u128) as u64)
            },
            ahead_behind: expected.map(|n| recorded_net - n),
        })
    }
    /// Forecast assumes the mean of the last at most seven *recorded* working
    /// dates repeats, including cuts. Missing dates are not invented as zero.
    pub fn forecast(
        &self,
        day: &str,
        recorded_net: i128,
        history: &BTreeMap<String, i128>,
    ) -> Result<Option<String>> {
        let today = crate::journal::date(day)?;
        if recorded_net >= self.target as i128 {
            return Ok(Some(day.into()));
        }
        let observations: Vec<_> = history
            .iter()
            .rev()
            .filter(|(d, _)| {
                d.as_str() <= day
                    && d.as_str() >= self.start.as_str()
                    && d.as_str() <= self.end.as_str()
                    && !self.rest_days.contains(*d)
            })
            .take(7)
            .map(|(_, n)| *n)
            .collect();
        if observations.len() < 3 {
            return Ok(None);
        }
        let sum: i128 = observations.iter().sum();
        if sum <= 0 {
            return Ok(None);
        }
        let required = (self.target as i128 - recorded_net) * observations.len() as i128;
        let needed = (required + sum - 1) / sum;
        let mut seen = 0i128;
        for i in 1..=3660 {
            let Some(date) = today.checked_add_signed(Duration::days(i)) else {
                return Ok(None);
            };
            if !self.rest_days.contains(&date.to_string()) {
                seen += 1;
            }
            if seen >= needed {
                return Ok(Some(date.to_string()));
            }
        }
        Ok(None)
    }
}
impl Validate for Schedule {
    fn validate(&self) -> Result<()> {
        let (a, b) = (
            crate::journal::date(&self.start)?,
            crate::journal::date(&self.end)?,
        );
        if b < a
            || b.signed_duration_since(a).num_days() > 3660
            || self.target == 0
            || self.target > 1_000_000_000
            || self.rest_days.len() > 3661
        {
            bail!("invalid schedule");
        }
        for day in &self.rest_days {
            let d = crate::journal::date(day)?;
            if d < a || d > b {
                bail!("rest day outside schedule");
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pace {
    pub working_days_left: u64,
    pub required_per_day: Option<u64>,
    pub ahead_behind: Option<i128>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SprintKind {
    Break,
    Timed,
    Words,
    Revision,
    NoEditing,
    Challenge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SprintStatus {
    Running,
    Paused,
    Completed,
    Stopped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprint {
    pub id: u64,
    pub kind: SprintKind,
    pub status: SprintStatus,
    pub started_at: String,
    pub last_checkpoint: String,
    pub target: u64,
    pub elapsed_ms: u64,
    pub progress: i128,
    pub segment_start: Option<i128>,
    pub include_footnotes: bool,
    pub mode: Option<Mode>,
    pub criterion: Option<crate::challenges::Criterion>,
    pub metrics_incomplete: bool,
    pub deletion_observed: bool,
    pub challenge: Option<(String, String)>,
    pub spark: Option<crate::spark::ResultRef>,
    pub break_timer: Option<Box<Sprint>>,
    pub breaks: Vec<Sprint>,
    #[serde(skip)]
    pub anchor_ms: Option<u64>,
}
impl Sprint {
    pub fn new(
        id: u64,
        kind: SprintKind,
        target: u64,
        at: &accounting::Moment,
        reading: Option<i128>,
        footnotes: bool,
    ) -> Result<Self> {
        if matches!(
            kind,
            SprintKind::Words | SprintKind::Revision | SprintKind::Challenge
        ) && reading.is_none()
        {
            bail!("measured sprint requires complete recorded metrics");
        }
        let s = Self {
            id,
            kind,
            status: SprintStatus::Running,
            started_at: at.stamp(),
            last_checkpoint: at.stamp(),
            target,
            elapsed_ms: 0,
            progress: 0,
            segment_start: reading,
            include_footnotes: footnotes,
            mode: if kind == SprintKind::Revision {
                Some(Mode::Revision)
            } else {
                None
            },
            criterion: None,
            metrics_incomplete: false,
            deletion_observed: false,
            challenge: None,
            spark: None,
            break_timer: None,
            breaks: vec![],
            anchor_ms: Some(at.mono_ms),
        };
        s.validate()?;
        Ok(s)
    }
    pub fn metric(&self) -> Metric {
        if let Some(c) = &self.criterion {
            return c.metric;
        }
        if self.kind == SprintKind::Revision {
            Metric::RevisionOperations
        } else {
            Metric::NetInput
        }
    }
    pub fn time_due(&self, now: &accounting::Moment) -> bool {
        if let Some(b) = &self.break_timer {
            return b.time_due(now);
        }
        self.status == SprintStatus::Running
            && matches!(
                self.kind,
                SprintKind::Timed | SprintKind::NoEditing | SprintKind::Break
            )
            && self
                .anchor_ms
                .and_then(|a| now.mono_ms.checked_sub(a))
                .is_some_and(|delta| {
                    self.elapsed_ms.saturating_add(delta) >= self.target.saturating_mul(60_000)
                })
    }
    pub fn advance(&mut self, now: &accounting::Moment, reading: Option<i128>) -> bool {
        if let Some(b) = &mut self.break_timer {
            if b.advance(now, None) {
                let finished = *self.break_timer.take().unwrap();
                self.breaks.push(finished);
                self.last_checkpoint = now.stamp();
                return true;
            }
            return false;
        }
        if self.status != SprintStatus::Running {
            return false;
        }
        let Some(last) = self.anchor_ms.replace(now.mono_ms) else {
            self.anchor_ms = None;
            return false;
        };
        let Some(delta) = now.mono_ms.checked_sub(last) else {
            self.status = SprintStatus::Paused;
            self.anchor_ms = None;
            return false;
        };
        self.elapsed_ms = self.elapsed_ms.saturating_add(delta);
        if reading.is_none() {
            self.metrics_incomplete = true;
        }
        let progress = self.progress.saturating_add(
            reading
                .zip(self.segment_start)
                .map_or(0, |(a, b)| a.saturating_sub(b)),
        );
        self.segment_start = reading;
        self.progress = progress;
        self.last_checkpoint = now.stamp();
        let done = match self.kind {
            SprintKind::Break | SprintKind::Timed | SprintKind::NoEditing => {
                self.elapsed_ms >= self.target.saturating_mul(60_000)
            }
            _ => {
                reading.is_some()
                    && !self.metrics_incomplete
                    && self.progress >= self.target as i128
            }
        };
        if done {
            self.status = SprintStatus::Completed;
            self.anchor_ms = None;
        }
        done
    }
    pub fn pause(&mut self, now: &accounting::Moment, reading: Option<i128>) {
        self.advance(now, reading);
        if let Some(b) = &mut self.break_timer {
            b.pause(now, None);
        }
        if self.status == SprintStatus::Running {
            self.status = SprintStatus::Paused;
        }
        self.anchor_ms = None;
    }
    pub fn resume(&mut self, now: &accounting::Moment, reading: Option<i128>) -> Result<()> {
        if let Some(b) = &mut self.break_timer {
            return b.resume(now, None);
        }
        if matches!(self.status, SprintStatus::Completed | SprintStatus::Stopped) {
            bail!("finished sprint cannot resume");
        }
        if matches!(
            self.kind,
            SprintKind::Words | SprintKind::Revision | SprintKind::Challenge
        ) && reading.is_none()
        {
            bail!("recorded metrics unavailable");
        }
        self.status = SprintStatus::Running;
        self.anchor_ms = Some(now.mono_ms);
        self.segment_start = reading;
        self.last_checkpoint = now.stamp();
        Ok(())
    }
    pub fn start_break(
        &mut self,
        id: u64,
        now: &accounting::Moment,
        reading: Option<i128>,
        minutes: u64,
    ) -> Result<()> {
        if self.kind == SprintKind::Break
            || self.break_timer.is_some()
            || matches!(self.status, SprintStatus::Completed | SprintStatus::Stopped)
            || minutes == 0
            || minutes > 120
            || self.breaks.len() >= 64
        {
            bail!(
                "finish/skip the existing break; optional breaks are 1–120 minutes, max 64 retained per sprint"
            );
        }
        self.pause(now, reading);
        if self.status == SprintStatus::Completed {
            bail!("sprint completed before break; start a standalone break instead");
        }
        self.break_timer = Some(Box::new(Self::new(
            id,
            SprintKind::Break,
            minutes,
            now,
            None,
            self.include_footnotes,
        )?));
        Ok(())
    }
    pub fn skip_break(&mut self, now: &accounting::Moment) -> Result<()> {
        let mut b = self
            .break_timer
            .take()
            .ok_or_else(|| anyhow::anyhow!("no active break"))?;
        b.pause(now, None);
        if b.status != SprintStatus::Completed {
            b.status = SprintStatus::Stopped;
        }
        self.breaks.push(*b);
        self.last_checkpoint = now.stamp();
        Ok(())
    }
    pub fn stop(&mut self, now: &accounting::Moment, reading: Option<i128>) {
        self.pause(now, reading);
        if self.break_timer.is_some() {
            let _ = self.skip_break(now);
        }
        self.status = SprintStatus::Stopped;
        self.anchor_ms = None;
    }
    pub fn effective_paused(&self) -> bool {
        if let Some(b) = &self.break_timer {
            return b.effective_paused();
        }
        self.status == SprintStatus::Paused
            || (self.status == SprintStatus::Running && self.anchor_ms.is_none())
    }
}
impl Validate for Sprint {
    fn validate(&self) -> Result<()> {
        if self.id == 0
            || self.target == 0
            || self.target > 1_000_000_000
            || self.breaks.len() > 64
            || self.elapsed_ms > 31_622_400_000
        {
            bail!("invalid sprint bound");
        }
        if self.kind == SprintKind::Break && (self.break_timer.is_some() || !self.breaks.is_empty())
        {
            bail!("nested break records refused");
        }
        if let Some(b) = &self.break_timer {
            if b.kind != SprintKind::Break
                || self.status != SprintStatus::Paused
                || !matches!(b.status, SprintStatus::Running | SprintStatus::Paused)
            {
                bail!("invalid break lifecycle");
            }
            b.validate()?;
        }
        for b in &self.breaks {
            if b.kind != SprintKind::Break
                || !matches!(b.status, SprintStatus::Completed | SprintStatus::Stopped)
            {
                bail!("unfinished break in history");
            }
            b.validate()?;
        }
        chrono::DateTime::parse_from_rfc3339(&self.started_at)?;
        chrono::DateTime::parse_from_rfc3339(&self.last_checkpoint)?;
        if let Some(spark) = &self.spark {
            spark.validate()?;
        }
        if let Some((day, id)) = &self.challenge {
            crate::journal::date(day)?;
            if id.is_empty() || id.len() > 80 {
                bail!("invalid sprint challenge reference");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Award {
    pub source: Option<PathBuf>,
    pub id: String,
    pub earned_at: String,
    pub measured: bool,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub tracking: bool,
    pub idle_seconds: u16,
    pub mode: Mode,
    pub include_footnotes: bool,
    pub achievements: bool,
    pub notifications: bool,
    pub challenges: bool,
    pub next_id: u64,
    pub goals: Vec<Goal>,
    pub schedule: Option<Schedule>,
    pub schedule_history: Vec<Schedule>,
    pub awards: Vec<Award>,
    pub sprint: Option<Sprint>,
    pub sprint_history: Vec<Sprint>,
    pub challenge_profile: crate::challenges::Profile,
    pub challenge_seed: u64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            tracking: true,
            idle_seconds: 60,
            mode: Mode::Drafting,
            include_footnotes: false,
            achievements: true,
            notifications: false,
            challenges: true,
            next_id: 1,
            goals: vec![],
            schedule: None,
            schedule_history: vec![],
            awards: vec![],
            sprint: None,
            sprint_history: vec![],
            challenge_profile: crate::challenges::Profile::default(),
            challenge_seed: 0x5744494e4b,
        }
    }
}
impl Validate for Settings {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.idle_seconds < 1
            || self.idle_seconds > 900
            || self.next_id == 0
            || self.goals.len() > 64
            || self.awards.len() > 512
            || self.sprint_history.len() > 128
            || self.schedule_history.len() > 120
        {
            bail!("unknown/invalid productivity settings; preserved");
        }
        let mut ids = BTreeSet::new();
        for g in &self.goals {
            g.validate()?;
            if g.id >= self.next_id || !ids.insert(g.id) {
                bail!("invalid goal ID");
            }
        }
        if let Some(s) = &self.schedule {
            s.validate()?;
        }
        for s in &self.schedule_history {
            s.validate()?;
        }
        let mut awards = BTreeSet::new();
        for a in &self.awards {
            if !awards.insert(&a.id)
                || a.id.is_empty()
                || a.id.len() > 160
                || a.label.chars().count() > 120
                || a.label.chars().any(char::is_control)
            {
                bail!("invalid award");
            }
            chrono::DateTime::parse_from_rfc3339(&a.earned_at)?;
            if let Some(source) = &a.source {
                crate::workspaces::relative(source)?;
                if !source.starts_with("manuscript") {
                    bail!("finished-source milestone outside manuscript");
                }
            }
        }
        if let Some(s) = &self.sprint {
            s.validate()?;
            if s.id >= self.next_id {
                bail!("invalid sprint ID");
            }
        }
        for s in &self.sprint_history {
            s.validate()?;
            if !matches!(s.status, SprintStatus::Completed | SprintStatus::Stopped) {
                bail!("unfinished sprint in results");
            }
        }
        fn sprint_ids(s: &Sprint, next: u64, ids: &mut BTreeSet<u64>) -> Result<()> {
            if s.id >= next || !ids.insert(s.id) {
                bail!("conflicting/out-of-range sprint ID");
            }
            if s.kind == SprintKind::Challenge && (s.challenge.is_none() || s.criterion.is_none()) {
                bail!("challenge-linked sprint lacks its measured source reference");
            }
            if let Some(b) = &s.break_timer {
                sprint_ids(b, next, ids)?;
            }
            for b in &s.breaks {
                sprint_ids(b, next, ids)?;
            }
            Ok(())
        }
        if let Some(s) = &self.sprint {
            sprint_ids(s, self.next_id, &mut ids)?;
        }
        for s in &self.sprint_history {
            sprint_ids(s, self.next_id, &mut ids)?;
        }
        self.challenge_profile.validate()?;
        Ok(())
    }
}
impl Settings {
    pub fn allocate(&mut self) -> Result<u64> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("productivity IDs exhausted"))?;
        Ok(id)
    }
    pub fn award(&mut self, id: &str, label: &str, measured: bool, at: &str) -> Result<bool> {
        if !self.achievements || self.awards.iter().any(|a| a.id == id) {
            return Ok(false);
        }
        if self.awards.len() >= 512 {
            return Ok(false);
        }
        self.awards.push(Award {
            source: None,
            id: id.into(),
            earned_at: at.into(),
            measured,
            label: label.into(),
        });
        Ok(true)
    }
    pub fn observe_awards(&mut self, snapshot: &Snapshot, at: &str) -> Result<Vec<String>> {
        let mut added = Vec::new();
        if !self.achievements {
            return Ok(added);
        }
        if let Some(n) = snapshot.lifetime(Metric::TypedAdded, None, false) {
            for (threshold, id, label) in [
                (1, "first-input", "First measured manuscript word unit"),
                (
                    100,
                    "hundred-input",
                    "One hundred keyboard-added word units",
                ),
                (
                    1000,
                    "thousand-input",
                    "One thousand keyboard-added word units",
                ),
            ] {
                if n >= threshold && self.award(id, label, true, at)? {
                    added.push(label.into());
                }
            }
        }
        if snapshot
            .lifetime(Metric::RevisionOperations, Some(Mode::Revision), false)
            .is_some_and(|n| n >= 1)
            && self.award("first-revision", "A measured revision experiment", true, at)?
        {
            added.push("A measured revision experiment".into());
        }
        let day = chrono::DateTime::parse_from_rfc3339(at)?
            .format("%Y-%m-%d")
            .to_string();
        for goal in self.goals.clone().into_iter().filter(|g| g.enabled) {
            if snapshot
                .goal(&goal, &day, self.include_footnotes)
                .is_some_and(|n| n >= goal.target as i128)
            {
                let (from, to) = goal.period.window(&day, &goal.created)?;
                let id = if goal.period == Period::SinceCreated {
                    format!("goal-{}-lifetime", goal.id)
                } else {
                    format!("goal-{}-{from}-{to}", goal.id)
                };
                let label = if matches!(goal.metric, Metric::ManuscriptWords | Metric::SceneWords) {
                    "Reached a native existing-source word target"
                } else {
                    "Reached a recorded-operation goal target"
                };
                if self.award(&id, label, true, at)? {
                    added.push(label.into());
                }
            }
        }
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounting::{Activity, Moment, Role, Source},
        measure::{Counts, Origin},
    };
    fn now(ms: u64) -> Moment {
        Moment {
            mono_ms: ms,
            wall: chrono::DateTime::parse_from_rfc3339("2026-11-06T12:00:00+00:00").unwrap()
                + Duration::milliseconds(ms as i64),
            zone: Some("UTC".into()),
        }
    }
    fn entry(origin: Origin, mode: Mode, c: Counts) -> Entry {
        Entry {
            id: "s-test-1".into(),
            date: "2026-11-06".into(),
            zone: Some("UTC".into()),
            first_at: now(0).stamp(),
            last_at: now(0).stamp(),
            source: Source {
                path: "manuscript/a.md".into(),
                role: Role::Manuscript,
            },
            mode,
            origin,
            counts: c,
            activity: vec![],
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            complete: true,
            manuscript_words: Some(42),
            ..Snapshot::default()
        }
    }
    #[test]
    fn daily_weekly_monthly_and_leap_windows_are_civil_not_elapsed_days() {
        assert_eq!(
            Period::Daily.window("2028-02-29", "2026-01-01").unwrap(),
            ("2028-02-29".into(), "2028-02-29".into())
        );
        assert_eq!(
            Period::Monthly.window("2028-02-29", "2026-01-01").unwrap(),
            ("2028-02-01".into(), "2028-02-29".into())
        );
        assert_eq!(
            Period::Weekly.window("2027-01-01", "2026-01-01").unwrap(),
            ("2026-12-28".into(), "2027-01-03".into())
        );
    }
    #[test]
    fn gross_keyboard_paste_replay_revision_footnote_and_native_are_distinct() {
        let mut s = snapshot();
        for (origin, mode, c) in [
            (
                Origin::Typing,
                Mode::Drafting,
                Counts {
                    added: 3,
                    foot_added: 4,
                    operations: 7,
                    ..Counts::default()
                },
            ),
            (
                Origin::Paste,
                Mode::Drafting,
                Counts {
                    added: 2,
                    operations: 1,
                    ..Counts::default()
                },
            ),
            (
                Origin::Undo,
                Mode::Drafting,
                Counts {
                    removed: 1,
                    operations: 1,
                    ..Counts::default()
                },
            ),
            (
                Origin::Replace,
                Mode::Revision,
                Counts {
                    added: 1,
                    removed: 1,
                    operations: 2,
                    ..Counts::default()
                },
            ),
        ] {
            s.entries.push(entry(origin, mode, c));
        }
        assert_eq!(s.lifetime(Metric::InputAdded, None, false), Some(6));
        assert_eq!(s.lifetime(Metric::TypedAdded, None, false), Some(3));
        assert_eq!(s.lifetime(Metric::InputAdded, None, true), Some(10));
        assert_eq!(s.lifetime(Metric::NetInput, None, false), Some(4));
        assert_eq!(
            s.lifetime(Metric::RevisionOperations, Some(Mode::Revision), false),
            Some(2)
        );
        assert_eq!(s.lifetime(Metric::ManuscriptWords, None, false), Some(42));
    }
    #[test]
    fn missing_history_and_word_gaps_are_not_known_zero() {
        let mut s = snapshot();
        s.complete = false;
        assert_eq!(s.lifetime(Metric::TypedAdded, None, false), None);
        assert_eq!(s.lifetime(Metric::ManuscriptWords, None, false), Some(42));
        s.complete = true;
        s.entries.push(entry(
            Origin::Typing,
            Mode::Revision,
            Counts {
                operations: 1,
                gaps: 1,
                ..Counts::default()
            },
        ));
        assert_eq!(s.lifetime(Metric::InputAdded, None, false), None);
        assert_eq!(s.lifetime(Metric::RevisionOperations, None, false), Some(1));
        assert_eq!(s.lifetime(Metric::ActiveMinutes, None, false), Some(0));
    }
    #[test]
    fn unknown_undo_words_do_not_destroy_known_gross_keyboard_counts() {
        let mut s = snapshot();
        s.entries.push(entry(
            Origin::Typing,
            Mode::Drafting,
            Counts {
                added: 2,
                ..Counts::default()
            },
        ));
        s.entries.push(entry(
            Origin::Undo,
            Mode::Drafting,
            Counts {
                gaps: 1,
                operations: 1,
                ..Counts::default()
            },
        ));
        assert_eq!(s.lifetime(Metric::TypedAdded, None, false), Some(2));
        assert_eq!(s.lifetime(Metric::NetInput, None, false), None);
    }
    #[test]
    fn ambiguous_clock_span_blocks_both_possible_daily_time_goals() {
        let mut s = snapshot();
        let mut e = entry(Origin::Typing, Mode::Drafting, Counts::default());
        e.activity.push(Activity {
            date: None,
            zone: Some("UTC".into()),
            span_start: "2026-11-05".into(),
            span_end: "2026-11-06".into(),
            milliseconds: 1000,
            typed_pair: true,
        });
        s.entries.push(e);
        assert_eq!(
            s.value(
                Metric::ActiveMinutes,
                "2026-11-05",
                "2026-11-05",
                None,
                None,
                false
            ),
            None
        );
        assert_eq!(
            s.value(
                Metric::ActiveMinutes,
                "2026-11-06",
                "2026-11-06",
                None,
                None,
                false
            ),
            None
        );
        assert_eq!(
            s.value(
                Metric::ActiveMinutes,
                "2026-11-04",
                "2026-11-04",
                None,
                None,
                false
            ),
            Some(0)
        );
    }
    #[test]
    fn source_scope_excludes_notes_and_another_scene() {
        let mut s = snapshot();
        let mut a = entry(
            Origin::Typing,
            Mode::Drafting,
            Counts {
                added: 4,
                ..Counts::default()
            },
        );
        a.source.role = Role::Notes;
        a.source.path = "notes/x.md".into();
        s.entries.push(a);
        s.entries.push(entry(
            Origin::Typing,
            Mode::Drafting,
            Counts {
                added: 2,
                ..Counts::default()
            },
        ));
        assert_eq!(s.lifetime(Metric::InputAdded, None, false), Some(2));
        assert_eq!(
            s.value(
                Metric::InputAdded,
                "2026-01-01",
                "2026-12-31",
                None,
                Some(&PathBuf::from("manuscript/missing.md")),
                false
            ),
            Some(0)
        );
        assert_eq!(
            s.value(
                Metric::SceneWords,
                "2026-01-01",
                "2026-12-31",
                None,
                Some(&PathBuf::from("manuscript/missing.md")),
                false
            ),
            None
        );
    }
    #[test]
    fn november_preset_rest_days_and_no_workdays_are_explicit() {
        let mut schedule = Schedule::november(2026).unwrap();
        assert_eq!(schedule.days().unwrap().len(), 30);
        let before = schedule.pace("2026-11-01", 0).unwrap();
        assert_eq!(before.required_per_day, Some(1667));
        schedule.toggle_rest("2026-11-01").unwrap();
        assert_eq!(
            schedule.pace("2026-11-01", 0).unwrap().working_days_left,
            29
        );
        assert!(schedule.toggle_rest("2026-12-01").is_err());
        for day in schedule.days().unwrap() {
            schedule.rest_days.insert(day.to_string());
        }
        assert_eq!(
            schedule.pace("2026-11-01", 0).unwrap().required_per_day,
            None
        );
        assert_eq!(
            schedule.pace("2026-11-01", 50000).unwrap().required_per_day,
            Some(0)
        );
    }
    #[test]
    fn forecast_requires_recorded_dates_and_states_its_mean_not_missing_zeroes() {
        let mut schedule = Schedule::november(2026).unwrap();
        schedule.target = 10000;
        let mut history = BTreeMap::new();
        history.insert("2026-11-01".into(), 1000);
        history.insert("2026-11-03".into(), 1000);
        assert_eq!(
            schedule.forecast("2026-11-06", 2000, &history).unwrap(),
            None
        );
        history.insert("2026-11-05".into(), 1000);
        assert_eq!(
            schedule.forecast("2026-11-06", 3000, &history).unwrap(),
            Some("2026-11-13".into())
        );
        schedule.toggle_rest("2026-11-07").unwrap();
        assert_eq!(
            schedule.forecast("2026-11-06", 3000, &history).unwrap(),
            Some("2026-11-14".into())
        );
    }
    #[test]
    fn word_sprint_pause_excludes_all_work_done_while_paused() {
        let mut s = Sprint::new(1, SprintKind::Words, 3, &now(0), Some(100), false).unwrap();
        assert!(!s.advance(&now(1000), Some(102)));
        s.pause(&now(2000), Some(102));
        s.resume(&now(3000), Some(200)).unwrap();
        assert!(s.advance(&now(4000), Some(201)));
        assert_eq!(s.progress, 3);
        assert_eq!(s.elapsed_ms, 3000);
        assert_eq!(s.status, SprintStatus::Completed);
    }
    #[test]
    fn restarting_timer_never_credits_wall_absence_or_previous_monotonic_epoch() {
        let mut s = Sprint::new(1, SprintKind::Timed, 1, &now(0), Some(0), false).unwrap();
        s.advance(&now(5000), Some(0));
        let text = serde_json::to_string(&s).unwrap();
        let mut s: Sprint = serde_json::from_str(&text).unwrap();
        assert!(s.effective_paused());
        assert!(!s.advance(&now(6000000), Some(99)));
        assert_eq!(s.elapsed_ms, 5000);
        s.resume(&now(6000000), Some(99)).unwrap();
        assert!(s.advance(&now(6055000), Some(100)));
        assert_eq!(s.elapsed_ms, 60000);
        assert_eq!(s.progress, 1);
    }
    #[test]
    fn optional_break_pauses_work_and_retains_its_own_actual_time() {
        let mut s = Sprint::new(1, SprintKind::Words, 3, &now(0), Some(0), false).unwrap();
        s.start_break(2, &now(5000), Some(1), 1).unwrap();
        assert_eq!(s.progress, 1);
        assert_eq!(s.status, SprintStatus::Paused);
        assert!(!s.advance(&now(35000), Some(10000)));
        assert!(s.advance(&now(65000), Some(10000)));
        assert_eq!(s.breaks.len(), 1);
        assert_eq!(s.breaks[0].elapsed_ms, 60000);
        assert_eq!(s.elapsed_ms, 5000);
        assert!(s.break_timer.is_none());
        assert!(s.effective_paused());
        s.resume(&now(70000), Some(10000)).unwrap();
        assert!(s.advance(&now(71000), Some(10002)));
        assert_eq!(s.progress, 3);
        assert_eq!(s.elapsed_ms, 6000);
        s.validate().unwrap();
    }
    #[test]
    fn break_restart_and_skip_preserve_records_without_automatic_work_resume() {
        let mut s = Sprint::new(1, SprintKind::Timed, 10, &now(0), None, false).unwrap();
        s.start_break(2, &now(1000), None, 1).unwrap();
        let mut s: Sprint = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(!s.advance(&now(900000), None));
        assert_eq!(s.break_timer.as_ref().unwrap().elapsed_ms, 0);
        s.resume(&now(900000), None).unwrap();
        s.skip_break(&now(910000)).unwrap();
        assert_eq!(s.breaks[0].status, SprintStatus::Stopped);
        assert_eq!(s.breaks[0].elapsed_ms, 10000);
        assert!(s.effective_paused());
        s.validate().unwrap();
    }
    #[test]
    fn monotonic_reset_pauses_instead_of_saturating_into_an_invented_interval() {
        let mut s = Sprint::new(1, SprintKind::Words, 5, &now(100), Some(0), false).unwrap();
        assert!(!s.advance(&now(99), Some(5)));
        assert_eq!(s.elapsed_ms, 0);
        assert_eq!(s.progress, 0);
        assert!(s.effective_paused());
    }
    #[test]
    fn incomplete_word_measurement_cannot_complete_a_word_sprint() {
        let mut s = Sprint::new(1, SprintKind::Words, 5, &now(0), Some(0), false).unwrap();
        s.advance(&now(100), None);
        assert!(!s.advance(&now(200), Some(10)));
        assert!(s.metrics_incomplete);
        assert_eq!(s.status, SprintStatus::Running);
    }
    #[test]
    fn optional_awards_deduplicate_and_disabled_collection_keeps_previous_work() {
        let mut settings = Settings::default();
        assert!(
            settings
                .award("a", "A real explicit experiment", false, &now(0).stamp())
                .unwrap()
        );
        assert!(
            !settings
                .award("a", "A real explicit experiment", false, &now(1).stamp())
                .unwrap()
        );
        settings.achievements = false;
        assert!(
            !settings
                .award("b", "Another", false, &now(2).stamp())
                .unwrap()
        );
        assert_eq!(settings.awards.len(), 1);
        settings.validate().unwrap();
    }
    #[test]
    fn unknown_settings_goal_and_sprint_fields_are_errors_not_default_migrations() {
        let settings = Settings::default();
        let mut v = serde_json::to_value(settings).unwrap();
        v["future"] = serde_json::json!({"retain":true});
        assert!(serde_json::from_value::<Settings>(v).is_err());
        let goal = Goal {
            id: 1,
            label: "Daily".into(),
            metric: Metric::TypedAdded,
            target: 10,
            period: Period::Daily,
            created: "2026-11-06".into(),
            source: None,
            mode: None,
            enabled: true,
        };
        let mut v = serde_json::to_value(goal).unwrap();
        v["future"] = true.into();
        assert!(serde_json::from_value::<Goal>(v).is_err());
    }
    #[test]
    fn full_optional_collection_does_not_block_future_metrics_or_settings() {
        let mut s = Settings::default();
        for id in 0..512 {
            assert!(
                s.award(&format!("award-{id}"), "Optional", false, &now(0).stamp())
                    .unwrap()
            );
        }
        assert!(
            !s.award("overflow", "Optional", true, &now(1).stamp())
                .unwrap()
        );
        assert!(s.observe_awards(&snapshot(), &now(1).stamp()).is_ok());
        assert_eq!(s.awards.len(), 512);
        s.validate().unwrap();
    }
    #[test]
    fn settings_reject_duplicate_global_goal_sprint_and_nested_break_ids() {
        let mut s = Settings::default();
        let id = s.allocate().unwrap();
        s.sprint = Some(Sprint::new(id, SprintKind::Timed, 1, &now(0), Some(0), false).unwrap());
        let mut b = Sprint::new(id, SprintKind::Break, 1, &now(0), None, false).unwrap();
        b.status = SprintStatus::Completed;
        s.sprint.as_mut().unwrap().breaks.push(b);
        assert!(s.validate().is_err());
    }
    #[test]
    fn finished_source_awards_reject_traversal_and_non_manuscript_references() {
        let mut s = Settings::default();
        s.award("done", "Author status", false, &now(0).stamp())
            .unwrap();
        for source in ["../outside.md", "notes/not-manuscript.md", "/absolute.md"] {
            s.awards[0].source = Some(source.into());
            assert!(s.validate().is_err());
        }
        s.awards[0].source = Some("manuscript/chapter/scene.md".into());
        s.validate().unwrap();
    }
}
