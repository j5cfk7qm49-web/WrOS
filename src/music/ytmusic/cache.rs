//! Download, stage and evict cached audio.

use super::{
    YtMusic,
    parse::{LIVE, valid_id},
    ytdlp_reason,
};
use crate::music::Item;
use anyhow::{Context, Result, anyhow, bail};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

const CACHE_FILES: usize = 20;
const CACHE_BYTES: u64 = 300 * 1024 * 1024;

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
        let path = cache.join(self.file.file_name().context("download has no filename")?);
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

pub(super) struct Download {
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

impl YtMusic {
    pub(super) fn keep_download(&mut self, id: Option<&str>) {
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

    pub(super) fn prefetch(&mut self) {
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
        let Some(playing) = self.loaded.as_ref().map(|loaded| &loaded.path) else {
            return;
        };
        let playing_bytes = playing.metadata().map_or(0, |info| info.len());
        if staged_bytes > CACHE_BYTES.saturating_sub(playing_bytes) {
            // Keep the prefetched file staged until the current file can be evicted.
            return;
        }
        let Some(job) = &mut self.download else {
            return;
        };
        let Some(Ok(staged)) = job.ready.take() else {
            return;
        };
        match staged.install(&self.cache, Some(playing)) {
            Ok(_) => self.download = None,
            Err(error) => job.ready = Some(Err(error)),
        }
    }

    pub(super) fn audio(&mut self, item: &Item) -> Option<Result<PathBuf>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::{Backend, Cmd, State, Update, YoutubePlay};
    use std::time::SystemTime;

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
