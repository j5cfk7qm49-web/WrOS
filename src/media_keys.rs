#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
}

pub fn parse(text: &str) -> Vec<(u64, MediaKey)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let seq = words.next()?.parse::<u64>().ok()?;
            let key = match words.next()? {
                "play-pause" => MediaKey::PlayPause,
                "next" => MediaKey::Next,
                "previous" => MediaKey::Previous,
                _ => return None,
            };
            (seq > 0 && words.next().is_none()).then_some((seq, key))
        })
        .collect()
}

fn read_log(path: &Path) -> io::Result<Vec<(u64, MediaKey)>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let mut entries = parse(&text);
            entries.sort_unstable_by_key(|&(seq, _)| seq);
            Ok(entries)
        }
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData
            ) =>
        {
            Ok(Vec::new())
        }
        Err(err) => Err(err),
    }
}

pub struct Watcher {
    keys: mpsc::Receiver<MediaKey>,
    // Dropping the sender wakes the thread, even when the log has no new presses.
    _stop: mpsc::Sender<()>,
}

impl Watcher {
    /// Watch the press log from this point on, ignoring presses from before startup.
    pub fn start(path: PathBuf) -> Self {
        let mut seen = read_log(&path)
            .unwrap_or_default()
            .last()
            .map_or(0, |&(seq, _)| seq);
        let (tx, keys) = mpsc::channel();
        let (stop, wake) = mpsc::channel();
        thread::spawn(move || {
            while wake.recv_timeout(POLL) == Err(mpsc::RecvTimeoutError::Timeout) {
                let Ok(entries) = read_log(&path) else {
                    continue;
                };
                // The helper starts again at one after the log is lost or garbled.
                if entries.last().map_or(0, |&(seq, _)| seq) < seen {
                    seen = 0;
                }
                for (seq, key) in entries {
                    if seq > seen {
                        if tx.send(key).is_err() {
                            return;
                        }
                        seen = seq;
                    }
                }
            }
        });
        Self { keys, _stop: stop }
    }

    pub fn try_iter(&self) -> impl Iterator<Item = MediaKey> {
        self.keys.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::Instant;

    fn received(watcher: &Watcher, count: usize) -> Vec<MediaKey> {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut keys = Vec::new();
        loop {
            keys.extend(watcher.try_iter());
            if keys.len() >= count || Instant::now() >= deadline {
                return keys;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn parse_reads_seq_and_action_and_skips_junk() {
        let text = "1 play-pause\n2 next\n3 previous\n\njunk\n-1 next\n0 next\n\
                    4 unknown\n5 next junk\n";
        assert_eq!(
            parse(text),
            vec![
                (1, MediaKey::PlayPause),
                (2, MediaKey::Next),
                (3, MediaKey::Previous),
            ]
        );
    }

    #[test]
    fn presses_from_before_start_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("music");
        fs::write(&path, "5 next\n").unwrap();
        let watcher = Watcher::start(path.clone());
        thread::sleep(Duration::from_millis(300));
        assert_eq!(watcher.try_iter().next(), None);

        OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(b"6 play-pause\n")
            .unwrap();
        assert_eq!(received(&watcher, 1), vec![MediaKey::PlayPause]);
    }

    #[test]
    fn two_quick_presses_both_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("music");
        let watcher = Watcher::start(path.clone());
        fs::write(path, "7 next\n8 next\n").unwrap();
        assert_eq!(received(&watcher, 2), vec![MediaKey::Next, MediaKey::Next]);
        thread::sleep(Duration::from_millis(300));
        assert_eq!(watcher.try_iter().next(), None);
    }

    #[test]
    fn a_restarted_log_is_heard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("music");
        fs::write(&path, "40 next\n").unwrap();
        let watcher = Watcher::start(path.clone());

        let tmp = dir.path().join("music.tmp");
        fs::write(&tmp, "1 play-pause\n").unwrap();
        fs::rename(tmp, path).unwrap();
        assert_eq!(received(&watcher, 1), vec![MediaKey::PlayPause]);
    }
}
