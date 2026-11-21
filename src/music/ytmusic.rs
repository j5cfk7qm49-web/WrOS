//! Public YouTube Music search and a local queue of downloaded audio.

use super::{Backend, Cmd, Item, Modes, Repeat, State, Track, Update, YoutubePlay, mpv::Mpv};
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
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
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
        am.modified()
            .ok()
            .cmp(&bm.modified().ok())
            .then_with(|| a.cmp(b))
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

fn download(cache: &Path, id: &str, url: &str, cancel: Receiver<()>) -> Result<Staged> {
    if !valid_id(id) {
        bail!("invalid YouTube Music id");
    }
    fs::create_dir_all(cache).context("creating the YouTube Music cache")?;
    // yt-dlp writes directly to its output file. Publish it only after a successful download.
    let dir = tempfile::tempdir_in(cache).context("staging downloaded audio")?;
    let stderr = dir.path().join(".stderr");
    let mut child = Command::new("yt-dlp")
        .args([
            "--ignore-config",
            "-q",
            "--no-warnings",
            "--no-playlist",
            "-f",
            "bestaudio",
        ])
        .args(["--no-part", "-o", "%(id)s.%(ext)s"])
        .arg(url)
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
        bail!("{}", ytdlp_reason(&stderr, status));
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
    fn start(cache: PathBuf, id: &str, url: String) -> Self {
        let (sender, result) = mpsc::channel();
        let (cancel, cancelled) = mpsc::channel();
        let video = id.to_string();
        let worker = thread::spawn(move || {
            let _ = sender.send(download(&cache, &video, &url, cancelled));
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

/// The last line yt-dlp wrote to stderr, or its exit status.
fn ytdlp_reason(stderr: &Path, status: std::process::ExitStatus) -> String {
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
        format!("yt-dlp exited with {status}")
    } else {
        reason
    }
}

/// One song's audio from yt-dlp into a FIFO that mpv plays as it arrives (rc22). mpv sees a
/// stream that breaks off as an ordinary end of file, so how it ended is yt-dlp's exit status.
#[cfg(unix)]
struct Stream {
    id: String,
    fifo: PathBuf,
    outcome: Receiver<Result<(), String>>,
    done: Option<Result<(), String>>,
    cancel: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
    // Last: the FIFO's folder goes once the worker has stopped writing into it.
    _dir: tempfile::TempDir,
}

#[cfg(unix)]
impl Stream {
    fn start(ytdlp: &Path, id: &str, url: String) -> Result<Self> {
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
    fn outcome(&mut self, wait: Duration) -> Option<&Result<(), String>> {
        if self.done.is_none() {
            self.done = self.outcome.recv_timeout(wait).ok();
        }
        self.done.as_ref()
    }
}

#[cfg(unix)]
impl Drop for Stream {
    fn drop(&mut self) {
        drop(self.cancel.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// "4:18", or "1:01:14" past an hour.
fn clock(secs: f64) -> String {
    let secs = secs.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (secs / 3600, secs / 60 % 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// What a live stream shows for its length.
const LIVE: &str = "live";

/// Videos from `yt-dlp --flat-playlist -J ytsearchN:…`. A live stream (a radio channel, say)
/// plays as it arrives and shows "live"; one that hasn't started yet is left out. A video whose
/// length YouTube doesn't give shows none.
fn parse_video_search(reply: &Value) -> Vec<Item> {
    reply["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["live_status"].as_str() != Some("is_upcoming"))
        .filter_map(|entry| {
            let id = entry["id"].as_str().filter(|id| valid_id(id))?;
            let live = entry["live_status"].as_str() == Some("is_live");
            Some(Item {
                id: id.to_string(),
                title: entry["title"].as_str().unwrap_or(id).to_string(),
                artist: entry["channel"]
                    .as_str()
                    .or_else(|| entry["uploader"].as_str())
                    .unwrap_or_default()
                    .to_string(),
                length: if live {
                    LIVE.to_string()
                } else {
                    entry["duration"].as_f64().map(clock).unwrap_or_default()
                },
                pos: 0,
                current: false,
                video: true,
            })
        })
        .enumerate()
        .map(|(pos, item)| Item { pos, ..item })
        .collect()
}

/// What to tell the writer when a streamed song ends: nothing if yt-dlp finished, or was
/// stopped on purpose.
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

struct Loaded {
    path: PathBuf,
    /// mpv has taken the file (a stream may still be waiting for its first bytes).
    started: bool,
    /// Sound has come out of it.
    played: bool,
}

pub(super) struct YtMusic {
    helper: PathBuf,
    cache: PathBuf,
    /// The yt-dlp to run; tests swap in a script.
    ytdlp: PathBuf,
    play: YoutubePlay,
    #[cfg(unix)]
    stream: Option<Stream>,
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
            ytdlp: PathBuf::from("yt-dlp"),
            play: YoutubePlay::Stream,
            #[cfg(unix)]
            stream: None,
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

    pub(super) fn playing(mut self, play: YoutubePlay) -> Self {
        self.play = play;
        self
    }

    /// Streaming needs a FIFO; elsewhere songs are downloaded first.
    fn streaming(&self) -> bool {
        cfg!(unix) && self.play == YoutubePlay::Stream
    }

    /// Where yt-dlp fetches a queued song or video.
    fn url(item: &Item) -> String {
        if item.video {
            format!("https://www.youtube.com/watch?v={}", item.id)
        } else {
            format!("https://music.youtube.com/watch?v={}", item.id)
        }
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
        Ok(self.stream.as_ref().unwrap().fifo.clone())
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
        #[cfg(unix)]
        {
            self.stream = None;
        }
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

    fn download_for(&mut self, item: &Item) -> &mut Download {
        self.download
            .get_or_insert_with(|| Download::start(self.cache.clone(), &item.id, Self::url(item)))
    }

    fn prefetch(&mut self) {
        let next = self.next_index().map(|i| self.queue[i].clone());
        self.keep_download(next.as_ref().map(|item| item.id.as_str()));
        let Some(next) = next else {
            return;
        };
        if cached_file(&self.cache, &next.id).is_some() {
            return;
        }
        let job = self.download_for(&next);
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

    fn audio(&mut self, item: &Item) -> Option<Result<PathBuf>> {
        if item.video && item.length == LIVE {
            // Downloading a live stream first would never finish.
            return Some(Err(anyhow!(
                "a live stream plays only as it arrives (Settings › Music › Play)"
            )));
        }
        if let Some(path) = cached_file(&self.cache, &item.id) {
            self.download = None;
            let trimmed = trim_cache(&self.cache, Some(&path), CACHE_FILES, CACHE_BYTES);
            return Some(trimmed.map(|()| path));
        }
        let job = self.download_for(item);
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
        self.search = None;
        let found = items(&self.search_for(query)?.songs, |row| row.length.clone());
        self.songs.clone_from(&found);
        Ok(found)
    }

    fn search_videos(&mut self, query: &str) -> Result<Vec<Item>> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 200 || query.chars().any(char::is_control) {
            bail!("search with up to 200 letters, numbers and marks");
        }
        let errors = tempfile::NamedTempFile::new().context("searching YouTube")?;
        // "ytsearch20:" first, so nothing the writer types can be read as an option.
        let status = Command::new(&self.ytdlp)
            .args(["--ignore-config", "--flat-playlist", "-J", "--no-warnings"])
            .arg(format!("ytsearch20:{query}"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(errors.reopen()?)
            .spawn()
            .context("couldn't start yt-dlp")?
            .wait_with_output()?;
        if !status.status.success() {
            bail!("{}", ytdlp_reason(errors.path(), status.status));
        }
        let reply: Value =
            serde_json::from_slice(&status.stdout).context("invalid yt-dlp search reply")?;
        let found = parse_video_search(&reply);
        self.songs.clone_from(&found);
        Ok(found)
    }

    fn playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
        let query = query
            .context("Your YouTube Music library comes in a later update; search instead (/)")?;
        let search = self.search_for(query)?;
        let mut found = items(&search.albums, |_| "album".into());
        let playlists = items(&search.playlists, |row| {
            row.count
                .map(|count| format!("{count} songs"))
                .unwrap_or_default()
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
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(age))
            .unwrap();
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
        assert!(
            cache_files(dir.path())
                .map(|(_, info)| info.len())
                .sum::<u64>()
                <= CACHE_BYTES
        );
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
            (
                &*songs[0].id,
                &*songs[0].title,
                &*songs[0].artist,
                &*songs[0].length
            ),
            ("song_1", "Song", "Artist", "3:12")
        );
        fs::remove_file(script).unwrap();
        let lists = backend.playlists(Some("one query")).unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!(
            (&*lists[0].id, &*lists[0].length, lists[0].pos),
            ("MPREbAlbum", "album", 0)
        );
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
            backend
                .load_playlist("MPREbAlbum".into(), "Result".into(), now, tell.clone())
                .unwrap();
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
        assert_eq!(
            queue.iter().map(|item| item.pos).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
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
        assert_eq!(
            backend.search("a song").unwrap_err().to_string(),
            "no network"
        );
    }

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
    fn video_search_reads_title_channel_and_length() {
        let reply = json!({"entries": [
            {"id": "dQw4w9WgXcQ", "title": "A video", "channel": "A channel", "duration": 258.0},
            {"id": "lTRiuFIWV54", "title": "An hour", "uploader": "Uploader", "duration": 3674.0},
            {"id": "../bad", "title": "Bad id", "channel": "", "duration": 1.0}
        ]});
        let found = parse_video_search(&reply);
        let rows: Vec<_> = found
            .iter()
            .map(|i| (&*i.id, &*i.title, &*i.artist, &*i.length, i.pos, i.video))
            .collect();
        assert_eq!(
            rows,
            [
                ("dQw4w9WgXcQ", "A video", "A channel", "4:18", 0, true),
                ("lTRiuFIWV54", "An hour", "Uploader", "1:01:14", 1, true),
            ]
        );
        assert_eq!(
            YtMusic::url(&found[0]),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn video_search_keeps_live_radio_drops_upcoming_and_keeps_unknown_lengths() {
        let reply = json!({"entries": [
            {"id": "live_now_1", "title": "Radio", "channel": "C", "live_status": "is_live",
             "duration": null},
            {"id": "soon_12345", "title": "Soon", "channel": "C", "live_status": "is_upcoming"},
            {"id": "short_1234", "title": "No length", "channel": "C", "duration": null}
        ]});
        let found = parse_video_search(&reply);
        let rows: Vec<_> = found.iter().map(|i| (&*i.title, &*i.length)).collect();
        assert_eq!(rows, [("Radio", "live"), ("No length", "")]);
    }

    #[test]
    fn a_live_stream_is_refused_when_songs_download_first() {
        let dir = tempfile::tempdir().unwrap();
        let (tell, _updates) = mpsc::channel();
        let mut backend =
            YtMusic::new(dir.path().join("wros-ytm"), tell).playing(YoutubePlay::Download);
        backend.cache = dir.path().join("cache");
        let radio = Item {
            id: "live_now_1".into(),
            title: "Radio".into(),
            artist: String::new(),
            length: LIVE.into(),
            pos: 0,
            current: false,
            video: true,
        };
        let error = backend.audio(&radio).unwrap().unwrap_err().to_string();
        assert_eq!(
            error,
            "a live stream plays only as it arrives (Settings › Music › Play)"
        );
        assert!(backend.download.is_none(), "no download was started");
    }

    #[cfg(unix)]
    #[test]
    fn all_of_youtube_asks_ytdlp_and_answers_with_videos() {
        let dir = tempfile::tempdir().unwrap();
        let ytdlp = fake_helper(
            dir.path(),
            r#"for a; do last=$a; done
[ "$last" = 'ytsearch20:-rf lofi beats' ] || { echo "ERROR: got $last" >&2; exit 2; }
printf '%s' '{"entries":[{"id":"abc_123","title":"Beats","channel":"Lofi","duration":61}]}'
"#,
        );
        let (tell, _updates) = mpsc::channel();
        let mut backend = YtMusic::new(dir.path().join("wros-ytm"), tell);
        backend.ytdlp = ytdlp;
        let found = backend.search_videos("  -rf lofi beats ").unwrap();
        assert_eq!((&*found[0].title, &*found[0].length), ("Beats", "1:01"));
        // Choosing it plays the video from YouTube itself.
        backend
            .command(Cmd::Enqueue {
                id: "abc_123".into(),
                now: false,
            })
            .unwrap();
        assert!(backend.queue().unwrap()[0].video);
        assert!(backend.search_videos("\u{7}").is_err());
        assert!(backend.search_videos(&"x".repeat(201)).is_err());
        assert_eq!(
            backend.search_videos("fail").unwrap_err().to_string(),
            "ERROR: got ytsearch20:fail"
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

    #[test]
    fn fetching_is_reported_once_and_three_failures_stop_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let (tell, updates) = mpsc::channel();
        let mut backend =
            YtMusic::new(dir.path().join("wros-ytm"), tell).playing(YoutubePlay::Download);
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

/// Real YouTube Music, yt-dlp and mpv: run with `--ignored`, network on, `WROS_YTM` naming the
/// built wros-ytm helper, in a private HOME whose mpv.conf says `ao=null` (rc20-work/live-music.sh).
#[cfg(all(test, unix))]
mod live {
    use super::*;
    use crate::music::{Backend, Cmd, State};
    use std::time::Instant;

    fn state(ytm: &mut YtMusic, within: Duration, ok: impl Fn(&State) -> bool) -> State {
        let end = Instant::now() + within;
        loop {
            let s = ytm.state().unwrap_or(State::Offline);
            if ok(&s) {
                return s;
            }
            assert!(Instant::now() < end, "gave up waiting: {s:?}");
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    #[test]
    #[ignore = "needs the network, yt-dlp, mpv and wros-ytm; rc20-work/live-music.sh"]
    fn an_album_downloads_plays_and_moves_on() {
        let helper = PathBuf::from(env::var("WROS_YTM").expect("WROS_YTM"));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut ytm = YtMusic::new(helper, tx.clone());
        let songs = ytm.search("clair de lune debussy").unwrap();
        assert!(!songs.is_empty());
        let sets = ytm.playlists(Some("kind of blue miles davis")).unwrap();
        let album = sets.iter().find(|s| s.length == "album").expect("an album");
        ytm.load_playlist(album.id.clone(), album.title.clone(), true, tx)
            .unwrap();
        let started = Instant::now();
        let first = state(
            &mut ytm,
            Duration::from_secs(120),
            |s| matches!(s, State::Playing(t) if t.playing && t.progress > 0.5),
        );
        let State::Playing(first) = first else {
            unreachable!()
        };
        eprintln!(
            "first song playing after {:?}: {}",
            started.elapsed(),
            first.title
        );
        // The next song is fetched while this one plays.
        std::thread::sleep(Duration::from_secs(30));
        let cached = fs::read_dir(&ytm.cache).map(|d| d.count()).unwrap_or(0);
        eprintln!("cache holds {cached} entries");
        // Near the end: the next song follows by itself.
        ytm.command(Cmd::Seek(
            (first.duration - first.progress - 3.0).max(0.0) as i32
        ))
        .unwrap();
        let next = state(
            &mut ytm,
            Duration::from_secs(90),
            |s| matches!(s, State::Playing(t) if t.playing && t.title != first.title && t.progress > 0.2),
        );
        let State::Playing(next) = next else {
            unreachable!()
        };
        eprintln!("then: {}", next.title);
        let notes: Vec<_> = rx
            .try_iter()
            .filter_map(|u| match u {
                Update::Note(n) => Some(n),
                _ => None,
            })
            .collect();
        eprintln!("notes: {notes:?}");
    }
}
