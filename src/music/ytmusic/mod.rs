//! Public YouTube Music search and a local queue of downloaded audio.

use super::{Item, Repeat, Update, YoutubePlay, mpv::Mpv};
use cache::Download;
use parse::Search;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
};
#[cfg(unix)]
use stream::Stream;

mod cache;
mod parse;
mod playback;
mod search;
#[cfg(unix)]
mod stream;

pub(crate) use search::helper_path;

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

    /// Where yt-dlp fetches a queued song or video.
    fn url(item: &Item) -> String {
        if item.video {
            format!("https://www.youtube.com/watch?v={}", item.id)
        } else {
            format!("https://music.youtube.com/watch?v={}", item.id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        YtMusic,
        parse::{parse_video_search, valid_id},
    };
    #[cfg(unix)]
    use crate::music::{Backend, Cmd, State, Update};
    use serde_json::json;
    #[cfg(unix)]
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::mpsc,
    };

    #[test]
    fn ids_match_the_ascii_contract() {
        for id in ["aB", "Az09_-", &"x".repeat(80)] {
            assert!(valid_id(id), "{id:?}");
        }
        for id in ["", "a", "a/b", "a.b", "aa\n", "éé", "a&b", &"x".repeat(81)] {
            assert!(!valid_id(id), "{id:?}");
        }
    }

    #[cfg(unix)]
    pub(super) fn fake_helper(dir: &Path, commands: &str) -> PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("wros-ytm");
        let mut script = fs::File::create(&path).unwrap();
        writeln!(script, "#!/bin/sh\n{commands}").unwrap();
        script.sync_all().unwrap();
        drop(script);
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
}

/// Real YouTube Music, yt-dlp and mpv: run with `--ignored`, network on, `WROS_YTM` naming the
/// built wros-ytm helper, in a private HOME whose mpv.conf says `ao=null` (rc20-work/live-music.sh).
#[cfg(all(test, unix))]
mod live {
    use super::*;
    use crate::music::{Backend, Cmd, State};
    use std::time::{Duration, Instant};

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
