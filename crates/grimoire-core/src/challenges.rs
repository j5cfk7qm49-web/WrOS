//! Original offline exercises. Stable civil-date assignments store references,
//! not copied prose; measurable work and self-report remain visibly distinct.
use crate::{
    accounting::{Mode, Moment},
    productivity::{Metric, Snapshot},
    statefile::{Store, Validate},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::OnceLock,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Productivity,
    Creative,
    Character,
    Plot,
    Dialogue,
    Description,
    PointOfView,
    Revision,
    ReadThrough,
    SparkCombination,
}
impl Category {
    pub const ALL: [Self; 10] = [
        Self::Productivity,
        Self::Creative,
        Self::Character,
        Self::Plot,
        Self::Dialogue,
        Self::Description,
        Self::PointOfView,
        Self::Revision,
        Self::ReadThrough,
        Self::SparkCombination,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Productivity => "Productivity",
            Self::Creative => "Creative writing",
            Self::Character => "Character",
            Self::Plot => "Plot",
            Self::Dialogue => "Dialogue",
            Self::Description => "Description",
            Self::PointOfView => "Point of view",
            Self::Revision => "Revision",
            Self::ReadThrough => "Read-through",
            Self::SparkCombination => "Spark combination",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub categories: BTreeSet<Category>,
    pub mode: Option<Mode>,
    pub difficulty: u8,
    pub max_minutes: u16,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            categories: Category::ALL.into_iter().collect(),
            mode: None,
            difficulty: 2,
            max_minutes: 30,
        }
    }
}
impl Validate for Profile {
    fn validate(&self) -> Result<()> {
        if self.difficulty < 1
            || self.difficulty > 3
            || self.max_minutes < 1
            || self.max_minutes > 120
        {
            bail!("invalid challenge filters");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exercise {
    pub id: String,
    pub category: Category,
    pub title: String,
    pub instructions: String,
    pub mode: Mode,
    pub difficulty: u8,
    pub minutes: u16,
    pub measured: Option<Criterion>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub metric: Metric,
    pub target: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bank {
    version: u32,
    source: String,
    license: String,
    exercises: Vec<Exercise>,
}
pub fn catalog() -> &'static [Exercise] {
    static BANK: OnceLock<Vec<Exercise>> = OnceLock::new();
    BANK.get_or_init(|| {
        let bank: Bank = serde_json::from_str(include_str!(
            "../assets/writingdeck/productivity-challenges.json"
        ))
        .expect("compiled original challenge bank");
        assert_eq!(bank.version, 1);
        assert_eq!(bank.license, "MIT");
        assert_eq!(bank.source, "WritingDeck original exercises");
        let mut ids = BTreeSet::new();
        let mut texts = BTreeSet::new();
        for e in &bank.exercises {
            assert!(ids.insert(e.id.clone()) && texts.insert(e.instructions.clone()));
            assert!(
                !e.id.is_empty()
                    && e.id.len() <= 80
                    && e.difficulty >= 1
                    && e.difficulty <= 3
                    && e.minutes >= 1
                    && e.minutes <= 120
            );
            assert!(!e.instructions.is_empty());
        }
        assert!(
            Category::ALL
                .iter()
                .all(|c| bank.exercises.iter().any(|e| e.category == *c))
        );
        bank.exercises
    })
    .as_slice()
}
pub fn exercise(id: &str) -> Option<&'static Exercise> {
    catalog().iter().find(|e| e.id == id)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Assigned,
    Accepted,
    Completed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    Measured,
    SelfReported,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub record_id: String,
    pub source_stamp: u64,
    pub status: Status,
    pub accepted_at: Option<String>,
    pub baseline: Option<i128>,
    pub completed_at: Option<String>,
    pub evidence: Option<Evidence>,
    pub include_footnotes: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reroll {
    pub old_assignment: Assignment,
    pub profile: Profile,
    pub old_record: String,
    pub replacement: String,
    pub at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub at: String,
    pub profile: Profile,
    pub records: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Day {
    pub created_at: String,
    pub selections: Vec<Selection>,
    pub version: u32,
    pub date: String,
    pub zone: Option<String>,
    pub bank_version: u32,
    pub profile: Profile,
    pub seed: u64,
    pub assignments: Vec<Assignment>,
    pub rerolls: Vec<Reroll>,
}
impl Validate for Day {
    fn validate(&self) -> Result<()> {
        crate::journal::date(&self.date)?;
        chrono::DateTime::parse_from_rfc3339(&self.created_at)?;
        self.profile.validate()?;
        if self.version != 1
            || self.bank_version != 1
            || self.assignments.len() > 3
            || self.rerolls.len() > 128
            || self.selections.len() > 128
        {
            bail!("unknown/invalid challenge day; preserved");
        }
        if let Some(z) = &self.zone {
            let _: chrono_tz::Tz = z.parse()?;
        }
        let mut ids = BTreeSet::new();
        for a in &self.assignments {
            validate_assignment(a)?;
            if !ids.insert(&a.record_id)
                || exercise(&a.record_id).is_none_or(|e| stamp(e) != a.source_stamp)
            {
                bail!("unknown/duplicate challenge reference; preserved");
            }
            for at in [&a.accepted_at, &a.completed_at].into_iter().flatten() {
                chrono::DateTime::parse_from_rfc3339(at)?;
            }
            if (a.status == Status::Assigned
                && (a.accepted_at.is_some()
                    || a.baseline.is_some()
                    || a.completed_at.is_some()
                    || a.evidence.is_some()))
                || (a.status == Status::Accepted
                    && (a.accepted_at.is_none()
                        || a.completed_at.is_some()
                        || a.evidence.is_some()))
                || (a.status == Status::Completed
                    && (a.accepted_at.is_none()
                        || a.completed_at.is_none()
                        || a.evidence.is_none()))
            {
                bail!("invalid challenge lifecycle");
            }
            if a.evidence == Some(Evidence::Measured)
                && exercise(&a.record_id)
                    .and_then(|e| e.measured.as_ref())
                    .is_none()
            {
                bail!("self-report cannot become measured");
            }
        }
        for r in &self.rerolls {
            r.profile.validate()?;
            validate_assignment(&r.old_assignment)?;
            if r.old_record != r.old_assignment.record_id
                || r.old_assignment.status == Status::Completed
            {
                bail!("invalid reroll prior state");
            }
            if exercise(&r.old_record).is_none() || exercise(&r.replacement).is_none() {
                bail!("unknown reroll reference");
            }
            chrono::DateTime::parse_from_rfc3339(&r.at)?;
        }
        for selection in &self.selections {
            chrono::DateTime::parse_from_rfc3339(&selection.at)?;
            selection.profile.validate()?;
            if selection.records.len() > 3
                || selection.records.iter().any(|id| exercise(id).is_none())
            {
                bail!("invalid explicit selection history");
            }
        }
        Ok(())
    }
}
fn validate_assignment(a: &Assignment) -> Result<()> {
    let e = exercise(&a.record_id).context("unknown exercise reference")?;
    if stamp(e) != a.source_stamp {
        bail!("exercise source stamp changed; preserved");
    }
    for at in [&a.accepted_at, &a.completed_at].into_iter().flatten() {
        chrono::DateTime::parse_from_rfc3339(at)?;
    }
    if a.status != Status::Assigned && (e.measured.is_some() != a.baseline.is_some()) {
        bail!("measured baseline missing or subjective baseline invented");
    }
    if a.status == Status::Assigned
        && (a.accepted_at.is_some()
            || a.baseline.is_some()
            || a.evidence.is_some()
            || a.completed_at.is_some())
        || a.status == Status::Accepted
            && (a.accepted_at.is_none() || a.evidence.is_some() || a.completed_at.is_some())
        || a.status == Status::Completed
            && (a.accepted_at.is_none() || a.evidence.is_none() || a.completed_at.is_none())
    {
        bail!("invalid assignment lifecycle");
    }
    if a.evidence == Some(Evidence::Measured) && e.measured.is_none()
        || a.evidence == Some(Evidence::SelfReported) && e.measured.is_some()
    {
        bail!("completion evidence kind mismatch");
    }
    Ok(())
}
fn eligible(profile: &Profile) -> Vec<&'static Exercise> {
    catalog()
        .iter()
        .filter(|e| {
            profile.categories.contains(&e.category)
                && profile.mode.is_none_or(|m| e.mode == m)
                && e.difficulty <= profile.difficulty
                && e.minutes <= profile.max_minutes
        })
        .collect()
}
fn next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
fn hash(text: &str) -> u64 {
    crate::sync::Fingerprint::of(text).hash
}
impl Day {
    pub fn assign(
        now: &Moment,
        profile: Profile,
        seed: u64,
        recent: &BTreeSet<String>,
    ) -> Result<Self> {
        now.validate()?;
        profile.validate()?;
        let day = now.day();
        let mut rng = seed ^ hash(&day);
        let mut candidates = eligible(&profile);
        let mut assignments = vec![];
        // A measurable drafting option is offered when the requested profile allows
        // it; the other slots vary across the eligible categories.
        candidates.sort_by_key(|e| recent.contains(&e.id));
        if let Some(at) = candidates.iter().position(|e| {
            e.category == Category::Productivity && e.measured.is_some() && !recent.contains(&e.id)
        }) {
            let e = candidates.remove(at);
            assignments.push(blank(&e.id));
        }
        while assignments.len() < 3 && !candidates.is_empty() {
            let fresh = candidates
                .iter()
                .take_while(|e| !recent.contains(&e.id))
                .count();
            let limit = if fresh > 0 { fresh } else { candidates.len() };
            let at = (next(&mut rng) % limit as u64) as usize;
            let e = candidates.remove(at);
            assignments.push(blank(&e.id));
        }
        let s = Self {
            created_at: now.stamp(),
            selections: vec![],
            version: 1,
            date: day,
            zone: now.zone.clone(),
            bank_version: 1,
            profile,
            seed,
            assignments,
            rerolls: vec![],
        };
        s.validate()?;
        Ok(s)
    }
    pub fn accept(
        &mut self,
        index: usize,
        now: &Moment,
        snapshot: &Snapshot,
        footnotes: bool,
    ) -> Result<()> {
        let a = self
            .assignments
            .get_mut(index)
            .context("choose a challenge")?;
        if a.status != Status::Assigned {
            bail!("already accepted/completed");
        }
        let e = exercise(&a.record_id).context("source exercise unavailable")?;
        a.baseline = if let Some(c) = &e.measured {
            Some(
                snapshot
                    .lifetime(c.metric, Some(e.mode), footnotes)
                    .context("recorded metrics unavailable; no baseline invented")?,
            )
        } else {
            None
        };
        a.status = Status::Accepted;
        a.accepted_at = Some(now.stamp());
        a.include_footnotes = footnotes;
        Ok(())
    }
    pub fn complete(
        &mut self,
        index: usize,
        now: &Moment,
        snapshot: &Snapshot,
        self_report: bool,
    ) -> Result<Evidence> {
        let a = self
            .assignments
            .get_mut(index)
            .context("choose a challenge")?;
        if a.status != Status::Accepted {
            bail!("accept it first; completed challenges do not duplicate");
        }
        let e = exercise(&a.record_id).context("exercise unavailable")?;
        let evidence = if let Some(c) = &e.measured {
            if self_report {
                bail!("measured criterion cannot be silently self-reported");
            }
            let current = snapshot
                .lifetime(c.metric, Some(e.mode), a.include_footnotes)
                .context("recorded metrics unavailable")?;
            let baseline = a.baseline.context("measured baseline unavailable")?;
            if current - baseline < c.target as i128 {
                bail!("measured criterion not yet reached");
            }
            Evidence::Measured
        } else {
            if !self_report {
                bail!(
                    "subjective exercise requires explicit self-report; not automatically judged"
                );
            }
            Evidence::SelfReported
        };
        a.status = Status::Completed;
        a.completed_at = Some(now.stamp());
        a.evidence = Some(evidence);
        Ok(evidence)
    }
    pub fn reroll(&mut self, index: usize, now: &Moment, recent: &BTreeSet<String>) -> Result<()> {
        let profile = self.profile.clone();
        self.reroll_with_profile(index, now, &profile, recent)
    }
    pub fn new_selection(
        &mut self,
        now: &Moment,
        profile: &Profile,
        recent: &BTreeSet<String>,
    ) -> Result<()> {
        if !self.assignments.is_empty() || self.selections.len() >= 128 || now.day() != self.date {
            bail!("only an empty current-date selection can be explicitly assigned again");
        }
        let next = Self::assign(now, profile.clone(), self.seed, recent)?;
        if next.assignments.is_empty() {
            bail!("no exercise matches requested filters");
        }
        self.selections.push(Selection {
            at: now.stamp(),
            profile: profile.clone(),
            records: next
                .assignments
                .iter()
                .map(|a| a.record_id.clone())
                .collect(),
        });
        self.assignments = next.assignments;
        Ok(())
    }
    pub fn reroll_with_profile(
        &mut self,
        index: usize,
        now: &Moment,
        profile: &Profile,
        recent: &BTreeSet<String>,
    ) -> Result<()> {
        profile.validate()?;
        now.validate()?;
        if self.rerolls.len() >= 128 {
            bail!("reroll history limit reached; original retained");
        }
        let a = self.assignments.get(index).context("choose a challenge")?;
        if a.status == Status::Completed {
            bail!("completed assignment retained; select another");
        }
        let mut used: BTreeSet<_> = self
            .assignments
            .iter()
            .map(|a| a.record_id.clone())
            .chain(
                self.rerolls
                    .iter()
                    .flat_map(|r| [r.old_record.clone(), r.replacement.clone()]),
            )
            .chain(recent.iter().cloned())
            .collect();
        let candidates: Vec<_> = eligible(profile)
            .into_iter()
            .filter(|e| !used.contains(&e.id))
            .collect();
        if candidates.is_empty() {
            bail!("eligible pool exhausted; no hidden repeat; widen future filters");
        }
        let mut seed = self.seed ^ hash(&self.date) ^ (self.rerolls.len() as u64 + 1);
        let replacement = candidates[(next(&mut seed) % candidates.len() as u64) as usize]
            .id
            .clone();
        let old_assignment = a.clone();
        let old = a.record_id.clone();
        used.insert(old.clone());
        self.assignments[index] = blank(&replacement);
        self.rerolls.push(Reroll {
            old_assignment,
            profile: profile.clone(),
            old_record: old,
            replacement,
            at: now.stamp(),
        });
        Ok(())
    }
}
fn stamp(exercise: &Exercise) -> u64 {
    crate::sync::Fingerprint::of(
        &serde_json::to_string(exercise).expect("compiled exercise serializes"),
    )
    .hash
}
fn blank(id: &str) -> Assignment {
    Assignment {
        record_id: id.into(),
        source_stamp: stamp(exercise(id).expect("compiled exercise reference")),
        status: Status::Assigned,
        accepted_at: None,
        baseline: None,
        completed_at: None,
        evidence: None,
        include_footnotes: false,
    }
}
/// Read-only audit of recognized civil-date files. Unknown filenames remain
/// uninterpreted; no missing date is generated and no metadata is published.
pub fn validate_existing(root: &Path) -> Result<usize> {
    let probe = crate::statefile::checked(
        root,
        Path::new(".grimoire/writingdeck/challenges/2000-01-01.json"),
    )?;
    let entries = match std::fs::read_dir(probe.parent().unwrap()) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    let mut count = 0;
    for entry in entries {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(date) = name.strip_suffix(".json") else {
            continue;
        };
        if crate::journal::date(date).is_err() {
            continue;
        }
        count += 1;
        if count > 36600 {
            bail!("too many challenge date files to audit");
        }
        let bytes = crate::statefile::read(root, &path(date)?)?
            .context("challenge date disappeared during audit")?;
        let day: Day = serde_json::from_slice(&bytes)?;
        day.validate()?;
        if day.date != date {
            bail!("challenge file/date mismatch; original retained");
        }
    }
    Ok(count)
}
pub fn path(day: &str) -> Result<PathBuf> {
    crate::journal::date(day)?;
    Ok(PathBuf::from(format!(
        ".grimoire/writingdeck/challenges/{day}.json"
    )))
}
pub fn load(
    root: &Path,
    now: &Moment,
    profile: Profile,
    seed: u64,
    recent: &BTreeSet<String>,
) -> Result<Store<Day>> {
    let p = path(&now.day())?;
    let default = match crate::statefile::read(root, &p)? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => Day::assign(now, profile, seed, recent)?,
    };
    let store = Store::load(root, &p, default)?;
    if store.value.date != now.day() {
        bail!("challenge date mismatch; preserved");
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounting::{Entry, Role, Source},
        measure::{Counts, Origin},
    };
    use chrono::{DateTime, TimeZone};
    fn time(utc: &str, zone: &str) -> Moment {
        let tz: chrono_tz::Tz = zone.parse().unwrap();
        Moment {
            mono_ms: 0,
            wall: DateTime::parse_from_rfc3339(utc)
                .unwrap()
                .with_timezone(&tz)
                .fixed_offset(),
            zone: Some(zone.into()),
        }
    }
    fn now() -> Moment {
        time("2026-10-25T12:00:00Z", "UTC")
    }
    fn snap(n: u64, mode: Mode) -> Snapshot {
        let at = now();
        Snapshot {
            complete: true,
            entries: vec![Entry {
                id: "s-test-1".into(),
                date: at.day(),
                zone: at.zone.clone(),
                first_at: at.stamp(),
                last_at: at.stamp(),
                source: Source {
                    path: "manuscript/a.md".into(),
                    role: Role::Manuscript,
                },
                mode,
                origin: Origin::Typing,
                counts: Counts {
                    added: n,
                    operations: n,
                    ..Counts::default()
                },
                activity: vec![],
            }],
            ..Snapshot::default()
        }
    }
    fn profile(category: Category, difficulty: u8) -> Profile {
        Profile {
            categories: [category].into_iter().collect(),
            mode: None,
            difficulty,
            max_minutes: 30,
        }
    }
    fn root(tag: &str) -> PathBuf {
        let r = std::env::temp_dir().join(format!("wd-challenges-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        r
    }
    #[test]
    fn ten_original_categories_have_real_distinct_exercises_and_deterministic_seeds() {
        assert_eq!(catalog().len(), 30);
        for category in Category::ALL {
            let day = Day::assign(&now(), profile(category, 3), 43, &BTreeSet::new()).unwrap();
            assert_eq!(day.assignments.len(), 3);
            assert!(
                day.assignments
                    .iter()
                    .all(|a| exercise(&a.record_id).unwrap().category == category)
            );
        }
        assert_eq!(
            Day::assign(&now(), Profile::default(), 44, &BTreeSet::new()).unwrap(),
            Day::assign(&now(), Profile::default(), 44, &BTreeSet::new()).unwrap()
        );
    }
    #[test]
    fn difficulty_duration_and_mode_filters_are_not_just_labels() {
        let p = Profile {
            mode: Some(Mode::Revision),
            max_minutes: 5,
            difficulty: 1,
            ..Profile::default()
        };
        let d = Day::assign(&now(), p, 1, &BTreeSet::new()).unwrap();
        assert_eq!(d.assignments.len(), 1);
        assert_eq!(
            exercise(&d.assignments[0].record_id).unwrap().mode,
            Mode::Revision
        );
        let mut p = profile(Category::Creative, 1);
        p.mode = Some(Mode::Revision);
        assert!(
            Day::assign(&now(), p, 1, &BTreeSet::new())
                .unwrap()
                .assignments
                .is_empty()
        );
    }
    #[test]
    fn persisted_day_reopen_does_not_reroll_or_rewrite_creation_zone() {
        let r = root("reopen");
        let mut s = load(&r, &now(), Profile::default(), 1, &BTreeSet::new()).unwrap();
        assert!(!s.exists());
        s.update(|_| Ok(())).unwrap();
        let before = std::fs::read(r.join(path(&now().day()).unwrap())).unwrap();
        let travel = time("2026-10-25T13:00:00Z", "Asia/Tokyo");
        let reopened = load(
            &r,
            &travel,
            profile(Category::Revision, 3),
            999,
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(reopened.value, s.value);
        assert_eq!(
            std::fs::read(r.join(path(&now().day()).unwrap())).unwrap(),
            before
        );
        std::fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn dst_23_and_25_hour_dates_use_one_assignment_key_each() {
        for (a, b) in [
            ("2026-03-08T08:01:00Z", "2026-03-09T06:59:00Z"),
            ("2026-11-01T07:01:00Z", "2026-11-02T07:59:00Z"),
        ] {
            let a = time(a, "America/Los_Angeles");
            let b = time(b, "America/Los_Angeles");
            assert_eq!(a.day(), b.day());
            let a = Day::assign(&a, Profile::default(), 7, &BTreeSet::new()).unwrap();
            let b = Day::assign(&b, Profile::default(), 7, &BTreeSet::new()).unwrap();
            assert_eq!(a.assignments, b.assignments);
        }
    }
    #[test]
    fn travel_repeated_dates_retrieve_both_original_files() {
        let r = root("travel");
        let a = time("2026-10-25T01:00:00Z", "Asia/Tokyo");
        let b = time("2026-10-25T02:00:00Z", "America/Los_Angeles");
        assert_ne!(a.day(), b.day());
        let mut a_store = load(&r, &a, Profile::default(), 5, &BTreeSet::new()).unwrap();
        a_store.update(|_| Ok(())).unwrap();
        let mut b_store = load(&r, &b, Profile::default(), 6, &BTreeSet::new()).unwrap();
        b_store.update(|_| Ok(())).unwrap();
        assert_eq!(
            load(&r, &a, Profile::default(), 100, &BTreeSet::new())
                .unwrap()
                .value,
            a_store.value
        );
        assert!(r.join(path(&b.day()).unwrap()).is_file());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn measured_challenge_excludes_preacceptance_and_wrong_mode_work() {
        let mut d = Day::assign(
            &now(),
            profile(Category::Productivity, 1),
            7,
            &BTreeSet::new(),
        )
        .unwrap();
        d.accept(0, &now(), &snap(100, Mode::Drafting), false)
            .unwrap();
        assert!(
            d.complete(0, &now(), &snap(149, Mode::Drafting), false)
                .is_err()
        );
        assert!(
            d.complete(0, &now(), &snap(1000, Mode::Revision), false)
                .is_err()
        );
        assert!(
            d.complete(0, &now(), &snap(150, Mode::Drafting), true)
                .is_err()
        );
        assert_eq!(
            d.complete(0, &now(), &snap(150, Mode::Drafting), false)
                .unwrap(),
            Evidence::Measured
        );
        assert!(
            d.complete(0, &now(), &snap(500, Mode::Drafting), false)
                .is_err()
        );
        d.validate().unwrap();
    }
    #[test]
    fn subjective_exercise_needs_explicit_self_report_and_never_becomes_measured() {
        let mut d =
            Day::assign(&now(), profile(Category::Creative, 1), 1, &BTreeSet::new()).unwrap();
        d.accept(0, &now(), &Snapshot::default(), false).unwrap();
        assert!(
            d.complete(0, &now(), &snap(10000, Mode::Drafting), false)
                .is_err()
        );
        assert_eq!(
            d.complete(0, &now(), &Snapshot::default(), true).unwrap(),
            Evidence::SelfReported
        );
        d.validate().unwrap();
    }
    #[test]
    fn measured_baselines_are_not_invented_when_ledger_is_incomplete() {
        let mut d = Day::assign(
            &now(),
            profile(Category::Productivity, 1),
            2,
            &BTreeSet::new(),
        )
        .unwrap();
        assert!(d.accept(0, &now(), &Snapshot::default(), false).is_err());
        assert_eq!(d.assignments[0].status, Status::Assigned);
        d.validate().unwrap();
    }
    #[test]
    fn reroll_retains_accepted_state_and_uses_explicit_updated_filters() {
        let mut d = Day::assign(
            &now(),
            profile(Category::Productivity, 1),
            8,
            &BTreeSet::new(),
        )
        .unwrap();
        d.accept(0, &now(), &snap(100, Mode::Drafting), false)
            .unwrap();
        assert!(d.reroll(0, &now(), &BTreeSet::new()).is_err());
        let old = d.assignments[0].clone();
        d.reroll_with_profile(0, &now(), &profile(Category::Creative, 3), &BTreeSet::new())
            .unwrap();
        assert_eq!(d.rerolls[0].old_assignment, old);
        assert_eq!(
            d.rerolls[0].profile.categories,
            [Category::Creative].into_iter().collect()
        );
        assert_eq!(d.assignments[0].status, Status::Assigned);
        assert_ne!(d.assignments[0].record_id, old.record_id);
        d.validate().unwrap();
    }
    #[test]
    fn reroll_refuses_recent_or_current_repeats_when_pool_exhausted() {
        let mut d =
            Day::assign(&now(), profile(Category::Creative, 1), 8, &BTreeSet::new()).unwrap();
        let before = d.clone();
        assert!(d.reroll(0, &now(), &BTreeSet::new()).is_err());
        assert_eq!(d, before);
        let recent: BTreeSet<_> = catalog().iter().map(|e| e.id.clone()).collect();
        assert!(
            d.reroll_with_profile(0, &now(), &Profile::default(), &recent)
                .is_err()
        );
        assert_eq!(d, before);
    }
    #[test]
    fn empty_selection_can_be_explicitly_filled_without_erasing_original_filters() {
        let mut p = profile(Category::Creative, 1);
        p.mode = Some(Mode::Revision);
        let mut d = Day::assign(&now(), p.clone(), 1, &BTreeSet::new()).unwrap();
        assert!(d.assignments.is_empty());
        d.new_selection(&now(), &Profile::default(), &BTreeSet::new())
            .unwrap();
        assert_eq!(d.profile, p);
        assert_eq!(d.selections.len(), 1);
        assert_eq!(d.assignments.len(), 3);
        d.validate().unwrap();
    }
    #[test]
    fn unknown_schema_unknown_nested_field_and_changed_source_stamp_are_preserved_errors() {
        let d = Day::assign(&now(), Profile::default(), 1, &BTreeSet::new()).unwrap();
        let mut v = serde_json::to_value(&d).unwrap();
        v["assignments"][0]["future"] = true.into();
        assert!(serde_json::from_value::<Day>(v).is_err());
        let mut d = d;
        d.assignments[0].source_stamp ^= 1;
        assert!(d.validate().is_err());
        d.version = 999;
        assert!(d.validate().is_err());
    }
    #[test]
    fn no_utc_or_elapsed_24_hour_assumption_in_date_selection() {
        let a = time("2026-10-25T01:00:00Z", "America/Los_Angeles");
        assert_eq!(a.day(), "2026-10-24");
        assert_eq!(
            path(&a.day()).unwrap(),
            PathBuf::from(".grimoire/writingdeck/challenges/2026-10-24.json")
        );
        assert!(
            chrono_tz::America::Los_Angeles
                .with_ymd_and_hms(2026, 11, 1, 1, 30, 0)
                .earliest()
                .is_some()
        );
    }
}
