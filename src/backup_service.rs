//! Private, cancellable JSON adapter to the unprivileged Restic integration.
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct Secret(pub Zeroizing<String>);
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_str().serialize(serializer)
    }
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum Request {
    Status,
    List,
    Unlock,
    Init {
        repository: String,
    },
    Connect {
        repository: String,
        key: String,
    },
    Backup {
        project: String,
    },
    Queue {
        project: String,
    },
    RunQueue,
    ClearQueue,
    Restore {
        snapshot: String,
        destination: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    ExportKey {
        destination: String,
    },
    ConnectGoogle {
        authorization: Secret,
    },
    /// The same authorization JSON, read by the helper from a file (a console has no clipboard).
    ConnectGoogleFile {
        path: String,
    },
    CheckGoogle,
    DisconnectGoogle,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub configured: bool,
    pub repository: String,
    pub recovery_key: String,
    pub google_connected: bool,
    #[serde(default)]
    pub google_state: String,
    #[serde(default)]
    pub google_verified_at: String,
    #[serde(default)]
    pub google_expiry: String,
    #[serde(default)]
    pub queued_backups: Vec<QueuedBackup>,
    pub snapshots: Vec<Snapshot>,
    pub notice: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedBackup {
    pub id: String,
    pub project: String,
    pub label: String,
    pub queued_at: String,
    pub attempts: u32,
    pub last_error: String,
    pub inventory_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub time: String,
    #[serde(rename = "path")]
    pub _path: String,
    pub project: String,
    pub restore_checked_at: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    ok: bool,
    data: Option<Status>,
    error: Option<String>,
}

/// The step a running operation reported last, and how far restic got.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub phase: String,
    #[serde(default)]
    pub percent: Option<f64>,
}

/// A helper stderr line, if it is a well-formed progress report. Anything
/// else (a traceback, tool noise) is never shown.
fn parse_progress(line: &str) -> Option<Progress> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Line {
        progress: Progress,
    }
    let Line { progress } = serde_json::from_str(line).ok()?;
    let fair = !progress.phase.is_empty()
        && progress.phase.chars().count() <= 80
        && !progress.phase.chars().any(char::is_control)
        && progress
            .percent
            .is_none_or(|p| p.is_finite() && (0.0..=1.0).contains(&p));
    fair.then_some(progress)
}

fn describe(progress: Option<&Progress>, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let time = if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min {:02} s", seconds / 60, seconds % 60)
    };
    match progress {
        Some(Progress {
            phase,
            percent: Some(p),
        }) => format!("Working: {phase} · {:.0}% · {time}", p * 100.0),
        Some(Progress { phase, .. }) => format!("Working: {phase} · {time}"),
        None => format!("Working: starting · {time}"),
    }
}

pub(crate) type Shared = Arc<std::sync::Mutex<Option<Progress>>>;

pub struct Job<T = Status> {
    pub result: Receiver<Result<T, String>>,
    cancelled: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    progress: Shared,
    started: Instant,
}
impl<T> Job<T> {
    /// What the operation is doing now, for the Backups screen.
    pub fn status_line(&self) -> String {
        let progress = self.progress.lock().ok().and_then(|p| p.clone());
        describe(progress.as_ref(), self.started.elapsed())
    }
}
impl Job {
    #[cfg(test)]
    pub fn running_for_test(progress: Option<Progress>) -> Self {
        let (sender, result) = mpsc::sync_channel(1);
        std::mem::forget(sender);
        Self {
            result,
            cancelled: Arc::default(),
            thread: None,
            progress: Arc::new(std::sync::Mutex::new(progress)),
            started: Instant::now(),
        }
    }
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub fn spawn(request: Request, state: std::path::PathBuf) -> Result<Job, String> {
    spawn_work("wros-backup", move |cancelled, progress| {
        execute_in(&request, cancelled, Some(state), Some(progress))
    })
}

/// Run `work` on a worker thread of its own, with cancellation and progress.
pub(crate) fn spawn_work<T: Send + 'static>(
    name: &str,
    work: impl FnOnce(&AtomicBool, Shared) -> Result<T, String> + Send + 'static,
) -> Result<Job<T>, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (sender, result) = mpsc::sync_channel(1);
    let progress = Shared::default();
    let worker_progress = progress.clone();
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = sender.send(work(&worker_cancelled, worker_progress));
        })
        .map_err(|_| format!("{name} worker unavailable"))?;
    Ok(Job {
        result,
        cancelled,
        thread: Some(thread),
        progress,
        started: Instant::now(),
    })
}

#[cfg(unix)]
fn terminate(child: &mut std::process::Child) {
    // Only the fresh session's process group created below, never a caller-supplied PID.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGTERM);
    }
    for _ in 0..20 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
}
#[cfg(not(unix))]
fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn execute(request: &Request, cancelled: &AtomicBool) -> Result<Status, String> {
    execute_in(request, cancelled, None, None)
}
fn execute_in(
    request: &Request,
    cancelled: &AtomicBool,
    state: Option<std::path::PathBuf>,
    progress: Option<Shared>,
) -> Result<Status, String> {
    let unavailable =
        || "Backup service unavailable; install the WrOS Restic/rclone integration.".to_owned();
    let installed = std::path::Path::new("/usr/libexec/wros-backup");
    let script = if installed.is_file() {
        installed.to_path_buf()
    } else if cfg!(debug_assertions) {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("os/scripts/wros-backup")
    } else {
        return Err(unavailable());
    };
    let input = Zeroizing::new(serde_json::to_vec(request).map_err(|_| unavailable())?);
    let env: Vec<(&str, std::path::PathBuf)> = state
        .map(|state| ("WROS_BACKUP_STATE_DIR", state))
        .into_iter()
        .collect();
    let bytes = run_helper(
        &script,
        &input,
        &env,
        cancelled,
        progress,
        &unavailable(),
        "Backup cancelled; no completed restore or verification claimed. Refresh snapshots before retrying.",
    )?;
    let response: Response = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if response.ok {
        response.data.ok_or_else(unavailable)
    } else {
        Err(response.error.unwrap_or_else(unavailable))
    }
}

/// Run a WrOS JSON helper with `input` on stdin: in a session of its own with no
/// terminal, progress lines read from stderr, cancellable. Returns its stdout.
pub(crate) fn run_helper(
    script: &std::path::Path,
    input: &[u8],
    env: &[(&str, std::path::PathBuf)],
    cancelled: &AtomicBool,
    progress: Option<Shared>,
    unavailable: &str,
    cancelled_message: &str,
) -> Result<Vec<u8>, String> {
    let unavailable = || unavailable.to_owned();
    let mut command = Command::new("/usr/bin/python3");
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .args(["-I"])
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // A session of its own with no controlling terminal. Restic gives the
        // terminal's foreground to the rclone it starts for Drive; on the
        // writing console the editor's input reader then failed on the next
        // key and the login session ended. As session leader its PID is also
        // the process group `terminate` signals.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(|_| unavailable())?;
    if child
        .stdin
        .take()
        .ok_or_else(unavailable)?
        .write_all(input)
        .is_err()
    {
        terminate(&mut child);
        return Err(unavailable());
    }
    // Progress lines only; the stream is drained so the helper never blocks.
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stderr.take(16 * 1024 * 1024))
                .split(b'\n')
                .map_while(Result::ok)
            {
                if let (Some(shared), Some(step)) = (
                    &progress,
                    std::str::from_utf8(&line).ok().and_then(parse_progress),
                ) && let Ok(mut current) = shared.lock()
                {
                    *current = Some(step);
                }
            }
        });
    }
    let stdout = child.stdout.take().ok_or_else(unavailable)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) || start.elapsed() >= Duration::from_secs(1900) {
            terminate(&mut child);
            let _ = reader.join();
            return Err(cancelled_message.into());
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                terminate(&mut child);
                return Err(unavailable());
            }
        }
    }
    let bytes = reader
        .join()
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(unavailable());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_well_formed_progress_lines_are_shown() {
        assert_eq!(
            parse_progress(
                r#"{"progress": {"phase": "Encrypting and saving the snapshot", "percent": 0.4213}}"#
            ),
            Some(Progress {
                phase: "Encrypting and saving the snapshot".into(),
                percent: Some(0.4213)
            })
        );
        assert_eq!(
            parse_progress(r#"{"progress": {"phase": "Comparing every restored file"}}"#)
                .unwrap()
                .percent,
            None
        );
        let long = format!(r#"{{"progress": {{"phase": "{}"}}}}"#, "a".repeat(200));
        for line in [
            "Traceback (most recent call last):",
            r#"{"ok": true}"#,
            r#"{"progress": {"phase": ""}}"#,
            r#"{"progress": {"phase": "x\u001b[2J"}}"#,
            r#"{"progress": {"phase": "x", "percent": 1.5}}"#,
            r#"{"progress": {"phase": "x", "file": "chapter.md"}}"#,
            &long,
        ] {
            assert_eq!(parse_progress(line), None, "{line}");
        }
    }

    #[test]
    fn progress_reads_as_a_step_a_percentage_and_the_time_taken() {
        let step = Progress {
            phase: "Encrypting and saving the snapshot".into(),
            percent: Some(0.42),
        };
        assert_eq!(
            describe(Some(&step), Duration::from_secs(65)),
            "Working: Encrypting and saving the snapshot · 42% · 1 min 05 s"
        );
        assert_eq!(
            describe(None, Duration::from_secs(7)),
            "Working: starting · 7 s"
        );
    }
}
