//! Which writing session this is (console or graphical) and the writer's choice
//! for the next login. /usr/libexec/wros-session reads the same files.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const CAGE: &str = "/usr/bin/cage";
const FOOT: &str = "/usr/bin/foot";

/// True inside the graphical session (cage running foot running this app).
pub fn graphical() -> bool {
    std::env::var_os("WROS_SESSION").is_some_and(|kind| kind == "graphical")
}

/// The graphical session can start only once the writer installed cage and foot.
pub fn graphical_installed() -> bool {
    Path::new(CAGE).is_file() && Path::new(FOOT).is_file()
}

/// The writer's choice: `~/.config/wros/session.json`.
pub fn settings_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    home.is_absolute()
        .then(|| home.join(".config/wros/session.json"))
}

/// Whether `settings` asks for the graphical session at the next login.
pub fn graphical_chosen(settings: &Path) -> bool {
    fs::read(settings)
        .ok()
        .filter(|bytes| bytes.len() <= 65536)
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value["session"] == "graphical")
}

/// Records the writer's choice. Turning the graphical session on again also
/// clears its failure count, so the next login tries it once more.
pub fn choose(settings: &Path, graphical: bool) -> io::Result<()> {
    let kind = if graphical { "graphical" } else { "console" };
    write_private(
        settings,
        &serde_json::json!({ "version": 1, "session": kind }),
    )?;
    if graphical && let Some(state) = state_path(settings) {
        write_private(&state, &serde_json::json!({ "version": 1, "failures": 0 }))?;
    }
    Ok(())
}

/// `~/.local/state/wros/session.json`, next to the settings' home.
fn state_path(settings: &Path) -> Option<PathBuf> {
    let home = settings.parent()?.parent()?.parent()?;
    Some(home.join(".local/state/wros/session.json"))
}

pub(crate) fn write_private(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("no folder for the session settings"))?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("json.tmp");
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temp)?;
        file.write_all(value.to_string().as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)
}

/// Tells the session launcher the app drew its first screen, so a graphical
/// session that got this far is not counted as failing to start.
pub fn mark_ready() {
    if let Some(path) = std::env::var_os("WROS_READY_FILE").map(PathBuf::from)
        && path.is_absolute()
    {
        let _ = fs::File::create(path);
    }
}

/// Why the launcher opened the console instead of the graphical session.
pub fn notice() -> Option<String> {
    let text = std::env::var("WROS_SESSION_NOTICE").ok()?;
    let clean: String = text.chars().filter(|c| !c.is_control()).take(300).collect();
    (!clean.trim().is_empty()).then_some(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wros-session-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn choosing_the_graphical_session_writes_what_the_launcher_reads() {
        let home = scratch("choose");
        let settings = home.join(".config/wros/session.json");
        let state = home.join(".local/state/wros/session.json");
        assert!(!graphical_chosen(&settings));
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(&state, r#"{"version":1,"failures":2}"#).unwrap();

        choose(&settings, true).unwrap();
        assert!(graphical_chosen(&settings));
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        assert_eq!(saved["session"], "graphical");
        // Turning it back on gives the graphical session another try.
        let count: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
        assert_eq!(count["failures"], 0);

        fs::write(&state, r#"{"version":1,"failures":1}"#).unwrap();
        choose(&settings, false).unwrap();
        assert!(!graphical_chosen(&settings));
        // Turning it off leaves the count alone.
        let count: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
        assert_eq!(count["failures"], 1);
    }

    #[test]
    fn unreadable_settings_mean_the_console() {
        let home = scratch("unreadable");
        let settings = home.join("session.json");
        for bad in [
            "",
            "[]",
            "{\"session\": 1}",
            "not json",
            "{\"session\": \"Graphical\"}",
        ] {
            fs::write(&settings, bad).unwrap();
            assert!(!graphical_chosen(&settings), "{bad:?}");
        }
    }
}
