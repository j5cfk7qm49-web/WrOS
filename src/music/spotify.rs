//! Spotify: drive the desktop app, no account setup at all.
//!
//! Spotify's Web API would mean OAuth, a registered application, a redirect
//! URL and a refresh-token dance — for the privilege of pressing pause. Both
//! platforms already expose the running player locally: AppleScript on macOS,
//! MPRIS over D-Bus on Linux. Nothing to sign in to, nothing to store.

#[cfg(target_os = "linux")]
use super::Item;
use super::{Backend, Cmd, State, Track};
use anyhow::Result;

pub(super) struct Spotify;

/// The MPRIS names of Spotify players, in the order playerctl prefers them:
/// the desktop app, then the terminal clients built on librespot (spotifyd as a
/// background player, spotify_player and ncspot as TUIs). All need Premium.
#[cfg(not(target_os = "macos"))]
const PLAYERS: &str = "spotify,spotifyd,spotify_player,ncspot";

impl Spotify {
    #[cfg(target_os = "macos")]
    fn read(&self) -> Result<State> {
        // One osascript call for the whole state — five would mean five
        // process spawns every poll.
        let script = r#"
            if application "Spotify" is running then
              tell application "Spotify"
                try
                  return (player state as string) & "|" & (name of current track) & "|" & ¬
                         (artist of current track) & "|" & ((duration of current track) / 1000) & "|" & (player position)
                on error
                  return "stopped|||0|0"
                end try
              end tell
            else
              return "notrunning"
            end if"#;
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw == "notrunning" || raw.is_empty() {
            return Ok(State::Offline);
        }
        let f: Vec<&str> = raw.split('|').collect();
        if f.len() < 5 || f[1].is_empty() {
            return Ok(State::Idle);
        }
        Ok(State::Playing(Track {
            title: f[1].to_string(),
            artist: f[2].to_string(),
            duration: f[3].parse().unwrap_or(0.0),
            progress: f[4].parse().unwrap_or(0.0),
            playing: f[0] == "playing",
        }))
    }

    #[cfg(not(target_os = "macos"))]
    fn read(&self) -> Result<State> {
        // playerctl speaks MPRIS, which every Linux Spotify player exposes.
        let out = std::process::Command::new("playerctl")
            .args([
                "-p",
                PLAYERS,
                "metadata",
                "--format",
                "{{status}}|{{title}}|{{artist}}|{{mpris:length}}|{{position}}",
            ])
            .output()?;
        if !out.status.success() {
            return Ok(State::Offline);
        }
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let f: Vec<&str> = raw.split('|').collect();
        if f.len() < 5 || f[1].is_empty() {
            return Ok(State::Idle);
        }
        // MPRIS reports both of these in microseconds.
        let us = |s: &str| s.parse::<f64>().unwrap_or(0.0) / 1_000_000.0;
        Ok(State::Playing(Track {
            title: f[1].to_string(),
            artist: f[2].to_string(),
            duration: us(f[3]),
            progress: us(f[4]),
            playing: f[0].eq_ignore_ascii_case("playing"),
        }))
    }

    #[cfg(target_os = "macos")]
    fn press(&self, c: Cmd) -> Result<()> {
        let verb = match c {
            Cmd::PlayPause => "playpause".to_string(),
            Cmd::Next => "next track".to_string(),
            Cmd::Prev => "previous track".to_string(),
            Cmd::Seek(s) => format!("set player position to (player position + {s})"),
            Cmd::Shuffle => "set shuffling to not shuffling".to_string(),
            Cmd::Repeat => "set repeating to not repeating".to_string(),
            Cmd::Volume(d) => format!("set sound volume to (sound volume + {d})"),
            _ => return Ok(()),
        };
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "if application \"Spotify\" is running then tell application \"Spotify\" to {verb}"
            ))
            .output()?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn press(&self, c: Cmd) -> Result<()> {
        let sign = |n: i32| if n >= 0 { "+" } else { "-" };
        let args: Vec<String> = match c {
            Cmd::PlayPause => vec!["play-pause".into()],
            Cmd::Next => vec!["next".into()],
            Cmd::Prev => vec!["previous".into()],
            Cmd::Seek(s) => vec!["position".into(), format!("{}{}", s.abs(), sign(s))],
            Cmd::Shuffle => vec!["shuffle".into(), "Toggle".into()],
            Cmd::Volume(d) => vec![
                "volume".into(),
                format!("{:.2}{}", d.abs() as f64 / 100.0, sign(d)),
            ],
            _ => return Ok(()),
        };
        let _ = std::process::Command::new("playerctl")
            .args(["-p", PLAYERS])
            .args(&args)
            .output()?;
        Ok(())
    }
}

impl Backend for Spotify {
    fn state(&mut self) -> Result<State> {
        #[cfg(target_os = "linux")]
        if wros_signin_available() {
            // On WrOS the player is spotify_player, signed in from Settings › Music.
            if super::spotify_signin::signed_in().is_none() {
                return Ok(State::NoToken);
            }
            let _ = super::spotify_signin::ensure_player();
        }
        self.read()
    }
    fn command(&mut self, c: Cmd) -> Result<()> {
        #[cfg(target_os = "linux")]
        if let Cmd::Enqueue { id, .. } = &c {
            let (kind, id) = cli::parse_id(id)?;
            return cli::start(kind, id);
        }
        self.press(c)
    }
    #[cfg(target_os = "linux")]
    fn search(&mut self, query: &str) -> Result<Vec<Item>> {
        Ok(cli::search(query)?.0)
    }
    #[cfg(target_os = "linux")]
    fn playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
        match query {
            Some(q) => Ok(cli::search(q)?.1),
            None => cli::own_playlists(),
        }
    }
    #[cfg(target_os = "linux")]
    fn load_playlist(
        &mut self,
        id: String,
        title: String,
        _now: bool,
        tell: std::sync::mpsc::Sender<super::Update>,
    ) -> Result<()> {
        let (kind, id) = cli::parse_id(&id)?;
        cli::start(kind, id)?;
        let _ = tell.send(super::Update::Note(format!("playing {title}")));
        Ok(())
    }
}

/// True on WrOS, where Spotify is spotify_player signed in through Settings, not a desktop app.
#[cfg(target_os = "linux")]
fn wros_signin_available() -> bool {
    std::path::Path::new(wros_sys::paths::WROS_MUSIC_SIGNIN).exists()
}

/// spotify_player's command line, which answers in JSON, for what MPRIS can't do: search,
/// playlists, and starting a track, album or playlist.
#[cfg(target_os = "linux")]
mod cli {
    use super::super::Item;
    use anyhow::{Context, Result, anyhow};
    use serde_json::Value;

    /// "track:ID", "album:ID" or "playlist:ID", with a Spotify base-62 id.
    pub(super) fn parse_id(id: &str) -> Result<(&'static str, &str)> {
        let (kind, raw) = id
            .split_once(':')
            .ok_or_else(|| anyhow!("not a Spotify item"))?;
        let kind = match kind {
            "track" => "track",
            "album" => "album",
            "playlist" => "playlist",
            _ => return Err(anyhow!("not a Spotify item")),
        };
        if raw.len() == 22 && raw.bytes().all(|b| b.is_ascii_alphanumeric()) {
            Ok((kind, raw))
        } else {
            Err(anyhow!("not a Spotify id"))
        }
    }

    fn run(args: &[&str]) -> Result<Vec<u8>> {
        super::super::spotify_signin::ensure_player()?;
        let out = std::process::Command::new("spotify_player")
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .context("running spotify_player")?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            let why = why.lines().last().unwrap_or("").trim();
            return Err(anyhow!(
                "Spotify: {}",
                why.chars().take(120).collect::<String>()
            ));
        }
        Ok(out.stdout)
    }

    pub(super) fn start(kind: &str, id: &str) -> Result<()> {
        if kind == "track" {
            run(&["playback", "start", "track", "--id", id])?;
        } else {
            run(&["playback", "start", "context", "--id", id, kind])?;
        }
        Ok(())
    }

    /// The id rspotify serialised: a bare id, or a "spotify:kind:id" URI.
    fn bare_id(v: &Value) -> Option<String> {
        let s = v.as_str()?;
        let id = s.rsplit(':').next()?;
        (id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| id.to_string())
    }

    fn names(v: &Value) -> String {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.get("name")?.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    fn length(v: &Value) -> String {
        let secs = v
            .get("secs")
            .and_then(Value::as_u64)
            .or_else(|| v.as_u64().map(|ms| ms / 1000))
            .unwrap_or(0);
        if secs == 0 {
            String::new()
        } else {
            format!("{}:{:02}", secs / 60, secs % 60)
        }
    }

    fn item(kind: &str, v: &Value, artist: String, length: String) -> Option<Item> {
        let title: String = v
            .get("name")?
            .as_str()?
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        Some(Item {
            title,
            artist,
            length,
            id: format!("{kind}:{}", bare_id(v.get("id")?)?),
            pos: 0,
            current: false,
            video: false,
        })
    }

    /// Songs, then albums and playlists, from one search.
    pub(super) fn search(query: &str) -> Result<(Vec<Item>, Vec<Item>)> {
        let query: String = query
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        if query.trim().is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        parse_search(&run(&["search", query.trim()])?)
    }

    pub(super) fn parse_search(json: &[u8]) -> Result<(Vec<Item>, Vec<Item>)> {
        let v: Value = serde_json::from_slice(json).context("reading Spotify's answer")?;
        let list = |k: &str| {
            v.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let songs = list("tracks")
            .iter()
            .filter_map(|t| item("track", t, names(&t["artists"]), length(&t["duration"])))
            .take(50)
            .collect();
        let mut sets: Vec<Item> = list("albums")
            .iter()
            .filter_map(|a| item("album", a, names(&a["artists"]), "album".into()))
            .take(20)
            .collect();
        sets.extend(
            list("playlists")
                .iter()
                .filter_map(|p| {
                    let owner = p["owner"][0].as_str().unwrap_or("").to_string();
                    item("playlist", p, owner, "playlist".into())
                })
                .take(20),
        );
        Ok((songs, sets))
    }

    pub(super) fn own_playlists() -> Result<Vec<Item>> {
        let v: Value = serde_json::from_slice(&run(&["get", "key", "user-playlists"])?)
            .context("reading Spotify's answer")?;
        Ok(v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let owner = p["owner"][0].as_str().unwrap_or("").to_string();
                        item("playlist", p, owner, "playlist".into())
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_search_answer_becomes_songs_then_albums_and_playlists() {
            let json = br#"{
              "tracks": [{"id": "spotify:track:4uLU6hMCjMI75M1A2tKUQC", "name": "Clair de lune",
                          "artists": [{"id": "x", "name": "Debussy"}, {"id": "y", "name": "Tharaud"}],
                          "duration": {"secs": 301, "nanos": 0}},
                         {"id": "bad id", "name": "dropped", "artists": [], "duration": {"secs": 1}}],
              "albums": [{"id": "6akEvsycLGftJxYudPjmqK", "name": "Suite bergamasque",
                          "artists": [{"id": "x", "name": "Debussy"}]}],
              "playlists": [{"id": "37i9dQZF1DWWEJlAGA9gs0", "name": "Classical Focus",
                             "owner": ["Spotify", "spotify"]}],
              "artists": [], "shows": [], "episodes": []
            }"#;
            let (songs, sets) = parse_search(json).unwrap();
            assert_eq!(songs.len(), 1, "an id that isn't base-62 is dropped");
            assert_eq!(songs[0].id, "track:4uLU6hMCjMI75M1A2tKUQC");
            assert_eq!(songs[0].artist, "Debussy, Tharaud");
            assert_eq!(songs[0].length, "5:01");
            assert_eq!(sets[0].id, "album:6akEvsycLGftJxYudPjmqK");
            assert_eq!(sets[1].id, "playlist:37i9dQZF1DWWEJlAGA9gs0");
            assert_eq!(sets[1].artist, "Spotify");
        }

        #[test]
        fn only_spotify_ids_reach_the_command_line() {
            assert_eq!(
                parse_id("album:6akEvsycLGftJxYudPjmqK").unwrap(),
                ("album", "6akEvsycLGftJxYudPjmqK")
            );
            for bad in [
                "album:--help",
                "track:6akEvsycLGftJxYudPjmq",
                "show:6akEvsycLGftJxYudPjmqK",
                "6akEvsycLGftJxYudPjmqK",
            ] {
                assert!(parse_id(bad).is_err(), "{bad}");
            }
        }
    }
}
