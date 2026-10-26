//! One lazy local analyzer thread, at most two queued/current documents, and
//! a bounded result channel. Replaced or canceled generations are discarded
//! before publication. The UI must *also* validate its actual current source.
use super::{
    AnalysisOptions, Snapshot, SourceStamp, SourceUnit,
    harper::{HarperEngine, Report, ReportStatus},
};
use anyhow::{Result, bail};
use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
};

pub const MAX_DOCUMENTS: usize = 2;
struct Job {
    snapshot: Snapshot,
    options: AnalysisOptions,
}
#[derive(Clone)]
struct Expected {
    source: SourceStamp,
    configuration: String,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub submitted: u64,
    pub completed: u64,
    pub discarded: u64,
    pub panics: u64,
    pub queued: usize,
    pub active: bool,
}
#[derive(Default)]
struct Pending {
    jobs: VecDeque<Job>,
    expected: VecDeque<Expected>,
    closed: bool,
    stats: Stats,
}
#[derive(Debug, Clone, Copy)]
pub struct Submission {
    pub replaced: bool,
    pub evicted: bool,
}
type Shared = Arc<(Mutex<Pending>, Condvar)>;
pub struct Worker {
    shared: Shared,
    receiver: Receiver<Report>,
}
impl Worker {
    /// Merely starting the thread does not initialize Harper/model/dictionary.
    pub fn start() -> Result<Self> {
        Self::with_factory(|| {
            let mut engine = HarperEngine::default();
            move |snapshot, options| engine.analyze(snapshot, options)
        })
    }
    fn with_factory<F, A>(factory: F) -> Result<Self>
    where
        F: FnOnce() -> A + Send + 'static,
        A: FnMut(Snapshot, AnalysisOptions) -> Report + 'static,
    {
        let shared: Shared = Arc::default();
        let (sender, receiver) = mpsc::sync_channel(MAX_DOCUMENTS);
        let running = shared.clone();
        thread::Builder::new()
            .name("writing-checks".into())
            .spawn(move || {
                let mut analyzer = factory();
                loop {
                    let job = {
                        let (mutex, wake) = &*running;
                        let Ok(mut pending) = mutex.lock() else {
                            break;
                        };
                        while pending.jobs.is_empty() && !pending.closed {
                            let Ok(next) = wake.wait(pending) else {
                                return;
                            };
                            pending = next;
                        }
                        if pending.closed {
                            break;
                        }
                        let Some(job) = pending.jobs.pop_front() else {
                            continue;
                        };
                        pending.stats.queued = pending.jobs.len();
                        if !current(&pending, &job.snapshot.stamp, &job.options.digest()) {
                            pending.stats.discarded = pending.stats.discarded.saturating_add(1);
                            continue;
                        }
                        pending.stats.active = true;
                        job
                    };
                    let backup = (job.snapshot.clone(), job.options.clone());
                    let output =
                        catch_unwind(AssertUnwindSafe(|| analyzer(job.snapshot, job.options)));
                    let panicked = output.is_err();
                    let report = output.unwrap_or_else(|_| {
                        Report::empty(
                            backup.0,
                            backup.1,
                            ReportStatus::EngineFailed(
                                "local grammar engine failed; source untouched".into(),
                            ),
                        )
                    });
                    {
                        let Ok(mut pending) = running.0.lock() else {
                            break;
                        };
                        pending.stats.active = false;
                        if panicked {
                            pending.stats.panics = pending.stats.panics.saturating_add(1);
                        }
                        if pending.closed {
                            break;
                        }
                        if !current(&pending, &report.snapshot.stamp, &report.options.digest()) {
                            pending.stats.discarded = pending.stats.discarded.saturating_add(1);
                            continue;
                        }
                        pending.stats.completed = pending.stats.completed.saturating_add(1);
                    }
                    // Bounded backpressure is on this thread, never on the editor.
                    // Dropping the receiver releases a blocked send during exit.
                    if sender.send(report).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self { shared, receiver })
    }
    #[cfg(test)]
    pub(super) fn with_analyzer<A>(analyzer: A) -> Result<Self>
    where
        A: FnMut(Snapshot, AnalysisOptions) -> Report + Send + 'static,
    {
        Self::with_factory(move || analyzer)
    }
    pub fn submit(&self, snapshot: Snapshot, options: AnalysisOptions) -> Result<Submission> {
        options.validate()?;
        let (mutex, wake) = &*self.shared;
        let mut pending = mutex
            .lock()
            .map_err(|_| anyhow::anyhow!("local grammar queue unavailable"))?;
        if pending.closed {
            bail!("local grammar worker is closed");
        }
        let same = |source: &SourceStamp| {
            source.document == snapshot.stamp.document && source.unit == snapshot.stamp.unit
        };
        let replaced = pending.expected.iter().any(|e| same(&e.source));
        pending.expected.retain(|e| !same(&e.source));
        pending.jobs.retain(|j| !same(&j.snapshot.stamp));
        let evicted = pending.expected.len() >= MAX_DOCUMENTS;
        if evicted && let Some(old) = pending.expected.pop_front() {
            pending.jobs.retain(|j| {
                j.snapshot.stamp.document != old.source.document
                    || j.snapshot.stamp.unit != old.source.unit
            });
        }
        pending.expected.push_back(Expected {
            source: snapshot.stamp.clone(),
            configuration: options.digest(),
        });
        pending.jobs.push_back(Job { snapshot, options });
        pending.stats.queued = pending.jobs.len();
        pending.stats.submitted = pending.stats.submitted.saturating_add(1);
        wake.notify_one();
        Ok(Submission { replaced, evicted })
    }
    pub fn cancel(&self, document: &str, unit: SourceUnit) {
        if let Ok(mut pending) = self.shared.0.lock() {
            pending
                .jobs
                .retain(|j| j.snapshot.stamp.document != document || j.snapshot.stamp.unit != unit);
            pending
                .expected
                .retain(|e| e.source.document != document || e.source.unit != unit);
            pending.stats.queued = pending.jobs.len();
        }
    }
    pub fn clear(&self) {
        if let Ok(mut pending) = self.shared.0.lock() {
            pending.jobs.clear();
            pending.expected.clear();
            pending.stats.queued = 0;
        }
    }
    pub fn try_recv(&self) -> Result<Option<Report>> {
        match self.receiver.try_recv() {
            Ok(report) => Ok(Some(report)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                bail!("local grammar worker stopped; source untouched")
            }
        }
    }
    #[cfg(test)]
    pub(super) fn wait_for_test(&self) -> Option<Report> {
        self.receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .ok()
    }
    pub fn stats(&self) -> Stats {
        self.shared.0.lock().map(|s| s.stats).unwrap_or_default()
    }
}
fn current(pending: &Pending, stamp: &SourceStamp, configuration: &str) -> bool {
    pending
        .expected
        .iter()
        .any(|e| e.source == *stamp && e.configuration == configuration)
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.shared.0.lock() {
            pending.closed = true;
            pending.jobs.clear();
            pending.expected.clear();
            self.shared.1.notify_one();
        }
        // Deliberately no join in input/quit. An in-flight bounded analysis can
        // finish privately; it cannot save prose or publish after cancellation.
    }
}
