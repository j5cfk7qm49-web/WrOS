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
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub configured: bool,
    pub repository: String,
    pub recovery_key: String,
    pub google_connected: bool,
    pub snapshots: Vec<Snapshot>,
    pub notice: String,
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

pub struct Job {
    pub result: Receiver<Result<Status, String>>,
    cancelled: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub fn spawn(request: Request, state: std::path::PathBuf) -> Result<Job, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (sender, result) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("wros-backup".into())
        .spawn(move || {
            let outcome = execute_in(&request, &worker_cancelled, Some(state));
            let _ = sender.send(outcome);
        })
        .map_err(|_| "Backup worker unavailable".to_owned())?;
    Ok(Job {
        result,
        cancelled,
        thread: Some(thread),
    })
}

#[cfg(unix)]
fn terminate(child: &mut std::process::Child) {
    // Only the fresh process group created below, never a caller-supplied PID.
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
    execute_in(request, cancelled, None)
}
fn execute_in(
    request: &Request,
    cancelled: &AtomicBool,
    state: Option<std::path::PathBuf>,
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
    let mut command = Command::new("/usr/bin/python3");
    if let Some(state) = state {
        command.env("WROS_BACKUP_STATE_DIR", state);
    }
    command
        .args(["-I"])
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| unavailable())?;
    if child
        .stdin
        .take()
        .ok_or_else(unavailable)?
        .write_all(&input)
        .is_err()
    {
        terminate(&mut child);
        return Err(unavailable());
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
            return Err("Backup cancelled; no completed restore or verification claimed. Refresh snapshots before retrying.".into());
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
    let response: Response = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if response.ok {
        response.data.ok_or_else(unavailable)
    } else {
        Err(response.error.unwrap_or_else(unavailable))
    }
}
