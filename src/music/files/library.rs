//! Discover, label and search audio files and album folders.

use crate::music::Item;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_DEPTH: usize = 6;
const MAX_FILES: usize = 5_000;
const MAX_RESULTS: usize = 200;

fn roots() -> Vec<PathBuf> {
    let mut roots = vec![grimoire_core::paths::home().join("Music")];
    if let Some(user) = std::env::var_os("USER")
        && Path::new(&user).file_name() == Some(user.as_os_str())
    {
        let mut mounts: Vec<_> = entries(&Path::new("/run/media").join(user))
            .filter(|e| e.file_type().is_ok_and(|ty| ty.is_dir()))
            .map(|e| e.path())
            .collect();
        mounts.sort();
        roots.extend(mounts);
    }
    roots
}

pub(super) fn is_audio(path: &Path) -> bool {
    const EXTENSIONS: &[&str] = &[
        "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "wav", "wma", "alac", "aiff",
    ];
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            EXTENSIONS
                .iter()
                .any(|audio| ext.eq_ignore_ascii_case(audio))
        })
}

fn entries(dir: &Path) -> impl Iterator<Item = fs::DirEntry> {
    // read_dir would follow a symlink passed as the root.
    fs::symlink_metadata(dir)
        .is_ok_and(|info| info.is_dir())
        .then(|| fs::read_dir(dir).ok())
        .flatten()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().as_encoded_bytes().starts_with(b"."))
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, song: &mut impl FnMut(PathBuf)) {
    for entry in entries(dir) {
        if *budget == 0 {
            break;
        }
        let Ok(ty) = entry.file_type() else {
            continue;
        };
        if ty.is_dir() && depth > 1 {
            walk(&entry.path(), depth - 1, budget, song);
        } else if ty.is_file() {
            *budget -= 1;
            let path = entry.path();
            // Both item IDs and mpv's JSON protocol use Unicode strings.
            if is_audio(&path) && path.to_str().is_some() {
                song(path);
            }
        }
    }
}

struct Song {
    path: PathBuf,
    relative: String,
}

fn scan(roots: &[PathBuf]) -> Vec<Song> {
    let mut songs = Vec::new();
    let mut budget = MAX_FILES;
    for root in roots {
        walk(root, MAX_DEPTH, &mut budget, &mut |path| {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            let relative = relative.to_lowercase();
            songs.push(Song { path, relative });
        });
    }
    songs
}

fn albums(songs: &[Song]) -> BTreeMap<&Path, usize> {
    let mut albums = BTreeMap::new();
    for folder in songs.iter().filter_map(|song| song.path.parent()) {
        *albums.entry(folder).or_default() += 1;
    }
    albums
}

pub(crate) fn file_counts() -> (usize, usize) {
    let songs = scan(&roots());
    (songs.len(), albums(&songs).len())
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

pub(super) fn parent_name(path: &Path) -> String {
    path.parent().map(file_name).unwrap_or_default().to_string()
}

pub(super) fn song_title(path: &Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn song_item(path: &Path, pos: usize, current: bool) -> Item {
    Item {
        title: song_title(path),
        artist: parent_name(path),
        length: String::new(),
        id: path.to_string_lossy().into_owned(),
        pos,
        current,
        video: false,
    }
}

fn album_items(songs: &[Song], query: Option<&str>) -> Vec<Item> {
    let query = query.map(str::to_lowercase);
    albums(songs)
        .into_iter()
        .filter(|(folder, _)| {
            query
                .as_ref()
                .is_none_or(|q| folder.to_string_lossy().to_lowercase().contains(q.as_str()))
        })
        .enumerate()
        .map(|(pos, (folder, count))| Item {
            title: file_name(folder).to_string(),
            artist: parent_name(folder),
            length: format!("{count} songs"),
            id: folder.to_string_lossy().into_owned(),
            pos,
            current: false,
            video: false,
        })
        .collect()
}

fn search_items(songs: &[Song], query: &str) -> Vec<Item> {
    let query = query.trim().to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    let contains_words = |text: &str| words.iter().all(|word| text.contains(*word));
    let mut matches: Vec<_> = songs
        .iter()
        .filter(|song| contains_words(&song.relative))
        .map(|song| {
            let title = song_title(&song.path).to_lowercase();
            let rank = if title.contains(&query) {
                0
            } else if contains_words(&title) {
                1
            } else if song.relative.contains(&query) {
                2
            } else {
                3
            };
            (rank, &song.relative, &song.path)
        })
        .collect();
    matches.sort_unstable();
    matches
        .into_iter()
        .take(MAX_RESULTS)
        .enumerate()
        .map(|(pos, (_, _, path))| song_item(path, pos, false))
        .collect()
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    // Compare digit runs by magnitude without a fixed integer size.
    fn digits(bytes: &[u8]) -> (&[u8], &[u8]) {
        let len = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        let (number, rest) = bytes.split_at(len);
        let start = number.iter().position(|&b| b != b'0').unwrap_or(len);
        (&number[start..], rest)
    }

    let (mut left, mut right) = (a.as_bytes(), b.as_bytes());
    while let (Some(&l), Some(&r)) = (left.first(), right.first()) {
        let order = if l.is_ascii_digit() && r.is_ascii_digit() {
            let (ln, lrest) = digits(left);
            let (rn, rrest) = digits(right);
            (left, right) = (lrest, rrest);
            ln.len().cmp(&rn.len()).then_with(|| ln.cmp(rn))
        } else {
            (left, right) = (&left[1..], &right[1..]);
            l.to_ascii_lowercase().cmp(&r.to_ascii_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len()).then_with(|| a.cmp(b))
}

pub(super) fn album_tracks(folder: &Path) -> Vec<PathBuf> {
    let mut tracks = Vec::new();
    let mut budget = MAX_FILES;
    walk(folder, 1, &mut budget, &mut |path| tracks.push(path));
    tracks.sort_by(|a, b| natural_cmp(file_name(a), file_name(b)));
    tracks
}

pub(super) fn search(query: &str) -> Vec<Item> {
    search_items(&scan(&roots()), query)
}

pub(super) fn playlists(query: Option<&str>) -> Vec<Item> {
    album_items(&scan(&roots()), query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn audio_filter_matches_only_supported_extensions() {
        let dir = tempfile::tempdir().unwrap();
        for ext in [
            "MP3", "flac", "OgG", "oga", "opus", "m4a", "aac", "wav", "wma", "alac", "aiff",
        ] {
            touch(dir.path(), &format!("song.{ext}"));
        }
        for name in ["song", "song.mp4", "song.MP3.txt", "song.aif"] {
            touch(dir.path(), name);
        }
        assert_eq!(scan(&[dir.path().to_path_buf()]).len(), 11);
    }

    #[test]
    fn scan_skips_hidden_entries_and_tracks_beyond_six_levels() {
        let dir = tempfile::tempdir().unwrap();
        let visible = touch(dir.path(), "a/b/c/d/e/heard.mp3");
        touch(dir.path(), "a/b/c/d/e/f/too deep.mp3");
        touch(dir.path(), ".secret/hidden.mp3");
        touch(dir.path(), "a/.hidden.mp3");
        let songs = scan(&[dir.path().to_path_buf()]);
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].path, visible);
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_symlinked_tracks_and_directories() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let song = touch(outside.path(), "song.flac");
        let link = dir.path().join("album");
        symlink(song, dir.path().join("link.flac")).unwrap();
        symlink(outside.path(), &link).unwrap();
        assert!(scan(&[dir.path().to_path_buf(), link]).is_empty());
    }

    #[test]
    fn albums_are_direct_folders_and_filter_by_case_insensitive_path() {
        let dir = tempfile::tempdir().unwrap();
        let track = touch(dir.path(), "Artist/Album One/Intro.mp3");
        touch(dir.path(), "Artist/Album One/Finale.flac");
        touch(dir.path(), "Artist/Album One/Disc Two/Bonus.opus");
        touch(dir.path(), "Empty/readme.txt");
        let songs = scan(&[dir.path().to_path_buf()]);
        let listed = album_items(&songs, None);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "Album One");
        assert_eq!(listed[0].artist, "Artist");
        assert_eq!(listed[0].length, "2 songs");
        assert_eq!(Path::new(&listed[0].id), track.parent().unwrap());
        let filtered = album_items(&songs, Some("dIsC tWo"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Disc Two");
        assert_eq!(filtered[0].artist, "Album One");
    }

    #[test]
    fn search_ranks_title_matches_first_and_matches_all_words_in_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let best = touch(dir.path(), "Z Album/Blue Moon.mp3");
        touch(dir.path(), "Album/Moon over Blue.flac");
        touch(dir.path(), "Blue Moon/Opening.ogg");
        touch(dir.path(), "Blue Album/Moon.m4a");
        touch(dir.path(), "Moon Only/Opening.aac");
        let songs = scan(&[dir.path().to_path_buf()]);
        let found = search_items(&songs, "BLUE moon");
        let titles: Vec<_> = found.iter().map(|song| song.title.as_str()).collect();
        assert_eq!(titles, ["Blue Moon", "Moon over Blue", "Opening", "Moon"]);
        assert_eq!(found[0].artist, "Z Album");
        assert_eq!(Path::new(&found[0].id), best.as_path());
        assert!(search_items(&songs, "missing moon").is_empty());
    }

    #[test]
    fn scan_and_search_respect_the_file_and_result_budgets() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..=MAX_RESULTS {
            touch(dir.path(), &format!("track {i}.flac"));
        }
        let mut budget = 2;
        let mut found = Vec::new();
        walk(dir.path(), MAX_DEPTH, &mut budget, &mut |path| {
            found.push(path)
        });
        assert_eq!(found.len(), 2);
        assert_eq!(budget, 0);
        let songs = scan(&[dir.path().to_path_buf()]);
        assert_eq!(search_items(&songs, "track").len(), MAX_RESULTS);
    }

    #[test]
    fn album_tracks_use_natural_name_order_and_exclude_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let ten = touch(dir.path(), "10 Finale.flac");
        let two = touch(dir.path(), "2 Intro.FLAC");
        touch(dir.path(), "Bonus/1 Overture.flac");
        assert_eq!(album_tracks(dir.path()), [two, ten]);
    }
}
