//! Offline, non-AI Story Spark selection. Corpus prose is immutable content;
//! user metadata stores references/receipts, never manuscript text. Rendering
//! does no I/O. Guarded stores are not file locks or cross-file transactions.
use crate::{
    accounting::{Mode as WorkMode, Moment},
    statefile::{Store, Validate},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const HISTORY_LIMIT: usize = 128;
pub const FAVORITE_LIMIT: usize = 64;
pub const DISMISSED_LIMIT: usize = 128;
pub const RECENT_LIMIT: usize = 12;
pub const EXPORT_LIMIT: usize = 256;
pub const GENRES: [&str; 9] = [
    "realist",
    "speculative",
    "mystery",
    "adventure",
    "romance",
    "family",
    "satire",
    "political",
    "historical",
];
pub const FEATURES: [&str; 13] = [
    "single-scene",
    "multi-scene",
    "continuous-time",
    "time-jumps",
    "dialogue-only",
    "narration-only",
    "no-spoken-dialogue",
    "visual-detail",
    "nonvisual-only",
    "first-person",
    "third-person",
    "nonlinear-order",
    "linear-order",
];
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Plot,
    Exercise,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub id: String,
    pub edition: String,
    pub section: String,
    pub file: String,
    pub line: usize,
    pub file_sha256: String,
    pub license: String,
    pub attribution: String,
    pub adaptation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    pub category: String,
    pub genres: BTreeSet<String>,
    pub difficulty: u8,
    pub minutes: u16,
    pub work_mode: Option<WorkMode>,
    pub requires: BTreeSet<String>,
    pub excludes: BTreeSet<String>,
    pub sha256: String,
    pub source: Provenance,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    schema_version: u32,
    pub version: String,
    pub license: String,
    pub attribution: String,
    pub plot_count: usize,
    pub exercise_count: usize,
    pub records: Vec<Record>,
    #[serde(skip)]
    index: BTreeMap<String, usize>,
}
fn opposing(left: &BTreeSet<String>, right: &BTreeSet<String>) -> bool {
    [
        ("single-scene", "multi-scene"),
        ("continuous-time", "time-jumps"),
        ("dialogue-only", "narration-only"),
        ("dialogue-only", "no-spoken-dialogue"),
        ("visual-detail", "nonvisual-only"),
        ("first-person", "third-person"),
        ("nonlinear-order", "linear-order"),
    ]
    .iter()
    .any(|(a, b)| {
        (left.contains(*a) && right.contains(*b)) || (left.contains(*b) && right.contains(*a))
    })
}
pub fn compatible(plot: &Record, exercise: &Record) -> bool {
    plot.kind == Kind::Plot
        && exercise.kind == Kind::Exercise
        && exercise.work_mode == Some(WorkMode::Drafting)
        && plot.requires.is_disjoint(&exercise.excludes)
        && exercise.requires.is_disjoint(&plot.excludes)
        && !opposing(&plot.requires, &exercise.requires)
}
impl Corpus {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 4_194_304 {
            bail!("Spark corpus exceeds 4 MiB; not loaded");
        }
        let mut bank: Self =
            serde_json::from_str(text).context("invalid Spark corpus; content not rewritten")?;
        if bank.schema_version != 1
            || !token(&bank.version, 80)
            || bank.license != "MIT"
            || bank.records.is_empty()
            || bank.records.len() > 8192
            || bank.attribution.is_empty()
        {
            bail!("unsupported/invalid Spark corpus");
        }
        let mut texts = BTreeSet::new();
        for (i, r) in bank.records.iter().enumerate() {
            if !token(&r.id, 80)
                || bank.index.insert(r.id.clone(), i).is_some()
                || !texts.insert(r.text.to_lowercase())
                || r.text.is_empty()
                || r.text.len() > 2000
                || r.text.chars().any(char::is_control)
                || r.title.is_empty()
                || r.title.len() > 200
                || !token(&r.category, 80)
                || !(1..=3).contains(&r.difficulty)
                || !(1..=120).contains(&r.minutes)
                || r.genres.iter().any(|g| !GENRES.contains(&g.as_str()))
                || r.requires
                    .iter()
                    .chain(&r.excludes)
                    .any(|f| !FEATURES.contains(&f.as_str()))
                || !r.requires.is_disjoint(&r.excludes)
                || opposing(&r.requires, &r.requires)
                || r.sha256 != sha256(r.text.as_bytes())
                || r.source.license != "MIT"
                || r.source.id != "writingdeck-original-m6"
                || r.source.edition != bank.version
                || r.source.section != r.id
                || r.source.attribution.is_empty()
                || r.source.adaptation.is_empty()
                || r.source.line == 0
                || !digest_valid(&r.source.file_sha256)
                || (r.kind == Kind::Plot) != r.work_mode.is_none()
            {
                bail!("invalid/duplicate/unlicensed Spark record {}", r.id);
            }
            crate::workspaces::relative(Path::new(&r.source.file))?;
        }
        if bank.records.iter().filter(|r| r.kind == Kind::Plot).count() != bank.plot_count
            || bank
                .records
                .iter()
                .filter(|r| r.kind == Kind::Exercise)
                .count()
                != bank.exercise_count
        {
            bail!("Spark corpus count receipt mismatch");
        }
        Ok(bank)
    }
    pub fn get(&self, id: &str) -> Option<&Record> {
        self.index.get(id).map(|&i| &self.records[i])
    }
    pub fn categories(&self, kind: Kind) -> Vec<String> {
        self.records
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn resolve<'a>(
        &'a self,
        result: &ResultRef,
    ) -> Result<(Option<&'a Record>, Option<&'a Record>)> {
        if result.corpus_version != self.version {
            bail!(
                "Historical corpus {} unavailable; reference preserved, not regenerated",
                result.corpus_version
            );
        }
        let one = |part: &Option<Component>, kind| -> Result<Option<&Record>> {
            part.as_ref()
                .map(|p| {
                    self.get(&p.id)
                        .filter(|r| r.kind == kind && r.sha256 == p.sha256)
                        .context("Historical component unavailable/changed; reference preserved")
                })
                .transpose()
        };
        let plot = one(&result.plot, Kind::Plot)?;
        let exercise = one(&result.exercise, Kind::Exercise)?;
        if let (Some(p), Some(e)) = (plot, exercise)
            && !compatible(p, e)
        {
            bail!("Historical pairing incompatible; references preserved, not silently rerolled");
        }
        Ok((plot, exercise))
    }
}
pub fn catalog() -> Result<&'static Corpus> {
    static CORPUS: OnceLock<std::result::Result<Corpus, String>> = OnceLock::new();
    match CORPUS.get_or_init(|| {
        Corpus::parse(include_str!("../assets/writingdeck/spark-corpus.json"))
            .map_err(|e| format!("{e:#}"))
    }) {
        Ok(c) => Ok(c),
        Err(e) => bail!("Spark corpus unavailable: {e}"),
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub plot_categories: BTreeSet<String>,
    pub exercise_categories: BTreeSet<String>,
    pub genres: BTreeSet<String>,
    pub max_difficulty: u8,
    pub max_minutes: u16,
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            plot_categories: BTreeSet::new(),
            exercise_categories: BTreeSet::new(),
            genres: BTreeSet::new(),
            max_difficulty: 3,
            max_minutes: 30,
        }
    }
}
impl Validate for Filter {
    fn validate(&self) -> Result<()> {
        if !(1..=3).contains(&self.max_difficulty)
            || !(1..=120).contains(&self.max_minutes)
            || self.plot_categories.len() > 100
            || self.exercise_categories.len() > 100
            || self
                .plot_categories
                .iter()
                .chain(&self.exercise_categories)
                .any(|s| !token(s, 80))
            || self.genres.iter().any(|g| !GENRES.contains(&g.as_str()))
        {
            bail!("invalid Spark filters; preserved");
        }
        Ok(())
    }
}
impl Filter {
    fn matches(&self, r: &Record) -> bool {
        let cats = if r.kind == Kind::Plot {
            &self.plot_categories
        } else {
            &self.exercise_categories
        };
        r.difficulty <= self.max_difficulty
            && r.minutes <= self.max_minutes
            && (cats.is_empty() || cats.contains(&r.category))
            && (r.kind == Kind::Exercise
                || self.genres.is_empty()
                || !self.genres.is_disjoint(&r.genres))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Plot,
    Exercise,
    Combined,
    Daily,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reroll {
    Both,
    Plot,
    Exercise,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub sha256: String,
}
impl From<&Record> for Component {
    fn from(r: &Record) -> Self {
        Self {
            id: r.id.clone(),
            sha256: r.sha256.clone(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairKey {
    pub corpus_version: String,
    pub plot: Option<String>,
    pub exercise: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRef {
    pub id: String,
    pub mode: Mode,
    pub corpus_version: String,
    pub plot: Option<Component>,
    pub exercise: Option<Component>,
    pub seed: u64,
    pub counter_start: u64,
    pub counter_end: u64,
    pub generated_at: String,
    pub date: String,
    pub zone: Option<String>,
    pub filter: Filter,
    pub pool_sha256: String,
    pub relaxed_recent: bool,
    pub derived_from: Option<String>,
}
impl ResultRef {
    pub fn key(&self) -> PairKey {
        PairKey {
            corpus_version: self.corpus_version.clone(),
            plot: self.plot.as_ref().map(|p| p.id.clone()),
            exercise: self.exercise.as_ref().map(|p| p.id.clone()),
        }
    }
}
impl Validate for PairKey {
    fn validate(&self) -> Result<()> {
        if !token(&self.corpus_version, 80)
            || (self.plot.is_none() && self.exercise.is_none())
            || self
                .plot
                .iter()
                .chain(&self.exercise)
                .any(|p| !token(p, 80))
        {
            bail!("invalid Spark component key");
        }
        Ok(())
    }
}
impl Validate for ResultRef {
    fn validate(&self) -> Result<()> {
        self.key().validate()?;
        self.filter.validate()?;
        let time = chrono::DateTime::parse_from_rfc3339(&self.generated_at)?;
        Moment {
            mono_ms: 0,
            wall: time,
            zone: self.zone.clone(),
        }
        .validate()?;
        crate::journal::date(&self.date)?;
        if !token(&self.id, 100)
            || self.date != time.format("%Y-%m-%d").to_string()
            || self.counter_start > self.counter_end
            || self.counter_end - self.counter_start > 4
            || !digest_valid(&self.pool_sha256)
            || self.derived_from.as_ref().is_some_and(|id| !token(id, 100))
            || self
                .plot
                .iter()
                .chain(&self.exercise)
                .any(|p| !token(&p.id, 80) || !digest_valid(&p.sha256))
            || match self.mode {
                Mode::Plot => self.plot.is_none() || self.exercise.is_some(),
                Mode::Exercise => self.plot.is_some() || self.exercise.is_none(),
                Mode::Combined | Mode::Daily => self.plot.is_none() || self.exercise.is_none(),
            }
            || (self.mode == Mode::Daily
                && (self.zone.is_none() || self.id != format!("daily-{}", self.date)))
        {
            bail!("invalid/future Spark result reference; preserved");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportKind {
    Save,
    Outline,
    Task,
    Scene,
    Scratch,
}
impl ExportKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Outline => "outline",
            Self::Task => "task",
            Self::Scene => "scene",
            Self::Scratch => "scratch",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Prepared,
    Ready,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportReceipt {
    pub result: ResultRef,
    pub kind: ExportKind,
    pub target: PathBuf,
    pub scratch_id: Option<u64>,
    pub sha256: String,
    pub phase: Phase,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub seed: u64,
    pub counter: u64,
    pub next_id: u64,
    pub filter: Filter,
    pub current: Option<ResultRef>,
    pub history: Vec<ResultRef>,
    pub favorites: BTreeMap<String, ResultRef>,
    pub dismissed: Vec<PairKey>,
    pub recent: Vec<PairKey>,
    pub exports: Vec<ExportReceipt>,
    pub roulette: bool,
}
impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            version: 1,
            seed,
            counter: 0,
            next_id: 1,
            filter: Filter::default(),
            current: None,
            history: vec![],
            favorites: BTreeMap::new(),
            dismissed: vec![],
            recent: vec![],
            exports: vec![],
            roulette: false,
        }
    }
    fn remember(&mut self, result: ResultRef) {
        if !self.history.iter().any(|r| r.id == result.id) {
            if self.history.len() >= HISTORY_LIMIT {
                self.history.remove(0);
            }
            self.history.push(result.clone());
            self.recent.push(result.key());
            if self.recent.len() > RECENT_LIMIT {
                self.recent.remove(0);
            }
        }
        self.current = Some(result);
    }
}
impl Validate for State {
    fn validate(&self) -> Result<()> {
        self.filter.validate()?;
        if self.version != 1
            || self.next_id == 0
            || self.next_id == u64::MAX
            || self.history.len() > HISTORY_LIMIT
            || self.favorites.len() > FAVORITE_LIMIT
            || self.dismissed.len() > DISMISSED_LIMIT
            || self.recent.len() > RECENT_LIMIT
            || self.exports.len() > EXPORT_LIMIT
        {
            bail!("unsupported/invalid Spark state; original preserved");
        }
        let mut ids = BTreeSet::new();
        for r in &self.history {
            r.validate()?;
            if !ids.insert(&r.id) {
                bail!("duplicate Spark history ID");
            }
        }
        for (id, r) in &self.favorites {
            r.validate()?;
            if id != &r.id {
                bail!("favorite reference mismatch");
            }
        }
        if let Some(r) = &self.current {
            r.validate()?;
        }
        for k in self.dismissed.iter().chain(&self.recent) {
            k.validate()?;
        }
        let mut exports = BTreeSet::new();
        for r in &self.exports {
            r.result.validate()?;
            if !digest_valid(&r.sha256)
                || r.target != export_path(&r.result.id, r.kind, r.scratch_id)?
                || !exports.insert((r.result.id.clone(), r.kind.label()))
            {
                bail!("invalid/duplicate Spark export receipt");
            }
        }
        Ok(())
    }
}
fn draw(seed: u64, counter: &mut u64, size: usize) -> Result<usize> {
    if size == 0 {
        bail!("No eligible Spark options; filters were not relaxed");
    }
    *counter = counter
        .checked_add(1)
        .context("Spark PRNG counter exhausted")?;
    let mut z = seed.wrapping_add(counter.wrapping_mul(0x9e3779b97f4a7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    Ok(((z ^ (z >> 31)) % (size as u64)) as usize)
}
fn pool_stamp(state: &State) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&(
        &state.filter,
        &state.recent,
        &state.dismissed,
    ))?))
}
fn select(
    bank: &Corpus,
    state: &State,
    mode: Mode,
    keep: Option<(&ResultRef, Reroll)>,
    now: &Moment,
    (seed, start): (u64, u64),
    id: String,
) -> Result<ResultRef> {
    now.validate()?;
    let previous = keep.map(|(r, _)| r);
    if let Some(r) = previous {
        bank.resolve(r)?;
    }
    let fixed_plot = keep.and_then(|(r, which)| {
        if which == Reroll::Exercise {
            r.plot.as_ref()
        } else {
            None
        }
    });
    let fixed_exercise = keep.and_then(|(r, which)| {
        if which == Reroll::Plot {
            r.exercise.as_ref()
        } else {
            None
        }
    });
    if keep.is_some_and(|(r, w)| {
        (w == Reroll::Plot && r.plot.is_none()) || (w == Reroll::Exercise && r.exercise.is_none())
    }) {
        bail!("That component is absent; choose another mode explicitly");
    }
    let change_plot = keep.is_some_and(|(_, w)| w != Reroll::Exercise);
    let change_exercise = keep.is_some_and(|(_, w)| w != Reroll::Plot);
    let recent_plot: BTreeSet<&str> = state
        .recent
        .iter()
        .filter(|k| k.corpus_version == bank.version)
        .filter_map(|k| k.plot.as_deref())
        .collect();
    let recent_exercise: BTreeSet<&str> = state
        .recent
        .iter()
        .filter(|k| k.corpus_version == bank.version)
        .filter_map(|k| k.exercise.as_deref())
        .collect();
    let dismissed = |p: Option<&Record>, e: Option<&Record>| {
        state.dismissed.iter().any(|k| {
            k.corpus_version == bank.version
                && k.plot.as_deref() == p.map(|r| r.id.as_str())
                && k.exercise.as_deref() == e.map(|r| r.id.as_str())
        })
    };
    for relaxed in [false, true] {
        let eligible = |r: &&Record| {
            let is_plot = r.kind == Kind::Plot;
            let fixed = if is_plot { fixed_plot } else { fixed_exercise };
            if let Some(f) = fixed {
                return r.id == f.id && r.sha256 == f.sha256;
            }
            if !state.filter.matches(r) {
                return false;
            }
            let changed = if is_plot {
                change_plot
            } else {
                change_exercise
            };
            let prior = previous.and_then(|p| {
                if is_plot {
                    p.plot.as_ref()
                } else {
                    p.exercise.as_ref()
                }
            });
            if changed && prior.is_some_and(|p| p.id == r.id) {
                return false;
            }
            relaxed
                || !if is_plot {
                    &recent_plot
                } else {
                    &recent_exercise
                }
                .contains(r.id.as_str())
        };
        let plots: Vec<_> = bank
            .records
            .iter()
            .filter(|r| r.kind == Kind::Plot)
            .filter(eligible)
            .collect();
        let exercises: Vec<_> = bank
            .records
            .iter()
            .filter(|r| r.kind == Kind::Exercise)
            .filter(eligible)
            .collect();
        let mut counter = start;
        let selected = match mode {
            Mode::Plot => {
                let p: Vec<_> = plots
                    .iter()
                    .copied()
                    .filter(|p| !dismissed(Some(p), None))
                    .collect();
                if p.is_empty() {
                    None
                } else {
                    Some((Some(p[draw(seed, &mut counter, p.len())?]), None))
                }
            }
            Mode::Exercise => {
                let e: Vec<_> = exercises
                    .iter()
                    .copied()
                    .filter(|e| !dismissed(None, Some(e)))
                    .collect();
                if e.is_empty() {
                    None
                } else {
                    Some((None, Some(e[draw(seed, &mut counter, e.len())?])))
                }
            }
            Mode::Combined | Mode::Daily => {
                let p: Vec<_> = plots
                    .iter()
                    .copied()
                    .filter(|p| {
                        exercises
                            .iter()
                            .any(|e| compatible(p, e) && !dismissed(Some(p), Some(e)))
                    })
                    .collect();
                if p.is_empty() {
                    None
                } else {
                    let p = p[draw(seed, &mut counter, p.len())?];
                    let e: Vec<_> = exercises
                        .iter()
                        .copied()
                        .filter(|e| compatible(p, e) && !dismissed(Some(p), Some(e)))
                        .collect();
                    Some((Some(p), Some(e[draw(seed, &mut counter, e.len())?])))
                }
            }
        };
        if let Some((plot, exercise)) = selected {
            let result = ResultRef {
                id,
                mode,
                corpus_version: bank.version.clone(),
                plot: plot.map(Component::from),
                exercise: exercise.map(Component::from),
                seed,
                counter_start: start,
                counter_end: counter,
                generated_at: now.stamp(),
                date: now.day(),
                zone: now.zone.clone(),
                filter: state.filter.clone(),
                pool_sha256: pool_stamp(state)?,
                relaxed_recent: relaxed,
                derived_from: previous.map(|p| p.id.clone()),
            };
            result.validate()?;
            return Ok(result);
        }
    }
    bail!(
        "No compatible new Spark result matches the filters/kept component; no filter or dismissal was silently relaxed"
    )
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Daily {
    pub version: u32,
    pub result: ResultRef,
}
impl Validate for Daily {
    fn validate(&self) -> Result<()> {
        self.result.validate()?;
        if self.version != 1 || self.result.mode != Mode::Daily {
            bail!("invalid/future Spark daily; preserved");
        }
        Ok(())
    }
}
pub fn daily_path(date: &str) -> Result<PathBuf> {
    crate::journal::date(date)?;
    Ok(PathBuf::from(format!(
        ".grimoire/writingdeck/spark/daily/{date}.json"
    )))
}
pub struct Engine<'a> {
    pub store: Store<State>,
    pub bank: &'a Corpus,
    root: PathBuf,
}
impl<'a> Engine<'a> {
    pub fn load(root: &Path, seed: u64, bank: &'a Corpus) -> Result<Self> {
        Ok(Self {
            store: Store::load(
                root,
                Path::new(".grimoire/writingdeck/spark/state.json"),
                State::new(seed),
            )?,
            bank,
            root: fs::canonicalize(root)?,
        })
    }
    pub fn state(&self) -> &State {
        &self.store.value
    }
    pub fn set_filter(&mut self, filter: Filter) -> Result<()> {
        filter.validate()?;
        if filter.plot_categories.iter().any(|c| {
            !self
                .bank
                .records
                .iter()
                .any(|r| r.kind == Kind::Plot && &r.category == c)
        }) || filter.exercise_categories.iter().any(|c| {
            !self
                .bank
                .records
                .iter()
                .any(|r| r.kind == Kind::Exercise && &r.category == c)
        }) {
            bail!("Unknown Spark category; no state changed");
        }
        self.store.update(|s| {
            s.filter = filter;
            Ok(())
        })
    }
    pub fn generate(&mut self, mode: Mode, now: &Moment) -> Result<ResultRef> {
        if mode == Mode::Daily {
            return self.daily(now);
        }
        let s = self.state();
        let id = format!("r-{:016x}", s.next_id);
        let result = select(self.bank, s, mode, None, now, (s.seed, s.counter), id)?;
        self.publish(result)
    }
    fn publish(&mut self, result: ResultRef) -> Result<ResultRef> {
        self.store.update(|s| {
            s.counter = result.counter_end;
            s.next_id = s.next_id.checked_add(1).context("Spark IDs exhausted")?;
            s.remember(result.clone());
            Ok(())
        })?;
        Ok(result)
    }
    pub fn reroll(&mut self, which: Reroll, now: &Moment) -> Result<ResultRef> {
        let s = self.state();
        let current = s
            .current
            .as_ref()
            .context("Generate or select a result first")?;
        let mode = if current.mode == Mode::Daily {
            Mode::Combined
        } else {
            current.mode
        };
        let result = select(
            self.bank,
            s,
            mode,
            Some((current, which)),
            now,
            (s.seed, s.counter),
            format!("r-{:016x}", s.next_id),
        )?;
        self.publish(result)
    }
    pub fn daily(&mut self, now: &Moment) -> Result<ResultRef> {
        now.validate()?;
        if now.zone.is_none() {
            bail!("Daily Spark needs a verified IANA civil timezone; no UTC date was invented");
        }
        let path = daily_path(&now.day())?;
        if let Some(bytes) = crate::statefile::read(&self.root, &path)? {
            let day: Daily = serde_json::from_slice(&bytes)?;
            day.validate()?;
            if day.result.date != now.day() {
                bail!("Spark daily filename/date mismatch; preserved");
            }
            let result = day.result;
            self.store.update(|s| {
                s.remember(result.clone());
                Ok(())
            })?;
            return Ok(result);
        }
        self.store.verify()?;
        let s = self.state();
        let hash = sha256(format!("{}:{}:{}", s.seed, now.day(), self.bank.version).as_bytes());
        let seed = u64::from_str_radix(&hash[..16], 16)?;
        let result = select(
            self.bank,
            s,
            Mode::Daily,
            None,
            now,
            (seed, 0),
            format!("daily-{}", now.day()),
        )?;
        let day = Daily {
            version: 1,
            result: result.clone(),
        };
        let mut store = Store::load(&self.root, &path, day.clone())?;
        if store.exists() {
            bail!("Daily appeared externally; reload explicitly, original preserved");
        }
        store.update(|d| {
            *d = day;
            Ok(())
        })?;
        self.store
            .update(|s| {
                s.remember(result.clone());
                Ok(())
            })
            .context(
                "Daily saved, history not published; retry Daily resumes the same saved result",
            )?;
        Ok(result)
    }
    pub fn show(&mut self, id: &str) -> Result<()> {
        let result = self
            .state()
            .history
            .iter()
            .find(|r| r.id == id)
            .or_else(|| self.state().favorites.get(id))
            .cloned()
            .context("Result not in retained history/favorites")?;
        self.store.update(|s| {
            s.current = Some(result);
            Ok(())
        })
    }
    pub fn favorite(&mut self) -> Result<bool> {
        self.store.update(|s| {
            let r = s.current.as_ref().context("Select a result first")?;
            if s.favorites.remove(&r.id).is_some() {
                return Ok(false);
            }
            if s.favorites.len() >= FAVORITE_LIMIT {
                bail!("64 favorites retained; remove one explicitly before adding another");
            }
            s.favorites.insert(r.id.clone(), r.clone());
            Ok(true)
        })
    }
    pub fn dismiss(&mut self) -> Result<()> {
        self.store.update(|s| {
            let key = s.current.as_ref().context("Select a result first")?.key();
            if !s.dismissed.contains(&key) {
                if s.dismissed.len() >= DISMISSED_LIMIT {
                    bail!(
                        "128 dismissals retained; clear dismissals explicitly before adding more"
                    );
                }
                s.dismissed.push(key);
            }
            Ok(())
        })
    }
    pub fn clear_dismissals(&mut self) -> Result<()> {
        self.store.update(|s| {
            s.dismissed.clear();
            Ok(())
        })
    }
    pub fn export(&mut self, kind: ExportKind, scratch_id: Option<u64>) -> Result<PathBuf> {
        // Retrying an already prepared/ready receipt must verify the store too;
        // otherwise a foreign state could be ignored before creating a source.
        self.store.verify()?;
        let result = self
            .state()
            .current
            .clone()
            .context("Select a result first")?;
        let prior = self
            .state()
            .exports
            .iter()
            .find(|r| r.result.id == result.id && r.kind == kind)
            .cloned();
        let receipt = if let Some(r) = prior {
            r
        } else {
            if self.state().exports.len() >= EXPORT_LIMIT {
                bail!(
                    "256 export receipts retained; capacity exhausted, no original rewritten. Receipt pruning is not automated"
                );
            }
            let target = export_path(&result.id, kind, scratch_id)?;
            let text = markdown(self.bank, &result, kind)?;
            let receipt = ExportReceipt {
                result: result.clone(),
                kind,
                target,
                scratch_id,
                sha256: sha256(text.as_bytes()),
                phase: Phase::Prepared,
            };
            self.store.update(|s| {
                s.exports.push(receipt.clone());
                Ok(())
            })?;
            receipt
        };
        let role = if kind == ExportKind::Scene {
            "manuscript"
        } else {
            "notes"
        };
        let full = crate::workflow::source_path(&self.root, &receipt.target, role)?;
        match fs::symlink_metadata(&full) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    bail!("Spark target alias/special path refused; preserved");
                }
                let mut bytes = Vec::new();
                fs::File::open(&full)?
                    .take(16_385)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > 16_384 {
                    bail!("Saved Spark source exceeds 16 KiB; preserved");
                }
                std::str::from_utf8(&bytes)
                    .context("Saved Spark original is not UTF-8; preserved")?;
                if receipt.phase == Phase::Prepared && sha256(&bytes) != receipt.sha256 {
                    bail!("Prepared Spark target differs; no overwrite or duplicate created");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if receipt.phase == Phase::Ready {
                    bail!("Saved Spark original missing; not recreated from metadata");
                }
                let text = markdown(self.bank, &result, kind)?;
                if sha256(text.as_bytes()) != receipt.sha256 {
                    bail!("Prepared corpus content changed; preserved");
                }
                fs::create_dir_all(full.parent().unwrap())?;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&full)
                    .context("Spark target exists; not overwritten")?;
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
            }
            Err(e) => return Err(e.into()),
        }
        if receipt.phase == Phase::Prepared {
            self.store.update(|s|{
                s.exports.iter_mut().find(|r|r.result.id==result.id&&r.kind==kind).context("Spark export receipt disappeared")?.phase=Phase::Ready;Ok(())
            }).context("Spark source created; receipt publication failed. Retry resumes the same target without re-copying")?;
        }
        Ok(receipt.target)
    }
}
pub fn export_path(id: &str, kind: ExportKind, scratch_id: Option<u64>) -> Result<PathBuf> {
    if !token(id, 100) {
        bail!("invalid Spark export ID");
    }
    if kind == ExportKind::Scratch {
        let n = scratch_id
            .filter(|n| *n > 0 && *n < u64::MAX)
            .context("scratchpad ID required")?;
        return Ok(crate::workflow::scratch_path(n));
    }
    if scratch_id.is_some() {
        bail!("unexpected scratchpad ID");
    }
    Ok(match kind {
        ExportKind::Scene => PathBuf::from(format!("manuscript/98-Spark-{id}-experiment.md")),
        ExportKind::Save => PathBuf::from(format!("notes/sparks/{id}.md")),
        ExportKind::Task => PathBuf::from(format!("notes/sparks/task-{id}.md")),
        ExportKind::Outline => PathBuf::from(format!("notes/sparks/outline-{id}.md")),
        ExportKind::Scratch => unreachable!(),
    })
}
pub fn markdown(bank: &Corpus, r: &ResultRef, kind: ExportKind) -> Result<String> {
    let (plot, exercise) = bank.resolve(r)?;
    if kind == ExportKind::Scene {
        return Ok(format!(
            "---\ntitle: Spark experiment {}\ncompile: false\nspark-result: {}\nspark-corpus: {}\nspark-reference: notes/sparks/{}.md\nspark-plot: {}\nspark-exercise: {}\n---\n\n",
            r.id,
            r.id,
            r.corpus_version,
            r.id,
            r.plot.as_ref().map_or("none", |p| p.id.as_str()),
            r.exercise.as_ref().map_or("none", |p| p.id.as_str())
        ));
    }
    let title = match kind {
        ExportKind::Scene => "Spark experiment",
        ExportKind::Task => "Spark task",
        ExportKind::Outline => "Spark outline note",
        ExportKind::Scratch => "Spark scratchpad",
        ExportKind::Save => "Saved Story Spark",
    };
    let mut text = format!(
        "---\ntitle: {title} {}\ncompile: false\nspark-result: {}\nspark-corpus: {}\n---\n\n",
        r.id, r.id, r.corpus_version
    );
    if kind == ExportKind::Task {
        text.push_str("- [ ] Explore this optional Spark; completion is self-reported.\n\n");
    }
    if kind == ExportKind::Outline {
        text.push_str("# Outline experiment\n\n- Situation:\n- Consequential choice:\n- Possible next scene:\n\n");
    }
    text.push_str(&format!(
        "Story Spark {} · {} · {}\n\n",
        r.id,
        r.date,
        r.zone.as_deref().unwrap_or("IANA zone unverified")
    ));
    for record in [plot, exercise].into_iter().flatten() {
        text.push_str(&format!(
            "## {} · {}\n\n{}\n\n",
            if record.kind == Kind::Plot {
                "Plot"
            } else {
                "Exercise"
            },
            record.id,
            record.text
        ));
        if !record.requires.is_empty() {
            text.push_str(&format!(
                "Narrative conditions: {}.\n\n",
                record
                    .requires
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        text.push_str(&format!(
            "Source: {} / {} / {}; text SHA-256 {}; license {}; {}\n\n",
            record.source.id,
            record.source.edition,
            record.source.section,
            record.sha256,
            record.source.license,
            record.source.attribution
        ));
    }
    Ok(text)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentStatus {
    Assigned,
    Accepted,
    Completed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub version: u32,
    pub date: String,
    pub assigned_at: String,
    pub zone: String,
    pub result: ResultRef,
    pub status: AssignmentStatus,
    pub accepted_at: Option<String>,
    pub completed_at: Option<String>,
}
impl Validate for Assignment {
    fn validate(&self) -> Result<()> {
        self.result.validate()?;
        crate::journal::date(&self.date)?;
        let wall = chrono::DateTime::parse_from_rfc3339(&self.assigned_at)?;
        Moment {
            mono_ms: 0,
            wall,
            zone: Some(self.zone.clone()),
        }
        .validate()?;
        if self.version != 1
            || wall.format("%Y-%m-%d").to_string() != self.date
            || (self.status != AssignmentStatus::Assigned) != self.accepted_at.is_some()
            || (self.status == AssignmentStatus::Completed) != self.completed_at.is_some()
        {
            bail!("invalid optional Spark challenge; preserved");
        }
        for at in [&self.accepted_at, &self.completed_at]
            .into_iter()
            .flatten()
        {
            chrono::DateTime::parse_from_rfc3339(at)?;
        }
        Ok(())
    }
}
pub fn assignment_path(date: &str) -> Result<PathBuf> {
    crate::journal::date(date)?;
    Ok(PathBuf::from(format!(
        ".grimoire/writingdeck/spark/assignments/{date}.json"
    )))
}
pub fn assignment(root: &Path, date: &str) -> Result<Option<Store<Assignment>>> {
    let path = assignment_path(date)?;
    let Some(bytes) = crate::statefile::read(root, &path)? else {
        return Ok(None);
    };
    let value: Assignment = serde_json::from_slice(&bytes)?;
    value.validate()?;
    if value.date != date {
        bail!("Spark assignment date mismatch; preserved");
    }
    Ok(Some(Store::load(root, &path, value)?))
}
pub fn assign(root: &Path, r: &ResultRef, now: &Moment) -> Result<()> {
    now.validate()?;
    r.validate()?;
    let zone = now
        .zone
        .clone()
        .context("Optional date assignment needs a verified IANA timezone")?;
    let path = assignment_path(&now.day())?;
    if let Some(existing) = assignment(root, &now.day())? {
        if existing.value.result == *r {
            return Ok(());
        }
        bail!("Today's optional Spark already assigned; original retained, not silently replaced");
    }
    let value = Assignment {
        version: 1,
        date: now.day(),
        assigned_at: now.stamp(),
        zone,
        result: r.clone(),
        status: AssignmentStatus::Assigned,
        accepted_at: None,
        completed_at: None,
    };
    let mut store = Store::load(root, &path, value.clone())?;
    if store.exists() {
        bail!("Optional assignment appeared externally; reload explicitly");
    }
    store.update(|v| {
        *v = value;
        Ok(())
    })
}
pub fn accept_or_complete(root: &Path, date: &str, now: &Moment, complete: bool) -> Result<()> {
    now.validate()?;
    if now.day() != date {
        bail!("Accept/report on today's civil date only; older assignment preserved");
    }
    let mut store = assignment(root, date)?.context("No optional Spark assigned today")?;
    store.update(|v| {
        if complete {
            if v.status != AssignmentStatus::Accepted {
                bail!("Accept the optional Spark before self-reporting completion");
            }
            v.status = AssignmentStatus::Completed;
            v.completed_at = Some(now.stamp());
        } else {
            if v.status != AssignmentStatus::Assigned {
                bail!("Optional Spark already accepted/completed");
            }
            v.status = AssignmentStatus::Accepted;
            v.accepted_at = Some(now.stamp());
        }
        Ok(())
    })
}
/// Read-only validation: unknown versions, references and originals are never
/// replaced. Older corpus references may be unavailable but remain valid history.
pub fn validate_existing(root: &Path) -> Result<(usize, usize)> {
    let _: Engine<'_> = Engine::load(root, 0, catalog()?)?;
    let mut counts = [0, 0];
    for (i, dir) in ["daily", "assignments"].into_iter().enumerate() {
        let rel = PathBuf::from(format!(".grimoire/writingdeck/spark/{dir}"));
        let base = crate::statefile::checked(root, &rel.join("probe.json"))?
            .parent()
            .unwrap()
            .to_path_buf();
        if !base.exists() {
            continue;
        }
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(date) = name.strip_suffix(".json") else {
                continue;
            };
            if crate::journal::date(date).is_err() {
                continue;
            }
            if i == 0 {
                let bytes = crate::statefile::read(root, &daily_path(date)?)?
                    .context("Daily disappeared during validation")?;
                let day: Daily = serde_json::from_slice(&bytes)?;
                day.validate()?;
                if day.result.date != date {
                    bail!("Daily date mismatch");
                }
            } else {
                assignment(root, date)?.context("Assignment disappeared during validation")?;
            }
            counts[i] += 1;
            if counts[i] > 4096 {
                bail!(
                    "Spark date-file validation bound (4096 per kind); archive/review explicitly"
                );
            }
        }
    }
    Ok((counts[0], counts[1]))
}

#[cfg(test)]
#[path = "spark_tests.rs"]
mod tests;
