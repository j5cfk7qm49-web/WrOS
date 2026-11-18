//! Whole-book history uses one worker per open book. Only checkpoints that
//! must precede a destructive operation wait for Git.

use anyhow::Result;
use chrono::{DateTime, Local};
use grimoire_core::history;
use grimoire_core::sessions;
use grimoire_core::timeline::{self, Entry, SceneVersion};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) const CHECKPOINT_WAIT: Duration = Duration::from_secs(2);
const IDLE_SESSION: Duration = Duration::from_secs(30 * 60);
const UPLOAD_EVERY: Duration = Duration::from_secs(60 * 60);
const SCENE_VERSION_LIMIT: usize = 100;

type Notice = String;
pub type SceneVersions = Option<Vec<SceneVersion>>;

enum Job {
    Record(Entry),
    EndSession(DateTime<Local>),
    Upload,
    SceneVersions(PathBuf, mpsc::Sender<SceneVersions>),
    Wake,
}

struct Checkpoint {
    entry: Entry,
    deadline: Instant,
    done: mpsc::Sender<bool>,
}

pub(super) struct PendingCheckpoint {
    result: mpsc::Receiver<bool>,
    deadline: Instant,
}

impl PendingCheckpoint {
    pub(super) fn wait(self) -> bool {
        self.result
            .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(false)
    }
}

pub struct Timeline {
    jobs: mpsc::Sender<Job>,
    urgent: mpsc::Sender<Checkpoint>,
    worker: JoinHandle<()>,
    failures: mpsc::Receiver<String>,
    last_record_ok: Arc<AtomicBool>,
    /// Set when the app is closing: queued records and session marks still run, uploads wait
    /// for the next session (what isn't uploaded stays pending in the repository).
    closing: Arc<AtomicBool>,
    notice_shown: bool,
    last_session_end: Option<Instant>,
    next_save: Option<Instant>,
    next_upload: Instant,
}

impl Timeline {
    pub fn start(root: PathBuf) -> Self {
        Self::start_with(root, crate::sync_service::run_timeline)
    }

    fn start_with(
        root: PathBuf,
        uploader: impl FnMut(Value) -> Result<Value, String> + Send + 'static,
    ) -> Self {
        let (jobs, work) = mpsc::channel();
        let (urgent, checkpoints) = mpsc::channel();
        let (failed, failures) = mpsc::sync_channel(1);
        let last_record_ok = Arc::new(AtomicBool::new(false));
        let status = Arc::clone(&last_record_ok);
        let closing = Arc::new(AtomicBool::new(false));
        let stop_uploads = Arc::clone(&closing);
        let worker = thread::spawn(move || {
            let lock = book_lock(&root);
            let _book = lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut worker = Worker {
                root,
                uploader,
                ready: false,
                status,
                failed,
            };
            let started = worker.enable();
            worker.report(started);
            while let Ok(job) = work.recv() {
                for checkpoint in checkpoints.try_iter() {
                    let Checkpoint {
                        entry,
                        deadline,
                        done,
                    } = checkpoint;
                    // An expired checkpoint may already have been followed by a file change.
                    if Instant::now() < deadline {
                        let ok = worker.record(&entry);
                        let _ = done.send(ok && Instant::now() <= deadline);
                    }
                }
                match job {
                    Job::Record(entry) => {
                        worker.record(&entry);
                    }
                    Job::EndSession(now) => {
                        if worker.record(&Entry::Save) {
                            let _ = timeline::mark_session(&worker.root, now);
                        }
                        if !stop_uploads.load(Ordering::Acquire) {
                            let _ = worker.upload();
                        }
                    }
                    Job::Upload if !stop_uploads.load(Ordering::Acquire) => {
                        let _ = worker.upload();
                    }
                    Job::Upload => {}
                    Job::SceneVersions(scene, done) if !stop_uploads.load(Ordering::Acquire) => {
                        let versions = if worker.status.load(Ordering::Acquire) {
                            timeline::scene_versions(&worker.root, &scene, SCENE_VERSION_LIMIT).ok()
                        } else {
                            None
                        };
                        let _ = done.send(versions);
                    }
                    Job::SceneVersions(..) | Job::Wake => {}
                }
            }
        });
        Self {
            jobs,
            urgent,
            worker,
            failures,
            last_record_ok,
            closing,
            notice_shown: false,
            last_session_end: None,
            next_save: None,
            next_upload: Instant::now() + UPLOAD_EVERY,
        }
    }

    pub fn record(&self, entry: Entry) {
        let _ = self.jobs.send(Job::Record(entry));
    }

    pub fn record_now(&self, entry: Entry, wait: Duration) -> bool {
        self.queue_checkpoint(entry, wait).wait()
    }

    pub(super) fn queue_checkpoint(&self, entry: Entry, wait: Duration) -> PendingCheckpoint {
        let deadline = Instant::now() + wait;
        let (done, result) = mpsc::channel();
        if self
            .urgent
            .send(Checkpoint {
                entry,
                deadline,
                done,
            })
            .is_ok()
        {
            let _ = self.jobs.send(Job::Wake);
        }
        PendingCheckpoint { result, deadline }
    }

    pub fn end_session(&self, now: DateTime<Local>) {
        let _ = self.jobs.send(Job::EndSession(now));
    }

    pub fn upload(&self) {
        let _ = self.jobs.send(Job::Upload);
    }

    pub(super) fn scene_versions(&self, scene: PathBuf) -> mpsc::Receiver<SceneVersions> {
        let (done, result) = mpsc::channel();
        let _ = self.jobs.send(Job::SceneVersions(scene, done));
        result
    }

    pub fn poll(&mut self) -> Option<Notice> {
        let error = self.failures.try_recv().ok()?;
        if std::mem::replace(&mut self.notice_shown, true) {
            return None;
        }
        Some(format!(
            "The book's timeline isn't recording ({error}); scene history keeps plain copies."
        ))
    }

    pub fn last_record_ok(&self) -> bool {
        self.last_record_ok.load(Ordering::Acquire)
    }

    /// Waits for queued records and session marks, not for uploads: closing the app (an app
    /// update's restart, shutting down) never waits on the network.
    pub(crate) fn finish(self) {
        self.closing.store(true, Ordering::Release);
        drop(self.jobs);
        drop(self.urgent);
        let _ = self.worker.join();
    }

    pub(super) fn carry_notice(&mut self, previous: &Self) {
        self.notice_shown = previous.notice_shown;
    }

    pub(super) fn saved(&mut self, now: Instant, fallback_snapshot: bool) {
        if fallback_snapshot || self.next_save.is_none_or(|due| now >= due) {
            self.record(Entry::Save);
            self.next_save = Some(now + history::GAP);
        }
    }
}

// A book can be reopened before its previous worker has drained its queue.
// Serialize workers for that book without making the app wait for either.
fn book_lock(root: &Path) -> Arc<Mutex<()>> {
    static BOOKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut books = BOOKS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    books.retain(|_, lock| lock.strong_count() != 0);
    if let Some(lock) = books.get(&root).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    books.insert(root, Arc::downgrade(&lock));
    lock
}

fn rebuild_from_drive(
    root: &Path,
    fetch: impl FnOnce(Value) -> Result<Value, String>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct Fetched {
        bundles: Vec<FetchedBundle>,
    }

    #[derive(Deserialize)]
    struct FetchedBundle {
        machine: String,
        number: u32,
        path: PathBuf,
    }

    let into = tempfile::tempdir()?;
    let result = fetch(json!({
        "op": "timeline-fetch",
        "project": root.to_string_lossy(),
        "into": into.path().canonicalize()?,
    }))
    .map_err(anyhow::Error::msg)?;
    let Fetched { mut bundles } = serde_json::from_value(result)?;
    bundles.sort_by(|a, b| a.machine.cmp(&b.machine).then(a.number.cmp(&b.number)));
    let bundles: Vec<_> = bundles.into_iter().map(|b| (b.machine, b.path)).collect();
    timeline::rebuild(root, &bundles)?;
    Ok(())
}

/// The verified book copy is already in place. Any history failure is a separate notice.
pub(super) fn import_history(
    root: &Path,
    fetch: impl FnOnce(Value) -> Result<Value, String>,
) -> Option<String> {
    let lock = book_lock(root);
    let _book = lock.lock().unwrap_or_else(|e| e.into_inner());
    let error = rebuild_from_drive(root, fetch).err()?;
    Some(match timeline::rebuild(root, &[]) {
        Ok(_) => {
            format!("Drive history could not be brought in ({error:#}). Started a fresh timeline.")
        }
        Err(fresh) => {
            format!("Drive history import failed ({error:#}). A fresh timeline failed ({fresh:#}).")
        }
    })
}

struct Worker<U> {
    root: PathBuf,
    uploader: U,
    ready: bool,
    status: Arc<AtomicBool>,
    failed: mpsc::SyncSender<String>,
}

impl<U: FnMut(Value) -> Result<Value, String>> Worker<U> {
    fn enable(&mut self) -> Result<()> {
        // Check existing outside repositories before enable can allocate a new id
        // for an unreadable one.
        if !self.root.join(".git").exists()
            && sessions::outside_dir(&self.root).is_some_and(|repo| repo.exists())
            && timeline::is_damaged(&self.root)
        {
            self.rebuild_damaged()?;
        } else {
            timeline::enable(&self.root)?;
            if timeline::is_damaged(&self.root) {
                self.rebuild_damaged()?;
            }
        }
        self.ready = true;
        Ok(())
    }

    fn rebuild_damaged(&mut self) -> Result<()> {
        if super::backup::sync_enabled(&self.root) {
            rebuild_from_drive(&self.root, &mut self.uploader)
        } else {
            timeline::rebuild(&self.root, &[])?;
            Ok(())
        }
    }

    fn record(&mut self, entry: &Entry) -> bool {
        let result = (|| {
            if !self.ready {
                self.enable()?;
            }
            timeline::record(&self.root, entry)?;
            Ok(())
        })();
        self.report(result)
    }

    fn report(&self, result: Result<()>) -> bool {
        let ok = result.is_ok();
        self.status.store(ok, Ordering::Release);
        if let Err(error) = result {
            let _ = self.failed.try_send(format!("{error:#}"));
        }
        ok
    }

    fn upload(&mut self) -> Result<()> {
        if !self.ready || !super::backup::sync_enabled(&self.root) {
            return Ok(());
        }
        let project = self.root.to_string_lossy();
        let machine = timeline::machine_name();
        let listed = (self.uploader)(json!({ "op": "timeline-list", "project": project }))
            .map_err(anyhow::Error::msg)?;
        let machines: HashMap<String, Vec<u32>> =
            serde_json::from_value(listed["machines"].clone())?;
        let on_drive = machines
            .get(&machine)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let out = tempfile::tempdir()?;
        if let Some(bundle) = timeline::next_bundle(&self.root, out.path(), on_drive, Local::now())?
        {
            (self.uploader)(json!({
                "op": "timeline-upload",
                "project": project,
                "machine": machine,
                "bundle": bundle.path,
                "name": bundle.name,
            }))
            .map_err(anyhow::Error::msg)?;
            timeline::mark_uploaded(&self.root, &bundle)?;
        }
        Ok(())
    }
}

impl super::App {
    pub(super) fn end_timeline_session(&mut self, now: DateTime<Local>) {
        if let Some(timeline) = &mut self.timeline {
            timeline.end_session(now);
            timeline.last_session_end = self.last_edit;
            timeline.next_save = None;
            self.pre_snapshotted.clear();
        }
    }

    pub(super) fn tick_timeline(&mut self, now: Instant, wall: DateTime<Local>) {
        self.tick_history();
        let Some(timeline) = &mut self.timeline else {
            return;
        };
        if let Some(notice) = timeline.poll() {
            self.deck.shell.notices.push(&notice);
        }
        let idle = self.last_edit.is_some_and(|edit| {
            Some(edit) != timeline.last_session_end && now.duration_since(edit) >= IDLE_SESSION
        }) && self.project.dirty_count() == 0;
        let upload_due = now >= timeline.next_upload;
        if upload_due {
            timeline.next_upload = now + UPLOAD_EVERY;
        }
        if idle {
            self.end_timeline_session(wall);
        } else if upload_due {
            timeline.upload();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Setup};
    use grimoire_core::project::{self, Project};
    use std::fs;

    const TEST_WAIT: Duration = Duration::from_secs(10);
    const TEST_DIR: &str = "GRIMOIRE_TIMELINE_TEST_DIR";

    fn with_book(test: &str, run: impl FnOnce(&Path)) {
        // paths::set_data_dir is process-wide. Give each worker test its own
        // process so other app tests cannot redirect it mid-record.
        if let Some(base) = std::env::var_os(TEST_DIR) {
            let base = PathBuf::from(base);
            grimoire_core::paths::set_data_dir(base.join("data"));
            let root = base.join("book");
            fs::create_dir(&root).unwrap();
            project::scaffold(&root).unwrap();
            fs::create_dir_all(root.join(".grimoire")).unwrap();
            fs::write(root.join(".grimoire/progress.toml"), "").unwrap();
            run(&root);
        } else if sessions::git_available() {
            let base = tempfile::tempdir().unwrap();
            let (_, module) = module_path!().split_once("::").unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &format!("{module}::{test}"), "--nocapture"])
                .env(TEST_DIR, base.path())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(
                result.status.success() && stdout.contains("1 passed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    fn open_book(root: &Path) -> App {
        App::with(
            Project::load(root).unwrap(),
            Setup {
                music: Default::default(),
                theme: crate::theme::default_theme(),
                settings: Default::default(),
                background: false,
            },
        )
        .unwrap()
    }

    fn no_upload(_: Value) -> Result<Value, String> {
        panic!("an unlinked book must never call the uploader");
    }

    fn block_upload(root: &Path) -> (Timeline, mpsc::Sender<()>) {
        fs::write(root.join(".grimoire/sync.json"), "{}").unwrap();
        let (entered, uploading) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let timeline = Timeline::start_with(root.to_path_buf(), move |request| {
            assert_eq!(request["op"], "timeline-list");
            entered.send(()).unwrap();
            blocked.recv_timeout(TEST_WAIT).unwrap();
            Err("Drive unavailable".into())
        });
        timeline.upload();
        uploading.recv_timeout(TEST_WAIT).unwrap();
        (timeline, release)
    }

    fn drive_bundles(root: &Path, out: &Path) -> [timeline::Bundle; 2] {
        let scene = root.join("draft.md");
        let now = Local::now();
        fs::write(&scene, "The first draft.\n").unwrap();
        timeline::enable(root).unwrap();
        let first = timeline::next_bundle(root, out, &[], now).unwrap().unwrap();
        timeline::mark_uploaded(root, &first).unwrap();
        fs::write(&scene, "The second draft.\n").unwrap();
        timeline::record(root, &Entry::ChangedOnDrive(vec!["draft.md".into()])).unwrap();
        let second = timeline::next_bundle(root, out, &[1], now)
            .unwrap()
            .unwrap();
        assert!(!second.full);
        [first, second]
    }

    fn fake_fetch(
        project: PathBuf,
        bundles: [timeline::Bundle; 2],
    ) -> impl FnMut(Value) -> Result<Value, String> {
        move |request| {
            assert_eq!(request["op"], "timeline-fetch");
            assert_eq!(request["project"].as_str(), project.to_str());
            let into = Path::new(request["into"].as_str().unwrap());
            assert!(into.is_dir());
            let mut fetched = Vec::new();
            for bundle in bundles.iter().rev() {
                let path = into.join(&bundle.name);
                fs::copy(&bundle.path, &path).unwrap();
                fetched.push(json!({
                    "machine": "writer-abcd",
                    "number": bundle.number,
                    "path": path,
                }));
            }
            Ok(json!({"bundles": fetched}))
        }
    }

    #[test]
    fn scene_history_reads_on_the_worker() {
        with_book("scene_history_reads_on_the_worker", |root| {
            let scene = root.join("draft.md");
            fs::write(&scene, "Some recorded words.").unwrap();
            let (worker, release) = block_upload(root);
            let versions = worker.scene_versions(scene);
            assert!(matches!(
                versions.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            release.send(()).unwrap();
            let versions = versions.recv_timeout(TEST_WAIT).unwrap().unwrap();
            assert_eq!(versions.len(), 1);
            assert_eq!(versions[0].text, "Some recorded words.");
            worker.finish();
        });
    }

    #[test]
    fn history_worker_falls_back_after_a_failed_record() {
        with_book("history_worker_falls_back_after_a_failed_record", |root| {
            let scene = root.join("draft.md");
            fs::write(&scene, "Words before the failure.").unwrap();
            let worker = Timeline::start_with(root.to_path_buf(), no_upload);
            assert!(worker.record_now(Entry::Save, TEST_WAIT));
            let repo = sessions::outside_dir(root).unwrap();
            fs::write(repo.join("index.lock"), "").unwrap();
            fs::write(&scene, "An unrecorded edit.").unwrap();
            assert!(!worker.record_now(Entry::Save, TEST_WAIT));
            // The repository is readable even though a record failed.
            assert_eq!(timeline::scene_versions(root, &scene, 1).unwrap().len(), 1);
            let versions = worker.scene_versions(scene);
            assert!(versions.recv_timeout(TEST_WAIT).unwrap().is_none());
            worker.finish();
        });
    }

    #[test]
    fn import_brings_back_every_version() {
        with_book("import_brings_back_every_version", |root| {
            let out = tempfile::tempdir().unwrap();
            let bundles = drive_bundles(root, out.path());
            let imported = root.with_file_name("imported");
            fs::create_dir(&imported).unwrap();
            project::scaffold(&imported).unwrap();
            let scene = imported.join("draft.md");
            fs::write(&scene, "The imported draft.\n").unwrap();
            fs::create_dir_all(imported.join(".grimoire")).unwrap();
            fs::write(imported.join(".grimoire/sync.json"), "{}").unwrap();

            assert!(import_history(&imported, fake_fetch(imported.clone(), bundles)).is_none());
            let worker = Timeline::start_with(imported.clone(), no_upload);
            let versions = worker
                .scene_versions(scene.clone())
                .recv_timeout(TEST_WAIT)
                .unwrap()
                .unwrap();
            assert_eq!(
                versions.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(),
                [
                    "The imported draft.\n",
                    "The second draft.\n",
                    "The first draft.\n"
                ]
            );
            assert_eq!(versions[0].label, "Brought in from Drive");
            assert_eq!(fs::read_to_string(scene).unwrap(), "The imported draft.\n");
            worker.finish();
        });
    }

    #[test]
    fn failed_import_history_starts_fresh_and_keeps_the_book() {
        with_book(
            "failed_import_history_starts_fresh_and_keeps_the_book",
            |root| {
                let out = tempfile::tempdir().unwrap();
                drive_bundles(root, out.path());
                let notice = import_history(root, |_| Err("Drive is offline".into())).unwrap();
                assert!(notice.contains("Drive is offline"));
                let scene = root.join("draft.md");
                let versions = timeline::scene_versions(root, &scene, 100).unwrap();
                assert_eq!(versions.len(), 1);
                assert_eq!(versions[0].label, "Timeline begins");
                assert_eq!(fs::read_to_string(scene).unwrap(), "The second draft.\n");
            },
        );
    }

    #[test]
    fn damaged_repository_rebuilds_from_drive_bundles() {
        with_book("damaged_repository_rebuilds_from_drive_bundles", |root| {
            let out = tempfile::tempdir().unwrap();
            let bundles = drive_bundles(root, out.path());
            fs::write(root.join(".grimoire/sync.json"), "{}").unwrap();
            let repo = sessions::outside_dir(root).unwrap();
            fs::write(repo.join("HEAD"), "broken\n").unwrap();
            assert!(timeline::is_damaged(root));

            let worker = Timeline::start_with(root.to_path_buf(), fake_fetch(root.into(), bundles));
            let scene = root.join("draft.md");
            let versions = worker
                .scene_versions(scene)
                .recv_timeout(TEST_WAIT)
                .unwrap()
                .unwrap();
            assert_eq!(
                versions.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(),
                ["The second draft.\n", "The first draft.\n"]
            );
            assert!(worker.last_record_ok());
            worker.finish();
        });
    }

    #[test]
    fn records_run_off_the_calling_thread() {
        with_book("records_run_off_the_calling_thread", |root| {
            let (timeline, release) = block_upload(root);
            let scene = root.join("draft.md");
            fs::write(&scene, "A new draft.").unwrap();
            let started = Instant::now();
            timeline.record(Entry::Save);
            let elapsed = started.elapsed();
            release.send(()).unwrap();
            timeline.finish();
            assert!(
                elapsed < Duration::from_millis(5),
                "record took {elapsed:?}"
            );
            assert_eq!(timeline::scene_versions(root, &scene, 1).unwrap().len(), 1);
        });
    }

    #[test]
    fn before_restore_is_recorded_before_the_restored_text() {
        with_book(
            "before_restore_is_recorded_before_the_restored_text",
            |root| {
                let mut app = open_book(root);
                let scene = app.project.nodes[app.open.unwrap()].path.clone();
                let title = app.project.nodes[app.open.unwrap()].title.clone();
                let timeline = Timeline::start_with(root.to_path_buf(), no_upload);
                assert!(timeline.record_now(Entry::Save, TEST_WAIT));
                app.timeline = Some(timeline);

                app.editor.set_text("The current draft.");
                app.restore_version(&scene, "The first restored draft.");
                app.restore_version(&scene, "The second restored draft.");
                assert!(app.commit_saves());
                app.timeline.take().unwrap().finish();

                let versions = timeline::scene_versions(root, &scene, 10).unwrap();
                assert!(versions[0].text.contains("The second restored draft."));
                assert!(versions[1].text.contains("The first restored draft."));
                assert!(versions[2].text.contains("The current draft."));
                let label = format!("Before restoring {title}");
                assert_eq!(versions[1].label, label);
                assert_eq!(versions[2].label, label);
            },
        );
    }

    #[test]
    fn record_now_times_out_and_reports_false() {
        with_book("record_now_times_out_and_reports_false", |root| {
            let (timeline, release) = block_upload(root);
            assert!(!timeline.record_now(
                Entry::BeforeRestoring("Draft".into()),
                Duration::from_millis(10),
            ));
            let scene = root.join("draft.md");
            fs::write(&scene, "Already restored.").unwrap();
            release.send(()).unwrap();
            timeline.finish();
            assert!(
                timeline::scene_versions(root, &scene, 1)
                    .unwrap()
                    .is_empty()
            );
        });
    }

    #[test]
    fn closing_the_app_marks_the_session_but_never_waits_on_uploads() {
        with_book(
            "closing_the_app_marks_the_session_but_never_waits_on_uploads",
            |root| {
                fs::write(root.join(".grimoire/sync.json"), "{}").unwrap();
                fs::write(root.join("draft.md"), "Words before closing.").unwrap();
                // Hold the book's worker lock so the jobs below are still queued when finish() runs.
                let lock = book_lock(root);
                let held = lock.lock().unwrap();
                let (called, calls) = mpsc::channel();
                let timeline = Timeline::start_with(root.to_path_buf(), move |request| {
                    called.send(request["op"].to_string()).unwrap();
                    Err("no network in this test".into())
                });
                timeline.end_session(Local::now());
                timeline.upload();
                let finished = thread::spawn(move || timeline.finish());
                thread::sleep(Duration::from_millis(200));
                drop(held);
                finished.join().unwrap();
                assert!(calls.try_recv().is_err(), "an upload ran while closing");
                let versions = timeline::scene_versions(root, &root.join("draft.md"), 1).unwrap();
                assert!(versions[0].session.is_some(), "the session was not marked");
            },
        );
    }

    #[test]
    fn end_session_marks_and_uploads() {
        with_book("end_session_marks_and_uploads", |root| {
            fs::write(root.join(".grimoire/sync.json"), "{}").unwrap();
            let scene = root.join("draft.md");
            let (uploaded, upload) = mpsc::channel();
            let project = root.to_path_buf();
            let timeline = Timeline::start_with(root.to_path_buf(), move |request| {
                assert_eq!(request["project"].as_str(), project.to_str());
                if request["op"] == "timeline-list" {
                    return Ok(json!({"machines": {}}));
                }
                assert_eq!(request["op"], "timeline-upload");
                assert!(Path::new(request["bundle"].as_str().unwrap()).is_file());
                assert_eq!(
                    request["machine"].as_str(),
                    Some(timeline::machine_name().as_str())
                );
                let name = request["name"].as_str().unwrap();
                uploaded.send(name.to_owned()).unwrap();
                Ok(json!({"uploaded": name}))
            });
            assert!(timeline.record_now(Entry::Save, TEST_WAIT));
            fs::write(&scene, "Words from this session.").unwrap();
            let now = Local::now();
            timeline.end_session(now);
            assert!(
                upload
                    .recv_timeout(TEST_WAIT)
                    .unwrap()
                    .starts_with("000001 ")
            );
            timeline.finish();

            let versions = timeline::scene_versions(root, &scene, 1).unwrap();
            assert!(versions[0].session.is_some());
            let out = tempfile::tempdir().unwrap();
            assert!(
                timeline::next_bundle(root, out.path(), &[1], now)
                    .unwrap()
                    .is_none()
            );
        });
    }

    #[test]
    fn one_notice_per_run_on_failure() {
        with_book("one_notice_per_run_on_failure", |root| {
            let mut timeline = Timeline::start_with(root.join("missing"), no_upload);
            assert!(!timeline.record_now(Entry::Save, TEST_WAIT));
            assert!(!timeline.last_record_ok());
            let notice = timeline.poll().unwrap();
            assert!(notice.starts_with("The book's timeline isn't recording ("));
            assert!(notice.ends_with("scene history keeps plain copies."));
            assert!(!timeline.record_now(Entry::Save, TEST_WAIT));
            assert!(timeline.poll().is_none());
            timeline.finish();
        });
    }

    #[test]
    fn idle_half_hour_ends_a_session() {
        with_book("idle_half_hour_ends_a_session", |root| {
            let mut app = open_book(root);
            let scene = app.project.nodes[app.open.unwrap()].path.clone();
            fs::write(root.join(".grimoire/sync.json"), "{}").unwrap();
            let (uploaded, uploads) = mpsc::channel();
            let timeline = Timeline::start_with(root.to_path_buf(), move |request| {
                assert_eq!(request["op"], "timeline-list");
                uploaded.send(()).unwrap();
                Err("Drive unavailable".into())
            });
            assert!(timeline.record_now(Entry::Save, TEST_WAIT));
            app.timeline = Some(timeline);
            app.editor.set_text("Words before the pause.");
            assert!(app.commit_saves());
            let edit = app.last_edit.unwrap();
            let wall = Local::now();

            app.tick_timeline(edit + IDLE_SESSION - Duration::from_secs(1), wall);
            assert!(app.timeline.as_ref().unwrap().last_session_end.is_none());
            app.tick_timeline(edit + IDLE_SESSION, wall);
            assert_eq!(app.timeline.as_ref().unwrap().last_session_end, Some(edit));
            assert!(app.pre_snapshotted.is_empty());
            uploads.recv_timeout(TEST_WAIT).unwrap();
            app.tick_timeline(edit + IDLE_SESSION + Duration::from_secs(1), wall);
            app.timeline.take().unwrap().finish();
            assert_eq!(uploads.try_iter().count(), 0, "one session end, one upload");
            let versions = timeline::scene_versions(root, &scene, 1).unwrap();
            assert!(versions[0].session.is_some());
        });
    }
}
