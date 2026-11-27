//! Deliver yt-dlp audio to mpv through a FIFO.

use super::ytdlp_reason;
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

/// One song's audio from yt-dlp into a FIFO that mpv plays as it arrives (rc22). mpv sees a
/// stream that breaks off as an ordinary end of file, so how it ended is yt-dlp's exit status.
pub(super) struct Stream {
    pub(super) id: String,
    pub(super) fifo: PathBuf,
    outcome: Receiver<Result<(), String>>,
    done: Option<Result<(), String>>,
    cancel: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
    // Last: the FIFO's folder goes once the worker has stopped writing into it.
    _dir: tempfile::TempDir,
}

impl Stream {
    pub(super) fn start(ytdlp: &Path, id: &str, url: String) -> Result<Self> {
        use std::os::unix::ffi::OsStrExt;

        let dir = tempfile::tempdir().context("making a place for the stream")?;
        let fifo = dir.path().join("audio");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes())?;
        // SAFETY: a valid NUL-terminated path; mkfifo touches nothing else.
        if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error()).context("making the stream's pipe");
        }
        let (send, outcome) = mpsc::channel();
        let (cancel, cancelled) = mpsc::channel();
        let (ytdlp, path, stderr) = (ytdlp.to_path_buf(), fifo.clone(), dir.path().join(".err"));
        let worker = thread::spawn(move || {
            let result = Self::run(&ytdlp, &url, &path, &stderr, &cancelled);
            let _ = send.send(result);
        });
        Ok(Self {
            id: id.to_string(),
            fifo,
            outcome,
            done: None,
            cancel: Some(cancel),
            worker: Some(worker),
            _dir: dir,
        })
    }

    /// Wait for mpv to open the pipe (never blocking past a cancel), then run yt-dlp into it.
    /// `Err("")` means cancelled, which is not worth a note.
    fn run(
        ytdlp: &Path,
        url: &str,
        fifo: &Path,
        stderr: &Path,
        cancel: &Receiver<()>,
    ) -> Result<(), String> {
        use std::os::unix::fs::OpenOptionsExt;

        let cancelled =
            |wait: Duration| cancel.recv_timeout(wait) != Err(RecvTimeoutError::Timeout);
        let pipe = loop {
            // Opening a FIFO to write fails with ENXIO until a reader has it open.
            match fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(fifo)
            {
                Ok(pipe) => break pipe,
                Err(e) if e.raw_os_error() == Some(libc::ENXIO) => {
                    if cancelled(Duration::from_millis(50)) {
                        return Err(String::new());
                    }
                }
                Err(e) => return Err(format!("the stream's pipe: {e}")),
            }
        };
        {
            use std::os::fd::AsRawFd;
            let fd = pipe.as_raw_fd();
            // SAFETY: fd is open and owned by `pipe`; yt-dlp must block when mpv is behind.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            }
        }
        let errors = fs::File::create(stderr).map_err(|e| e.to_string())?;
        let mut child = Command::new(ytdlp)
            .args(["--ignore-config", "-q", "--no-warnings", "--no-playlist"])
            .args(["-f", "bestaudio", "--no-part", "-o", "-"])
            .arg(url)
            .stdin(Stdio::null())
            .stdout(pipe)
            .stderr(errors)
            .spawn()
            .map_err(|e| format!("couldn't start yt-dlp: {e}"))?;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(ytdlp_reason(stderr, status)),
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
            if cancelled(Duration::from_millis(100)) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(String::new());
            }
        }
    }

    /// How yt-dlp ended, waiting up to `wait` for it: mpv's end of file and yt-dlp's exit
    /// are the same moment seen from two processes.
    pub(super) fn outcome(&mut self, wait: Duration) -> Option<&Result<(), String>> {
        if self.done.is_none() {
            self.done = self.outcome.recv_timeout(wait).ok();
        }
        self.done.as_ref()
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        drop(self.cancel.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
