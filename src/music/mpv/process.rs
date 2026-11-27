//! Attach to or launch mpv and manage the child process.

use super::Mpv;
use anyhow::{Result, anyhow};

#[cfg(unix)]
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufReader, ErrorKind},
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

impl Mpv {
    pub(in crate::music) fn new() -> Self {
        Self {
            #[cfg(unix)]
            socket: std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .map(|dir| dir.join("grimoire-mpv.sock"))
                .unwrap_or_else(|| {
                    let uid = unsafe { libc::geteuid() };
                    std::env::temp_dir().join(format!("grimoire-mpv-{uid}.sock"))
                }),
            #[cfg(unix)]
            stream: None,
            #[cfg(unix)]
            child: None,
            #[cfg(unix)]
            request_id: 0,
        }
    }

    /// Polling can attach to an existing player without launching one.
    #[cfg(unix)]
    pub(in crate::music) fn connect_existing(&mut self) -> Result<bool> {
        if self.stream.is_some() {
            return Ok(true);
        }
        match UnixStream::connect(&self.socket) {
            Ok(stream) => {
                let timeout = Some(Duration::from_secs(1));
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)?;
                self.stream = Some(BufReader::new(stream));
                Ok(true)
            }
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(unix))]
    pub(in crate::music) fn connect_existing(&mut self) -> Result<bool> {
        Err(anyhow!("music files need mpv on Linux"))
    }

    #[cfg(unix)]
    pub(super) fn start(&mut self) -> Result<()> {
        use anyhow::{Context, bail};

        // Keep simultaneous Grimoire runs from both trying to bind the same socket.
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.socket.with_extension("lock"))?;
        lock.lock()?;
        if self.connect_existing()? {
            return Ok(());
        }
        match fs::symlink_metadata(&self.socket) {
            Ok(info) if info.file_type().is_socket() => fs::remove_file(&self.socket)?,
            Ok(_) => bail!("mpv socket path is occupied: {}", self.socket.display()),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        let mut ipc = OsString::from("--input-ipc-server=");
        ipc.push(&self.socket);
        self.stop_child();
        self.child = Some(
            Command::new("mpv")
                .args([
                    "--idle=yes",
                    "--no-video",
                    "--no-terminal",
                    "--really-quiet",
                ])
                .arg(ipc)
                .args(["--volume=100", "--volume-max=100"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("couldn't start mpv")?,
        );
        let result = self.wait_for_socket();
        if result.is_err() {
            self.stop_child();
        }
        result
    }

    #[cfg(unix)]
    fn wait_for_socket(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.connect_existing()? {
                return Ok(());
            }
            if let Some(child) = &mut self.child
                && let Some(status) = child.try_wait()?
            {
                return Err(anyhow!("mpv exited before opening its socket ({status})"));
            }
            if Instant::now() >= deadline {
                return Err(anyhow!("mpv did not open its socket"));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
impl Drop for Mpv {
    fn drop(&mut self) {
        self.stop_child();
    }
}
