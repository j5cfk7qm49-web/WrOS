//! Public YouTube Music search and a local queue of downloaded audio.

use super::{Backend, Cmd, Item, Modes, Repeat, State, Track, Update, mpv::Mpv};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::hash_map::RandomState,
    env, fs,
    hash::BuildHasher,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

const HELPER: &str = "wros-ytm";
const CACHE_FILES: usize = 20;
const CACHE_BYTES: u64 = 300 * 1024 * 1024;

pub(crate) fn helper_path() -> Option<PathBuf> {
    let path = env::var_os("PATH");
    std::iter::once(Path::new("/run/current-system/sw/bin").join(HELPER))
        .chain(
            path.iter()
                .flat_map(|path| env::split_paths(path))
                .map(|dir| dir.join(HELPER)),
        )
        .find(|path| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::metadata(path)
                    .is_ok_and(|info| info.is_file() && info.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            path.is_file()
        })
}

fn valid_id(id: &str) -> bool {
    (2..=80).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn helper<T: DeserializeOwned>(path: &Path, action: &str, arg: &str) -> Result<T> {
    let output = Command::new(path)
        .args([action, arg])
        .stdin(Stdio::null())
        .output()
        .context("couldn't run wros-ytm")?;
    let reply: Value = serde_json::from_slice(&output.stdout).context("invalid wros-ytm reply")?;
    if let Some(error) = reply["error"].as_str() {
        bail!("{error}");
    }
    if !output.status.success() {
        bail!("wros-ytm exited with {}", output.status);
    }
    serde_json::from_value(reply).context("invalid wros-ytm reply")
}

#[derive(Deserialize)]
struct Row {
    id: String,
    title: String,
    artist: String,
    length: String,
    count: Option<u64>,
}

#[derive(Deserialize)]
struct Search {
    songs: Vec<Row>,
    albums: Vec<Row>,
    playlists: Vec<Row>,
}

#[derive(Deserialize)]
struct Collection {
    title: String,
    tracks: Vec<Row>,
}

fn items(rows: &[Row], length: impl Fn(&Row) -> String) -> Vec<Item> {
    rows.iter()
        .filter(|row| valid_id(&row.id))
        .enumerate()
        .map(|(pos, row)| Item {
            id: row.id.clone(),
            title: row.title.clone(),
            artist: row.artist.clone(),
            length: length(row),
            pos,
            current: false,
            video: false,
        })
        .collect()
}

fn cache_files(cache: &Path) -> impl Iterator<Item = (PathBuf, fs::Metadata)> {
    fs::read_dir(cache)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let info = entry.metadata().ok()?;
            info.is_file().then(|| (entry.path(), info))
        })
}

fn cached_file(cache: &Path, id: &str) -> Option<PathBuf> {
    cache_files(cache)
        .find(|(path, info)| {
            info.len() > 0
                && path.extension().is_some()
                && path.file_stem().and_then(|stem| stem.to_str()) == Some(id)
        })
        .map(|(path, _)| path)
}

fn trim_cache(cache: &Path, keep: Option<&Path>, max_files: usize, max_bytes: u64) -> Result<()> {
    let mut files: Vec<_> = cache_files(cache).collect();
    files.sort_unstable_by(|(a, am), (b, bm)| {
        am.modified().ok().cmp(&bm.modified().ok()).then_with(|| a.cmp(b))
    });
    let mut count = files.len();
    let mut bytes: u64 = files.iter().map(|(_, info)| info.len()).sum();
    for (path, info) in files {
        if count <= max_files && bytes <= max_bytes {
            return Ok(());
        }
        if keep == Some(path.as_path()) {
            continue;
        }
        if let Err(e) = fs::remove_file(&path)
            && e.kind() != ErrorKind::NotFound
        {
            return Err(e).with_context(|| format!("removing {}", path.display()));
        }
        count -= 1;
        bytes -= info.len();
    }
    if count > max_files || bytes > max_bytes {
        bail!("the audio doesn't fit in the YouTube Music cache");
    }
    Ok(())
}

struct Staged {
    _dir: tempfile::TempDir,
    file: PathBuf,
    bytes: u64,
}

impl Staged {
    fn install(self, cache: &Path, keep: Option<&Path>) -> Result<PathBuf> {
        trim_cache(cache, keep, CACHE_FILES - 1, CACHE_BYTES - self.bytes)?;
        let path = cache.join(self.file.file_name().unwrap());
        fs::rename(&self.file, &path).context("saving downloaded audio in the cache")?;
        Ok(path)
    }
}

fn download(cache: &Path, id: &str, cancel: Receiver<()>) -> Result<Staged> {
    if !valid_id(id) {
        bail!("invalid YouTube Music id");
    }
    fs::create_dir_all(cache).context("creating the YouTube Music cache")?;
    // yt-dlp writes directly to its output file. Publish it only after a successful download.
    let dir = tempfile::tempdir_in(cache).context("staging downloaded audio")?;
    let stderr = dir.path().join(".stderr");
    let mut child = Command::new("yt-dlp")
        .args(["--ignore-config", "-q", "--no-warnings", "--no-playlist", "-f", "bestaudio"])
        .args(["--no-part", "-o", "%(id)s.%(ext)s"])
        .arg(format!("https://music.youtube.com/watch?v={id}"))
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&stderr)?)
        .spawn()
        .context("couldn't start yt-dlp")?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.recv_timeout(Duration::from_millis(100)) != Err(RecvTimeoutError::Timeout) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("download cancelled");
        }
    };
    if !status.success() {
        let stderr = fs::read(stderr).unwrap_or_default();
        let reason: String = String::from_utf8_lossy(&stderr)
            .lines()
            .last()
            .unwrap_or_default()
            .trim()
            .chars()
            .take(120)
            .collect();
        if reason.is_empty() {
            bail!("yt-dlp exited with {status}");
        }
        bail!("{reason}");
    }
    let file = cached_file(dir.path(), id).context("yt-dlp produced no audio file")?;
    let bytes = file.metadata()?.len();
    if bytes > CACHE_BYTES {
        bail!("the audio doesn't fit in the YouTube Music cache");
    }
    Ok(Staged {
        _dir: dir,
        file,
        bytes,
    })
}

struct Download {
    id: String,
    result: Receiver<Result<Staged>>,
    ready: Option<Result<Staged>>,
    cancel: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Download {
    fn start(cache: PathBuf, id: &str) -> Self {
        let (sender, result) = mpsc::channel();
        let (cancel, cancelled) = mpsc::channel();
        let video = id.to_string();
        let worker = thread::spawn(move || {
            let _ = sender.send(download(&cache, &video, cancelled));
        });
        Self {
            id: id.to_string(),
            result,
            ready: None,
            cancel: Some(cancel),
            worker: Some(worker),
        }
    }

    fn poll(&mut self) {
        if self.ready.is_none() {
            self.ready = match self.result.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(anyhow!("download worker stopped"))),
            };
        }
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        drop(self.cancel.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Loaded {
    path: PathBuf,
    started: bool,
}

pub(super) struct YtMusic {
    helper: PathBuf,
    cache: PathBuf,
    tell: Sender<Update>,
    mpv: Mpv,
    search: Option<(String, Search)>,
    songs: Vec<Item>,
    queue: Vec<Item>,
    current: Option<usize>,
    loaded: Option<Loaded>,
    download: Option<Download>,
    fetching: Option<String>,
    failures: u8,
    pause_on_load: bool,
    repeat: Repeat,
    shuffle: bool,
}

impl YtMusic {
    pub(super) fn new(helper: PathBuf, tell: Sender<Update>) -> Self {
        let cache = env::var_os("XDG_CACHE_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| grimoire_core::paths::home().join(".cache"))
            .join("grimoire/youtube-music");
        Self {
            helper,
            cache,
            tell,
            mpv: Mpv::new(),
            search: None,
            songs: Vec::new(),
            queue: Vec::new(),
            current: None,
            loaded: None,
            download: None,
            fetching: None,
            failures: 0,
            pause_on_load: false,
            repeat: Repeat::Off,
            shuffle: false,
        }
    }

    fn search_for(&mut self, query: &str) -> Result<&Search> {
        if self.search.as_ref().is_none_or(|(q, _)| q != query) {
            let result = helper(&self.helper, "search", query)?;
            self.search = Some((query.to_string(), result));
        }
        Ok(&self.search.as_ref().unwrap().1)
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
        self.current = index;
        self.pause_on_load = false;
        self.keep_download(index.map(|i| self.queue[i].id.clone()).as_deref());
        Ok(())
    }

    fn next_index(&self) -> Option<usize> {
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

    fn keep_download(&mut self, id: Option<&str>) {
        if self
            .download
            .as_ref()
            .is_some_and(|job| Some(job.id.as_str()) != id)
        {
            self.download = None;
        }
    }

    fn download_for(&mut self, id: &str) -> &mut Download {
        self.download.get_or_insert_with(|| Download::start(self.cache.clone(), id))
    }

    fn prefetch(&mut self) {
        let id = self.next_index().map(|i| self.queue[i].id.clone());
        self.keep_download(id.as_deref());
        let Some(id) = id else {
            return;
        };
        if cached_file(&self.cache, &id).is_some() {
            return;
        }
        let job = self.download_for(&id);
        job.poll();
        let Some(Ok(staged)) = &job.ready else {
            return;
        };
        let staged_bytes = staged.bytes;
        let playing = &self.loaded.as_ref().unwrap().path;
        let playing_bytes = playing.metadata().map_or(0, |info| info.len());
        if staged_bytes > CACHE_BYTES.saturating_sub(playing_bytes) {
            // Keep the prefetched file staged until the current file can be evicted.
            return;
        }
        let job = self.download.as_mut().unwrap();
        let staged = job.ready.take().unwrap().unwrap();
        match staged.install(&self.cache, Some(playing)) {
            Ok(_) => self.download = None,
            Err(error) => job.ready = Some(Err(error)),
        }
    }

    fn audio(&mut self, id: &str) -> Option<Result<PathBuf>> {
        if let Some(path) = cached_file(&self.cache, id) {
            self.download = None;
            let trimmed = trim_cache(&self.cache, Some(&path), CACHE_FILES, CACHE_BYTES);
            return Some(trimmed.map(|()| path));
        }
        let job = self.download_for(id);
        job.poll();
        let result = job.ready.take()?;
        self.download = None;
        Some(result.and_then(|staged| staged.install(&self.cache, None)))
    }

    fn track_state(&self, props: &Value, playing: bool) -> State {
        let Some(item) = self.current.and_then(|i| self.queue.get(i)) else {
            return State::Idle;
        };
        State::Playing(Track {
            title: item.title.clone(),
            artist: item.artist.clone(),
            progress: props["time-pos"].as_f64().unwrap_or_default(),
            duration: props["duration"].as_f64().unwrap_or_default(),
            playing,
        })
    }

    fn load_audio(&mut self, path: PathBuf) -> Result<()> {
        self.mpv.set_loop_playlist(false)?;
        self.set_loop_file(self.repeat)?;
        self.mpv.set_property("pause", json!(self.pause_on_load))?;
        self.mpv.load(path.to_str().context("the cache path isn't valid UTF-8")?, true)?;
        self.loaded = Some(Loaded {
            path,
            started: false,
        });
        Ok(())
    }

    fn set_loop_file(&mut self, repeat: Repeat) -> Result<()> {
        self.mpv.set_property("loop-file", json!(if repeat == Repeat::One { "inf" } else { "no" }))
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
                self.loaded = None;
                self.move_to(self.next_index())?;
            } else {
                loaded.started |= !idle;
                if !idle && props["pause"].as_bool() != Some(true) {
                    self.prefetch();
                    return Ok(self.track_state(&props, true));
                }
                return Ok(self.track_state(&props, false));
            }
        }

        let Some(item) = self.current.and_then(|i| self.queue.get(i)) else {
            return Ok(State::Idle);
        };
        let (id, title) = (item.id.clone(), item.title.clone());
        let Some(audio) = self.audio(&id) else {
            if self.fetching.is_none() {
                let note = format!("Fetching {title} from YouTube Music…");
                let _ = self.tell.send(Update::Note(note.clone()));
                self.fetching = Some(note);
            }
            return Ok(self.track_state(&Value::Null, false));
        };
        self.clear_fetching();
        match audio.and_then(|path| self.load_audio(path)) {
            Ok(()) => {
                self.failures = 0;
                if !self.pause_on_load {
                    self.prefetch();
                }
                Ok(self.track_state(&Value::Null, !self.pause_on_load))
            }
            Err(error) => {
                self.failures += 1;
                let next = if self.failures >= 3 { None } else { self.next_index() };
                self.move_to(next)?;
                let _ = self.tell.send(Update::Note(format!("Couldn't play {title}: {error}")));
                Ok(self.track_state(&Value::Null, false))
            }
        }
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
        self.search = None;
        let found = items(&self.search_for(query)?.songs, |row| row.length.clone());
        self.songs.clone_from(&found);
        Ok(found)
    }

    fn playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
        let query = query.context(
            "Your YouTube Music library comes in a later update; search instead (/)",
        )?;
        let search = self.search_for(query)?;
        let mut found = items(&search.albums, |_| "album".into());
        let playlists = items(&search.playlists, |row| {
            row.count.map(|count| format!("{count} songs")).unwrap_or_default()
        });
        found.extend(playlists);
        for (pos, item) in found.iter_mut().enumerate() {
            item.pos = pos;
        }
        Ok(found)
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
    use std::time::SystemTime;

    #[test]
    fn ids_match_the_ascii_contract() {
        for id in ["aB", "Az09_-", &"x".repeat(80)] {
            assert!(valid_id(id), "{id:?}");
        }
        for id in ["", "a", "a/b", "a.b", "aa\n", "éé", "a&b", &"x".repeat(81)] {
            assert!(!valid_id(id), "{id:?}");
        }
    }

    fn cache_file(dir: &Path, name: &str, age: u64, bytes: u64) -> PathBuf {
        let path = dir.join(name);
        let file = fs::File::create(&path).unwrap();
        file.set_len(bytes).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(age)).unwrap();
        path
    }

    #[test]
    fn cache_evicts_oldest_to_meet_both_limits_and_keeps_playing_and_prefetch() {
        let dir = tempfile::tempdir().unwrap();
        let playing = cache_file(dir.path(), "playing.m4a", 1, 2);
        let oldest = cache_file(dir.path(), "oldest.webm", 2, 2);
        for i in 0..CACHE_FILES - 1 {
            cache_file(dir.path(), &format!("song{i}.m4a"), 3 + i as u64, 2);
        }
        trim_cache(dir.path(), Some(&playing), CACHE_FILES, CACHE_BYTES).unwrap();
        assert_eq!(cache_files(dir.path()).count(), CACHE_FILES);
        assert!(!oldest.exists());
        assert!(playing.is_file());

        let bigger = cache_file(dir.path(), "large.m4a", 25, CACHE_BYTES - 4);
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        let prefetched = cache_file(stage.path(), "next.m4a", 26, 5);
        let staged = Staged {
            _dir: stage,
            file: prefetched,
            bytes: 5,
        };
        let installed = staged.install(dir.path(), Some(&playing)).unwrap();
        assert!(playing.is_file());
        assert!(!bigger.exists());
        assert_eq!(installed, dir.path().join("next.m4a"));
        assert!(installed.is_file());
        assert!(cache_files(dir.path()).map(|(_, info)| info.len()).sum::<u64>() <= CACHE_BYTES);
    }

    #[cfg(unix)]
    fn fake_helper(dir: &Path, commands: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("wros-ytm");
        fs::write(&path, format!("#!/bin/sh\n{commands}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn helper_search_parses_songs_albums_and_playlists_and_filters_ids() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_helper(
            dir.path(),
            r#"
[ "$1" = search ] && [ "$2" = 'one query' ] || exit 2
cat <<'JSON'
{
  "songs": [
    {"id":"../bad","title":"Bad","artist":"","length":"","count":null},
    {"id":"song_1","title":"Song","artist":"Artist","length":"3:12","count":null}
  ],
  "albums": [{"id":"MPREbAlbum","title":"Album","artist":"Artist","length":"","count":null}],
  "playlists": [{"id":"VLPlaylist","title":"Playlist","artist":"","length":"","count":6}]
}
JSON
"#,
        );
        let (tell, _updates) = mpsc::channel();
        let mut backend = YtMusic::new(script.clone(), tell);
        let songs = backend.search("one query").unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(
            (&*songs[0].id, &*songs[0].title, &*songs[0].artist, &*songs[0].length),
            ("song_1", "Song", "Artist", "3:12")
        );
        fs::remove_file(script).unwrap();
        let lists = backend.playlists(Some("one query")).unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!((&*lists[0].id, &*lists[0].length, lists[0].pos), ("MPREbAlbum", "album", 0));
        assert_eq!(
            (&*lists[1].id, &*lists[1].length, lists[1].pos),
            ("VLPlaylist", "6 songs", 1)
        );
        assert_eq!(
            backend.playlists(None).unwrap_err().to_string(),
            "Your YouTube Music library comes in a later update; search instead (/)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn queue_appends_replaces_inserts_jumps_and_stops_at_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_helper(
            dir.path(),
            r#"
[ "$1" = tracks ] && [ "$2" = MPREbAlbum ] || exit 2
cat <<'JSON'
{"title":"Album","tracks":[
  {"id":"first","title":"First","artist":"Artist","length":"2:16"},
  {"id":"second","title":"Second","artist":"Artist","length":"4:16"},
  {"id":"bad?","title":"Bad","artist":"","length":""}
]}
JSON
"#,
        );
        let (tell, updates) = mpsc::channel();
        let mut backend = YtMusic::new(script, tell.clone());
        for (now, count, current) in [(true, 2, Some(0)), (false, 4, Some(0)), (true, 2, Some(0))] {
            backend.load_playlist("MPREbAlbum".into(), "Result".into(), now, tell.clone()).unwrap();
            assert!(matches!(updates.try_recv(), Ok(Update::Note(note))
                if note == "Queued 2 songs from Album"));
            assert!(matches!(updates.try_recv(), Ok(Update::Queue(queue))
                if queue.len() == count && queue.iter().position(|item| item.current) == current));
        }
        backend
            .command(Cmd::Enqueue {
                id: "up_next".into(),
                now: false,
            })
            .unwrap();
        backend
            .command(Cmd::Enqueue {
                id: "play_now".into(),
                now: true,
            })
            .unwrap();
        let queue = backend.queue().unwrap();
        let ids: Vec<_> = queue.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["first", "play_now", "up_next", "second"]);
        assert_eq!(queue.iter().position(|item| item.current), Some(1));
        assert_eq!(queue.iter().map(|item| item.pos).collect::<Vec<_>>(), [0, 1, 2, 3]);
        backend.command(Cmd::JumpTo(3)).unwrap();
        assert!(backend.command(Cmd::JumpTo(4)).is_err());
        assert!(backend.queue().unwrap()[3].current);
        backend.command(Cmd::Next).unwrap();
        assert!(backend.queue().unwrap().iter().all(|item| !item.current));
        assert_eq!(backend.state().unwrap(), State::Idle);
    }

    #[cfg(unix)]
    #[test]
    fn helper_errors_return_the_json_reason() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_helper(
            dir.path(),
            "printf '%s\n' '{\"error\":\"no network\"}'\nexit 1\n",
        );
        let (tell, _updates) = mpsc::channel();
        let mut backend = YtMusic::new(script, tell);
        assert_eq!(backend.search("a song").unwrap_err().to_string(), "no network");
    }

    #[test]
    fn fetching_is_reported_once_and_three_failures_stop_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let (tell, updates) = mpsc::channel();
        let mut backend = YtMusic::new(dir.path().join("wros-ytm"), tell);
        backend.cache = dir.path().join("cache");
        for id in ["first", "second", "third", "fourth"] {
            backend
                .command(Cmd::Enqueue {
                    id: id.into(),
                    now: false,
                })
                .unwrap();
        }
        backend.command(Cmd::JumpTo(0)).unwrap();

        for (pos, id) in ["first", "second", "third"].into_iter().enumerate() {
            let (send, result) = mpsc::channel();
            backend.download = Some(Download {
                id: id.into(),
                result,
                ready: None,
                cancel: None,
                worker: None,
            });
            for _ in 0..2 {
                assert!(matches!(backend.state().unwrap(), State::Playing(track)
                    if track.title == id && !track.playing));
            }
            assert!(matches!(updates.try_recv(), Ok(Update::Note(note))
                if note == format!("Fetching {id} from YouTube Music…")));
            assert!(matches!(updates.try_recv(), Err(TryRecvError::Empty)));

            assert!(send.send(Err(anyhow!("no network"))).is_ok());
            let state = backend.state().unwrap();
            assert!(matches!(updates.try_recv(), Ok(Update::ClearNote(_))));
            assert!(matches!(updates.try_recv(), Ok(Update::Queue(_))));
            assert!(matches!(updates.try_recv(), Ok(Update::Note(note))
                if note == format!("Couldn't play {id}: no network")));
            if pos == 2 {
                assert_eq!(state, State::Idle);
                assert!(backend.queue().unwrap().iter().all(|item| !item.current));
            } else {
                assert!(backend.queue().unwrap()[pos + 1].current);
            }
        }
    }
}
