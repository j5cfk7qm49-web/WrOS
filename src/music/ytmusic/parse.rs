//! JSON replies from wros-ytm and yt-dlp.

use crate::music::Item;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::process::Output;

pub(super) fn valid_id(id: &str) -> bool {
    (2..=80).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

pub(super) fn helper_reply<T: DeserializeOwned>(output: Output) -> Result<T> {
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
pub(super) struct Row {
    id: String,
    title: String,
    artist: String,
    pub(super) length: String,
    pub(super) count: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct Search {
    pub(super) songs: Vec<Row>,
    pub(super) albums: Vec<Row>,
    pub(super) playlists: Vec<Row>,
}

#[derive(Deserialize)]
pub(super) struct Collection {
    pub(super) title: String,
    pub(super) tracks: Vec<Row>,
}

pub(super) fn items(rows: &[Row], length: impl Fn(&Row) -> String) -> Vec<Item> {
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
pub(super) const LIVE: &str = "live";

/// Videos from `yt-dlp --flat-playlist -J ytsearchN:…`. A live stream (a radio channel, say)
/// plays as it arrives and shows "live"; one that hasn't started yet is left out. A video whose
/// length YouTube doesn't give shows none.
pub(super) fn parse_video_search(reply: &Value) -> Vec<Item> {
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
