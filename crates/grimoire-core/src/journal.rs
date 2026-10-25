//! Saved manuscript observations, never a gross-edit or active-time ledger.
//! Daily facts are separate from ordinary, user-authored reflection Markdown.
use crate::sync::{Fingerprint, SaveOutcome};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, SecondsFormat};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_FACTS: usize = 1024;
const MAX_BYTES: usize = 1_048_576;
const MAX_WORDS: u64 = 1_000_000_000;

pub fn date(text: &str) -> Result<NaiveDate> {
    let d =
        NaiveDate::parse_from_str(text, "%Y-%m-%d").context("journal date must be YYYY-MM-DD")?;
    if !(1..=9999).contains(&d.year()) || d.format("%Y-%m-%d").to_string() != text {
        bail!("journal date must be a canonical YYYY-MM-DD");
    }
    Ok(d)
}
fn instant(text: &str) -> Result<DateTime<FixedOffset>> {
    if text.len() > 64 {
        bail!("journal timestamp too long");
    }
    let at = DateTime::parse_from_rfc3339(text).context("invalid journal RFC3339 timestamp")?;
    date(&at.format("%Y-%m-%d").to_string())?;
    Ok(at)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    SavedCheckpoint,
    /// A successful save boundary before a requested quit/book switch, not
    /// evidence that the process exited or a measurement of active duration.
    ExitRequested,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SavedCheckpoint => "Saved checkpoint",
            Self::ExitRequested => "Exit/book-change requested",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Local offset retained; the UTC instant remains derivable/auditable.
    pub at: String,
    pub app_opened_at: String,
    pub initial_manuscript_words: Option<u64>,
    pub manuscript_words: u64,
    /// Existing daily tracker, not gross additions, exclusive typing or time.
    /// None when the tracker's date doesn't match this observation's date.
    pub daily_net_words: Option<i64>,
}
impl Observation {
    pub fn new(
        at: DateTime<FixedOffset>,
        opened: DateTime<FixedOffset>,
        initial: Option<u64>,
        total: u64,
        daily: Option<i64>,
    ) -> Result<Self> {
        let o = Self {
            at: at.to_rfc3339_opts(SecondsFormat::Nanos, false),
            app_opened_at: opened.to_rfc3339_opts(SecondsFormat::Nanos, false),
            initial_manuscript_words: initial,
            manuscript_words: total,
            daily_net_words: daily,
        };
        o.validate()?;
        Ok(o)
    }
    pub fn day(&self) -> Result<String> {
        Ok(instant(&self.at)?.format("%Y-%m-%d").to_string())
    }
    pub fn delta_since_open(&self) -> Option<i64> {
        self.initial_manuscript_words
            .map(|n| self.manuscript_words as i64 - n as i64)
    }
    fn validate(&self) -> Result<()> {
        instant(&self.at)?;
        instant(&self.app_opened_at)?;
        if self.initial_manuscript_words.is_some_and(|n| n > MAX_WORDS)
            || self.manuscript_words > MAX_WORDS
            || self
                .daily_net_words
                .is_some_and(|n| n.unsigned_abs() > MAX_WORDS)
        {
            bail!("journal word count out of range");
        }
        // Clock adjustment may put an observation before app-open wall time.
        // Preserve both instants; never manufacture elapsed/active time.
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub id: u64,
    pub kind: Kind,
    pub observation: Observation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Day {
    pub version: u32,
    pub date: String,
    pub next_id: u64,
    pub facts: Vec<Fact>,
}
impl Day {
    pub fn empty(day: &str) -> Result<Self> {
        date(day)?;
        Ok(Self {
            version: 1,
            date: day.into(),
            next_id: 1,
            facts: Vec::new(),
        })
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "unsupported journal version {}; original preserved",
                self.version
            );
        }
        date(&self.date)?;
        if self.facts.len() > MAX_FACTS {
            bail!("too many journal facts; original preserved");
        }
        let mut previous = 0;
        for f in &self.facts {
            f.observation.validate()?;
            if f.id <= previous || f.observation.day()? != self.date {
                bail!("invalid journal fact order/date; original preserved");
            }
            previous = f.id;
        }
        if self.next_id <= previous || self.next_id == u64::MAX {
            bail!("invalid journal next id; original preserved");
        }
        Ok(())
    }
}
/// Check all existing ancestors before creating anything. This is containment
/// and optimistic conflict detection, not a hostile-filesystem race defense.
fn safe_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let path = root.join(relative);
    for ancestor in path.ancestors().take_while(|p| *p != root) {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                if !fs::canonicalize(ancestor)?.starts_with(&root) {
                    bail!("journal path resolves outside project; preserved");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
pub struct Store {
    day: Day,
    root: PathBuf,
    seen: Option<Fingerprint>,
}
impl Store {
    pub fn path(root: &Path, day: &str) -> Result<PathBuf> {
        date(day)?;
        Ok(root
            .join(".grimoire/writingdeck/journal")
            .join(format!("{day}.json")))
    }
    fn checked_path(root: &Path, day: &str) -> Result<PathBuf> {
        let relative = Self::path(Path::new(""), day)?;
        safe_path(root, &relative)
    }
    /// Read-only, including when no journal directory or date file exists.
    pub fn load(root: &Path, day: &str) -> Result<Self> {
        let root = fs::canonicalize(root)?;
        let path = Self::checked_path(&root, day)?;
        let (record, seen) = match fs::File::open(path) {
            Ok(file) => {
                let mut text = String::new();
                file.take((MAX_BYTES + 1) as u64)
                    .read_to_string(&mut text)?;
                if text.len() > MAX_BYTES {
                    bail!("journal exceeds 1 MiB; original preserved");
                }
                let record: Day = serde_json::from_str(&text)
                    .context("invalid/unknown journal JSON; original preserved")?;
                record.validate()?;
                if record.date != day {
                    bail!("journal date does not match filename; original preserved");
                }
                (record, Some(Fingerprint::of(&text)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Day::empty(day)?, None),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            day: record,
            root,
            seen,
        })
    }
    pub fn day(&self) -> &Day {
        &self.day
    }
    /// Append only. No API edits/removes a recorded fact. An exact retry of a
    /// persisted observation is idempotent after an explicit conflict reload.
    pub fn append(&mut self, kind: Kind, observation: Observation) -> Result<u64> {
        if let Some(f) = self
            .day
            .facts
            .iter()
            .find(|f| f.kind == kind && f.observation == observation)
        {
            return Ok(f.id);
        }
        let mut next = self.day.clone();
        let id = next.next_id;
        next.next_id = next
            .next_id
            .checked_add(1)
            .context("journal id exhausted")?;
        next.facts.push(Fact {
            id,
            kind,
            observation,
        });
        next.validate()?;
        let text = serde_json::to_string_pretty(&next)? + "\n";
        if text.len() > MAX_BYTES {
            bail!("journal exceeds size limit; original preserved");
        }
        let path = Self::checked_path(&self.root, &next.date)?;
        fs::create_dir_all(path.parent().unwrap())?;
        match crate::sync::save_guarded(&path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                self.day = next;
                Ok(id)
            }
            SaveOutcome::Conflict { .. } => bail!(
                "journal changed externally; original not overwritten; explicitly reload/retry"
            ),
        }
    }
}
/// Audit known date files only; unknown filenames are retained, not interpreted.
/// No directories/files are created and no future/unknown field is normalized.
pub fn validate_existing(root: &Path) -> Result<usize> {
    let dir = safe_path(root, Path::new(".grimoire/writingdeck/journal"))?;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    let mut n = 0;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(day) = name.strip_suffix(".json") else {
            continue;
        };
        if date(day).is_err() {
            continue;
        }
        n += 1;
        if n > 36_600 {
            bail!("too many journal date files to audit");
        }
        Store::load(root, day)?;
    }
    Ok(n)
}
pub fn reflection_path(root: &Path, day: &str) -> Result<PathBuf> {
    date(day)?;
    Ok(root.join("notes/journal").join(format!("{day}.md")))
}
pub fn reflection_date(relative: &Path) -> Option<String> {
    let tail = relative.strip_prefix("notes/journal").ok()?;
    if tail.components().count() != 1 {
        return None;
    }
    let day = tail.file_name()?.to_str()?.strip_suffix(".md")?;
    date(day).ok()?;
    Some(day.into())
}
/// Only explicit user source-edit requests call this. Existing source bytes,
/// frontmatter, aliases and manual prose are never rewritten by automation.
pub fn ensure_reflection(root: &Path, day: &str) -> Result<PathBuf> {
    let relative = reflection_path(Path::new(""), day)?;
    let path = safe_path(root, &relative)?;
    let canonical_root = fs::canonicalize(root)?;
    let notes = canonical_root.join("notes");
    // A symlink from notes into manuscript/research must not turn creating a
    // reflection into creating compileable manuscript text in another role.
    for ancestor in [notes.clone(), notes.join("journal"), path.clone()] {
        match fs::canonicalize(&ancestor) {
            Ok(actual) if !actual.starts_with(&notes) => bail!(
                "reflection must remain inside the project's notes directory; source preserved"
            ),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            crate::pins::identify(root, &relative)?;
            return Ok(path);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::create_dir_all(path.parent().unwrap())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .context("reflection already exists or cannot be created; not overwritten")?;
    file.write_all(format!("---\ntitle: Reflection {day}\n---\n\n").as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wd-journal-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn observation(at: &str) -> Observation {
        Observation::new(
            instant(at).unwrap(),
            instant("2026-10-24T08:00:00-07:00").unwrap(),
            Some(100),
            93,
            Some(-7),
        )
        .unwrap()
    }
    #[test]
    fn journal_read_is_side_effect_free_and_facts_round_trip_without_prose() {
        let r = root("roundtrip");
        let mut s = Store::load(&r, "2026-10-24").unwrap();
        assert_eq!(fs::read_dir(&r).unwrap().count(), 0);
        let o = observation("2026-10-24T09:00:00-07:00");
        assert_eq!(s.append(Kind::SavedCheckpoint, o.clone()).unwrap(), 1);
        let bytes = fs::read(Store::path(&r, "2026-10-24").unwrap()).unwrap();
        assert_eq!(s.append(Kind::SavedCheckpoint, o).unwrap(), 1);
        assert_eq!(
            fs::read(Store::path(&r, "2026-10-24").unwrap()).unwrap(),
            bytes
        );
        assert_eq!(Store::load(&r, "2026-10-24").unwrap().day(), s.day());
        assert_eq!(s.day.facts[0].observation.delta_since_open(), Some(-7));
        assert!(!String::from_utf8(bytes).unwrap().contains("reflection"));
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_counts_are_net_not_gross_or_active_time_and_keep_clock_offsets() {
        let r = root("clock");
        let mut s = Store::load(&r, "2026-10-24").unwrap();
        let mut o = observation("2026-10-24T00:15:00+09:00");
        o.daily_net_words = None;
        s.append(Kind::SavedCheckpoint, o.clone()).unwrap();
        assert!(s.day.facts[0].observation.at.ends_with("+09:00"));
        assert!(s.day.facts[0].observation.daily_net_words.is_none());
        let text = serde_json::to_string(s.day()).unwrap();
        for unsupported in ["active_seconds", "gross_words", "deleted_words", "duration"] {
            assert!(!text.contains(unsupported));
        }
        assert!(instant(&o.at).unwrap() < instant(&o.app_opened_at).unwrap());
        o.initial_manuscript_words = None;
        assert!(o.delta_since_open().is_none());
        s.append(Kind::ExitRequested, o).unwrap();
        assert!(
            Store::load(&r, "2026-10-24")
                .unwrap()
                .day()
                .facts
                .last()
                .unwrap()
                .observation
                .initial_manuscript_words
                .is_none()
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_days_rotate_by_observed_local_date_not_utc_or_app_open_date() {
        let r = root("days");
        let mut first = Store::load(&r, "2026-10-24").unwrap();
        let o = observation("2026-10-25T00:01:00-07:00");
        assert!(first.append(Kind::SavedCheckpoint, o.clone()).is_err());
        assert!(first.day.facts.is_empty());
        let mut next = Store::load(&r, "2026-10-25").unwrap();
        next.append(Kind::ExitRequested, o).unwrap();
        assert_eq!(validate_existing(&r).unwrap(), 1);
        assert!(!Store::path(&r, "2026-10-24").unwrap().exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_future_unknown_nested_corrupt_and_mismatched_records_are_preserved() {
        let r = root("invalid");
        let path = Store::path(&r, "2026-10-24").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut d = Day::empty("2026-10-24").unwrap();
        d.facts.push(Fact {
            id: 1,
            kind: Kind::SavedCheckpoint,
            observation: observation("2026-10-24T10:00:00-07:00"),
        });
        d.next_id = 2;
        let base = serde_json::to_value(d).unwrap();
        let mut cases = vec!["{ broken".to_string()];
        for (where_, key, value) in [
            ("day", "version", serde_json::json!(999)),
            ("day", "future", serde_json::json!(true)),
            ("day", "date", serde_json::json!("2026-10-23")),
            ("fact", "future", serde_json::json!(true)),
            ("observation", "future", serde_json::json!(true)),
        ] {
            let mut v = base.clone();
            let target = match where_ {
                "day" => &mut v,
                "fact" => &mut v["facts"][0],
                _ => &mut v["facts"][0]["observation"],
            };
            target[key] = value;
            cases.push(v.to_string());
        }
        for text in cases {
            fs::write(&path, &text).unwrap();
            assert!(Store::load(&r, "2026-10-24").is_err());
            assert!(validate_existing(&r).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_conflict_and_io_failure_never_advance_memory_or_overwrite() {
        let r = root("conflict");
        let mut a = Store::load(&r, "2026-10-24").unwrap();
        let mut b = Store::load(&r, "2026-10-24").unwrap();
        a.append(
            Kind::SavedCheckpoint,
            observation("2026-10-24T10:00:00-07:00"),
        )
        .unwrap();
        let path = Store::path(&r, "2026-10-24").unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(
            b.append(
                Kind::SavedCheckpoint,
                observation("2026-10-24T11:00:00-07:00")
            )
            .is_err()
        );
        assert!(b.day.facts.is_empty());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let before = a.day.clone();
        assert!(
            a.append(
                Kind::ExitRequested,
                observation("2026-10-24T12:00:00-07:00")
            )
            .is_err()
        );
        assert_eq!(a.day, before);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_limits_dates_and_invalid_fact_order_are_rejected() {
        for d in [
            "../2026-10-24",
            "2026-02-30",
            "2026-1-1",
            "0000-01-01",
            "+10000-01-01",
        ] {
            assert!(date(d).is_err());
        }
        let mut d = Day::empty("2026-10-24").unwrap();
        let f = Fact {
            id: 1,
            kind: Kind::SavedCheckpoint,
            observation: observation("2026-10-24T10:00:00-07:00"),
        };
        d.facts = vec![f.clone(), f];
        d.next_id = 3;
        assert!(d.validate().is_err());
        d.facts.truncate(1);
        d.next_id = 1;
        assert!(d.validate().is_err());
        d.next_id = 2;
        d.facts[0].observation.manuscript_words = MAX_WORDS + 1;
        assert!(d.validate().is_err());
        let r = root("oversize");
        let p = Store::path(&r, "2026-10-24").unwrap();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, " ".repeat(MAX_BYTES + 1)).unwrap();
        assert!(Store::load(&r, "2026-10-24").is_err());
        assert_eq!(fs::metadata(&p).unwrap().len(), (MAX_BYTES + 1) as u64);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn journal_fact_limit_preserves_existing_snapshot() {
        let r = root("limit");
        let mut s = Store::load(&r, "2026-10-24").unwrap();
        let o = observation("2026-10-24T10:00:00-07:00");
        s.day.facts = (1..=MAX_FACTS as u64)
            .map(|id| Fact {
                id,
                kind: Kind::SavedCheckpoint,
                observation: o.clone(),
            })
            .collect();
        s.day.next_id = MAX_FACTS as u64 + 1;
        let before = s.day.clone();
        assert!(s.append(Kind::ExitRequested, o).is_err());
        assert_eq!(s.day, before);
        assert!(!Store::path(&r, "2026-10-24").unwrap().exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn reflection_creation_is_explicit_original_markdown_and_never_replaces_bytes() {
        let r = root("reflection");
        assert!(!reflection_path(&r, "2026-10-24").unwrap().exists());
        let p = ensure_reflection(&r, "2026-10-24").unwrap();
        let source =
            "---\ntitle: My reflection\nextra: 'raw: value' # keep\n---\n\nI found a new ending.\n";
        fs::write(&p, source).unwrap();
        assert_eq!(ensure_reflection(&r, "2026-10-24").unwrap(), p);
        assert_eq!(fs::read_to_string(p).unwrap(), source);
        assert_eq!(
            reflection_date(Path::new("notes/journal/2026-10-24.md")).as_deref(),
            Some("2026-10-24")
        );
        assert!(reflection_date(Path::new("notes/journal/nested/2026-10-24.md")).is_none());
        assert_eq!(validate_existing(&r).unwrap(), 0);
        fs::remove_dir_all(r).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn journal_and_reflection_early_and_late_symlink_escapes_are_refused() {
        use std::os::unix::fs::symlink;
        for late in [false, true] {
            let r = root(if late { "late" } else { "early" });
            let outside = root(if late { "out-late" } else { "out-early" });
            let mut s = if late {
                Some(Store::load(&r, "2026-10-24").unwrap())
            } else {
                None
            };
            symlink(&outside, r.join(".grimoire")).unwrap();
            assert!(Store::load(&r, "2026-10-24").is_err());
            if let Some(s) = &mut s {
                assert!(
                    s.append(
                        Kind::SavedCheckpoint,
                        observation("2026-10-24T10:00:00-07:00")
                    )
                    .is_err()
                );
                assert!(s.day.facts.is_empty());
            }
            symlink(&outside, r.join("notes")).unwrap();
            assert!(ensure_reflection(&r, "2026-10-24").is_err());
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
            fs::remove_dir_all(r).unwrap();
            fs::remove_dir_all(outside).unwrap();
        }
    }
}

#[cfg(all(test, unix))]
#[test]
fn reflection_creation_refuses_an_internal_notes_to_manuscript_symlink() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("wd-journal-role-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("manuscript")).unwrap();
    symlink(root.join("manuscript"), root.join("notes")).unwrap();
    assert!(ensure_reflection(&root, "2026-10-24").is_err());
    assert_eq!(fs::read_dir(root.join("manuscript")).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}
