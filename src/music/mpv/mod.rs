//! The background player for music files.

use anyhow::Result;
use serde_json::{Map, Value, json};

#[cfg(unix)]
use std::{io::BufReader, os::unix::net::UnixStream, path::PathBuf, process::Child};

mod ipc;
mod process;

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, Write},
        os::unix::net::UnixListener,
        thread,
        time::Duration,
    };

    #[test]
    fn ipc_skips_events_and_other_ids_and_reports_command_errors() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
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
                writeln!(
                    stream,
                    "{}",
                    json!({ "event": "start-file", "request_id": id })
                )
                .unwrap();
                writeln!(
                    stream,
                    "{}",
                    json!({ "request_id": id - 1, "error": "success", "data": -1 })
                )
                .unwrap();
                writeln!(
                    stream,
                    "{}",
                    json!({ "request_id": id, "error": error, "data": data })
                )
                .unwrap();
            }
        });

        let mut mpv = Mpv::new();
        mpv.socket = socket;
        assert!(mpv.connect_existing().unwrap());
        assert_eq!(
            mpv.command(&[json!("get_property"), json!("volume")])
                .unwrap(),
            json!(82.5)
        );
        assert_eq!(
            mpv.command(&[json!("bad-command")])
                .unwrap_err()
                .to_string(),
            "mpv: error running command"
        );
        assert_eq!(mpv.property("time-pos").unwrap(), None);
        assert!(mpv.child.is_none());
        server.join().unwrap();
    }
}
