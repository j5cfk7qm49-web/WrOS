//! Headless storage and projection measurements for the Stage 0 gate.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::loro_store::LoroStore;
use crate::model::{Builtin, Change, Edit, FieldId, NodeId, OpId, Place, Snapshot};
use crate::storage::DurableStore;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const DEVICE: u64 = 1;
const MIB: u64 = 1024 * 1024;
const QUERY: &str = "SELECT rowid FROM notes WHERE notes MATCH ?1 ORDER BY rank LIMIT 20";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Smoke,
    Aged,
}

#[derive(Serialize)]
struct Workload {
    visible_nodes: usize,
    deleted_nodes: usize,
    accumulated_edits: u64,
    review_events: u64,
    latency_samples: usize,
}

impl Profile {
    pub fn parse(value: &std::ffi::OsStr) -> Result<Self> {
        match value.to_str() {
            Some("smoke") => Ok(Self::Smoke),
            Some("aged") => Ok(Self::Aged),
            _ => Err("Choose the smoke or aged profile".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Aged => "aged",
        }
    }

    fn workload(self) -> Workload {
        match self {
            Self::Smoke => Workload {
                visible_nodes: 64,
                deleted_nodes: 16,
                accumulated_edits: 320,
                review_events: 48,
                latency_samples: 5,
            },
            Self::Aged => Workload {
                visible_nodes: 50_000,
                deleted_nodes: 10_000,
                accumulated_edits: 500_000,
                review_events: 100_000,
                latency_samples: 20,
            },
        }
    }
}

#[derive(Serialize)]
struct Metric {
    observed: Number,
    limit: Number,
    passed: bool,
}

impl Metric {
    fn bytes(observed: u64, limit: u64) -> Self {
        Self {
            observed: observed.into(),
            limit: limit.into(),
            passed: observed <= limit,
        }
    }

    fn milliseconds(observed: Duration, limit: u64) -> Self {
        let ms = observed.as_secs_f64() * 1000.0;
        Self {
            observed: Number::from_f64(ms).unwrap(),
            limit: limit.into(),
            passed: ms <= limit as f64,
        }
    }
}

#[derive(Serialize)]
struct Machine {
    os: &'static str,
    architecture: &'static str,
    kernel: String,
    cpu: String,
    memory_bytes: u64,
    cgroup_memory_limit: Option<u64>,
}

#[derive(Serialize)]
struct Disk {
    store_bytes: u64,
    index_bytes: u64,
    index_wal_bytes: u64,
    markdown_bytes: u64,
}

#[derive(Serialize)]
struct Report<'a> {
    schema: u32,
    seed: u64,
    profile: Profile,
    revision: &'a str,
    measured_at_unix_seconds: u64,
    machine: Machine,
    workload: Workload,
    initial_disk: Disk,
    final_disk: Disk,
    background_batches: usize,
    checkpoint_size_bytes: u64,
    peak_index_wal_bytes: u64,
    process_write_bytes_during_edit_samples: u64,
    cold_open_peak_rss_bytes: u64,
    checkpoint_start_rss_bytes: u64,
    storage_only_checkpoint_peak_rss_bytes: u64,
    metrics: BTreeMap<&'static str, Metric>,
    within_limits: bool,
}

pub fn run(seed: u64, profile: Profile, revision: &str, output: &Path) -> Result<bool> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let scratch = tempfile::Builder::new()
        .prefix("brain-bench-")
        .tempdir_in(parent)?;
    let paths = BenchPaths::new(scratch.path());
    let workload = profile.workload();
    let prepared = Command::new(std::env::current_exe()?)
        .args(["--prepare-fixture", profile.name(), &seed.to_string()])
        .arg(scratch.path())
        .status()?;
    if !prepared.success() {
        return Err("Brain benchmark fixture preparation failed".into());
    }
    let target: FixtureNote =
        serde_json::from_slice(&fs::read(scratch.path().join("target.json"))?)?;
    let initial_disk = paths.disk()?;
    eprintln!("Measuring Brain storage and search with the background writer active");
    let (sample, background_batches) = with_background_writer(&paths, || {
        sample_workload(scratch.path(), &paths, &workload, target)
    })?;
    let final_disk = paths.disk()?;
    let storage_only: CheckpointSample = run_worker("--storage-checkpoint", scratch.path())?;
    let metrics = BTreeMap::from([
        (
            "cold_open_p95_ms",
            Metric::milliseconds(p95(sample.cold_open), 3000),
        ),
        (
            "edit_to_text_p95_ms",
            Metric::milliseconds(p95(sample.edits), 50),
        ),
        (
            "fts_p95_ms",
            Metric::milliseconds(p95(sample.searches), 150),
        ),
        (
            "checkpoint_peak_rss_bytes",
            Metric::bytes(sample.checkpoint.peak_rss, 512 * MIB),
        ),
        (
            "journal_bytes_per_edit",
            Metric::bytes(sample.edit_bytes, 64 * 1024),
        ),
        (
            "checkpoint_temporary_bytes",
            Metric::bytes(sample.checkpoint.growth, sample.checkpoint_size + 64 * MIB),
        ),
    ]);
    let within_limits = metrics.values().all(|metric| metric.passed);
    let report = Report {
        schema: 1,
        seed,
        profile,
        revision,
        measured_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        machine: machine()?,
        workload,
        initial_disk,
        final_disk,
        background_batches,
        checkpoint_size_bytes: sample.checkpoint_size,
        peak_index_wal_bytes: sample.checkpoint.peak_wal,
        process_write_bytes_during_edit_samples: sample.edit_process_writes,
        cold_open_peak_rss_bytes: sample.cold_open_peak_rss,
        checkpoint_start_rss_bytes: sample.checkpoint.start_rss,
        storage_only_checkpoint_peak_rss_bytes: storage_only.peak_rss,
        metrics,
        within_limits,
    };
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist_noclobber(output)?;
    Ok(within_limits)
}

// Separate worker processes ensure fixture allocation and prior opens cannot
// leave resident allocator pages in the checkpoint measurement process.
pub fn prepare_worker(seed: u64, profile: Profile, root: &Path) -> Result<()> {
    let paths = BenchPaths::new(root);
    let workload = profile.workload();
    eprintln!("Preparing the Brain benchmark fixture");
    let target = seed_store(&paths.store, &workload, seed)?;
    eprintln!("Writing the initial SQLite index and Markdown copies");
    setup_projections(&paths, &workload)?;
    fs::write(root.join("target.json"), serde_json::to_vec(&target)?)?;
    Ok(())
}

pub fn cold_open_worker(root: &Path) -> Result<()> {
    let target: FixtureNote = serde_json::from_slice(&fs::read(root.join("target.json"))?)?;
    let paths = BenchPaths::new(root);
    let start = Instant::now();
    let disk = DurableStore::open(&paths.store, DEVICE)?;
    let projection = disk.project()?;
    first_note(&projection, target.node)?;
    let elapsed = start.elapsed();
    let record = ColdOpen {
        elapsed,
        peak_rss: proc_kib("/proc/self/status", "VmHWM:")?,
    };
    serde_json::to_writer(io::stdout().lock(), &record)?;
    Ok(())
}

pub fn checkpoint_worker(root: &Path) -> Result<()> {
    let paths = BenchPaths::new(root);
    let mut disk = DurableStore::open(&paths.store, DEVICE)?;
    let record = measure_checkpoint(&mut disk, &paths)?;
    serde_json::to_writer(io::stdout().lock(), &record)?;
    Ok(())
}

fn run_worker<T: serde::de::DeserializeOwned>(mode: &str, root: &Path) -> Result<T> {
    let result = Command::new(std::env::current_exe()?)
        .arg(mode)
        .arg(root)
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "Benchmark worker {mode} failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
        .into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}

#[derive(Deserialize, Serialize)]
struct ColdOpen {
    elapsed: Duration,
    peak_rss: u64,
}

struct BenchPaths {
    store: PathBuf,
    index: PathBuf,
    readable: PathBuf,
}

impl BenchPaths {
    fn new(root: &Path) -> Self {
        Self {
            store: root.join("store"),
            index: root.join("search.sqlite"),
            readable: root.join("readable"),
        }
    }

    fn disk(&self) -> io::Result<Disk> {
        Ok(Disk {
            store_bytes: directory_bytes(&self.store)?,
            index_bytes: fs::metadata(&self.index)?.len(),
            index_wal_bytes: optional_bytes(&self.index.with_file_name("search.sqlite-wal"))?,
            markdown_bytes: directory_bytes(&self.readable)?,
        })
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct FixtureNote {
    node: NodeId,
    token: OpId,
    last_text: OpId,
    last_field: Option<OpId>,
    last_tag: Option<OpId>,
}

struct Fixture {
    store: LoroStore,
    clock: u64,
    random: u64,
}

impl Fixture {
    fn next_random(&mut self) -> u64 {
        // SplitMix64, including a valid sequence for seed zero.
        self.random = self.random.wrapping_add(0x9e3779b97f4a7c15);
        let mut number = self.random;
        number = (number ^ (number >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        number = (number ^ (number >> 27)).wrapping_mul(0x94d049bb133111eb);
        number ^ (number >> 31)
    }

    fn record(&mut self, change: Change, extra: Option<OpId>) -> Result<OpId> {
        let edit = make_edit(self.clock, change, extra);
        self.clock += 1;
        let id = edit.id;
        self.store.record_fixture_edit(edit)?;
        Ok(id)
    }
}

fn make_edit(previous: u64, change: Change, extra: Option<OpId>) -> Edit {
    let dependency = (previous != 0).then_some(OpId {
        clock: previous,
        device: DEVICE,
    });
    Edit {
        id: OpId {
            clock: previous + 1,
            device: DEVICE,
        },
        dependencies: dependency.into_iter().chain(extra).collect(),
        change,
    }
}

fn seed_store(root: &Path, workload: &Workload, seed: u64) -> Result<FixtureNote> {
    let mut fixture = Fixture {
        store: LoroStore::new(DEVICE)?,
        clock: 0,
        random: seed,
    };
    let total = workload.visible_nodes + workload.deleted_nodes;
    let roots = (workload.visible_nodes / 200).max(2);
    let tag = NodeId::Builtin(Builtin::Concept);
    let mut notes: Vec<FixtureNote> = Vec::with_capacity(total);
    for index in 0..total {
        let node = if index == 1 {
            tag
        } else {
            NodeId::Note(OpId {
                clock: fixture.clock + 1,
                device: DEVICE,
            })
        };
        let parent = (index >= roots).then(|| notes[index % roots].node);
        fixture.record(
            Change::Create {
                node,
                place: Place {
                    parent,
                    after: None,
                },
            },
            None,
        )?;
        let salt = fixture.next_random();
        let token = fixture.record(
            Change::InsertText {
                node,
                after: None,
                text: fixture_text(salt),
            },
            None,
        )?;
        notes.push(FixtureNote {
            node,
            token,
            last_text: token,
            last_field: None,
            last_tag: None,
        });
    }
    for note in &notes[workload.visible_nodes..] {
        fixture.record(
            Change::Delete {
                members: [note.node].into(),
            },
            None,
        )?;
    }
    let field = FieldId::Custom(OpId {
        clock: 1,
        device: DEVICE,
    });
    while fixture.clock < workload.accumulated_edits {
        let salt = fixture.next_random();
        let note = &mut notes[(salt as usize) % workload.visible_nodes];
        match (salt >> 20) % 4 {
            0 => {
                note.last_text = fixture.record(
                    Change::EditText {
                        token: note.token,
                        text: fixture_text(salt),
                    },
                    Some(note.last_text),
                )?;
            }
            1 => {
                note.last_tag = Some(fixture.record(
                    Change::SetTag {
                        node: note.node,
                        tag,
                        present: (salt >> 24).is_multiple_of(2),
                    },
                    note.last_tag,
                )?);
            }
            _ => {
                note.last_field = Some(fixture.record(
                    Change::SetField {
                        node: note.node,
                        field,
                        value: Some(format!("Shelf {}", salt % 1024)),
                    },
                    note.last_field,
                )?);
            }
        }
    }
    for event in 0..workload.review_events {
        fixture.store.record_fixture_review(format!(
            r#"{{"event":{event},"card":{},"grade":{},"day":{}}}"#,
            event % workload.visible_nodes as u64,
            event % 6,
            event / 300,
        ))?;
    }
    assert_eq!(
        fixture.store.edit_count() as u64,
        workload.accumulated_edits
    );
    let checkpoint = fixture.store.checkpoint()?;
    drop(fixture);
    let mut disk = DurableStore::open(root, DEVICE)?;
    disk.import(&checkpoint.snapshot)?;
    drop(checkpoint);
    // Leave a normal pair of usable generations with no setup journal.
    disk.checkpoint()?;
    disk.checkpoint()?;
    Ok(notes[0])
}

fn fixture_text(salt: u64) -> String {
    const PARAGRAPH: &str = "The river trail leads to a quiet ridge near the lake. ";
    let length = 56 + (salt as usize % (2048 - 56 + 1));
    let mut text = PARAGRAPH.repeat(length.div_ceil(PARAGRAPH.len()));
    text.truncate(length);
    text
}

fn setup_projections(paths: &BenchPaths, workload: &Workload) -> Result<()> {
    let view = DurableStore::open(&paths.store, DEVICE)?.project()?;
    fs::create_dir(&paths.readable)?;
    let mut connection = Connection::open(&paths.index)?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; CREATE VIRTUAL TABLE notes USING fts5(body);")?;
    let transaction = connection.transaction()?;
    let mut insert = transaction.prepare("INSERT INTO notes(body) VALUES (?1)")?;
    let mut rows = 0;
    for (page, root) in view.children(None).into_iter().enumerate() {
        let mut readable = String::new();
        let mut stack = vec![(root, 0)];
        while let Some((node, depth)) = stack.pop() {
            let body = view
                .body(node)
                .ok_or("Visible benchmark note has no body")?;
            insert.execute([&body])?;
            rows += 1;
            readable.push_str(&"  ".repeat(depth));
            readable.push_str("- ");
            readable.push_str(&body);
            readable.push('\n');
            stack.extend(
                view.children(Some(node))
                    .into_iter()
                    .rev()
                    .map(|child| (child, depth + 1)),
            );
        }
        fs::write(paths.readable.join(format!("page-{page}.md")), readable)?;
    }
    drop(insert);
    transaction.commit()?;
    if rows != workload.visible_nodes {
        return Err(format!("Indexed {rows} notes, expected {}", workload.visible_nodes).into());
    }
    Ok(())
}

fn with_background_writer<T>(
    paths: &BenchPaths,
    measure: impl FnOnce() -> Result<T>,
) -> Result<(T, usize)> {
    let stop = AtomicBool::new(false);
    let (first_batch, ready) = mpsc::sync_channel(0);
    thread::scope(|scope| {
        let worker = scope.spawn(|| background_batches(paths, &stop, first_batch));
        let sampled = ready
            .recv()
            .map_err(|_| "Background projection could not start".into())
            .and_then(|()| measure());
        stop.store(true, Ordering::Relaxed);
        let count = worker
            .join()
            .map_err(|_| "Background projection panicked")??;
        Ok((sampled?, count))
    })
}

fn background_batches(
    paths: &BenchPaths,
    stop: &AtomicBool,
    ready: mpsc::SyncSender<()>,
) -> Result<usize> {
    let connection = Connection::open(&paths.index)?;
    connection.busy_timeout(Duration::from_secs(2))?;
    let mut update = connection.prepare("UPDATE notes SET body=?1 WHERE rowid=?2")?;
    let temporary = paths.readable.join("refresh.md.new");
    let published = paths.readable.join("refresh.md");
    let mut ready = Some(ready);
    let mut batches = 0;
    while !stop.load(Ordering::Relaxed) {
        let changed = format!("The river trail now has revision {batches} at the quiet ridge.");
        update.execute(params![changed, 1])?;
        fs::write(&temporary, changed)?;
        fs::rename(&temporary, &published)?;
        batches += 1;
        if let Some(signal) = ready.take() {
            signal.send(())?;
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(batches)
}

struct Sample {
    cold_open: Vec<Duration>,
    cold_open_peak_rss: u64,
    edits: Vec<Duration>,
    searches: Vec<Duration>,
    edit_bytes: u64,
    edit_process_writes: u64,
    checkpoint: CheckpointSample,
    checkpoint_size: u64,
}

fn sample_workload(
    root: &Path,
    paths: &BenchPaths,
    workload: &Workload,
    target: FixtureNote,
) -> Result<Sample> {
    let mut cold_open = Vec::new();
    let mut cold_open_peak_rss = 0;
    for index in 0..workload.latency_samples {
        let record: ColdOpen = run_worker("--cold-open", root)?;
        cold_open.push(record.elapsed);
        cold_open_peak_rss = cold_open_peak_rss.max(record.peak_rss);
        if (index + 1).is_multiple_of(5) {
            eprintln!(
                "Measured {} of {} cold opens",
                index + 1,
                workload.latency_samples
            );
        }
    }
    let mut disk = DurableStore::open(&paths.store, DEVICE)?;
    let mut line = first_note(&disk.project()?, target.node)?;
    let initial_journal = directory_bytes(&paths.store.join("journal"))?;
    let mut edits = Vec::new();
    let mut searches = Vec::new();
    let search = Connection::open(&paths.index)?;
    search.busy_timeout(Duration::from_secs(2))?;
    let mut query = search.prepare_cached(QUERY)?;
    let initial_writes = proc_value("/proc/self/io", "write_bytes:")?;
    for i in 0..workload.latency_samples {
        let start = Instant::now();
        line.push('.');
        let clock = workload.accumulated_edits + i as u64;
        let previous_text = if i == 0 {
            target.last_text
        } else {
            OpId {
                clock,
                device: DEVICE,
            }
        };
        disk.append(make_edit(
            clock,
            Change::EditText {
                token: target.token,
                text: line.clone(),
            },
            Some(previous_text),
        ))?;
        // Construct the line that would be handed to the foreground writer.
        std::hint::black_box(format!("{}\n", line));
        edits.push(start.elapsed());

        let start = Instant::now();
        let term = ["river", "\"quiet ridge\"", "lake AND trail"][i % 3];
        let mut rows = query.query([term])?;
        let mut matches = 0;
        while rows.next()?.is_some() {
            matches += 1;
        }
        if matches == 0 {
            return Err("The benchmark search matched no notes".into());
        }
        searches.push(start.elapsed());
    }
    let edit_process_writes =
        proc_value("/proc/self/io", "write_bytes:")?.saturating_sub(initial_writes);
    let journal = directory_bytes(&paths.store.join("journal"))?;
    let new_bytes = journal.saturating_sub(initial_journal);
    let checkpoint = measure_checkpoint(&mut disk, paths)?;
    let checkpoint_size = checkpoint_size(&paths.store)?;
    if first_note(&disk.project()?, target.node)? != line {
        return Err("Saved edits did not appear in the Brain projection".into());
    }
    Ok(Sample {
        cold_open,
        cold_open_peak_rss,
        edits,
        searches,
        edit_bytes: new_bytes.div_ceil(workload.latency_samples as u64),
        edit_process_writes,
        checkpoint,
        checkpoint_size,
    })
}

fn first_note(view: &Snapshot, node: NodeId) -> Result<String> {
    if let Some(body) = view.body(node)
        && view.is_visible(node)
    {
        return Ok(body);
    }
    Err("The first benchmark note is missing".into())
}

fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[(samples.len() * 95).div_ceil(100) - 1]
}

#[derive(Deserialize, Serialize)]
struct CheckpointSample {
    start_rss: u64,
    peak_rss: u64,
    growth: u64,
    peak_wal: u64,
}

fn measure_checkpoint(disk: &mut DurableStore, paths: &BenchPaths) -> Result<CheckpointSample> {
    let baseline = directory_bytes(&paths.store)?;
    let rss_before = proc_kib("/proc/self/status", "VmRSS:")?;
    let wal = paths.index.with_file_name("search.sqlite-wal");
    let wal_before = optional_bytes(&wal)?;
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let sample = scope.spawn(|| -> Result<CheckpointSample> {
            let mut peak_rss = rss_before;
            let mut peak_wal = wal_before;
            let mut peak_disk = baseline;
            loop {
                peak_rss = peak_rss.max(proc_kib("/proc/self/status", "VmRSS:")?);
                peak_disk = peak_disk.max(directory_bytes(&paths.store)?);
                peak_wal = peak_wal.max(optional_bytes(&wal)?);
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok(CheckpointSample {
                start_rss: rss_before,
                peak_rss,
                growth: peak_disk.saturating_sub(baseline),
                peak_wal,
            })
        });
        let saved = disk.checkpoint();
        stop.store(true, Ordering::Relaxed);
        saved?;
        sample.join().map_err(|_| "Checkpoint sampler panicked")?
    })
}

fn checkpoint_size(root: &Path) -> io::Result<u64> {
    let mut largest = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "snapshot")
        {
            largest = largest.max(entry.metadata()?.len());
        }
    }
    Ok(largest)
}

fn optional_bytes(file: &Path) -> io::Result<u64> {
    match fs::metadata(file) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn directory_bytes(root: &Path) -> io::Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            total += directory_bytes(&entry.path())?;
        } else {
            total += optional_bytes(&entry.path())?;
        }
    }
    Ok(total)
}

fn proc_kib(path: &str, key: &str) -> Result<u64> {
    Ok(proc_value(path, key)? * 1024)
}

fn proc_value(path: &str, key: &str) -> Result<u64> {
    let text = fs::read_to_string(path)?;
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| format!("Missing {key} in {path}"))?;
    Ok(value.parse::<u64>()?)
}

fn machine() -> Result<Machine> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo")?;
    let cpu = cpuinfo
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.trim() == "model name")
                .map(|(_, value)| value.trim())
        })
        .unwrap_or("unknown")
        .to_owned();
    let cgroup = fs::read_to_string("/proc/self/cgroup")?;
    let limit = cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .and_then(|path| {
            fs::read_to_string(
                Path::new("/sys/fs/cgroup")
                    .join(path.trim_start_matches('/'))
                    .join("memory.max"),
            )
            .ok()
        })
        .and_then(|size| size.trim().parse::<u64>().ok());
    Ok(Machine {
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")?
            .trim()
            .to_owned(),
        cpu,
        memory_bytes: proc_kib("/proc/meminfo", "MemTotal:")?,
        cgroup_memory_limit: limit,
    })
}
