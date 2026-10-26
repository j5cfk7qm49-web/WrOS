//! Real editor-operation ledger and injected monotonic/civil clocks.
//! Source text is never serialized. Inter-edit activity is explicitly bounded;
//! neither a wall-clock baseline nor Git can become gross/typing-time evidence.
use crate::{
    measure::{Counts, Mutation, Origin},
    statefile::{self, Store, Validate},
};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub mono_ms: u64,
    pub wall: DateTime<FixedOffset>,
    pub zone: Option<String>,
}
impl Moment {
    pub fn day(&self) -> String {
        self.wall.format("%Y-%m-%d").to_string()
    }
    pub fn stamp(&self) -> String {
        self.wall.to_rfc3339()
    }
    pub fn validate(&self) -> Result<()> {
        crate::journal::date(&self.day())?;
        if let Some(z) = &self.zone {
            let zone: Tz = z.parse().context("unknown IANA timezone")?;
            if self.wall.with_timezone(&zone).offset().fix() != *self.wall.offset() {
                bail!("timezone/offset mismatch");
            }
        }
        Ok(())
    }
}
use chrono::Offset;
pub trait Clock: Send {
    fn now(&self) -> Moment;
}
pub struct SystemClock {
    base: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            base: Instant::now(),
        }
    }
}
fn system_zone() -> Option<Tz> {
    std::env::var("TZ")
        .ok()
        .and_then(|s| s.trim_start_matches(':').parse().ok())
        .or_else(|| {
            fs::read_link("/etc/localtime").ok().and_then(|p| {
                p.to_string_lossy()
                    .split("zoneinfo/")
                    .nth(1)
                    .and_then(|s| s.parse().ok())
            })
        })
        .or_else(|| {
            fs::read_to_string("/etc/timezone")
                .ok()
                .and_then(|s| s.trim().parse().ok())
        })
}
impl Clock for SystemClock {
    fn now(&self) -> Moment {
        let wall = Local::now().fixed_offset();
        let zone = system_zone()
            .filter(|z| wall.with_timezone(z).offset().fix() == *wall.offset())
            .map(|z| z.name().into());
        Moment {
            mono_ms: self.base.elapsed().as_millis().min(u64::MAX as u128) as u64,
            wall,
            zone,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Manuscript,
    Notes,
    Research,
    Other,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Drafting,
    Revision,
    ReadThrough,
}
impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Drafting => Self::Revision,
            Self::Revision => Self::ReadThrough,
            Self::ReadThrough => Self::Drafting,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: PathBuf,
    pub role: Role,
}
impl Validate for Source {
    fn validate(&self) -> Result<()> {
        crate::pins::source_relative(&self.path)?;
        if self.role == Role::Manuscript && !self.path.starts_with("manuscript") {
            bail!("manuscript ledger source outside manuscript");
        }
        if self.role == Role::Research && !self.path.starts_with("research") {
            bail!("research ledger source outside research");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub date: Option<String>,
    pub zone: Option<String>,
    pub span_start: String,
    pub span_end: String,
    pub milliseconds: u64,
    pub typed_pair: bool,
}
#[derive(Default)]
pub struct ActiveClock {
    last: Option<(Moment, Source, Mode, Origin)>,
}
impl ActiveClock {
    pub fn pause(&mut self) {
        self.last = None;
    }
    /// Credit only an observed pair of edits, never startup, idle tail, a long
    /// gap, another document/mode, or restored/imported text. UTC/zone changes
    /// retain monotonic duration as unassigned, not invented daily time.
    pub fn edit(
        &mut self,
        now: Moment,
        source: &Source,
        mode: Mode,
        origin: Origin,
        idle_ms: u64,
    ) -> Vec<Activity> {
        let previous = self
            .last
            .replace((now.clone(), source.clone(), mode, origin));
        let Some((old, old_source, old_mode, old_origin)) = previous else {
            return vec![];
        };
        let Some(elapsed) = now.mono_ms.checked_sub(old.mono_ms) else {
            return vec![];
        };
        if elapsed == 0
            || elapsed > idle_ms
            || old_source != *source
            || old_mode != mode
            || matches!(origin, Origin::Restore | Origin::Programmatic)
            || matches!(old_origin, Origin::Restore | Origin::Programmatic)
        {
            return vec![];
        }
        let typed = origin == Origin::Typing && old_origin == Origin::Typing;
        let activity = |date: Option<String>, ms| Activity {
            span_start: date.clone().unwrap_or_else(|| old.day().min(now.day())),
            span_end: date.clone().unwrap_or_else(|| old.day().max(now.day())),
            date,
            zone: now.zone.clone(),
            milliseconds: ms,
            typed_pair: typed,
        };
        let utc_ms = now.wall.timestamp_millis() - old.wall.timestamp_millis();
        if old.validate().is_err()
            || now.validate().is_err()
            || old.zone != now.zone
            || utc_ms <= 0
            || (utc_ms as i128 - elapsed as i128).abs() > 20
        {
            return vec![activity(None, elapsed)];
        }
        if old.day() == now.day() && old.wall.offset() == now.wall.offset() {
            return vec![activity(Some(now.day()), elapsed)];
        }
        let Some(zone) = now.zone.as_ref().and_then(|z| z.parse::<Tz>().ok()) else {
            return vec![activity(None, elapsed)];
        };
        let first = old.wall.with_timezone(&zone).date_naive();
        let last = now.wall.with_timezone(&zone).date_naive();
        if first == last {
            return vec![activity(Some(first.to_string()), elapsed)];
        }
        // Binary search the actual IANA civil-date boundary, including DST. No
        // assumption that a day lasts 24 hours or offset remains fixed overnight.
        let mut low = 0i64;
        let mut high = utc_ms;
        while low < high {
            let mid = low + (high - low) / 2;
            let date = Utc
                .timestamp_millis_opt(old.wall.timestamp_millis() + mid)
                .single()
                .unwrap()
                .with_timezone(&zone)
                .date_naive();
            if date == first {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let split = ((low as u128 * elapsed as u128) / utc_ms as u128) as u64;
        vec![
            activity(Some(first.to_string()), split),
            activity(Some(last.to_string()), elapsed - split),
        ]
        .into_iter()
        .filter(|a| a.milliseconds > 0)
        .collect()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub date: String,
    pub zone: Option<String>,
    pub first_at: String,
    pub last_at: String,
    pub source: Source,
    pub mode: Mode,
    pub origin: Origin,
    pub counts: Counts,
    pub activity: Vec<Activity>,
}
impl Validate for Entry {
    fn validate(&self) -> Result<()> {
        crate::journal::date(&self.date)?;
        self.source.validate()?;
        if !identifier(&self.id) || self.activity.len() > 16 {
            bail!("invalid ledger identity/activity bound");
        }
        let first = DateTime::parse_from_rfc3339(&self.first_at)?;
        let last = DateTime::parse_from_rfc3339(&self.last_at)?;
        if last.format("%Y-%m-%d").to_string() != self.date
            || first.format("%Y-%m-%d").to_string() != self.date
        {
            bail!("ledger date differs from observed local date");
        }
        if let Some(z) = &self.zone {
            let _: Tz = z.parse().context("unknown IANA timezone")?;
        }
        for a in &self.activity {
            crate::journal::date(&a.span_start)?;
            crate::journal::date(&a.span_end)?;
            if a.span_start > a.span_end
                || a.date
                    .as_ref()
                    .is_some_and(|d| d != &a.span_start || d != &a.span_end)
            {
                bail!("invalid activity civil span");
            }
            if let Some(d) = &a.date {
                crate::journal::date(d)?;
            }
            if let Some(z) = &a.zone {
                let _: Tz = z.parse().context("unknown activity IANA timezone")?;
            }
            if a.milliseconds > 31_622_400_000 {
                bail!("activity exceeds yearly bound");
            }
        }
        if [
            self.counts.added,
            self.counts.removed,
            self.counts.foot_added,
            self.counts.foot_removed,
            self.counts.to_footnote,
            self.counts.from_footnote,
            self.counts.to_excluded,
            self.counts.from_excluded,
            self.counts.foot_to_excluded,
            self.counts.excluded_to_foot,
            self.counts.gaps,
        ]
        .into_iter()
        .any(|n| n > 1_000_000_000_000)
            || self.counts.operations > 1_000_000_000
            || self.counts.inserted_chars > 1_000_000_000_000
            || self.counts.deleted_chars > 1_000_000_000_000
        {
            bail!("ledger counters out of range");
        }
        Ok(())
    }
}
fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 120
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    version: u32,
    session: String,
    part: u32,
    entries: Vec<Entry>,
}
impl Validate for Part {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || !identifier(&self.session)
            || self.part == 0
            || self.entries.len() > 128
        {
            bail!("unknown/invalid ledger part; preserved");
        }
        let mut ids = BTreeSet::new();
        for e in &self.entries {
            e.validate()?;
            if !ids.insert(&e.id) || !e.id.starts_with(&format!("{}-", self.session)) {
                bail!("conflicting ledger record ID");
            }
        }
        Ok(())
    }
}
struct Pending {
    entry: Entry,
    sealed: bool,
}
pub struct Ledger {
    root: PathBuf,
    pub session: String,
    next: u64,
    pub known: Vec<Entry>,
    pending: Vec<Pending>,
    writers: BTreeMap<String, (u32, Store<Part>)>,
    pub warning: String,
    pub complete: bool,
}
impl Ledger {
    pub fn load(root: &Path) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let session = format!(
            "s{:x}-{:x}-{:x}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let mut s = Self {
            root: fs::canonicalize(root).unwrap_or_else(|_| root.into()),
            session,
            next: 1,
            known: vec![],
            pending: vec![],
            writers: BTreeMap::new(),
            warning: String::new(),
            complete: false,
        };
        s.reload();
        s
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn pending_entries(&self) -> impl Iterator<Item = &Entry> {
        self.pending.iter().map(|p| &p.entry)
    }
    pub fn observe(
        &mut self,
        now: &Moment,
        source: Source,
        mode: Mode,
        mutation: Mutation,
        activity: Vec<Activity>,
    ) -> Result<()> {
        if mutation.origin == Origin::Programmatic {
            return Ok(());
        }
        now.validate()?;
        source.validate()?;
        if !mutation.counts.input() {
            return Ok(());
        }
        if let Some(p) = self.pending.last_mut().filter(|p| {
            !p.sealed
                && p.entry.date == now.day()
                && p.entry.zone == now.zone
                && p.entry.source == source
                && p.entry.mode == mode
                && p.entry.origin == mutation.origin
        }) {
            p.entry.counts.plus(mutation.counts);
            p.entry.last_at = now.stamp();
            merge_activity(&mut p.entry.activity, activity);
            return Ok(());
        }
        if self.pending.len() >= 4096 {
            bail!(
                "4096 pending operation groups; ledger not recorded; save/retry before continuing metrics"
            );
        }
        let id = format!("{}-{:x}", self.session, self.next);
        self.next = self.next.checked_add(1).context("ledger IDs exhausted")?;
        self.pending.push(Pending {
            entry: Entry {
                id,
                date: now.day(),
                zone: now.zone.clone(),
                first_at: now.stamp(),
                last_at: now.stamp(),
                source,
                mode,
                origin: mutation.origin,
                counts: mutation.counts,
                activity,
            },
            sealed: false,
        });
        Ok(())
    }
    pub fn flush(&mut self) -> Result<usize> {
        for pending in &mut self.pending {
            pending.sealed = true;
        }
        let mut written = 0;
        while let Some(entry) = self.pending.first().map(|p| p.entry.clone()) {
            if let Some(old) = self.known.iter().find(|e| e.id == entry.id) {
                if old != &entry {
                    bail!("conflicting existing operation group; all versions retained");
                }
                self.pending.remove(0);
                written += 1;
                continue;
            }
            let day = entry.date.clone();
            if !self.writers.contains_key(&day) {
                let part = 1;
                let store = self.open_part(&day, part)?;
                self.writers.insert(day.clone(), (part, store));
            }
            if self.writers[&day].1.value.entries.len() >= 128 {
                let next = self.writers[&day]
                    .0
                    .checked_add(1)
                    .context("ledger parts exhausted")?;
                let store = self.open_part(&day, next)?;
                self.writers.insert(day.clone(), (next, store));
                continue;
            }
            // Publish a bounded same-date prefix once, not a full metadata
            // rewrite/fsync for every group. Only dequeue after success. Size
            // rollover also handles long references/activity without requiring
            // exactly 128 records in every part.
            let store = &self.writers[&day].1;
            let room = 128 - store.value.entries.len();
            let mut count = self
                .pending
                .iter()
                .take(room)
                .take_while(|p| {
                    p.entry.date == day && !self.known.iter().any(|e| e.id == p.entry.id)
                })
                .count();
            let planned = loop {
                let mut next = store.value.clone();
                for p in self.pending.iter().take(count) {
                    if let Some(existing) = next.entries.iter().find(|e| e.id == p.entry.id) {
                        if existing != &p.entry {
                            bail!("conflicting operation ID; preserved");
                        }
                    } else {
                        next.entries.push(p.entry.clone());
                    }
                }
                if serde_json::to_vec_pretty(&next)?.len() < statefile::MAX_BYTES {
                    break Some(next);
                }
                if count > 1 {
                    count = count.div_ceil(2);
                    continue;
                }
                if store.value.entries.is_empty() {
                    bail!("operation group exceeds metadata size bound; pending original retained");
                }
                break None;
            };
            let Some(next) = planned else {
                let number = self.writers[&day]
                    .0
                    .checked_add(1)
                    .context("ledger parts exhausted")?;
                let store = self.open_part(&day, number)?;
                self.writers.insert(day, (number, store));
                continue;
            };
            self.writers.get_mut(&day).unwrap().1.update(|part| {
                *part = next;
                Ok(())
            })?;
            for pending in self.pending.drain(..count) {
                self.known.push(pending.entry);
            }
            written += count;
        }
        if self.complete {
            self.warning.clear();
        }
        Ok(written)
    }
    fn open_part(&self, day: &str, number: u32) -> Result<Store<Part>> {
        crate::journal::date(day)?;
        let relative = PathBuf::from(format!(
            ".grimoire/writingdeck/ledger/{day}/{}-{number:06}.json",
            self.session
        ));
        let store = Store::load(
            &self.root,
            &relative,
            Part {
                version: 1,
                session: self.session.clone(),
                part: number,
                entries: vec![],
            },
        )?;
        if store.value.session != self.session
            || store.value.part != number
            || store.value.entries.iter().any(|e| e.date != day)
        {
            bail!("ledger path/identity mismatch; preserved");
        }
        Ok(store)
    }
    pub fn reload(&mut self) {
        self.writers.clear();
        match self.read_all() {
            Ok(entries) => {
                self.known = entries;
                self.complete = true;
                self.warning.clear();
            }
            Err(e) => {
                self.complete = false;
                self.warning = format!("Ledger unavailable/incomplete: {e:#}; originals preserved");
            }
        }
    }
    fn read_all(&self) -> Result<Vec<Entry>> {
        let probe = statefile::checked(
            &self.root,
            Path::new(".grimoire/writingdeck/ledger/probe.json"),
        )?;
        let root = probe.parent().unwrap();
        if !root.exists() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        let mut ids = BTreeSet::new();
        let mut files = 0;
        let mut total_bytes = 0;
        for (day_count, day) in fs::read_dir(root)?.enumerate() {
            if day_count >= 16384 {
                bail!("ledger date directory bound exceeded; history unavailable, not zero");
            }
            let day = day?;
            let name = day.file_name().to_string_lossy().to_string();
            if crate::journal::date(&name).is_err() {
                if day.file_type()?.is_dir()
                    || day.file_type()?.is_symlink()
                    || day.path().extension().is_some_and(|e| e == "json")
                {
                    bail!(
                        "unrecognized ledger entry; originals preserved; no complete total assumed"
                    );
                }
                continue;
            }
            if day.file_type()?.is_symlink() || !day.file_type()?.is_dir() {
                bail!("invalid ledger date directory");
            }
            for file in fs::read_dir(day.path())? {
                let file = file?;
                files += 1;
                if files > 16384 {
                    bail!(
                        "ledger history exceeds bounded scan; narrow/archive with originals retained"
                    );
                }
                if file.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let relative = file.path().strip_prefix(&self.root)?.to_path_buf();
                let bytes =
                    statefile::read(&self.root, &relative)?.context("ledger part disappeared")?;
                total_bytes += bytes.len();
                if total_bytes > 64 * 1024 * 1024 {
                    bail!("ledger history exceeds 64 MiB scan");
                }
                let part: Part = serde_json::from_slice(&bytes)?;
                part.validate()?;
                let expected = format!("{}-{:06}.json", part.session, part.part);
                if file.file_name().to_string_lossy() != expected {
                    bail!("ledger filename/receipt mismatch");
                }
                for entry in part.entries {
                    if entry.date != name || !ids.insert(entry.id.clone()) {
                        bail!("ledger dates or IDs conflict; no total guessed");
                    }
                    out.push(entry);
                }
            }
        }
        Ok(out)
    }
}
fn merge_activity(target: &mut Vec<Activity>, more: Vec<Activity>) {
    for a in more {
        if let Some(old) = target.iter_mut().find(|b| {
            b.date == a.date
                && b.zone == a.zone
                && b.typed_pair == a.typed_pair
                && b.span_start == a.span_start
                && b.span_end == a.span_end
        }) {
            old.milliseconds = old.milliseconds.saturating_add(a.milliseconds);
        } else {
            target.push(a);
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Totals {
    pub input: Counts,
    pub typed: Counts,
    pub pasted: Counts,
    pub replay: Counts,
    pub active_ms: u64,
    pub typed_pair_ms: u64,
    pub unassigned_ms: u64,
    pub revision_operations: u64,
    pub entries: u64,
}
impl Totals {
    pub fn collect<'a>(
        entries: impl Iterator<Item = &'a Entry>,
        from: &str,
        to: &str,
        role: Role,
    ) -> Self {
        let mut t = Self::default();
        for e in entries {
            if e.source.role != role {
                continue;
            }
            if e.date.as_str() >= from && e.date.as_str() <= to {
                t.entries += 1;
                match e.origin {
                    Origin::Undo | Origin::Redo | Origin::Restore => t.replay.plus(e.counts),
                    Origin::Programmatic => {}
                    _ => t.input.plus(e.counts),
                }
                if e.origin == Origin::Typing {
                    t.typed.plus(e.counts);
                }
                if e.origin == Origin::Paste {
                    t.pasted.plus(e.counts);
                }
                if e.mode == Mode::Revision
                    && !matches!(
                        e.origin,
                        Origin::Undo | Origin::Redo | Origin::Restore | Origin::Programmatic
                    )
                {
                    t.revision_operations =
                        t.revision_operations.saturating_add(e.counts.operations);
                }
            }
            for a in &e.activity {
                match &a.date {
                    Some(d) if d.as_str() >= from && d.as_str() <= to => {
                        t.active_ms = t.active_ms.saturating_add(a.milliseconds);
                        if a.typed_pair {
                            t.typed_pair_ms = t.typed_pair_ms.saturating_add(a.milliseconds);
                        }
                    }
                    None if a.span_start.as_str() <= to && a.span_end.as_str() >= from => {
                        t.unassigned_ms = t.unassigned_ms.saturating_add(a.milliseconds)
                    }
                    _ => {}
                }
            }
        }
        t
    }
    pub fn net(&self, footnotes: bool) -> i128 {
        self.input.net(footnotes) + self.replay.net(footnotes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(mono: u64, utc: &str, zone: &str) -> Moment {
        let z: Tz = zone.parse().unwrap();
        let at = DateTime::parse_from_rfc3339(utc)
            .unwrap()
            .with_timezone(&z)
            .fixed_offset();
        Moment {
            mono_ms: mono,
            wall: at,
            zone: Some(zone.into()),
        }
    }
    fn source() -> Source {
        Source {
            path: "manuscript/scene.md".into(),
            role: Role::Manuscript,
        }
    }
    fn count() -> Mutation {
        Mutation {
            origin: Origin::Typing,
            counts: Counts {
                added: 1,
                inserted_chars: 4,
                operations: 1,
                ..Counts::default()
            },
        }
    }
    fn root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wd-ledger-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn only_observed_edit_pairs_get_time_no_startup_or_idle_tail() {
        let mut c = ActiveClock::default();
        let a = time(0, "2026-10-25T00:00:00Z", "UTC");
        let b = time(500, "2026-10-25T00:00:00.500Z", "UTC");
        assert!(
            c.edit(a, &source(), Mode::Drafting, Origin::Typing, 60_000)
                .is_empty()
        );
        assert_eq!(
            c.edit(b, &source(), Mode::Drafting, Origin::Typing, 60_000)[0].milliseconds,
            500
        );
        c.pause();
        assert!(
            c.edit(
                time(2000, "2026-10-25T00:00:02Z", "UTC"),
                &source(),
                Mode::Drafting,
                Origin::Typing,
                60_000
            )
            .is_empty()
        );
    }
    #[test]
    fn idle_document_mode_and_clock_resets_are_not_invented_time() {
        for scenario in 0..4 {
            let mut c = ActiveClock::default();
            c.edit(
                time(100, "2026-10-25T00:00:00Z", "UTC"),
                &source(),
                Mode::Drafting,
                Origin::Typing,
                1000,
            );
            let mut s = source();
            let mut mode = Mode::Drafting;
            let mono = match scenario {
                0 => 5000,
                1 => 99,
                _ => 200,
            };
            if scenario == 2 {
                s.path = "notes/other.md".into();
                s.role = Role::Notes;
            }
            if scenario == 3 {
                mode = Mode::Revision;
            }
            assert!(
                c.edit(
                    time(mono, "2026-10-25T00:00:00.100Z", "UTC"),
                    &s,
                    mode,
                    Origin::Typing,
                    1000
                )
                .is_empty()
            );
        }
    }
    #[test]
    fn civil_midnight_splits_monotonic_time_exactly() {
        let mut c = ActiveClock::default();
        c.edit(
            time(0, "2026-10-25T06:59:59Z", "America/Los_Angeles"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        let a = c.edit(
            time(2000, "2026-10-25T07:00:01Z", "America/Los_Angeles"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        assert_eq!(
            a.iter()
                .map(|a| (a.date.as_deref(), a.milliseconds))
                .collect::<Vec<_>>(),
            vec![(Some("2026-10-24"), 1000), (Some("2026-10-25"), 1000)]
        );
    }
    #[test]
    fn dst_fall_back_and_spring_forward_are_not_wall_duration() {
        for (a, b) in [
            ("2026-11-01T08:59:59Z", "2026-11-01T09:00:01Z"),
            ("2026-03-08T09:59:59Z", "2026-03-08T10:00:01Z"),
        ] {
            let mut c = ActiveClock::default();
            c.edit(
                time(0, a, "America/Los_Angeles"),
                &source(),
                Mode::Drafting,
                Origin::Typing,
                60_000,
            );
            let x = c.edit(
                time(2000, b, "America/Los_Angeles"),
                &source(),
                Mode::Drafting,
                Origin::Typing,
                60_000,
            );
            assert_eq!(x.len(), 1);
            assert_eq!(x[0].milliseconds, 2000);
            assert!(x[0].date.is_some());
        }
    }
    #[test]
    fn wall_jumps_and_zone_travel_keep_unassigned_duration_not_false_days() {
        let mut c = ActiveClock::default();
        c.edit(
            time(0, "2026-10-25T00:00:00Z", "UTC"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        let x = c.edit(
            time(500, "2026-10-24T23:00:00Z", "UTC"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        assert_eq!(x[0].date, None);
        assert_eq!(x[0].milliseconds, 500);
        let x = c.edit(
            time(1000, "2026-10-24T23:00:00.500Z", "Asia/Tokyo"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        assert_eq!(x[0].date, None);
    }
    #[test]
    fn paste_and_history_do_not_manufacture_exclusive_keyboard_time() {
        let mut c = ActiveClock::default();
        c.edit(
            time(0, "2026-10-25T00:00:00Z", "UTC"),
            &source(),
            Mode::Drafting,
            Origin::Typing,
            60_000,
        );
        let a = c.edit(
            time(500, "2026-10-25T00:00:00.500Z", "UTC"),
            &source(),
            Mode::Drafting,
            Origin::Paste,
            60_000,
        );
        assert!(!a[0].typed_pair);
        assert!(
            c.edit(
                time(1000, "2026-10-25T00:00:01Z", "UTC"),
                &source(),
                Mode::Drafting,
                Origin::Restore,
                60_000
            )
            .is_empty()
        );
    }
    #[test]
    fn ledger_writes_metadata_only_and_reopens_without_duplicating() {
        let r = root("roundtrip");
        let mut l = Ledger::load(&r);
        assert!(!r.join(".grimoire").exists());
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        l.observe(&t, source(), Mode::Drafting, count(), vec![])
            .unwrap();
        assert_eq!(l.pending_count(), 1);
        assert_eq!(l.flush().unwrap(), 1);
        assert_eq!(l.flush().unwrap(), 0);
        let reopened = Ledger::load(&r);
        assert!(reopened.complete);
        assert_eq!(reopened.known, l.known);
        assert_eq!(
            Totals::collect(
                reopened.known.iter(),
                "2026-10-25",
                "2026-10-25",
                Role::Manuscript
            )
            .input
            .added,
            1
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn aggregates_are_sealed_on_failure_and_repaired_retry_keeps_both() {
        let r = root("failure");
        let mut l = Ledger::load(&r);
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        l.observe(&t, source(), Mode::Drafting, count(), vec![])
            .unwrap();
        l.flush().unwrap();
        let file = fs::read_dir(r.join(".grimoire/writingdeck/ledger/2026-10-25"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let saved = fs::read(&file).unwrap();
        let future = b"{\"version\":999,\"preserve\":true}";
        fs::write(&file, future).unwrap();
        l.observe(&t, source(), Mode::Drafting, count(), vec![])
            .unwrap();
        assert!(l.flush().is_err());
        l.observe(&t, source(), Mode::Drafting, count(), vec![])
            .unwrap();
        assert_eq!(l.pending_count(), 2);
        assert_eq!(fs::read(&file).unwrap(), future);
        l.reload();
        assert!(!l.complete);
        fs::write(&file, &saved).unwrap();
        l.reload();
        l.flush().unwrap();
        assert_eq!(l.known.len(), 3);
        assert_eq!(Ledger::load(&r).known.len(), 3);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn non_manuscript_replay_and_typing_are_separate() {
        let r = root("roles");
        let mut l = Ledger::load(&r);
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        let s = Source {
            path: "notes/a.md".into(),
            role: Role::Notes,
        };
        l.observe(&t, s, Mode::Drafting, count(), vec![]).unwrap();
        l.observe(&t, source(), Mode::Drafting, count(), vec![])
            .unwrap();
        l.observe(
            &t,
            source(),
            Mode::Drafting,
            Mutation {
                origin: Origin::Undo,
                counts: count().counts.inverse(),
            },
            vec![],
        )
        .unwrap();
        l.flush().unwrap();
        let totals = Totals::collect(l.known.iter(), "2026-10-25", "2026-10-25", Role::Manuscript);
        assert_eq!(totals.input.added, 1);
        assert_eq!(totals.replay.removed, 1);
        assert_eq!(totals.net(false), 0);
        assert_eq!(totals.typed.added, 1);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn pending_groups_stay_in_memory_until_a_successful_guarded_write() {
        let r = root("pending");
        let mut l = Ledger::load(&r);
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        for _ in 0..1000 {
            l.observe(&t, source(), Mode::Drafting, count(), vec![])
                .unwrap();
        }
        assert!(l.known.is_empty());
        assert_eq!(l.pending_count(), 1);
        l.flush().unwrap();
        assert_eq!(l.known[0].counts.added, 1000);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn separate_sessions_do_not_share_writers_or_overwrite() {
        let r = root("sessions");
        let mut a = Ledger::load(&r);
        let mut b = Ledger::load(&r);
        assert_ne!(a.session, b.session);
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        for l in [&mut a, &mut b] {
            l.observe(&t, source(), Mode::Drafting, count(), vec![])
                .unwrap();
            l.flush().unwrap();
        }
        let l = Ledger::load(&r);
        assert!(l.complete);
        assert_eq!(l.known.len(), 2);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn part_rotation_keeps_all_unique_records() {
        let r = root("rotate");
        let mut l = Ledger::load(&r);
        let t = time(0, "2026-10-25T00:00:00Z", "UTC");
        for _ in 0..130 {
            l.observe(&t, source(), Mode::Drafting, count(), vec![])
                .unwrap();
            l.flush().unwrap();
        }
        assert_eq!(Ledger::load(&r).known.len(), 130);
        assert_eq!(
            fs::read_dir(r.join(".grimoire/writingdeck/ledger/2026-10-25"))
                .unwrap()
                .count(),
            2
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn batched_multipart_failure_retains_the_unpublished_prefix_and_exact_retry_has_no_duplicates()
    {
        let root = std::env::temp_dir().join(format!("wd-batch-ledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let mut ledger = Ledger::load(&root);
        let at = Moment {
            mono_ms: 0,
            wall: DateTime::parse_from_rfc3339("2026-10-25T12:00:00+00:00").unwrap(),
            zone: Some("UTC".into()),
        };
        for i in 0..260 {
            ledger
                .observe(
                    &at,
                    Source {
                        path: format!("manuscript/scene-{i}.md").into(),
                        role: Role::Manuscript,
                    },
                    Mode::Drafting,
                    Mutation {
                        origin: Origin::Typing,
                        counts: Counts {
                            added: 1,
                            inserted_chars: 4,
                            operations: 1,
                            ..Counts::default()
                        },
                    },
                    vec![],
                )
                .unwrap();
        }
        let blocked = root.join(format!(
            ".grimoire/writingdeck/ledger/2026-10-25/{}-000002.json",
            ledger.session
        ));
        fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        let future = b"{\"version\":999,\"keep\":true}";
        fs::write(&blocked, future).unwrap();
        assert!(ledger.flush().is_err());
        assert_eq!(ledger.known.len(), 128);
        assert_eq!(ledger.pending_count(), 132);
        assert_eq!(fs::read(&blocked).unwrap(), future);
        fs::remove_file(blocked).unwrap();
        assert_eq!(ledger.flush().unwrap(), 132);
        assert_eq!(ledger.pending_count(), 0);
        let loaded = Ledger::load(&root);
        assert!(loaded.complete);
        assert_eq!(loaded.known.len(), 260);
        assert_eq!(ledger.flush().unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn long_reference_activity_groups_roll_parts_by_bytes_before_the_record_limit() {
        let root = std::env::temp_dir().join(format!("wd-byte-ledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let mut ledger = Ledger::load(&root);
        let at = Moment {
            mono_ms: 0,
            wall: DateTime::parse_from_rfc3339("2026-10-25T12:00:00+00:00").unwrap(),
            zone: Some("UTC".into()),
        };
        let activity = Activity {
            date: Some("2026-10-25".into()),
            zone: Some("UTC".into()),
            span_start: "2026-10-25".into(),
            span_end: "2026-10-25".into(),
            milliseconds: 1,
            typed_pair: false,
        };
        for i in 0..128 {
            ledger
                .observe(
                    &at,
                    Source {
                        path: format!("manuscript/{}-{i}.md", "語".repeat(3000)).into(),
                        role: Role::Manuscript,
                    },
                    Mode::Drafting,
                    Mutation {
                        origin: Origin::Typing,
                        counts: Counts {
                            added: 1,
                            inserted_chars: 4,
                            operations: 1,
                            ..Counts::default()
                        },
                    },
                    vec![activity.clone(); 16],
                )
                .unwrap();
        }
        assert_eq!(ledger.flush().unwrap(), 128);
        let entries: Vec<_> = fs::read_dir(root.join(".grimoire/writingdeck/ledger/2026-10-25"))
            .unwrap()
            .map(|p| p.unwrap())
            .collect();
        assert!(entries.len() > 1);
        assert!(
            entries
                .iter()
                .all(|p| p.metadata().unwrap().len() <= statefile::MAX_BYTES as u64)
        );
        let loaded = Ledger::load(&root);
        assert!(loaded.complete);
        assert_eq!(loaded.known.len(), 128);
        fs::remove_dir_all(root).unwrap();
    }
}
