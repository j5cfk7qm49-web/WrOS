//! Music files, local YouTube Music playback on WrOS, and remote music sources.
//! On other desktops YouTube Music uses th-ch/youtube-music's API Server plugin.

use crate::library;
use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

mod files;
mod local;
mod mpv;
mod setup;
mod spotify;
mod ytm;
mod ytmusic;

use files::Files;
pub(crate) use files::file_counts;
use local::Local;
pub use setup::{authenticate, setup_for};
use spotify::Spotify;
use ytm::Ytm;
use ytmusic::YtMusic;
pub(crate) use ytmusic::helper_path as ytm_helper_path;

pub const APP_ID: &str = "grimoiretui";
pub const DEFAULT_PORT: u16 = 26538;
pub(crate) const SETTINGS_PATH: &str = "Settings › Music";

/// Where the music comes from: local files, a desktop player, or your server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Files,
    YouTubeMusic,
    Spotify,
    Jellyfin,
    Plex,
}

impl Source {
    pub const ALL: [Source; 5] = [
        Source::Files,
        Source::YouTubeMusic,
        Source::Spotify,
        Source::Jellyfin,
        Source::Plex,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Source::Files => "files",
            Source::YouTubeMusic => "youtube-music",
            Source::Spotify => "spotify",
            Source::Jellyfin => "jellyfin",
            Source::Plex => "plex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Files => "Music files",
            Source::YouTubeMusic => "YouTube Music",
            Source::Spotify => "Spotify",
            Source::Jellyfin => "Jellyfin",
            Source::Plex => "Plex",
        }
    }

    pub fn parse(s: &str) -> Option<Source> {
        Source::ALL
            .into_iter()
            .find(|x| x.slug() == s.trim().to_lowercase())
    }

    /// True when Grimoire plays the audio itself rather than driving another app.
    pub fn plays_audio(self) -> bool {
        matches!(self, Source::Files | Source::Jellyfin | Source::Plex)
    }

    /// Whether this source can be chosen. On WrOS Spotify can't play: Spotify refuses
    /// unofficial players the keys to its music for newer accounts (librespot #1649), so it
    /// waits for Spotify's own player in a later update.
    pub fn available(self, wros: bool) -> bool {
        !(wros && self == Source::Spotify)
    }
}

/// What WrOS says about Spotify until it can play it.
pub(crate) const SPOTIFY_UNAVAILABLE: &str = "Spotify isn't available on WrOS yet: Spotify no longer lets players like WrOS's \
     unlock its music. Spotify's own player is planned for a later update.";

/// True on WrOS, where the sound helper is installed. Tests never ask this: they set
/// `App::wros_music` themselves, because a Nix test build can see the helper's path.
pub fn on_wros() -> bool {
    std::path::Path::new(wros_sys::paths::WROS_SOUND).exists()
}

/// Removes everything rc20's Spotify player kept: its login, web token, settings and logs
/// (which held tokens). Nothing outside those two folders is touched.
pub fn forget_spotify(home: &std::path::Path) {
    for dir in [".cache/spotify-player", ".config/spotify-player"] {
        let path = home.join(dir);
        if path.exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Once at start-up on WrOS: forget rc20's Spotify login whatever the source, and move a
/// writer whose source was Spotify to music files. Off WrOS this does nothing.
pub fn migrate_off_spotify(home: &std::path::Path, wros: bool) {
    if !wros {
        return;
    }
    forget_spotify(home);
    // rc20's sign-in worked in the session's runtime folder; a restart clears it, but an
    // interrupted sign-in in this session could have left a login there.
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        && run.is_absolute()
    {
        let _ = std::fs::remove_dir_all(run.join("wros-spotify-signin"));
    }
    let path = Config::path_in(home);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let mut cfg = Config::parse(&text);
        if cfg.source == Source::Spotify {
            cfg.source = Source::Files;
            let _ = cfg.save_to(&path);
        }
    }
}

const POLL: Duration = Duration::from_millis(1500);
const HTTP_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone)]
pub struct Config {
    pub source: Source,
    /// YouTube Music: where its API Server listens.
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
    /// Jellyfin / Plex: your own server.
    pub server: String,
    pub api_key: String,
    pub user_id: String,
    /// YouTube: play while yt-dlp fetches, or once the whole file is here.
    pub youtube_play: YoutubePlay,
    /// What the player's search looks through, as last chosen with Tab.
    pub youtube_search: SearchScope,
}

/// The YouTube Music source searches YouTube Music (songs), or all of YouTube (videos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    #[default]
    YoutubeMusic,
    AllOfYoutube,
}

impl SearchScope {
    fn slug(self) -> &'static str {
        match self {
            SearchScope::YoutubeMusic => "music",
            SearchScope::AllOfYoutube => "all",
        }
    }

    pub fn other(self) -> SearchScope {
        match self {
            SearchScope::YoutubeMusic => SearchScope::AllOfYoutube,
            SearchScope::AllOfYoutube => SearchScope::YoutubeMusic,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SearchScope::YoutubeMusic => "YouTube Music",
            SearchScope::AllOfYoutube => "all of YouTube",
        }
    }
}

/// How YouTube audio reaches the player. Streaming starts in seconds whatever the length;
/// downloading first is how rc20 and rc21 played, kept as a choice in Settings › Music.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YoutubePlay {
    #[default]
    Stream,
    Download,
}

impl YoutubePlay {
    fn slug(self) -> &'static str {
        match self {
            YoutubePlay::Stream => "stream",
            YoutubePlay::Download => "download",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: Source::Files,
            host: "127.0.0.1".into(),
            port: DEFAULT_PORT,
            token: None,
            server: String::new(),
            api_key: String::new(),
            user_id: String::new(),
            youtube_play: YoutubePlay::Stream,
            youtube_search: SearchScope::YoutubeMusic,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        Config::path_in(&grimoire_core::paths::home())
    }

    fn path_in(home: &std::path::Path) -> PathBuf {
        home.join(".config").join("grimoire").join("music.toml")
    }

    /// Missing or unreadable config just means "music off".
    pub fn load() -> Config {
        match std::fs::read_to_string(Config::path()) {
            Ok(s) => Config::parse(&s),
            Err(_) => Config::default(),
        }
    }

    fn parse(s: &str) -> Config {
        // Older configs without a source implicitly used YouTube Music.
        let mut cfg = Config {
            source: Source::YouTubeMusic,
            ..Config::default()
        };
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "source" => {
                    if let Some(x) = Source::parse(&v) {
                        cfg.source = x;
                    }
                }
                "token" if !v.is_empty() => cfg.token = Some(v),
                "host" if !v.is_empty() => cfg.host = v,
                "port" => {
                    if let Ok(p) = v.parse() {
                        cfg.port = p;
                    }
                }
                "server" => cfg.server = v,
                "api_key" => cfg.api_key = v,
                "user_id" => cfg.user_id = v,
                "youtube_play" if v == "download" => cfg.youtube_play = YoutubePlay::Download,
                "youtube_search" if v == "all" => cfg.youtube_search = SearchScope::AllOfYoutube,
                _ => {}
            }
        }
        cfg
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Config::path())
    }

    fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let body = format!(
            "# Where music comes from: Esc › Settings › Music.\n\
             source = \"{}\"\nhost = \"{}\"\nport = {}\ntoken = \"{}\"\n\
             server = \"{}\"\napi_key = \"{}\"\nuser_id = \"{}\"\nyoutube_play = \"{}\"\n\
             youtube_search = \"{}\"\n",
            self.source.slug(),
            self.host,
            self.port,
            self.token.as_deref().unwrap_or(""),
            self.server,
            self.api_key,
            self.user_id,
            self.youtube_play.slug(),
            self.youtube_search.slug(),
        );
        grimoire_core::atomic::write_text(path, &body)?;
        Ok(())
    }

    /// A Jellyfin or Plex server and its token are saved.
    pub fn server_ready(&self) -> bool {
        !self.server.is_empty() && !self.api_key.is_empty()
    }

    fn base(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub progress: f64,
    pub duration: f64,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// No token configured — the pane invites you to pair.
    NoToken,
    /// Configured, but the client isn't reachable.
    Offline,
    /// Connected, nothing loaded.
    Idle,
    Playing(Track),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    PlayPause,
    Next,
    Prev,
    /// Seconds forward, or back if negative.
    Seek(i32),
    Shuffle,
    Repeat,
    /// Nudge the volume by this many percent.
    Volume(i32),
    Like,
    /// Play the track at this position in the queue.
    JumpTo(usize),
    /// Queue a track by id, and play it if `now`.
    Enqueue {
        id: String,
        now: bool,
    },
    /// Ask for the queue; answered by a fresh `Music::queue`.
    FetchQueue,
    /// Search; answered by `Music::results`.
    Search {
        query: String,
        scope: SearchScope,
    },
    /// Your library's playlists, or YouTube Music's for a query; answered by
    /// `Music::playlists`.
    Playlists(Option<String>),
    /// Replace the queue with a playlist and play it (`now`), or queue the
    /// whole playlist. Progress arrives as notes.
    Playlist {
        id: String,
        title: String,
        now: bool,
    },
}

/// A row in the queue or in search results.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub title: String,
    pub artist: String,
    /// "4:18", or empty when the source doesn't say.
    pub length: String,
    /// What to hand back to play or queue it (such as a video id or file path).
    pub id: String,
    /// Position in the source's own queue, for jumping to it.
    pub pos: usize,
    /// The track playing now.
    pub current: bool,
    /// A music video rather than a song.
    pub video: bool,
}

/// What the poller thread sends back.
enum Update {
    State(State),
    Queue(Vec<Item>),
    Results(Vec<Item>),
    Playlists(Vec<Item>),
    Note(String),
    ClearNote(String),
    Modes(Modes),
}

/// Repeat, as the player has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }
}

/// The player's settings that a keypress changes but playback doesn't show:
/// without them on screen, pressing `r` or `+` looked like nothing happened.
/// `None` where the source can't say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Modes {
    pub repeat: Option<Repeat>,
    pub shuffle: Option<bool>,
    pub volume: Option<u8>,
    pub muted: bool,
    pub liked: Option<bool>,
}

impl Modes {
    /// A few glyphs for a narrow pane: ↻ repeat all, ↻1 repeat one, ⇄
    /// shuffle, ♥ liked, and the volume only when it's muted.
    pub fn badges(&self) -> String {
        let mut v: Vec<&str> = Vec::new();
        match self.repeat {
            Some(Repeat::All) => v.push("↻"),
            Some(Repeat::One) => v.push("↻1"),
            _ => {}
        }
        if self.shuffle == Some(true) {
            v.push("⇄");
        }
        if self.liked == Some(true) {
            v.push("♥");
        }
        let vol;
        if self.muted {
            v.push("muted");
        } else if let Some(n) = self.volume.filter(|&n| n < 100) {
            // Below full, the level is always in sight.
            vol = format!("{n}%");
            v.push(&vol);
        }
        v.join(" ")
    }

    /// Everything known, in words, for the player.
    pub fn line(&self) -> Vec<(String, bool)> {
        let mut v = Vec::new();
        if let Some(r) = self.repeat {
            v.push((format!("repeat {}", r.label()), r != Repeat::Off));
        }
        if let Some(s) = self.shuffle {
            v.push((format!("shuffle {}", if s { "on" } else { "off" }), s));
        }
        if let Some(vol) = self.volume {
            v.push((
                if self.muted {
                    "muted".to_string()
                } else {
                    format!("volume {vol}")
                },
                false,
            ));
        }
        if self.liked == Some(true) {
            v.push(("♥ liked".to_string(), true));
        }
        v
    }

    /// Whether the setting `c` changes differs between `self` and `now`.
    fn moved(&self, now: &Modes, c: &Cmd) -> bool {
        match c {
            Cmd::Repeat => self.repeat != now.repeat,
            Cmd::Shuffle => self.shuffle != now.shuffle,
            Cmd::Volume(_) => self.volume != now.volume || self.muted != now.muted,
            Cmd::Like => self.liked != now.liked,
            _ => true,
        }
    }

    /// What a command just did, said once it's known.
    fn said(&self, c: &Cmd) -> Option<String> {
        match c {
            Cmd::Repeat => self.repeat.map(|r| format!("repeat: {}", r.label())),
            Cmd::Shuffle => self
                .shuffle
                .map(|s| format!("shuffle {}", if s { "on" } else { "off" })),
            Cmd::Volume(d) => self.volume.map(|v| match (v, *d > 0) {
                (100.., true) => "volume 100 — as loud as it goes".to_string(),
                (0, false) => "volume 0".to_string(),
                _ => format!("volume {v}"),
            }),
            Cmd::Like => self
                .liked
                .map(|l| if l { "liked ♥" } else { "like taken back" }.to_string()),
            _ => None,
        }
    }
}

pub struct Music {
    /// Asleep until music is first asked for (the Music view shown, a music key, a source
    /// chosen): no poller, no player, no network until then. Holds the settings to wake with.
    asleep: Option<Config>,
    /// Jellyfin / Plex: a server and its token are saved, so the source is signed in.
    pub server_ready: bool,
    pub state: State,
    pub source: Source,
    /// The last queue fetched, and the last search's results.
    pub queue: Vec<Item>,
    pub results: Vec<Item>,
    /// Your playlists, or the last playlist search's. `length` holds the
    /// song count and `id` the playlist id.
    pub playlists: Vec<Item>,
    /// What the YouTube Music source's search looks through: YouTube Music, or all of YouTube.
    pub youtube_search: SearchScope,
    /// A short line for the player: "searching…", or why something failed.
    pub note: Option<String>,
    /// A note arrived since the app last looked (see `take_fresh_note`).
    fresh_note: bool,
    /// Repeat, shuffle, volume, like — as last read from the player.
    pub modes: Modes,
    rx: Option<Receiver<Update>>,
    tx: Option<Sender<Cmd>>,
    files_worker: Option<thread::JoinHandle<()>>,
}

/// One music source. Whether it is a remote control or a real player is an
/// implementation detail above this line.
trait Backend: Send {
    fn state(&mut self) -> Result<State>;
    fn command(&mut self, c: Cmd) -> Result<()>;
    /// The play queue, for sources that can share it.
    fn queue(&mut self) -> Result<Vec<Item>> {
        Err(anyhow!("this source doesn't share its queue"))
    }
    fn search(&mut self, _query: &str) -> Result<Vec<Item>> {
        Err(anyhow!("search works with YouTube Music"))
    }
    /// Videos from all of YouTube, as YouTube's own search finds them.
    fn search_videos(&mut self, _query: &str) -> Result<Vec<Item>> {
        Err(anyhow!("searching all of YouTube works with YouTube Music"))
    }
    /// Your own playlists (`None`), or everyone's that match a query.
    fn playlists(&mut self, _query: Option<&str>) -> Result<Vec<Item>> {
        Err(anyhow!("playlists work with YouTube Music"))
    }
    /// Repeat, shuffle, volume and like, for sources that can say.
    fn modes(&mut self) -> Result<Modes> {
        Ok(Modes::default())
    }
    /// Start queueing a playlist. That takes a while, so it runs in the
    /// background and reports its progress, and the final queue, on `tell`.
    fn load_playlist(
        &mut self,
        _id: String,
        _title: String,
        _now: bool,
        _tell: Sender<Update>,
    ) -> Result<()> {
        Err(anyhow!("playlists work with YouTube Music"))
    }
}

impl Music {
    /// Spawns the poller. Unconfigured sources stay inert and never touch the
    /// network or shell out.
    /// Music that starts nothing until [`Music::wake`]: how a desk opens.
    pub fn dormant(cfg: Config) -> Music {
        let mut music = Music::inert(cfg.source);
        music.server_ready = cfg.server_ready();
        music.youtube_search = cfg.youtube_search;
        music.asleep = Some(cfg);
        music
    }

    /// What a search looks through: the writer's choice on the YouTube Music source, which is
    /// the only one that can search all of YouTube.
    pub fn search_scope(&self) -> SearchScope {
        if self.source == Source::YouTubeMusic {
            self.youtube_search
        } else {
            SearchScope::YoutubeMusic
        }
    }

    /// Start the player and poller, once. Later calls do nothing.
    pub fn wake(&mut self) {
        if let Some(cfg) = self.asleep.take() {
            *self = Music::spawn(cfg);
        }
    }

    pub fn is_awake(&self) -> bool {
        self.asleep.is_none()
    }

    /// No backend: nothing set up, or asleep.
    fn inert(source: Source) -> Music {
        Music {
            asleep: None,
            server_ready: false,
            state: State::NoToken,
            source,
            queue: Vec::new(),
            results: Vec::new(),
            playlists: Vec::new(),
            youtube_search: SearchScope::default(),
            note: None,
            fresh_note: false,
            modes: Modes::default(),
            rx: None,
            tx: None,
            files_worker: None,
        }
    }

    pub fn spawn(cfg: Config) -> Music {
        let source = cfg.source;
        let server_ready = cfg.server_ready();
        let helper = (source == Source::YouTubeMusic)
            .then(ytm_helper_path)
            .flatten();
        let local_mpv = source == Source::Files || helper.is_some();
        let (up_tx, up_rx) = mpsc::channel::<Update>();
        let backend: Option<Box<dyn Backend>> = {
            match source {
                Source::Files => Some(Box::new(Files::new())),
                Source::YouTubeMusic => match helper {
                    Some(path) => Some(Box::new(
                        YtMusic::new(path, up_tx.clone()).playing(cfg.youtube_play),
                    )),
                    None => cfg
                        .token
                        .clone()
                        .map(|t| Box::new(Ytm::new(&cfg, t)) as Box<dyn Backend>),
                },
                Source::Spotify => Some(Box::new(Spotify) as Box<dyn Backend>),
                Source::Jellyfin => {
                    (!cfg.server.is_empty() && !cfg.api_key.is_empty()).then(|| {
                        Box::new(Local::new(Box::new(library::Jellyfin {
                            server: cfg.server.clone(),
                            token: cfg.api_key.clone(),
                            user_id: cfg.user_id.clone(),
                        }))) as Box<dyn Backend>
                    })
                }
                Source::Plex => (!cfg.server.is_empty() && !cfg.api_key.is_empty()).then(|| {
                    Box::new(Local::new(Box::new(library::Plex {
                        server: cfg.server.clone(),
                        token: cfg.api_key.clone(),
                    }))) as Box<dyn Backend>
                }),
            }
        };

        let youtube_search = cfg.youtube_search;
        let Some(mut backend) = backend else {
            let mut music = Music::inert(source);
            music.server_ready = server_ready;
            music.youtube_search = youtube_search;
            return music;
        };

        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();

        let worker = thread::spawn(move || {
            let mut polls = 0u32;
            loop {
                let next = backend.state().unwrap_or(State::Offline);
                if up_tx.send(Update::State(next)).is_err() {
                    return;
                }
                // The settings change rarely; look every few polls, and
                // straight after a key that changes one (below).
                if polls.is_multiple_of(4)
                    && let Ok(m) = backend.modes()
                    && up_tx.send(Update::Modes(m)).is_err()
                {
                    return;
                }
                polls = polls.wrapping_add(1);
                // Wait out the poll interval, but act on a command the moment
                // it arrives, and re-poll straight after one that changes
                // playback so the pane catches up with the keypress.
                let deadline = Instant::now() + POLL;
                loop {
                    let wait = deadline.saturating_duration_since(Instant::now());
                    let c = match cmd_rx.recv_timeout(wait) {
                        Ok(c) => c,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    };
                    let repoll =
                        !matches!(c, Cmd::FetchQueue | Cmd::Search { .. } | Cmd::Playlists(_));
                    let reply = match c {
                        Cmd::FetchQueue => Some(backend.queue().map(Update::Queue)),
                        Cmd::Search { query, scope } => Some(
                            match scope {
                                SearchScope::YoutubeMusic => backend.search(&query),
                                SearchScope::AllOfYoutube => backend.search_videos(&query),
                            }
                            .map(Update::Results),
                        ),
                        Cmd::Playlists(q) => {
                            Some(backend.playlists(q.as_deref()).map(Update::Playlists))
                        }
                        // Answers for itself, as it goes; only a refusal comes back here.
                        Cmd::Playlist { id, title, now } => backend
                            .load_playlist(id, title, now, up_tx.clone())
                            .err()
                            .map(Err),
                        // These reshape the queue, so send the new one along.
                        c @ (Cmd::JumpTo(_) | Cmd::Enqueue { .. }) => Some(
                            backend
                                .command(c)
                                .and_then(|_| backend.queue())
                                .map(Update::Queue),
                        ),
                        // Settings: do it, then read back what it became, and say so.
                        c @ (Cmd::Repeat | Cmd::Shuffle | Cmd::Volume(_) | Cmd::Like) => {
                            let before = backend.modes().ok();
                            match backend.command(c.clone()) {
                                Ok(()) => {
                                    // YouTube Music answers before it applies the
                                    // change: read until it shows (a volume already
                                    // at the top never will; that's fine).
                                    let mut after = None;
                                    for _ in 0..8 {
                                        thread::sleep(Duration::from_millis(150));
                                        if let Ok(m) = backend.modes() {
                                            let moved =
                                                before.as_ref().is_none_or(|b| b.moved(&m, &c));
                                            after = Some(m);
                                            if moved {
                                                break;
                                            }
                                        }
                                    }
                                    match after {
                                        Some(m) => {
                                            let note = m.said(&c);
                                            if up_tx.send(Update::Modes(m)).is_err() {
                                                return;
                                            }
                                            note.map(|n| Ok(Update::Note(n)))
                                        }
                                        None => None,
                                    }
                                }
                                Err(e) => Some(Err(e)),
                            }
                        }
                        // A refusal used to vanish here; now it's said.
                        c => backend.command(c).err().map(Err),
                    };
                    let sent = match reply {
                        Some(Ok(u)) => up_tx.send(u),
                        Some(Err(e)) => up_tx.send(Update::Note(e.to_string())),
                        None => Ok(()),
                    };
                    if sent.is_err() {
                        return;
                    }
                    if repoll {
                        break;
                    }
                }
            }
        });

        Music {
            asleep: None,
            server_ready,
            state: if local_mpv {
                State::Idle
            } else {
                State::Offline
            },
            source,
            queue: Vec::new(),
            results: Vec::new(),
            playlists: Vec::new(),
            youtube_search,
            note: None,
            fresh_note: false,
            modes: Modes::default(),
            rx: Some(up_rx),
            tx: Some(cmd_tx),
            files_worker: local_mpv.then_some(worker),
        }
    }

    /// The note the poller just sent, once: so a key pressed on the music
    /// pane (where the player's note line isn't showing) is answered in the
    /// status bar.
    pub fn take_fresh_note(&mut self) -> Option<String> {
        std::mem::take(&mut self.fresh_note)
            .then(|| self.note.clone())
            .flatten()
    }

    /// Drain whatever the poller has sent since the last frame.
    pub fn drain(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        while let Ok(u) = rx.try_recv() {
            match u {
                Update::State(s) => self.state = s,
                Update::Queue(q) => self.queue = q,
                Update::Results(r) => {
                    self.note = r.is_empty().then(|| "nothing playable found".to_string());
                    self.results = r;
                }
                Update::Playlists(p) => {
                    self.note = p.is_empty().then(|| "no playlists found".to_string());
                    self.playlists = p;
                }
                Update::Note(n) => {
                    self.note = Some(n);
                    self.fresh_note = true;
                }
                Update::ClearNote(n) => {
                    if self.note.as_deref() == Some(n.as_str()) {
                        self.note = None;
                        self.fresh_note = false;
                    }
                }
                Update::Modes(m) => self.modes = m,
            }
        }
    }

    /// Send a command to the player, waking it first: a music key is asking for music.
    pub fn send(&mut self, c: Cmd) {
        self.wake();
        if let Some(tx) = &self.tx {
            let _ = tx.send(c);
        }
    }

    /// For tests: the commands the keys send, caught instead of played.
    #[cfg(test)]
    pub fn catch_commands(&mut self) -> Receiver<Cmd> {
        let (tx, rx) = mpsc::channel();
        self.asleep = None;
        self.tx = Some(tx);
        rx
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        if let Some(worker) = self.files_worker.take() {
            // Let the poller drop its owned mpv before Grimoire exits.
            drop(self.rx.take());
            drop(self.tx.take());
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod config_tests {

    #[test]
    fn the_pane_badges_say_only_what_is_on() {
        let mut m = Modes::default();
        assert_eq!(m.badges(), "");
        m.repeat = Some(Repeat::Off);
        m.shuffle = Some(false);
        assert_eq!(m.badges(), "", "off shows nothing");
        m.repeat = Some(Repeat::One);
        m.shuffle = Some(true);
        m.liked = Some(true);
        assert_eq!(m.badges(), "↻1 ⇄ ♥");
        m.volume = Some(100);
        assert_eq!(m.badges(), "↻1 ⇄ ♥", "full volume isn't shown");
        m.volume = Some(70);
        assert_eq!(m.badges(), "↻1 ⇄ ♥ 70%");
        m.repeat = Some(Repeat::All);
        m.muted = true;
        assert_eq!(m.badges(), "↻ ⇄ ♥ muted");
    }

    #[test]
    fn a_setting_is_said_as_it_now_is() {
        let m = Modes {
            repeat: Some(Repeat::One),
            shuffle: Some(false),
            volume: Some(100),
            muted: false,
            liked: Some(false),
        };
        assert_eq!(m.said(&Cmd::Repeat).as_deref(), Some("repeat: one"));
        assert_eq!(m.said(&Cmd::Shuffle).as_deref(), Some("shuffle off"));
        assert_eq!(
            m.said(&Cmd::Volume(10)).as_deref(),
            Some("volume 100 — as loud as it goes")
        );
        assert_eq!(m.said(&Cmd::Volume(-10)).as_deref(), Some("volume 100"));
        assert_eq!(m.said(&Cmd::Like).as_deref(), Some("like taken back"));
        assert_eq!(m.said(&Cmd::Next), None);
        let words: Vec<String> = m.line().into_iter().map(|(w, _)| w).collect();
        assert_eq!(words, ["repeat one", "shuffle off", "volume 100"]);
    }

    #[test]
    fn a_change_is_seen_only_in_the_setting_it_touches() {
        let before = Modes {
            repeat: Some(Repeat::All),
            ..Modes::default()
        };
        let after = Modes {
            repeat: Some(Repeat::One),
            ..Modes::default()
        };
        assert!(before.moved(&after, &Cmd::Repeat));
        assert!(!before.moved(&after, &Cmd::Shuffle));
        assert!(
            !before.moved(&before, &Cmd::Repeat),
            "unapplied yet: keep reading"
        );
    }

    use super::*;

    #[test]
    fn an_old_on_off_line_is_ignored_and_never_written_again() {
        // rc20 and before saved `enabled`; music needs no switch now.
        let old = Config::parse("enabled = false\nsource = \"youtube-music\"\ntoken = \"abc\"\n");
        assert_eq!(old.source, Source::YouTubeMusic);
        assert_eq!(old.token.as_deref(), Some("abc"));
        let home = tempfile::tempdir().unwrap();
        let path = Config::path_in(home.path());
        old.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("enabled"), "{text}");
    }

    #[test]
    fn config_reads_youtube_play_and_defaults_to_stream() {
        assert_eq!(Config::parse("").youtube_play, YoutubePlay::Stream);
        assert_eq!(
            Config::parse("youtube_play = \"download\"\n").youtube_play,
            YoutubePlay::Download
        );
        assert_eq!(
            Config::parse("youtube_play = \"x\"\n").youtube_play,
            YoutubePlay::Stream
        );
        let home = tempfile::tempdir().unwrap();
        let path = Config::path_in(home.path());
        let cfg = Config {
            youtube_play: YoutubePlay::Download,
            ..Config::default()
        };
        cfg.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("youtube_play = \"download\"\n"), "{text}");
        assert_eq!(Config::parse(&text).youtube_play, YoutubePlay::Download);
    }

    #[test]
    fn config_remembers_where_youtube_search_looks() {
        assert_eq!(Config::parse("").youtube_search, SearchScope::YoutubeMusic);
        assert_eq!(
            Config::parse("youtube_search = \"all\"\n").youtube_search,
            SearchScope::AllOfYoutube
        );
        let cfg = Config {
            youtube_search: SearchScope::AllOfYoutube,
            ..Config::default()
        };
        let home = tempfile::tempdir().unwrap();
        let path = Config::path_in(home.path());
        cfg.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            Config::parse(&text).youtube_search,
            SearchScope::AllOfYoutube
        );
        // Only the YouTube Music source searches all of YouTube.
        let music = Music::dormant(cfg);
        assert_eq!(music.search_scope(), SearchScope::YoutubeMusic);
    }

    #[test]
    fn a_desk_opens_with_music_asleep_and_a_music_key_wakes_it() {
        let mut music = Music::dormant(Config::default());
        assert!(!music.is_awake(), "nothing starts until music is asked for");
        assert_eq!(music.state, State::NoToken);
        music.wake();
        assert!(music.is_awake());

        // A key (F5, or the laptop's play key) wakes it too, then sends. A Jellyfin source
        // with no server has no player, so nothing is started here.
        let mut music = Music::dormant(Config {
            source: Source::Jellyfin,
            ..Config::default()
        });
        music.send(Cmd::PlayPause);
        assert!(music.is_awake());
        assert_eq!(music.source, Source::Jellyfin);
    }

    #[test]
    fn new_music_configs_default_to_files_and_saved_sources_keep_their_value() {
        assert_eq!(Config::default().source, Source::Files);
        assert_eq!(Source::ALL[0], Source::Files);
        for source in Source::ALL {
            let saved = format!("source = \"{}\"\n", source.slug());
            assert_eq!(Config::parse(&saved).source, source);
        }
        assert_eq!(
            Config::parse("token = \"abc\"\n").source,
            Source::YouTubeMusic
        );
    }
}

#[cfg(test)]
mod spotify_leaves_wros_tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// A home with rc20's Spotify leftovers, an unrelated cache file and a music.toml.
    fn home_with_spotify(source: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        fs::create_dir_all(h.join(".config/grimoire")).unwrap();
        fs::write(
            h.join(".config/grimoire/music.toml"),
            format!("source = \"{source}\"\nenabled = true\n"),
        )
        .unwrap();
        for file in [
            ".cache/spotify-player/credentials.json",
            ".cache/spotify-player/spotify-player.log",
            ".config/spotify-player/app.toml",
            ".cache/other/keep",
        ] {
            fs::create_dir_all(h.join(file).parent().unwrap()).unwrap();
            fs::write(h.join(file), "x").unwrap();
        }
        home
    }

    fn source_in(home: &Path) -> Source {
        Config::parse(&fs::read_to_string(Config::path_in(home)).unwrap()).source
    }

    #[test]
    fn a_spotify_source_on_wros_becomes_music_files_and_its_login_is_removed() {
        let home = home_with_spotify("spotify");
        migrate_off_spotify(home.path(), true);
        assert_eq!(source_in(home.path()), Source::Files);
        assert!(!home.path().join(".cache/spotify-player").exists());
        assert!(!home.path().join(".config/spotify-player").exists());
        assert!(
            home.path().join(".cache/other/keep").exists(),
            "nothing else goes"
        );
    }

    #[test]
    fn a_writer_who_switched_away_from_spotify_loses_the_login_too() {
        let home = home_with_spotify("files");
        migrate_off_spotify(home.path(), true);
        assert_eq!(source_in(home.path()), Source::Files);
        assert!(!home.path().join(".cache/spotify-player").exists());
        assert!(!home.path().join(".config/spotify-player").exists());
    }

    #[test]
    fn off_wros_spotify_stays() {
        let home = home_with_spotify("spotify");
        migrate_off_spotify(home.path(), false);
        assert_eq!(source_in(home.path()), Source::Spotify);
        assert!(
            home.path()
                .join(".cache/spotify-player/credentials.json")
                .exists()
        );
        assert!(home.path().join(".config/spotify-player/app.toml").exists());
    }

    #[test]
    fn a_home_with_no_music_settings_is_left_without_them() {
        let home = tempfile::tempdir().unwrap();
        migrate_off_spotify(home.path(), true);
        assert!(!Config::path_in(home.path()).exists());
    }

    #[test]
    fn reading_the_settings_never_deletes_anything() {
        let home = home_with_spotify("spotify");
        let text = fs::read_to_string(Config::path_in(home.path())).unwrap();
        assert_eq!(Config::parse(&text).source, Source::Spotify);
        assert!(
            home.path()
                .join(".cache/spotify-player/credentials.json")
                .exists()
        );
    }

    #[test]
    fn spotify_is_unavailable_only_on_wros() {
        for source in Source::ALL {
            assert!(source.available(false), "{source:?} off WrOS");
            assert_eq!(
                source.available(true),
                source != Source::Spotify,
                "{source:?}"
            );
        }
    }
}
