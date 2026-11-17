//! Finding books on disk and naming incoming copies.

use chrono::NaiveDateTime;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ShelfBook {
    pub root: PathBuf,
    pub title: String,
    pub project_id: Option<String>,
}

fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

/// A book has a manuscript directory; neither directory may be a symlink.
pub fn is_book(dir: &Path) -> bool {
    is_directory(dir) && is_directory(&dir.join("manuscript"))
}

fn read_text(path: &Path) -> Option<String> {
    if !fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// The title in novel.toml, falling back to the folder's name.
pub fn title_of(root: &Path) -> String {
    if is_directory(root)
        && let Some(title) = read_text(&root.join("novel.toml"))
            .and_then(|s| toml::from_str::<toml::Table>(&s).ok())
            .and_then(|table| table.get("title")?.as_str().map(str::to_string))
            .filter(|title| !title.trim().is_empty())
    {
        return title;
    }
    root.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".into())
}

/// The synced book's UUID, when its local identity can be read.
pub fn project_id_of(root: &Path) -> Option<String> {
    if !is_directory(root) || !is_directory(&root.join(".grimoire")) {
        return None;
    }
    let text = read_text(&root.join(".grimoire/sync.json"))?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let id = value.get("project_id")?.as_str()?;
    let valid = id.len() == 36
        && id.bytes().enumerate().all(|(i, byte)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        });
    valid.then(|| id.to_string())
}

/// How many folders one search reads at most, so a huge non-book tree (a photo dump) can't
/// slow the shelf down.
const MAX_FOLDERS: usize = 5_000;

/// Search breadth-first, with `top` at depth zero. Books are never descended into.
pub fn find_books(top: &Path, depth: usize, limit: usize) -> Vec<ShelfBook> {
    find_books_reading(top, depth, limit, MAX_FOLDERS)
}

fn find_books_reading(top: &Path, depth: usize, limit: usize, folders: usize) -> Vec<ShelfBook> {
    let mut books = Vec::new();
    if limit == 0 {
        return books;
    }
    let mut pending = VecDeque::from([(top.to_path_buf(), 0)]);
    let mut read = 0;
    while let Some((root, level)) = pending.pop_front() {
        read += 1;
        if read > folders {
            break;
        }
        if root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            || !is_directory(&root)
        {
            continue;
        }
        if is_book(&root) {
            books.push(ShelfBook {
                title: title_of(&root),
                project_id: project_id_of(&root),
                root,
            });
            if books.len() == limit {
                break;
            }
        } else if level < depth
            && let Ok(entries) = fs::read_dir(&root)
        {
            for entry in entries.flatten() {
                if !entry.file_name().to_string_lossy().starts_with('.')
                    && entry.file_type().is_ok_and(|kind| kind.is_dir())
                {
                    pending.push_back((entry.path(), level + 1));
                }
            }
        }
    }
    books.sort_by_cached_key(|book| (book.title.to_lowercase(), book.root.clone()));
    books
}

/// Keep the incoming folder's name, numbering it when anything already owns it.
pub fn free_destination(documents: &Path, name: &str) -> PathBuf {
    let mut destination = documents.join(name);
    let mut n = 2;
    while fs::symlink_metadata(&destination).is_ok() {
        destination = documents.join(format!("{name} {n}"));
        n += 1;
    }
    destination
}

/// Prefer sync identities; without both identities, compare folder names.
pub fn same_book(local: &Path, incoming_name: &str, incoming_project_id: Option<&str>) -> bool {
    match (project_id_of(local), incoming_project_id) {
        (Some(local_id), Some(incoming_id)) => local_id == incoming_id,
        _ => local.file_name().is_some_and(|name| name == incoming_name),
    }
}

/// A conflict-copy stem that the existing sync conflict screen recognises.
pub fn merge_copy_name(stem: &str, source: &str, when: NaiveDateTime) -> String {
    format!("{stem} (from {source}, {})", when.format("%Y-%m-%d %H-%M"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "b8f5e168-9c7b-4a65-8504-fc025b254e46";
    const OTHER_ID: &str = "954176a0-0627-44dd-bfb9-07cc11f6ad8b";

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("grimoire-shelf-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn book(top: &Path, name: &str) -> PathBuf {
        let root = top.join(name);
        fs::create_dir_all(root.join("manuscript")).unwrap();
        root
    }

    fn identity(root: &Path, id: &str) {
        fs::create_dir_all(root.join(".grimoire")).unwrap();
        fs::write(
            root.join(".grimoire/sync.json"),
            serde_json::json!({ "project_id": id }).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn finds_books_to_depth_three_and_skips_hidden_and_non_books() {
        let top = scratch("discovery");
        book(&top, "A");
        let clip = book(&top, "WrOS/Clip says Hello World.");
        fs::write(
            clip.join("novel.toml"),
            "title = \"Clip Says Hello World\"\n",
        )
        .unwrap();
        book(&top, ".hidden/B");
        book(&top, "a/b/c/d/Deep");
        fs::create_dir_all(top.join("Notes")).unwrap();
        book(&top, "A/inner");

        let found = find_books(&top, 3, 500);
        let titles: Vec<&str> = found.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "Clip Says Hello World"]);
        assert_eq!(found[1].root, clip);
        assert!(found[1].root.ends_with("Clip says Hello World."));
        fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn find_books_is_bounded() {
        let top = scratch("bounded");
        for i in 0..600 {
            fs::create_dir(top.join(format!("empty-{i}"))).unwrap();
        }
        for name in ["A", "B", "C"] {
            book(&top, name);
        }
        assert_eq!(find_books(&top, 3, 2).len(), 2);
        assert!(find_books(&top, 3, 0).is_empty());
        // The folder budget stops the search: breadth-first, all 601 folders at depth one are
        // read before the book at depth two, so a budget of 50 never reaches it.
        let deep = scratch("bounded-deep");
        for i in 0..600 {
            fs::create_dir(deep.join(format!("empty-{i}"))).unwrap();
        }
        fs::create_dir(deep.join("shelf")).unwrap();
        book(&deep.join("shelf"), "Late");
        assert!(find_books_reading(&deep, 3, 500, 50).is_empty());
        assert_eq!(find_books_reading(&deep, 3, 500, 5_000).len(), 1);
        fs::remove_dir_all(top).unwrap();
        fs::remove_dir_all(deep).unwrap();
    }

    #[test]
    fn free_destination_numbers_any_taken_name() {
        let documents = scratch("destination");
        assert_eq!(free_destination(&documents, "Book"), documents.join("Book"));
        fs::create_dir(documents.join("Book")).unwrap();
        assert_eq!(
            free_destination(&documents, "Book"),
            documents.join("Book 2")
        );
        fs::write(documents.join("Book 2"), "not a book").unwrap();
        assert_eq!(
            free_destination(&documents, "Book"),
            documents.join("Book 3")
        );
        let name = "Café says Hello World.";
        assert_eq!(free_destination(&documents, name), documents.join(name));
        book(&documents, name);
        assert_eq!(
            free_destination(&documents, name),
            documents.join(format!("{name} 2"))
        );
        fs::remove_dir_all(documents).unwrap();
    }

    #[test]
    fn same_book_prefers_project_ids() {
        let top = scratch("same");
        let local = book(&top, "Original");
        identity(&local, ID);
        assert!(same_book(&local, "Renamed", Some(ID)));
        assert!(!same_book(&local, "Original", Some(OTHER_ID)));
        assert!(same_book(&local, "Original", None));
        assert!(!same_book(&local, "Renamed", None));
        fs::remove_file(local.join(".grimoire/sync.json")).unwrap();
        assert!(same_book(&local, "Original", None));
        assert!(same_book(&local, "Original", Some(ID)));
        assert!(!same_book(&local, "Renamed", Some(ID)));
        fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn merge_copy_names_round_trip_through_copy_of() {
        let when = chrono::NaiveDate::from_ymd_opt(2026, 11, 17)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let name = merge_copy_name("Scene One", "Google Drive", when);
        assert_eq!(name, "Scene One (from Google Drive, 2026-11-17 09-30)");
        let copy = crate::sync::copy_of(&name, &|s| s == "Scene One", None).unwrap();
        assert_eq!(copy.original, "Scene One");
    }

    #[test]
    fn title_of_reads_toml_and_falls_back_to_the_folder() {
        let top = scratch("title");
        let root = book(&top, "Café");
        assert_eq!(title_of(&root), "Café");
        for text in [
            "author = 'Writer'",
            "title = ''",
            "title = '  '",
            "title = 42",
            "[",
        ] {
            fs::write(root.join("novel.toml"), text).unwrap();
            assert_eq!(title_of(&root), "Café", "{text}");
        }
        fs::write(
            root.join("novel.toml"),
            "title='A \"quoted\" title' # comment\n",
        )
        .unwrap();
        assert_eq!(title_of(&root), "A \"quoted\" title");
        fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn project_id_of_ignores_missing_or_invalid_identities() {
        let top = scratch("identity");
        let root = book(&top, "Book");
        assert_eq!(project_id_of(&root), None);
        for id in [ID.to_string(), ID.to_uppercase()] {
            identity(&root, &id);
            assert_eq!(project_id_of(&root).as_deref(), Some(id.as_str()));
        }
        for id in [
            "",
            "not-a-uuid",
            "b8f5e168-9c7b-4a65-8504-fc025b254e4g",
            "b8f5e1689-c7b-4a65-8504-fc025b254e46",
            "b8f5e168-9c7b-4a65-8504-fc025b254e460",
        ] {
            identity(&root, id);
            assert_eq!(project_id_of(&root), None, "{id}");
        }
        for text in ["{", "{}", r#"{"project_id":42}"#, r#"{"project_id":null}"#] {
            fs::write(root.join(".grimoire/sync.json"), text).unwrap();
            assert_eq!(project_id_of(&root), None, "{text}");
        }
        fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn find_books_obeys_depth_breadth_first_limit_and_title_then_path_order() {
        let top = scratch("order");
        let direct = book(&top, "direct");
        let second = book(&top, "group/second");
        let first = book(&top, "group/first");
        let third = book(&top, "group/nested/third");
        book(&top, "group/nested/deeper/fourth");
        fs::write(direct.join("novel.toml"), "title = 'Beta'").unwrap();
        fs::write(second.join("novel.toml"), "title = 'ALPHA'").unwrap();
        fs::write(first.join("novel.toml"), "title = 'alpha'").unwrap();
        identity(&first, ID);

        assert!(find_books(&top, 0, 500).is_empty());
        assert_eq!(find_books(&top, 1, 500)[0].root, direct);
        let limited = find_books(&top, 3, 1);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].root, direct);
        let found = find_books(&top, 3, 500);
        let roots: Vec<&Path> = found.iter().map(|b| b.root.as_path()).collect();
        assert_eq!(
            roots,
            vec![
                first.as_path(),
                second.as_path(),
                direct.as_path(),
                third.as_path(),
            ]
        );
        assert_eq!(found[0].project_id.as_deref(), Some(ID));
        assert_eq!(found[1].project_id, None);
        assert_eq!(find_books(&direct, 0, 500)[0].root, direct);
        assert!(find_books(&top.join("missing"), 3, 500).is_empty());
        fs::remove_dir_all(top).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_are_skipped_and_dangling_destinations_are_taken() {
        use std::os::unix::fs::symlink;

        let dir = scratch("symlinks");
        let top = dir.join("Documents");
        fs::create_dir(&top).unwrap();
        let real = book(&dir, "real");
        symlink(&real, top.join("linked-book")).unwrap();
        symlink(&dir, top.join("linked-folder")).unwrap();
        let linked_manuscript = top.join("linked-manuscript");
        fs::create_dir(&linked_manuscript).unwrap();
        symlink(
            real.join("manuscript"),
            linked_manuscript.join("manuscript"),
        )
        .unwrap();
        assert!(!is_book(&top.join("linked-book")));
        assert!(!is_book(&linked_manuscript));
        assert!(is_book(&real));
        assert!(find_books(&top, 3, 500).is_empty());
        assert!(find_books(&top.join("linked-book"), 3, 500).is_empty());
        let hidden = book(&dir, ".hidden");
        assert!(find_books(&hidden, 3, 500).is_empty());

        let local = book(&top, "Book");
        fs::write(real.join("novel.toml"), "title = 'Elsewhere'").unwrap();
        symlink(real.join("novel.toml"), local.join("novel.toml")).unwrap();
        assert_eq!(title_of(&local), "Book");
        identity(&real, ID);
        symlink(real.join(".grimoire"), local.join(".grimoire")).unwrap();
        assert_eq!(project_id_of(&local), None);
        fs::remove_file(local.join(".grimoire")).unwrap();
        fs::create_dir(local.join(".grimoire")).unwrap();
        symlink(
            real.join(".grimoire/sync.json"),
            local.join(".grimoire/sync.json"),
        )
        .unwrap();
        assert_eq!(project_id_of(&local), None);
        symlink(dir.join("missing"), top.join("Taken")).unwrap();
        assert_eq!(free_destination(&top, "Taken"), top.join("Taken 2"));
        fs::remove_dir_all(dir).unwrap();
    }
}
