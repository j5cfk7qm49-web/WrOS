//! The background player for music files.

use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

#[cfg(unix)]
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) struct Mpv {
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(unix)]
    stream: Option<BufReader<UnixStream>>,
    #[cfg(unix)]
    child: Option<Child>,
    #[cfg(unix)]
    request_id: i64,
}

impl Mpv {
    pub(super) fn new() -> Self {
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
    pub(super) fn connect_existing(&mut self) -> Result<bool> {
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
    pub(super) fn connect_existing(&mut self) -> Result<bool> {
        Err(anyhow!("music files need mpv on Linux"))
    }

    #[cfg(unix)]
    fn start(&mut self) -> Result<()> {
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
                .args(["--idle=yes", "--no-video", "--no-terminal", "--really-quiet"])
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
        let deadline = Instant::now() + Duration::from_secs(1);
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

    #[cfg(unix)]
    fn reply(&mut self, args: &[Value]) -> Result<Value> {
        if !self.connect_existing()? {
            self.start()?;
        }
        self.request_id = self.request_id.wrapping_add(1);
        let result = exchange(self.stream.as_mut().unwrap(), args, self.request_id);
        if result.is_err() {
            self.stream = None;
        }
        result
    }

    #[cfg(not(unix))]
    fn reply(&mut self, _: &[Value]) -> Result<Value> {
        Err(anyhow!("music files need mpv on Linux"))
    }

    pub(super) fn command(&mut self, args: &[Value]) -> Result<Value> {
        reply_data(self.reply(args)?)
    }

    pub(super) fn property(&mut self, name: &str) -> Result<Option<Value>> {
        let reply = self.reply(&[json!("get_property"), json!(name)])?;
        match reply["error"].as_str() {
            Some("property unavailable" | "property not found") => Ok(None),
            _ => {
                let data = reply_data(reply)?;
                Ok((!data.is_null()).then_some(data))
            }
        }
    }

    pub(super) fn load(&mut self, path_or_url: &str, replace: bool) -> Result<()> {
        let action = if replace { "replace" } else { "append-play" };
        self.command(&[json!("loadfile"), json!(path_or_url), json!(action)])?;
        Ok(())
    }

    // A replacement load already clears the queue.
    #[allow(dead_code)]
    pub(super) fn playlist_clear(&mut self) -> Result<()> {
        self.command(&[json!("playlist-clear")])?;
        Ok(())
    }

    pub(super) fn play_index(&mut self, i: usize) -> Result<()> {
        self.command(&[json!("playlist-play-index"), json!(i)])?;
        self.set_property("pause", json!(false))
    }

    pub(super) fn toggle_pause(&mut self) -> Result<()> {
        self.command(&[json!("cycle"), json!("pause")])?;
        Ok(())
    }

    pub(super) fn next(&mut self) -> Result<()> {
        self.command(&[json!("playlist-next")])?;
        Ok(())
    }

    pub(super) fn prev(&mut self) -> Result<()> {
        self.command(&[json!("playlist-prev")])?;
        Ok(())
    }

    pub(super) fn seek(&mut self, secs: i32) -> Result<()> {
        self.command(&[json!("seek"), json!(secs), json!("relative")])?;
        Ok(())
    }

    pub(super) fn add_volume(&mut self, d: i32) -> Result<()> {
        self.command(&[json!("add"), json!("volume"), json!(d)])?;
        Ok(())
    }

    pub(super) fn set_property(&mut self, name: &str, value: Value) -> Result<()> {
        self.command(&[json!("set_property"), json!(name), value])?;
        Ok(())
    }

    pub(super) fn set_loop_playlist(&mut self, looping: bool) -> Result<()> {
        self.set_property("loop-playlist", json!(if looping { "inf" } else { "no" }))
    }

    pub(super) fn set_shuffle(&mut self, shuffled: bool) -> Result<()> {
        let command = if shuffled {
            "playlist-shuffle"
        } else {
            "playlist-unshuffle"
        };
        self.command(&[json!(command)])?;
        Ok(())
    }

    pub(super) fn snapshot(&mut self) -> Result<Value> {
        let mut snapshot = Map::new();
        for name in [
            "media-title",
            "metadata",
            "path",
            "time-pos",
            "duration",
            "pause",
            "idle-active",
            "playlist-pos",
            "volume",
            "mute",
            "loop-playlist",
        ] {
            if let Some(value) = self.property(name)? {
                snapshot.insert(name.to_string(), value);
            }
        }
        Ok(Value::Object(snapshot))
    }
}

#[cfg(unix)]
impl Drop for Mpv {
    fn drop(&mut self) {
        self.stop_child();
    }
}

#[cfg(unix)]
fn exchange(stream: &mut BufReader<UnixStream>, args: &[Value], id: i64) -> Result<Value> {
    let socket = stream.get_mut();
    serde_json::to_writer(&mut *socket, &json!({ "command": args, "request_id": id }))?;
    socket.write_all(b"\n")?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut line = String::new();
    loop {
        line.clear();
        if stream.read_line(&mut line)? == 0 {
            return Err(anyhow!("mpv closed its socket"));
        }
        let reply: Value = serde_json::from_str(&line)?;
        if reply.get("event").is_none() && reply["request_id"].as_i64() == Some(id) {
            return Ok(reply);
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("timed out waiting for mpv"));
        }
    }
}

fn reply_data(mut reply: Value) -> Result<Value> {
    match reply["error"].as_str() {
        Some("success") => Ok(reply.get_mut("data").map(Value::take).unwrap_or_default()),
        Some(error) => Err(anyhow!("mpv: {error}")),
        None => Err(anyhow!("mpv reply is missing an error status")),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn ipc_skips_events_and_other_ids_and_reports_command_errors() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut stream = BufReader::new(stream);
            for (expected, error, data) in [
                (json!(["get_property", "volume"]), "success", json!(82.5)),
                (json!(["bad-command"]), "error running command", Value::Null),
                (
                    json!(["get_property", "time-pos"]),
                    "property unavailable",
                    Value::Null,
                ),
            ] {
                let mut line = String::new();
                stream.read_line(&mut line).unwrap();
                assert!(line.ends_with('\n'));
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["command"], expected);
                let id = request["request_id"].as_i64().unwrap();
                let stream = stream.get_mut();
                writeln!(stream, "{}", json!({ "event": "start-file", "request_id": id })).unwrap();
                writeln!(
                    stream,
                    "{}",
                    json!({ "request_id": id - 1, "error": "success", "data": -1 })
                )
                .unwrap();
                writeln!(stream, "{}", json!({ "request_id": id, "error": error, "data": data }))
                    .unwrap();
            }
        });

        let mut mpv = Mpv::new();
        mpv.socket = socket;
        assert!(mpv.connect_existing().unwrap());
        assert_eq!(
            mpv.command(&[json!("get_property"), json!("volume")]).unwrap(),
            json!(82.5)
        );
        assert_eq!(
            mpv.command(&[json!("bad-command")]).unwrap_err().to_string(),
            "mpv: error running command"
        );
        assert_eq!(mpv.property("time-pos").unwrap(), None);
        assert!(mpv.child.is_none());
        server.join().unwrap();
    }
}
