//! What closing the lid does. The writer's choice is kept in
//! `~/.config/grimoire/lid.json`, which backups carry with the app's other settings.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LidClose {
    /// Save, lock, then sleep. Where sleep is unavailable, save and lock.
    #[default]
    LockAndSleep,
    /// Save and lock; the laptop stays awake.
    Lock,
    /// Keep writing: closing the lid changes nothing.
    Nothing,
}

impl LidClose {
    const ALL: [Self; 3] = [Self::LockAndSleep, Self::Lock, Self::Nothing];

    fn key(self) -> &'static str {
        match self {
            Self::LockAndSleep => "lock-and-sleep",
            Self::Lock => "lock",
            Self::Nothing => "nothing",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LockAndSleep => "lock and sleep",
            Self::Lock => "lock, stay awake",
            Self::Nothing => "do nothing",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

pub fn settings_path() -> PathBuf {
    grimoire_core::paths::home().join(".config/grimoire/lid.json")
}

/// The saved choice; a missing or unreadable file means the default.
pub fn load(settings: &Path) -> LidClose {
    let value = fs::read(settings)
        .ok()
        .filter(|bytes| bytes.len() <= 65536)
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let key = value.as_ref().and_then(|v| v["lid"].as_str());
    LidClose::ALL
        .into_iter()
        .find(|c| Some(c.key()) == key)
        .unwrap_or_default()
}

pub fn save(settings: &Path, choice: LidClose) -> io::Result<()> {
    crate::session::write_private(
        settings,
        &serde_json::json!({ "version": 1, "lid": choice.key() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_survives_a_save_and_odd_files_mean_the_default() {
        let dir = std::env::temp_dir().join(format!("wros-lid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let settings = dir.join(".config/grimoire/lid.json");
        assert_eq!(load(&settings), LidClose::LockAndSleep);
        for choice in LidClose::ALL {
            save(&settings, choice).unwrap();
            assert_eq!(load(&settings), choice);
        }
        for text in [
            "{not json",
            r#"{"lid": "hibernate"}"#,
            r#"{"lid": 3}"#,
            "[]",
        ] {
            fs::write(&settings, text).unwrap();
            assert_eq!(load(&settings), LidClose::LockAndSleep, "{text}");
        }
        assert_eq!(LidClose::Nothing.next(), LidClose::LockAndSleep);
        fs::remove_dir_all(&dir).unwrap();
    }
}
