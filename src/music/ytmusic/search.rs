//! Search YouTube Music through wros-ytm and all of YouTube through yt-dlp.

use super::{
    YtMusic,
    parse::{Search, helper_reply, items, parse_video_search},
    ytdlp_reason,
};
use crate::music::Item;
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;
#[cfg(unix)]
use std::fs;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const HELPER: &str = "wros-ytm";

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

pub(super) fn helper<T: DeserializeOwned>(path: &Path, action: &str, arg: &str) -> Result<T> {
    let output = Command::new(path)
        .args([action, arg])
        .stdin(Stdio::null())
        .output()
        .context("couldn't run wros-ytm")?;
    helper_reply(output)
}

impl YtMusic {
    fn search_for(&mut self, query: &str) -> Result<&Search> {
        if self.search.as_ref().is_none_or(|(q, _)| q != query) {
            let result = helper(&self.helper, "search", query)?;
            self.search = Some((query.to_string(), result));
        }
        self.search
            .as_ref()
            .map(|(_, search)| search)
            .context("music search unavailable")
    }

    pub(super) fn search_songs(&mut self, query: &str) -> Result<Vec<Item>> {
        self.search = None;
        let found = items(&self.search_for(query)?.songs, |row| row.length.clone());
        self.songs.clone_from(&found);
        Ok(found)
    }

    pub(super) fn find_videos(&mut self, query: &str) -> Result<Vec<Item>> {
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

    pub(super) fn search_playlists(&mut self, query: Option<&str>) -> Result<Vec<Item>> {
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
}
