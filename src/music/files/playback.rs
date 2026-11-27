//! Play local files and album folders through mpv.

use super::library::{self, album_tracks, is_audio, parent_name, song_item, song_title};
use crate::music::{Backend, Cmd, Item, Modes, Repeat, State, Track, Update, mpv::Mpv};
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::mpsc::Sender};

pub(in crate::music) struct Files {
    mpv: Mpv,
    shuffle: bool,
}

impl Files {
    pub(in crate::music) fn new() -> Self {
        Self {
            mpv: Mpv::new(),
            shuffle: false,
        }
    }

    fn repeat(&mut self) -> Result<Repeat> {
        let looping = |mode, once| match mode {
            Some(Value::Bool(on)) => on,
            Some(Value::String(name)) => matches!(name.as_str(), "inf" | "force" | "yes"),
            Some(Value::Number(n)) => n.as_i64().is_some_and(|n| n > once),
            _ => false,
        };
        if looping(self.mpv.property("loop-file")?, 0) {
            Ok(Repeat::One)
        } else if looping(self.mpv.property("loop-playlist")?, 1) {
            Ok(Repeat::All)
        } else {
            Ok(Repeat::Off)
        }
    }

    fn enqueue(&mut self, id: &str, now: bool) -> Result<()> {
        let path = Path::new(id);
        if !is_audio(path) || !fs::symlink_metadata(path).is_ok_and(|info| info.is_file()) {
            return Err(anyhow!("music file is no longer available"));
        }
        if !now {
            return self.mpv.load(id, false);
        }
        let added = self
            .mpv
            .command(&[json!("loadfile"), json!(id), json!("insert-next")])?;
        let id = added["playlist_entry_id"]
            .as_i64()
            .ok_or_else(|| anyhow!("mpv did not return a playlist entry id"))?;
        let playlist = self.mpv.property("playlist")?.unwrap_or_default();
        let index = playlist
            .as_array()
            .and_then(|entries| entries.iter().position(|e| e["id"].as_i64() == Some(id)))
            .ok_or_else(|| anyhow!("mpv did not queue the file"))?;
        self.mpv.play_index(index)
    }
}

impl Backend for Files {
    fn state(&mut self) -> Result<State> {
        if !cfg!(unix) || !self.mpv.connect_existing()? {
            return Ok(State::Idle);
        }
        let props = self.mpv.snapshot()?;
        if props["idle-active"].as_bool() == Some(true) {
            return Ok(State::Idle);
        }
        let path = props["path"].as_str().map(Path::new);
        let tags = props["metadata"].as_object();
        let tag = |key: &str| {
            tags?
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .and_then(|(_, value)| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        Ok(State::Playing(Track {
            title: tag("title")
                .or_else(|| path.map(song_title))
                .unwrap_or_else(|| {
                    props["media-title"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                }),
            artist: tag("artist")
                .or_else(|| path.map(parent_name))
                .unwrap_or_default(),
            progress: props["time-pos"].as_f64().unwrap_or_default(),
            duration: props["duration"].as_f64().unwrap_or_default(),
            playing: props["pause"].as_bool() != Some(true),
        }))
    }

    fn command(&mut self, c: Cmd) -> Result<()> {
        match c {
            Cmd::PlayPause => self.mpv.toggle_pause(),
            Cmd::Next => self.mpv.next(),
            Cmd::Prev => self.mpv.prev(),
            Cmd::Seek(secs) => self.mpv.seek(secs),
            Cmd::Volume(d) => self.mpv.add_volume(d),
            Cmd::Shuffle => {
                self.mpv.set_shuffle(!self.shuffle)?;
                self.shuffle = !self.shuffle;
                Ok(())
            }
            Cmd::Repeat => {
                let repeat = self.repeat()?;
                self.mpv.set_loop_playlist(repeat == Repeat::Off)?;
                let loop_file = if repeat == Repeat::All { "inf" } else { "no" };
                self.mpv.set_property("loop-file", json!(loop_file))
            }
            Cmd::JumpTo(i) => self.mpv.play_index(i),
            Cmd::Enqueue { id, now } => self.enqueue(&id, now),
            Cmd::Like => Err(anyhow!("Music files can't be liked")),
            _ => Ok(()),
        }
    }

    fn queue(&mut self) -> Result<Vec<Item>> {
        if !self.mpv.connect_existing()? {
            return Ok(Vec::new());
        }
        let playlist = self.mpv.property("playlist")?.unwrap_or_default();
        Ok(playlist
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(pos, entry)| {
                let path = Path::new(entry["filename"].as_str()?);
                let current = entry["current"].as_bool().unwrap_or_default();
                Some(song_item(path, pos, current))
            })
            .collect())
    }

    fn search(&mut self, query: &str) -> Result<Vec<Item>> {
        Ok(library::search(query))
    }

    fn playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
        Ok(library::playlists(query))
    }

    fn load_playlist(
        &mut self,
        id: String,
        title: String,
        now: bool,
        tell: Sender<Update>,
    ) -> Result<()> {
        let tracks = album_tracks(Path::new(&id));
        if tracks.is_empty() {
            return Err(anyhow!("no music files in {title}"));
        }
        let count = tracks.len();
        for (i, path) in tracks.iter().enumerate() {
            if i.is_multiple_of(100)
                && tell
                    .send(Update::Note(format!("Queueing {title} ({i}/{count})…")))
                    .is_err()
            {
                return Ok(());
            }
            let name = path
                .to_str()
                .with_context(|| format!("Music file path is not UTF-8: {}", path.display()))?;
            self.mpv.load(name, now && i == 0)?;
            if now && i == 0 {
                self.mpv.set_property("pause", json!(false))?;
            }
        }
        let _ = tell.send(Update::Note(format!("Queued {count} songs from {title}")));
        let _ = tell.send(Update::Queue(self.queue()?));
        Ok(())
    }

    fn modes(&mut self) -> Result<Modes> {
        if !self.mpv.connect_existing()? {
            return Ok(Modes {
                repeat: Some(Repeat::Off),
                shuffle: Some(self.shuffle),
                ..Modes::default()
            });
        }
        Ok(Modes {
            repeat: Some(self.repeat()?),
            shuffle: Some(self.shuffle),
            volume: self
                .mpv
                .property("volume")?
                .and_then(|v| v.as_f64())
                .map(|v| v.round().clamp(0.0, 100.0) as u8),
            muted: self
                .mpv
                .property("mute")?
                .and_then(|v| v.as_bool())
                .unwrap_or_default(),
            ..Modes::default()
        })
    }
}
