//! Signing in to Spotify on WrOS without a browser (#122).
//!
//! spotify_player's own sign-in finishes in a browser on this machine, and WrOS has none. So the
//! phone's Spotify app hands its login over instead: for at most three minutes librespot appears
//! on the local network as a Spotify Connect speaker named "WrOS", and when the writer picks it,
//! Spotify gives it a reusable login, which librespot saves as `credentials.json`. That file, in
//! the same librespot format, is what spotify_player keeps in its cache folder, and its search
//! token comes from it (login5), so after this spotify_player needs nothing else.
//!
//! The phone must reach the laptop for the handoff: `wros-music-signin open` (root, polkit) lets the
//! Connect port and mDNS through the firewall from local addresses for 180 s, and closes by itself.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long the laptop shows up as a speaker; the firewall window is the same.
pub const WINDOW: Duration = Duration::from_secs(180);
/// Fixed, so the firewall opens exactly this port (nixos/modules/music.nix).
const CONNECT_PORT: &str = "57621";
const DEVICE_NAME: &str = "WrOS";

/// spotify_player's cache folder, where its login lives.
pub fn cache_dir() -> PathBuf {
    grimoire_core::paths::home().join(".cache/spotify-player")
}

fn config_dir() -> PathBuf {
    grimoire_core::paths::home().join(".config/spotify-player")
}

pub fn credentials_path() -> PathBuf {
    cache_dir().join("credentials.json")
}

/// The Spotify account WrOS is signed in as, if any.
pub fn signed_in() -> Option<String> {
    read_username(&credentials_path())
}

/// A librespot `credentials.json` that names its account; anything else counts as no login.
fn read_username(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.len() > 64 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let username = value.get("username")?.as_str()?;
    let has_data = value
        .get("auth_data")
        .and_then(|d| d.as_str())
        .is_some_and(|d| !d.is_empty());
    (!username.is_empty() && has_data && value.get("auth_type").is_some()).then(|| {
        username
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect()
    })
}

/// Forget the login and stop the player.
pub fn sign_out() -> Result<()> {
    stop_player();
    match std::fs::remove_file(credentials_path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("removing the Spotify login"),
    }
}

/// Whether spotify_player is running for this user.
pub fn player_running() -> bool {
    let me = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|e| {
        let dir = e.path();
        std::fs::read_to_string(dir.join("comm")).is_ok_and(|c| c.trim() == "spotify_player")
            && std::os::unix::fs::MetadataExt::uid(&match std::fs::metadata(&dir) {
                Ok(m) => m,
                Err(_) => return false,
            }) == me
    })
}

/// Start spotify_player in the background (it daemonizes itself) when signed in.
pub fn ensure_player() -> Result<()> {
    if signed_in().is_none() || player_running() {
        return Ok(());
    }
    write_player_config()?;
    let status = Command::new("spotify_player")
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("starting spotify_player")?;
    if !status.success() {
        return Err(anyhow!("spotify_player didn't start ({status})"));
    }
    Ok(())
}

fn stop_player() {
    let _ = Command::new("pkill")
        .args([
            "-u",
            &unsafe { libc::getuid() }.to_string(),
            "-x",
            "spotify_player",
        ])
        .status();
}

/// The device name Spotify shows, once; a writer's own edits are kept.
fn write_player_config() -> Result<()> {
    let path = config_dir().join("app.toml");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(config_dir())?;
    grimoire_core::atomic::write_text(
        &path,
        &format!(
            "# Written by WrOS. Spotify shows this laptop as:\n[device]\nname = \"{DEVICE_NAME}\"\n"
        ),
    )
}

pub enum Update {
    /// Waiting for the phone; the window has this long left.
    Waiting(Duration),
    Done(String),
    Failed(String),
}

/// One sign-in attempt, run off the input thread.
pub struct SignIn {
    rx: Receiver<Update>,
    cancel: Arc<AtomicBool>,
    pub last: Option<Update>,
}

impl SignIn {
    pub fn start() -> SignIn {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let result = run(&stop, &|left| {
                let _ = tx.send(Update::Waiting(left));
            });
            let _ = tx.send(match result {
                Ok(name) => Update::Done(name),
                Err(e) => Update::Failed(format!("{e:#}")),
            });
        });
        SignIn {
            rx,
            cancel,
            last: Some(Update::Waiting(WINDOW)),
        }
    }

    /// Take what the worker said since last time; `true` once it finished.
    pub fn poll(&mut self) -> bool {
        while let Ok(update) = self.rx.try_recv() {
            self.last = Some(update);
        }
        matches!(self.last, Some(Update::Done(_) | Update::Failed(_)))
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// The line the Music page shows.
    pub fn line(&self) -> String {
        let finished = matches!(self.last, Some(Update::Done(_) | Update::Failed(_)));
        if self.cancel.load(Ordering::SeqCst) && !finished {
            return "Stopping the sign-in…".into();
        }
        match &self.last {
            Some(Update::Waiting(left)) => format!(
                "Open Spotify on your phone, on this Wi-Fi. Tap the speaker icon and choose \
                 \"{DEVICE_NAME}\". {}:{:02} left",
                left.as_secs() / 60,
                left.as_secs() % 60
            ),
            None => "Starting the sign-in…".into(),
            Some(Update::Done(name)) => format!("Signed in to Spotify as {name}."),
            Some(Update::Failed(why)) => format!("Not signed in: {why}"),
        }
    }
}

/// Firewall window open → librespot until a login arrives → window closed → login saved.
fn run(cancel: &AtomicBool, waiting: &dyn Fn(Duration)) -> Result<String> {
    let work = runtime_dir().join("wros-spotify-signin");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    std::fs::set_permissions(&work, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;

    firewall("open").context("couldn't let the phone reach this laptop")?;
    let mut speaker = start_speaker(&work);
    let result = match speaker.as_mut() {
        Ok(child) => wait_for_login(child, &work, cancel, waiting),
        Err(e) => Err(anyhow!("{e:#}")),
    };
    if let Ok(mut child) = speaker {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = firewall("close");
    let name = result?;
    install_login(&work.join("credentials.json"))?;
    let _ = std::fs::remove_dir_all(&work);
    ensure_player()?;
    Ok(name)
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
}

fn firewall(verb: &str) -> Result<()> {
    let out = Command::new(wros_sys::paths::PKEXEC)
        .args([wros_sys::paths::WROS_MUSIC_SIGNIN, verb])
        .stdin(Stdio::null())
        .output()
        .context("running pkexec")?;
    if out.status.success() {
        Ok(())
    } else {
        let why = String::from_utf8_lossy(&out.stderr);
        Err(anyhow!(
            "{}",
            why.trim().chars().take(160).collect::<String>()
        ))
    }
}

fn start_speaker(work: &Path) -> Result<Child> {
    Command::new("librespot")
        .args(["--name", DEVICE_NAME, "--zeroconf-port", CONNECT_PORT])
        .arg("--system-cache")
        .arg(work)
        .arg("--cache")
        .arg(work)
        .args([
            "--disable-audio-cache",
            "--backend",
            "pipe",
            "--device",
            "/dev/null",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting librespot")
}

fn wait_for_login(
    child: &mut Child,
    work: &Path,
    cancel: &AtomicBool,
    waiting: &dyn Fn(Duration),
) -> Result<String> {
    let started = Instant::now();
    loop {
        if let Some(name) = read_username(&work.join("credentials.json")) {
            return Ok(name);
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("the Spotify Connect speaker stopped ({status})"));
        }
        let elapsed = started.elapsed();
        if elapsed >= WINDOW {
            return Err(anyhow!(
                "no phone picked \"{DEVICE_NAME}\" within three minutes. Try again"
            ));
        }
        waiting(WINDOW - elapsed);
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Put the login where spotify_player reads it, readable only by this user.
fn install_login(from: &Path) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let text = std::fs::read(from).context("reading the new login")?;
    std::fs::create_dir_all(cache_dir())?;
    std::fs::set_permissions(
        cache_dir(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let tmp = cache_dir().join(".credentials.json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(&text)?;
    file.sync_all()?;
    std::fs::rename(&tmp, credentials_path())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_complete_librespot_login_counts() {
        let dir = std::env::temp_dir().join(format!("wros-sp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials.json");
        for (text, want) in [
            (
                r#"{"username":"reader","auth_type":1,"auth_data":"AbC"}"#,
                Some("reader"),
            ),
            (r#"{"username":"","auth_type":1,"auth_data":"AbC"}"#, None),
            (
                r#"{"username":"reader","auth_type":1,"auth_data":""}"#,
                None,
            ),
            (r#"{"username":"reader","auth_data":"AbC"}"#, None),
            ("not json", None),
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(read_username(&path).as_deref(), want, "{text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
