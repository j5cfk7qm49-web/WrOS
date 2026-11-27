//! Control the YouTube Music queue and its playback in mpv.

#[cfg(unix)]
use super::Stream;
use super::{
    Loaded, YtMusic,
    parse::{Collection, LIVE, items, valid_id},
    search::helper,
};
use crate::music::{Backend, Cmd, Item, Modes, Repeat, State, Track, Update, YoutubePlay};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
#[cfg(unix)]
use std::time::Duration;
use std::{
    collections::hash_map::RandomState, hash::BuildHasher, path::PathBuf, sync::mpsc::Sender,
};

/// What to tell the writer when a streamed song ends: nothing if yt-dlp finished, or was
/// stopped on purpose.
#[cfg(any(test, unix))]
fn stream_note(title: &str, started: bool, outcome: Option<&Result<(), String>>) -> Option<String> {
    match outcome {
        Some(Err(reason)) if !reason.is_empty() => Some(if started {
            format!("Couldn't finish {title}: {reason}")
        } else {
            format!("Couldn't play {title}: {reason}")
        }),
        _ => None,
    }
}

impl YtMusic {
    /// Streaming needs a FIFO; elsewhere songs are downloaded first.
    fn streaming(&self) -> bool {
        cfg!(unix) && self.play == YoutubePlay::Stream
    }

    /// The song to play when this one ends. A stream can't loop the way a file does (a FIFO
    /// can't be read twice), so Repeat one fetches it again.
    fn after_end(&self) -> Option<usize> {
        if self.streaming() && self.repeat == Repeat::One {
            self.current
        } else {
            self.next_index()
        }
    }

    #[cfg(unix)]
    fn stream_audio(&mut self, item: &Item) -> Result<PathBuf> {
        if !valid_id(&item.id) {
            bail!("invalid YouTube Music id");
        }
        if self.stream.as_ref().is_none_or(|s| s.id != item.id) {
            self.stream = None;
            self.stream = Some(Stream::start(&self.ytdlp, &item.id, Self::url(item))?);
        }
        self.stream
            .as_ref()
            .map(|stream| stream.fifo.clone())
            .context("music stream unavailable")
    }

    fn clear_fetching(&mut self) {
        if let Some(note) = self.fetching.take() {
            let _ = self.tell.send(Update::ClearNote(note));
        }
    }

    fn select(&mut self, index: Option<usize>) -> Result<()> {
        if self.loaded.is_some() {
            self.mpv.command(&[json!("stop")])?;
            self.loaded = None;
        }
        self.clear_fetching();
        #[cfg(unix)]
        {
            self.stream = None;
        }
        self.current = index;
        self.pause_on_load = false;
        self.keep_download(index.map(|i| self.queue[i].id.clone()).as_deref());
        Ok(())
    }

    pub(super) fn next_index(&self) -> Option<usize> {
        let next = self.current.map_or(0, |i| i + 1);
        if next < self.queue.len() {
            Some(next)
        } else if self.repeat == Repeat::All && !self.queue.is_empty() {
            Some(0)
        } else {
            None
        }
    }

    fn tell_queue(&mut self) {
        if let Ok(queue) = self.queue() {
            let _ = self.tell.send(Update::Queue(queue));
        }
    }

    fn move_to(&mut self, next: Option<usize>) -> Result<()> {
        self.select(next)?;
        self.tell_queue();
        Ok(())
    }

    fn track_state(&self, props: &Value, playing: bool) -> State {
        let Some(item) = self.current.and_then(|i| self.queue.get(i)) else {
            return State::Idle;
        };
        State::Playing(Track {
            title: item.title.clone(),
            artist: item.artist.clone(),
            progress: props["time-pos"].as_f64().unwrap_or_default(),
            // A live stream has no length; mpv would report what it has buffered.
            duration: if item.length == LIVE {
                0.0
            } else {
                props["duration"].as_f64().unwrap_or_default()
            },
            playing,
        })
    }

    fn load_audio(&mut self, path: PathBuf) -> Result<()> {
        self.mpv.set_loop_playlist(false)?;
        self.set_loop_file(self.repeat)?;
        self.mpv.set_property("pause", json!(self.pause_on_load))?;
        self.mpv.load(
            path.to_str().context("the cache path isn't valid UTF-8")?,
            true,
        )?;
        self.loaded = Some(Loaded {
            path,
            started: false,
            played: false,
        });
        Ok(())
    }

    /// Hand the audio to mpv, or count a failure and move on (three in a row stop the queue).
    fn load_or_skip(&mut self, audio: Result<PathBuf>, title: &str) -> Result<State> {
        self.clear_fetching();
        match audio.and_then(|path| self.load_audio(path)) {
            Ok(()) => {
                if self.streaming() {
                    // A stream takes a few seconds to start; it counts as played once it sounds.
                    let note = format!("Fetching {title} from YouTube…");
                    let _ = self.tell.send(Update::Note(note.clone()));
                    self.fetching = Some(note);
                } else {
                    self.failures = 0;
                    if !self.pause_on_load {
                        self.prefetch();
                    }
                }
                Ok(self.track_state(&Value::Null, !self.pause_on_load))
            }
            Err(error) => {
                self.failures += 1;
                let next = if self.failures >= 3 {
                    None
                } else {
                    self.next_index()
                };
                self.move_to(next)?;
                let _ = self
                    .tell
                    .send(Update::Note(format!("Couldn't play {title}: {error}")));
                Ok(self.track_state(&Value::Null, false))
            }
        }
    }

    /// The note for a streamed song that just ended, if yt-dlp didn't finish it.
    fn end_stream(&mut self, played: bool) -> Option<String> {
        #[cfg(unix)]
        if let Some(mut stream) = self.stream.take() {
            let title = self
                .current
                .and_then(|i| self.queue.get(i))
                .map(|item| item.title.clone())
                .unwrap_or_default();
            return stream_note(&title, played, stream.outcome(Duration::from_millis(500)));
        }
        let _ = played;
        None
    }

    fn set_loop_file(&mut self, repeat: Repeat) -> Result<()> {
        // mpv reopening a FIFO to loop it would wait forever for a writer.
        let one = repeat == Repeat::One && !self.streaming();
        self.mpv
            .set_property("loop-file", json!(if one { "inf" } else { "no" }))
    }

    fn enqueue(&mut self, id: String, now: bool) -> Result<()> {
        if !valid_id(&id) {
            bail!("invalid YouTube Music id");
        }
        let item = self
            .songs
            .iter()
            .chain(&self.queue)
            .find(|item| item.id == id)
            .cloned()
            .unwrap_or_else(|| Item {
                title: id.clone(),
                id,
                artist: String::new(),
                length: String::new(),
                pos: 0,
                current: false,
                video: false,
            });
        let index = self.current.map_or(self.queue.len(), |i| i + 1);
        self.queue.insert(index, item);
        if now {
            if let Err(error) = self.select(Some(index)) {
                self.queue.remove(index);
                return Err(error);
            }
            self.failures = 0;
        }
        Ok(())
    }
}

impl Backend for YtMusic {
    fn state(&mut self) -> Result<State> {
        if let Some(loaded) = &mut self.loaded {
            let props = self.mpv.snapshot()?;
            let idle = props["idle-active"].as_bool() == Some(true);
            if props["playlist-pos"].as_i64() == Some(-1) || (idle && loaded.started) {
                let played = loaded.played;
                self.loaded = None;
                let note = self.end_stream(played);
                if note.is_some() && !played {
                    self.failures += 1;
                }
                let next = if self.failures >= 3 {
                    None
                } else {
                    self.after_end()
                };
                self.move_to(next)?;
                if let Some(note) = note {
                    let _ = self.tell.send(Update::Note(note));
                }
            } else {
                loaded.started |= !idle;
                if !loaded.played && props["time-pos"].as_f64().is_some_and(|t| t > 0.0) {
                    loaded.played = true;
                    self.failures = 0;
                    self.clear_fetching();
                }
                if !idle && props["pause"].as_bool() != Some(true) {
                    if !self.streaming() {
                        self.prefetch();
                    }
                    return Ok(self.track_state(&props, true));
                }
                return Ok(self.track_state(&props, false));
            }
        }

        let Some(item) = self.current.and_then(|i| self.queue.get(i)).cloned() else {
            return Ok(State::Idle);
        };
        let title = item.title.clone();
        #[cfg(unix)]
        if self.streaming() {
            let fifo = self.stream_audio(&item);
            return self.load_or_skip(fifo, &title);
        }
        let Some(audio) = self.audio(&item) else {
            if self.fetching.is_none() {
                let note = format!("Fetching {title} from YouTube Music…");
                let _ = self.tell.send(Update::Note(note.clone()));
                self.fetching = Some(note);
            }
            return Ok(self.track_state(&Value::Null, false));
        };
        self.load_or_skip(audio, &title)
    }

    fn command(&mut self, c: Cmd) -> Result<()> {
        match c {
            Cmd::PlayPause if self.loaded.is_some() => self.mpv.toggle_pause(),
            Cmd::PlayPause if self.current.is_some() => {
                self.pause_on_load = !self.pause_on_load;
                Ok(())
            }
            Cmd::PlayPause | Cmd::Next => {
                self.failures = 0;
                self.move_to(self.next_index())
            }
            Cmd::Prev => {
                self.failures = 0;
                let prev = self.current.map_or_else(
                    || self.queue.len().checked_sub(1),
                    |i| Some(i.saturating_sub(1)),
                );
                self.move_to(prev)
            }
            Cmd::JumpTo(i) => {
                if i >= self.queue.len() {
                    bail!("that song is no longer in the queue");
                }
                self.failures = 0;
                self.select(Some(i))
            }
            Cmd::Enqueue { id, now } => self.enqueue(id, now),
            Cmd::Seek(secs) if self.loaded.is_some() => self.mpv.seek(secs),
            Cmd::Volume(delta) => self.mpv.add_volume(delta),
            Cmd::Repeat => {
                let repeat = match self.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
                if self.loaded.is_some() {
                    self.set_loop_file(repeat)?;
                }
                self.repeat = repeat;
                Ok(())
            }
            Cmd::Shuffle => {
                self.shuffle = !self.shuffle;
                if self.shuffle {
                    let start = self.current.map_or(0, |i| i + 1);
                    let tail = &mut self.queue[start..];
                    let random = RandomState::new();
                    for i in (1..tail.len()).rev() {
                        tail.swap(i, (random.hash_one(i) % (i + 1) as u64) as usize);
                    }
                }
                self.tell_queue();
                Ok(())
            }
            Cmd::Like => bail!("Liking songs comes with YouTube Music sign-in, in a later update"),
            _ => Ok(()),
        }
    }

    fn queue(&mut self) -> Result<Vec<Item>> {
        let mut queue = self.queue.clone();
        for (pos, item) in queue.iter_mut().enumerate() {
            item.pos = pos;
            item.current = self.current == Some(pos);
        }
        Ok(queue)
    }

    fn search(&mut self, query: &str) -> Result<Vec<Item>> {
        self.search_songs(query)
    }

    fn search_videos(&mut self, query: &str) -> Result<Vec<Item>> {
        self.find_videos(query)
    }

    fn playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
        self.search_playlists(query)
    }

    fn load_playlist(
        &mut self,
        id: String,
        title: String,
        now: bool,
        tell: Sender<Update>,
    ) -> Result<()> {
        if !valid_id(&id) {
            bail!("invalid YouTube Music id");
        }
        let collection: Collection = helper(&self.helper, "tracks", &id)?;
        let tracks = items(&collection.tracks, |row| row.length.clone());
        let title = if collection.title.is_empty() {
            title
        } else {
            collection.title
        };
        let count = tracks.len();
        if count == 0 {
            bail!("no playable songs in {title}");
        }
        if now {
            self.select(None)?;
            self.queue = tracks;
            self.current = Some(0);
            self.failures = 0;
        } else {
            self.queue.extend(tracks);
        }
        let _ = tell.send(Update::Note(format!("Queued {count} songs from {title}")));
        let _ = tell.send(Update::Queue(self.queue()?));
        Ok(())
    }

    fn modes(&mut self) -> Result<Modes> {
        let mut modes = Modes {
            repeat: Some(self.repeat),
            shuffle: Some(self.shuffle),
            ..Modes::default()
        };
        if self.mpv.connect_existing()? {
            modes.volume = self
                .mpv
                .property("volume")?
                .and_then(|v| v.as_f64())
                .map(|v| v.round().clamp(0.0, 100.0) as u8);
            modes.muted = self
                .mpv
                .property("mute")?
                .and_then(|v| v.as_bool())
                .unwrap_or_default();
        }
        Ok(modes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[cfg(unix)]
    use {
        super::super::tests::fake_helper,
        std::{fs, sync::mpsc::Receiver, thread},
    };

    /// A stand-in for mpv: open the stream's FIFO and read it, saying when each chunk arrives.
    #[cfg(unix)]
    fn reader(fifo: PathBuf) -> Receiver<Vec<u8>> {
        use std::io::Read;
        let (send, chunks) = mpsc::channel();
        thread::spawn(move || {
            let mut pipe = fs::File::open(fifo).unwrap();
            let mut buf = [0u8; 64];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 || send.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        chunks
    }

    #[cfg(unix)]
    #[test]
    fn streaming_starts_without_waiting_for_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let ytdlp = fake_helper(
            dir.path(),
            "for a; do last=$a; done\n\
             [ \"$last\" = https://music.youtube.com/watch?v=song_1 ] || exit 3\n\
             printf first; sleep 3; printf second\n",
        );
        let item = Item {
            id: "song_1".into(),
            title: "Song".into(),
            artist: String::new(),
            length: String::new(),
            pos: 0,
            current: false,
            video: false,
        };
        let mut stream = Stream::start(&ytdlp, &item.id, YtMusic::url(&item)).unwrap();
        let chunks = reader(stream.fifo.clone());
        assert_eq!(
            chunks.recv_timeout(Duration::from_secs(2)).unwrap(),
            b"first"
        );
        assert!(
            stream.outcome(Duration::ZERO).is_none(),
            "the first bytes came while yt-dlp was still fetching"
        );
        assert_eq!(stream.outcome(Duration::from_secs(10)), Some(&Ok(())));
    }

    #[cfg(unix)]
    #[test]
    fn a_stream_that_breaks_off_notes_it_and_moves_on() {
        let dir = tempfile::tempdir().unwrap();
        let ytdlp = fake_helper(
            dir.path(),
            "printf abc; echo 'ERROR: connection reset by peer' >&2; exit 1\n",
        );
        let mut stream = Stream::start(
            &ytdlp,
            "song_1",
            "https://music.youtube.com/watch?v=song_1".into(),
        )
        .unwrap();
        let chunks = reader(stream.fifo.clone());
        assert_eq!(chunks.recv_timeout(Duration::from_secs(5)).unwrap(), b"abc");
        let outcome = stream.outcome(Duration::from_secs(5));
        assert_eq!(
            stream_note("Song", true, outcome).as_deref(),
            Some("Couldn't finish Song: ERROR: connection reset by peer")
        );
        assert_eq!(
            stream_note("Song", false, outcome).as_deref(),
            Some("Couldn't play Song: ERROR: connection reset by peer")
        );
    }

    #[test]
    fn a_finished_or_stopped_stream_says_nothing() {
        assert_eq!(stream_note("Song", true, Some(&Ok(()))), None);
        assert_eq!(stream_note("Song", true, Some(&Err(String::new()))), None);
        assert_eq!(stream_note("Song", true, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_stream_nobody_opened_stops_at_once_without_running_ytdlp() {
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran");
        let ytdlp = fake_helper(dir.path(), &format!("touch '{}'\n", ran.display()));
        let stream = Stream::start(
            &ytdlp,
            "song_1",
            "https://music.youtube.com/watch?v=song_1".into(),
        )
        .unwrap();
        let fifo = stream.fifo.clone();
        let started = std::time::Instant::now();
        drop(stream);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!ran.exists(), "yt-dlp waits for the player");
        assert!(!fifo.exists(), "the pipe and its folder are gone");
    }

    #[test]
    fn repeat_one_fetches_a_streamed_song_again() {
        let dir = tempfile::tempdir().unwrap();
        let (tell, _updates) = mpsc::channel();
        let mut backend = YtMusic::new(dir.path().join("wros-ytm"), tell);
        for id in ["first", "second"] {
            backend
                .command(Cmd::Enqueue {
                    id: id.into(),
                    now: false,
                })
                .unwrap();
        }
        backend.current = Some(0);
        backend.repeat = Repeat::One;
        assert_eq!(
            backend.after_end(),
            if cfg!(unix) { Some(0) } else { Some(1) }
        );
        let mut backend = backend.playing(YoutubePlay::Download);
        assert_eq!(backend.after_end(), Some(1));
        backend.repeat = Repeat::Off;
        assert_eq!(backend.after_end(), Some(1));
    }
}
